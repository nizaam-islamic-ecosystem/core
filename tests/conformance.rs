//! Integration tests for cross-cutting architectural conformance.
//!
//! These tests verify Core boundaries as an external consumer would observe
//! them. Individual primitive behavior belongs to unit tests, public security
//! API composition belongs to `tests/security.rs`, Event subsystem behavior
//! belongs to `tests/events.rs`, and runtime-specific execution coverage
//! belongs to `tests/runtime.rs`.

use std::sync::{Arc, Barrier, Mutex};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalEvent, UniversalRequest, UniversalResponse,
};
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, DestinationEligibilityInput, DestinationRequest,
    EngineObservation, EngineRegistration, EngineRegistry, Observations, ResolutionInput,
    ResolvedCapability, ResolvedContract, ResolvedRouting, RoutingStrategy, eligible_destinations,
};
use nizaam_core::events::{
    Event, EventContext, EventLifecycle, EventName, EventPublisher, EventSubscription, Scope,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EventId, MessageId, NodeId,
    OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::prelude::{Status, Version};
use nizaam_core::retry::{Attempt, AttemptLifecycleState};
use nizaam_core::runtime::pipeline::RequestPipelineError;
use nizaam_core::runtime::{EngineContext, ExecutionPipeline};
use nizaam_core::runtime::{EngineRuntime, LifecycleState, RequestAdmissionError};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};
use nizaam_core::transport::{InMemoryTransport, TransportError};

fn principal(principal_type: PrincipalType, id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(principal_type, PrincipalId::new(id).unwrap())
}

fn user_principal(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::User, id)
}

fn service_principal(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::Service, id)
}

fn operation_context(operation_id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(operation_id).unwrap(),
        CorrelationId::new(format!("{operation_id}-correlation")).unwrap(),
    ))
}

fn context(operation_id: &str) -> EngineContext {
    EngineContext::new(operation_context(operation_id))
}

fn operation_context_for_attempt(
    operation_id: &str,
    node_id: &str,
    attempt_id: &str,
) -> OperationContext {
    operation_context(operation_id).for_attempt(
        NodeId::new(node_id).unwrap(),
        AttemptId::new(attempt_id).unwrap(),
    )
}

fn context_for_attempt(
    operation_id: &str,
    node_id: &str,
    attempt_id: &str,
) -> (EngineContext, OperationContext) {
    let operation_context = operation_context_for_attempt(operation_id, node_id, attempt_id);
    (
        EngineContext::new(operation_context.clone()),
        operation_context,
    )
}

fn request_with_context(
    message_id: &str,
    capability: &str,
    payload: &[u8],
    operation_context: OperationContext,
) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new("conformance.contract").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("conformance-sender").unwrap(),
            EngineId::new("conformance-receiver").unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message_id).unwrap(),
        operation_context,
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn request(
    message_id: &str,
    operation_id: &str,
    capability: &str,
    payload: &[u8],
) -> UniversalRequest {
    request_with_context(
        message_id,
        capability,
        payload,
        operation_context(operation_id),
    )
}

fn response(request: &UniversalRequest, payload: &[u8]) -> UniversalResponse {
    let mut envelope = request.event.envelope.clone();
    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        payload.to_vec(),
    );

    UniversalResponse::new(envelope, Status::Success)
}

#[derive(Clone)]
struct StaticAuthenticator {
    result: Result<PrincipalIdentity, AuthenticationError>,
}

impl Authenticator for StaticAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.result.clone()
    }
}

struct AllowingAuthorizer;

impl Authorizer for AllowingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}

struct StaticCredentialExtractor {
    credentials: Option<Vec<u8>>,
}

impl CredentialExtractor for StaticCredentialExtractor {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.credentials.clone()
    }
}

#[derive(Debug)]
struct RecordingMiddleware {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Middleware for RecordingMiddleware {
    fn on_request(
        &self,
        _context: &mut EngineContext,
        _request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        self.events.lock().unwrap().push("middleware");
        MiddlewareResult::Continue
    }
}

struct RecordingAuthenticator {
    events: Arc<Mutex<Vec<&'static str>>>,
    principal: PrincipalIdentity,
}

impl Authenticator for RecordingAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.events.lock().unwrap().push("authenticate");
        Ok(self.principal.clone())
    }
}

struct RecordingAuthorizer {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        self.events.lock().unwrap().push("authorize");
        Ok(AuthorizationDecision::Allow)
    }
}

struct RejectingAuthenticator;

impl Authenticator for RejectingAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Err(AuthenticationError::InvalidCredentials)
    }
}

struct CredentialByOperation;

impl CredentialExtractor for CredentialByOperation {
    fn extract(&self, _context: &EngineContext, request: &UniversalRequest) -> Option<Vec<u8>> {
        Some(
            request
                .event
                .envelope
                .operation_context
                .operation
                .id
                .to_string()
                .into_bytes(),
        )
    }
}

struct PrincipalByCredential;

impl Authenticator for PrincipalByCredential {
    fn authenticate(
        &self,
        request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        match request.credentials() {
            b"concurrent-a" => Ok(user_principal("concurrent-user-a")),
            b"concurrent-b" => Ok(user_principal("concurrent-user-b")),
            _ => Err(AuthenticationError::InvalidCredentials),
        }
    }
}

struct InspectingAuthorizer {
    expected_capability: CapabilityId,
    observed: Arc<Mutex<Vec<CapabilityId>>>,
}

impl Authorizer for InspectingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.capability(), &self.expected_capability);
        self.observed
            .lock()
            .unwrap()
            .push(request.capability().clone());

        Ok(AuthorizationDecision::Allow)
    }
}

// ---------------------------------------------------------------------------
// Phase 15 cross-boundary helpers
// ---------------------------------------------------------------------------

const PHASE15_CAPABILITY: &str = "conformance.phase15";
const PHASE15_CONTRACT: &str = "conformance.phase15.contract";

fn phase15_resolved_contract() -> ResolvedContract {
    ResolvedContract::new(ContractId::new(PHASE15_CONTRACT).unwrap(), "1.0.0")
}

fn phase15_resolved_capability() -> ResolvedCapability {
    ResolvedCapability::new(CapabilityId::new(PHASE15_CAPABILITY).unwrap())
}

fn phase15_resolution(
    operation_id: &str,
    destination: &str,
) -> nizaam_core::control_plane::Resolution {
    ControlPlane::new().resolve(ResolutionInput::new(
        OperationId::new(operation_id).unwrap(),
        phase15_resolved_contract(),
        phase15_resolved_capability(),
        ResolvedRouting::new(
            nizaam_core::identity::EngineInstanceId::new(destination).unwrap(),
            RoutingStrategy::Deterministic,
        ),
    ))
}

fn phase15_attempt(operation_id: &str, attempt_id: &str, number: u32) -> Attempt {
    Attempt::new(
        OperationId::new(operation_id).unwrap(),
        AttemptId::new(attempt_id).unwrap(),
        number,
    )
    .unwrap()
}

fn request_with_target_instance(
    message_id: &str,
    capability: &str,
    payload: &[u8],
    operation_context: OperationContext,
    target_instance: nizaam_core::identity::EngineInstanceId,
) -> UniversalRequest {
    let mut request = request_with_context(message_id, capability, payload, operation_context);

    request.event.envelope.metadata.participants = request
        .event
        .envelope
        .metadata
        .participants
        .clone()
        .with_target_instance(target_instance);

    request
}

fn phase15_registration(engine: &str, instance: &str) -> EngineRegistration {
    let engine_id = EngineId::new(engine).unwrap();
    let instance_id = nizaam_core::identity::EngineInstanceId::new(instance).unwrap();
    let capability_id = CapabilityId::new(PHASE15_CAPABILITY).unwrap();

    let definition = nizaam_core::capability::CapabilityDefinition::new(
        capability_id,
        engine_id.clone(),
        "Phase 15 conformance capability",
    )
    .unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new(PHASE15_CONTRACT).unwrap(),
        CapabilityId::new(PHASE15_CAPABILITY).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );

    EngineRegistration::new(engine_id, instance_id)
        .with_capability(definition)
        .unwrap()
        .with_contract(descriptor)
        .with_runtime_metadata(
            nizaam_core::control_plane::RuntimeRegistrationMetadata::new()
                .with_lifecycle(LifecycleState::Serving)
                .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
        )
}

fn phase15_healthy_observation(engine: &str, instance: &str) -> EngineObservation {
    let engine_id = EngineId::new(engine).unwrap();
    let instance_id = nizaam_core::identity::EngineInstanceId::new(instance).unwrap();

    let health = HealthReport::new(
        engine_id.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    EngineObservation::new(engine_id, instance_id, health).unwrap()
}

fn serving_runtime(engine: &str, instance: &str) -> EngineRuntime {
    let runtime = EngineRuntime::new(
        EngineId::new(engine).unwrap(),
        nizaam_core::identity::EngineInstanceId::new(instance).unwrap(),
    );

    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime.transition(state).unwrap();
    }

    runtime
}

// ---------------------------------------------------------------------------
// Phase 14 Event architectural conformance
// ---------------------------------------------------------------------------

fn event_envelope_with_context(
    message_id: &str,
    operation_context: OperationContext,
) -> MessageEnvelope {
    let mut envelope = request_with_context(
        message_id,
        "conformance.event-capability",
        b"opaque event payload",
        operation_context,
    )
    .event
    .envelope;

    envelope.metadata.descriptor.interaction = Interaction::Event;
    envelope
}

#[test]
fn event_and_message_identity_must_remain_distinct_at_the_contract_boundary() {
    let envelope = event_envelope_with_context(
        "conformance-event-message-1",
        operation_context("conformance-event-operation"),
    );

    let event = UniversalEvent::new(
        envelope,
        "operation.completed",
        "operation.completed",
        "engine:test",
    )
    .unwrap();

    assert!(event.has_event_interaction());
    assert!(!event.event_id().as_str().is_empty());
    assert_eq!(event.message_id().as_str(), "conformance-event-message-1");
    assert_ne!(event.event_id().as_str(), event.message_id().as_str());
}

#[test]
fn event_semantic_metadata_must_remain_distinct_from_request_capability_metadata() {
    let envelope = event_envelope_with_context(
        "conformance-event-message-2",
        operation_context("conformance-event-operation-2"),
    );

    let event = UniversalEvent::new(
        envelope,
        "operation.completed",
        "operation.completed",
        "engine:test",
    )
    .unwrap();

    assert_eq!(
        event.envelope.metadata.descriptor.capability_id.as_str(),
        "conformance.event-capability"
    );
    assert_eq!(event.event_type(), "operation.completed");
    assert_eq!(event.scope(), "engine:test");
    assert_ne!(
        event.event_type(),
        event.envelope.metadata.descriptor.capability_id.as_str()
    );
    assert_eq!(
        event.envelope.metadata.descriptor.interaction,
        Interaction::Event
    );
}

#[test]
fn event_context_must_reuse_core_operation_and_security_context() {
    let operation_context = operation_context("conformance-event-context");
    let principal = user_principal("event-context-user");
    let calling_service = service_principal("event-context-service");
    let security_context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

    let event_context = EventContext::empty()
        .with_operation_context(operation_context.clone())
        .with_security_context(security_context.clone());

    let event = Event::new_with_context(
        EventId::new("conformance-event-context-1").unwrap(),
        EventName::new("operation.completed").unwrap(),
        "operation.completed",
        Scope::new("engine:test").unwrap(),
        event_context,
    )
    .unwrap();

    assert_eq!(
        event.context().operation_context(),
        Some(&operation_context)
    );
    assert_eq!(event.context().security_context(), Some(&security_context));
    assert_eq!(
        event.context().security_context().unwrap().principal(),
        &principal
    );
    assert_eq!(
        event
            .context()
            .security_context()
            .unwrap()
            .calling_service(),
        Some(&calling_service)
    );
}

#[test]
fn event_subscription_authorization_must_use_existing_core_security_context_and_authorizer() {
    struct InspectingEventAuthorizer {
        expected_principal: PrincipalIdentity,
        expected_calling_service: PrincipalIdentity,
        expected_capability: CapabilityId,
        observed_calls: Arc<Mutex<usize>>,
    }

    impl Authorizer for InspectingEventAuthorizer {
        fn authorize(
            &self,
            request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            assert_eq!(request.principal(), &self.expected_principal);
            assert_eq!(
                request.calling_service(),
                Some(&self.expected_calling_service)
            );
            assert_eq!(request.capability(), &self.expected_capability);
            *self.observed_calls.lock().unwrap() += 1;
            Ok(AuthorizationDecision::Allow)
        }
    }

    let owner = CancellationToken::new();
    let principal = user_principal("event-subscriber-user");
    let calling_service = service_principal("event-subscriber-service");
    let capability = CapabilityId::new("events.read").unwrap();
    let observed_calls = Arc::new(Mutex::new(0usize));

    let subscription = EventSubscription::new(
        EventName::new("operation.completed").unwrap(),
        "operation.completed",
        Scope::new("engine:test").unwrap(),
        |_event: &Event| {},
        &owner,
    )
    .unwrap()
    .with_security_context(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ))
    .with_authorizer(Arc::new(InspectingEventAuthorizer {
        expected_principal: principal,
        expected_calling_service: calling_service,
        expected_capability: capability.clone(),
        observed_calls: Arc::clone(&observed_calls),
    }))
    .requiring_capability(capability.clone());

    assert_eq!(subscription.authorization_capability(), Some(&capability));
    assert_eq!(
        subscription.authorization_decision().unwrap(),
        AuthorizationDecision::Allow
    );
    assert_eq!(*observed_calls.lock().unwrap(), 1);
}

#[test]
fn event_publication_and_request_pipeline_must_remain_independent() {
    let event_calls = Arc::new(Mutex::new(0usize));
    let lifecycle = Arc::new(EventLifecycle::new());
    let owner = CancellationToken::new();
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let event_calls_for_handler = Arc::clone(&event_calls);
    let subscription = EventSubscription::new(
        EventName::new("operation.completed").unwrap(),
        "operation.completed",
        Scope::new("engine:test").unwrap(),
        move |_event: &Event| {
            *event_calls_for_handler.lock().unwrap() += 1;
        },
        &owner,
    )
    .unwrap();

    publisher.subscribe(subscription).unwrap();

    let publication = publisher
        .publish(
            Event::new(
                EventId::new("conformance-independent-event").unwrap(),
                EventName::new("operation.completed").unwrap(),
                "operation.completed",
                Scope::new("engine:test").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert_eq!(*event_calls.lock().unwrap(), 0);

    let pipeline_events = Arc::new(Mutex::new(Vec::new()));
    let pipeline_events_for_downstream = Arc::clone(&pipeline_events);
    let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
        events: Arc::clone(&pipeline_events),
    });

    let mut context = context("conformance-event-request-independence");
    let mut request = request(
        "conformance-event-request-message",
        "conformance-event-request",
        "conformance.request",
        b"request",
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            pipeline_events_for_downstream
                .lock()
                .unwrap()
                .push("downstream");
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        pipeline_events.lock().unwrap().as_slice(),
        ["middleware", "downstream"]
    );
    assert_eq!(*event_calls.lock().unwrap(), 0);
}

#[test]
fn request_must_pass_through_middleware_before_downstream_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let middleware_events = Arc::clone(&events);
    let downstream_events = Arc::clone(&events);

    let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
        events: middleware_events,
    });

    let mut context = context("middleware-order");
    let mut request = request(
        "middleware-order-message",
        "middleware-order",
        "conformance.test",
        b"payload",
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            downstream_events.lock().unwrap().push("downstream");
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["middleware", "downstream"],
    );
}

#[test]
fn security_rejection_must_stop_downstream_execution() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RejectingAuthenticator,
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"invalid".to_vec()),
        },
    ));

    let (mut context, operation_context) = context_for_attempt(
        "security-rejection",
        "conformance-node-2",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "security-rejection-message",
        "conformance.test",
        b"payload",
        operation_context,
    );
    let downstream_called = Arc::new(Mutex::new(false));
    let downstream_called_by_handler = Arc::clone(&downstream_called);

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            *downstream_called_by_handler.lock().unwrap() = true;
            Ok(response(request, b"must-not-run"))
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));
    assert!(!*downstream_called.lock().unwrap());
    assert!(context.security().is_none());
}

#[test]
fn generic_authorization_must_precede_capability_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let principal = user_principal("conformance-user");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RecordingAuthenticator {
            events: Arc::clone(&events),
            principal,
        },
        RecordingAuthorizer {
            events: Arc::clone(&events),
        },
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let (mut context, operation_context) = context_for_attempt(
        "authorization-order",
        "conformance-node-3",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "authorization-order-message",
        "conformance.capability",
        b"opaque payload",
        operation_context,
    );

    let downstream_events = Arc::clone(&events);
    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            downstream_events.lock().unwrap().push("dispatch");
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["authenticate", "authorize", "dispatch"],
    );
}

#[test]
fn core_authorization_must_not_require_domain_payload_interpretation() {
    let expected_capability = CapabilityId::new("domain.operation").unwrap();
    let observed_capability = Arc::new(Mutex::new(Vec::new()));

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("payload-opaque-user")),
        },
        InspectingAuthorizer {
            expected_capability: expected_capability.clone(),
            observed: Arc::clone(&observed_capability),
        },
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let original_payload = vec![0, 255, 17, 42, 128, 3, 99];

    let (mut context, operation_context) = context_for_attempt(
        "payload-opaque",
        "conformance-node-4",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "payload-opaque-message",
        "domain.operation",
        &original_payload,
        operation_context,
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, |_context, request| {
            assert_eq!(
                request.event.envelope.payload.bytes(),
                original_payload.as_slice()
            );
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        *observed_capability.lock().unwrap(),
        vec![expected_capability],
    );
    assert_eq!(
        request.event.envelope.payload.bytes(),
        original_payload.as_slice(),
    );
}

#[test]
fn trusted_security_context_must_preserve_principal_and_calling_service() {
    let authenticated_principal = user_principal("authenticated-user");
    let calling_service = service_principal("gateway-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(authenticated_principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let expected_principal = authenticated_principal.clone();
    let expected_calling_service = calling_service.clone();

    let (context, operation_context) = context_for_attempt(
        "identity-preservation",
        "conformance-node-5",
        "conformance-attempt-1",
    );
    let mut context = context.with_security(SecurityContext::new(
        user_principal("original-principal"),
        Some(calling_service.clone()),
    ));
    let mut request = request_with_context(
        "identity-preservation-message",
        "conformance.identity",
        b"payload",
        operation_context,
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |context, request| {
            let security = context
                .security()
                .expect("successful authentication must establish security context");

            assert_eq!(security.principal(), &expected_principal);
            assert_eq!(security.calling_service(), Some(&expected_calling_service),);

            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());

    let security = context
        .security()
        .expect("successful authentication must establish security context");
    assert_eq!(security.principal(), &authenticated_principal);
    assert_eq!(security.calling_service(), Some(&calling_service));
}

#[test]
fn concurrent_requests_must_isolate_security_context() {
    let pipeline = Arc::new(
        ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
            PrincipalByCredential,
            AllowingAuthorizer,
            CredentialByOperation,
        )),
    );

    let barrier = Arc::new(Barrier::new(3));
    let observed = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();

    for (message_id, operation_id, expected_principal) in [
        (
            "concurrent-message-a",
            "concurrent-a",
            user_principal("concurrent-user-a"),
        ),
        (
            "concurrent-message-b",
            "concurrent-b",
            user_principal("concurrent-user-b"),
        ),
    ] {
        let pipeline = Arc::clone(&pipeline);
        let barrier = Arc::clone(&barrier);
        let observed = Arc::clone(&observed);

        handles.push(std::thread::spawn(move || {
            let attempt_id = format!("{operation_id}-attempt-1");
            let (mut context, operation_context) =
                context_for_attempt(operation_id, "conformance-concurrent-node", &attempt_id);
            let mut request = request_with_context(
                message_id,
                "conformance.concurrent",
                b"payload",
                operation_context,
            );

            barrier.wait();

            let result: Result<UniversalResponse, RequestPipelineError<()>> =
                pipeline.run_request(&mut context, &mut request, move |context, request| {
                    let security = context
                        .security()
                        .expect("authentication must establish security context");

                    assert_eq!(security.principal(), &expected_principal);
                    assert_eq!(
                        context.operation().operation.id.as_str(),
                        request
                            .event
                            .envelope
                            .operation_context
                            .operation
                            .id
                            .as_str(),
                    );
                    assert_eq!(
                        context.operation().attempt_id,
                        request.event.envelope.operation_context.attempt_id,
                    );
                    observed.lock().unwrap().push(expected_principal.clone());

                    Ok(response(request, b"response"))
                });

            assert!(result.is_ok());
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert!(observed.contains(&user_principal("concurrent-user-a")));
    assert!(observed.contains(&user_principal("concurrent-user-b")));
}

#[test]
fn retry_attempt_identity_survives_the_security_pipeline() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_downstream = Arc::clone(&observed);
    let principal = user_principal("retry-security-user");
    let calling_service = service_principal("retry-security-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"retry-credentials".to_vec()),
        },
    ));

    for (message_id, attempt_id) in [
        ("retry-security-message-1", "retry-security-attempt-1"),
        ("retry-security-message-2", "retry-security-attempt-2"),
    ] {
        let (context, operation_context) = context_for_attempt(
            "retry-security-operation",
            "retry-security-node",
            attempt_id,
        );
        let mut context = context.with_security(SecurityContext::new(
            principal.clone(),
            Some(calling_service.clone()),
        ));

        let mut request = request_with_context(
            message_id,
            "conformance.retry-security",
            b"retry-payload",
            operation_context,
        );

        let observed_downstream = Arc::clone(&observed_downstream);
        let principal_expected = principal.clone();
        let calling_service_expected = calling_service.clone();

        let result: Result<UniversalResponse, RequestPipelineError<()>> =
            pipeline.run_request(&mut context, &mut request, move |context, request| {
                let security = context
                    .security()
                    .expect("security context must survive into downstream execution");

                assert_eq!(security.principal(), &principal_expected);
                assert_eq!(security.calling_service(), Some(&calling_service_expected),);
                assert_eq!(
                    context.operation().operation.id,
                    request.event.envelope.operation_context.operation.id,
                );
                assert_eq!(
                    context.operation().attempt_id,
                    request.event.envelope.operation_context.attempt_id,
                );

                observed_downstream
                    .lock()
                    .unwrap()
                    .push(context.operation().attempt_id.clone());

                Ok(response(request, b"response"))
            });

        assert!(result.is_ok());
    }

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_ne!(observed[0], observed[1]);
    assert_eq!(
        observed[0].as_ref().unwrap().as_str(),
        "retry-security-attempt-1",
    );
    assert_eq!(
        observed[1].as_ref().unwrap().as_str(),
        "retry-security-attempt-2",
    );
}

#[test]
fn failed_attempt_can_be_followed_by_new_attempt_with_same_security_context() {
    let principal = user_principal("retry-preservation-user");
    let calling_service = service_principal("retry-preservation-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"retry-preservation-credentials".to_vec()),
        },
    ));

    let operation_id = OperationId::new("retry-preservation-operation").unwrap();

    let first_attempt = Attempt::new(
        operation_id.clone(),
        AttemptId::new("retry-preservation-attempt-1").unwrap(),
        1,
    )
    .unwrap();
    first_attempt.start().unwrap();

    let (first_context, first_operation_context) = context_for_attempt(
        "retry-preservation-operation",
        "retry-preservation-node",
        "retry-preservation-attempt-1",
    );
    let mut first_context = first_context.with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));

    let mut first_request = request_with_context(
        "retry-preservation-message-1",
        "conformance.retry-preservation",
        b"payload",
        first_operation_context,
    );

    let first_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut first_context,
        &mut first_request,
        |context, request| {
            let security = context
                .security()
                .expect("first attempt must have security context");
            assert_eq!(security.principal(), &principal);
            assert_eq!(security.calling_service(), Some(&calling_service));
            Ok(response(request, b"first response"))
        },
    );

    assert!(first_result.is_ok());
    first_attempt.fail().unwrap();
    assert_eq!(first_attempt.state(), AttemptLifecycleState::Failed);

    let second_attempt = Attempt::new(
        operation_id.clone(),
        AttemptId::new("retry-preservation-attempt-2").unwrap(),
        2,
    )
    .unwrap();
    second_attempt.start().unwrap();

    let (second_context, second_operation_context) = context_for_attempt(
        "retry-preservation-operation",
        "retry-preservation-node",
        "retry-preservation-attempt-2",
    );
    let mut second_context = second_context.with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));

    let mut second_request = request_with_context(
        "retry-preservation-message-2",
        "conformance.retry-preservation",
        b"payload",
        second_operation_context,
    );

    let second_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut second_context,
        &mut second_request,
        |context, request| {
            let security = context
                .security()
                .expect("second attempt must have security context");
            assert_eq!(security.principal(), &principal);
            assert_eq!(security.calling_service(), Some(&calling_service));
            Ok(response(request, b"second response"))
        },
    );

    assert!(second_result.is_ok());
    second_attempt.succeed().unwrap();

    assert_eq!(first_attempt.operation_id(), second_attempt.operation_id());
    assert_ne!(first_attempt.attempt_id(), second_attempt.attempt_id());
    assert_eq!(first_attempt.state(), AttemptLifecycleState::Failed);
    assert_eq!(second_attempt.state(), AttemptLifecycleState::Succeeded);
    assert_eq!(first_context.security(), second_context.security(),);
}

#[test]
fn security_rejection_does_not_create_an_automatic_retry_attempt() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RejectingAuthenticator,
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"rejected-credentials".to_vec()),
        },
    ));

    let attempt = Attempt::new(
        OperationId::new("rejection-operation").unwrap(),
        AttemptId::new("rejection-attempt-1").unwrap(),
        1,
    )
    .unwrap();

    let attempt_id = attempt.attempt_id().clone();
    let (mut context, operation_context) = context_for_attempt(
        "rejection-operation",
        "rejection-node",
        "rejection-attempt-1",
    );
    let mut request = request_with_context(
        "rejection-message",
        "conformance.rejection",
        b"payload",
        operation_context,
    );

    let downstream_called = Arc::new(Mutex::new(false));
    let downstream_called_by_handler = Arc::clone(&downstream_called);

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            *downstream_called_by_handler.lock().unwrap() = true;
            Ok(response(request, b"must-not-run"))
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));
    assert!(!*downstream_called.lock().unwrap());
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    assert_eq!(context.operation().attempt_id, Some(attempt_id));
}

#[test]
fn authorization_keeps_capability_identity_stable_across_attempts() {
    let expected_capability = CapabilityId::new("conformance.retry-capability").unwrap();
    let observed_capabilities = Arc::new(Mutex::new(Vec::new()));

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("retry-capability-user")),
        },
        InspectingAuthorizer {
            expected_capability: expected_capability.clone(),
            observed: Arc::clone(&observed_capabilities),
        },
        StaticCredentialExtractor {
            credentials: Some(b"retry-capability-credentials".to_vec()),
        },
    ));

    let (mut first_context, first_operation_context) = context_for_attempt(
        "retry-capability-operation",
        "retry-capability-node",
        "retry-capability-attempt-1",
    );
    let mut first_request = request_with_context(
        "retry-capability-message-1",
        "conformance.retry-capability",
        b"first",
        first_operation_context,
    );

    let first_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut first_context,
        &mut first_request,
        |_context, request| Ok(response(request, b"response-1")),
    );
    assert!(first_result.is_ok());

    let (mut second_context, second_operation_context) = context_for_attempt(
        "retry-capability-operation",
        "retry-capability-node",
        "retry-capability-attempt-2",
    );
    let mut second_request = request_with_context(
        "retry-capability-message-2",
        "conformance.retry-capability",
        b"second",
        second_operation_context,
    );

    let second_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut second_context,
        &mut second_request,
        |_context, request| Ok(response(request, b"response-2")),
    );
    assert!(second_result.is_ok());

    assert_eq!(
        first_context.operation().operation.id,
        second_context.operation().operation.id,
    );
    assert_ne!(
        first_context.operation().attempt_id,
        second_context.operation().attempt_id,
    );

    assert_eq!(
        *observed_capabilities.lock().unwrap(),
        vec![expected_capability.clone(), expected_capability],
    );
}

// ---------------------------------------------------------------------------
// Phase 15 Control Plane architectural conformance
// ---------------------------------------------------------------------------

#[test]
fn control_plane_resolution_and_routing_must_not_execute_capability() {
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_handler = Arc::clone(&calls);

    let transport = InMemoryTransport::new();
    let instance = nizaam_core::identity::EngineInstanceId::new("phase15-instance-01").unwrap();

    transport.register(
        EngineId::new("phase15-engine").unwrap(),
        instance.clone(),
        move |_request| {
            *calls_for_handler.lock().unwrap() += 1;
            panic!("routing must not execute the capability or invoke transport");
        },
    );

    let operation_id = "phase15-no-execution-operation";
    let attempt = phase15_attempt(operation_id, "phase15-no-execution-attempt", 1);
    let resolution = phase15_resolution(operation_id, instance.as_str());

    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();

    assert_eq!(decision.destination(), &instance);
    assert_eq!(decision.operation_id().as_str(), operation_id);
    assert_eq!(
        *calls.lock().unwrap(),
        0,
        "Control Plane routing must stop at the routing-decision boundary",
    );

    drop(transport);
}

#[test]
fn routing_to_communication_must_preserve_concrete_instance_identity() {
    let first_calls = Arc::new(Mutex::new(0usize));
    let second_calls = Arc::new(Mutex::new(0usize));

    let first_calls_for_handler = Arc::clone(&first_calls);
    let second_calls_for_handler = Arc::clone(&second_calls);

    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase15-engine").unwrap();
    let first = nizaam_core::identity::EngineInstanceId::new("phase15-instance-01").unwrap();
    let second = nizaam_core::identity::EngineInstanceId::new("phase15-instance-02").unwrap();

    transport.register(engine.clone(), first.clone(), move |request| {
        *first_calls_for_handler.lock().unwrap() += 1;
        response(&request, b"first")
    });
    transport.register(engine.clone(), second.clone(), move |request| {
        *second_calls_for_handler.lock().unwrap() += 1;
        response(&request, b"second")
    });

    let communication = nizaam_core::control_plane::ControlPlaneCommunication::new(transport);

    let operation = OperationId::new("phase15-concrete-target-operation").unwrap();
    let attempt = phase15_attempt(operation.as_str(), "phase15-concrete-target-attempt", 1);
    let resolution = phase15_resolution(operation.as_str(), second.as_str());
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();

    let request = request_with_target_instance(
        "phase15-concrete-target-message",
        PHASE15_CAPABILITY,
        b"payload",
        operation_context_for_attempt(
            operation.as_str(),
            "phase15-concrete-target-node",
            "phase15-concrete-target-attempt",
        ),
        second.clone(),
    );

    let result =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .unwrap();

    assert_eq!(decision.destination(), &second);
    assert_eq!(result.event.envelope.payload.bytes(), b"second");
    assert_eq!(*first_calls.lock().unwrap(), 0);
    assert_eq!(*second_calls.lock().unwrap(), 1);
}

#[test]
fn communication_must_reject_mismatched_request_target() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase15-engine").unwrap();
    let transport_target =
        nizaam_core::identity::EngineInstanceId::new("phase15-instance-02").unwrap();
    let request_target =
        nizaam_core::identity::EngineInstanceId::new("phase15-instance-01").unwrap();

    transport.register(engine, transport_target.clone(), |_request| {
        panic!("mismatched request target must not reach transport")
    });

    let communication = nizaam_core::control_plane::ControlPlaneCommunication::new(transport);

    let request = request_with_target_instance(
        "phase15-mismatched-target-message",
        PHASE15_CAPABILITY,
        b"payload",
        operation_context("phase15-mismatched-target-operation"),
        request_target,
    );

    let result =
        futures::executor::block_on(communication.send_to_engine(&transport_target, request));

    assert_eq!(
        result,
        Err(TransportError::Peer(
            "request target instance does not match the client target".into(),
        )),
    );
}

#[test]
fn stale_control_plane_decision_must_not_override_runtime_admission() {
    let runtime = serving_runtime("phase15-runtime-engine", "phase15-runtime-instance");
    let operation = "phase15-stale-decision-operation";

    let resolution = phase15_resolution(operation, runtime.instance_id().as_str());
    let attempt = phase15_attempt(operation, "phase15-stale-decision-attempt", 1);
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();

    assert_eq!(runtime.admit_request(), Ok(()));

    runtime.transition(LifecycleState::Draining).unwrap();

    assert_eq!(
        runtime.admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Draining)),
    );

    // The already-issued routing decision remains immutable. It does not
    // mutate runtime lifecycle state and cannot turn a draining runtime back
    // into a serving runtime.
    assert_eq!(decision.destination(), runtime.instance_id());
    assert_eq!(runtime.state(), LifecycleState::Draining);
}

#[test]
fn security_success_must_be_required_before_routed_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let authenticated_principal = user_principal("phase15-security-user");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RecordingAuthenticator {
            events: Arc::clone(&events),
            principal: authenticated_principal,
        },
        RecordingAuthorizer {
            events: Arc::clone(&events),
        },
        StaticCredentialExtractor {
            credentials: Some(b"phase15-security-credentials".to_vec()),
        },
    ));

    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase15-security-engine").unwrap();
    let instance =
        nizaam_core::identity::EngineInstanceId::new("phase15-security-instance").unwrap();

    let events_for_transport = Arc::clone(&events);
    transport.register(engine, instance.clone(), move |request| {
        events_for_transport.lock().unwrap().push("communicate");
        response(&request, b"executed")
    });

    let communication = nizaam_core::control_plane::ControlPlaneCommunication::new(transport);

    let operation = "phase15-security-routing-operation";
    let attempt_id = "phase15-security-routing-attempt";
    let (mut context, operation_context) =
        context_for_attempt(operation, "phase15-security-node", attempt_id);
    let mut request = request_with_target_instance(
        "phase15-security-routing-message",
        PHASE15_CAPABILITY,
        b"payload",
        operation_context,
        instance.clone(),
    );

    let communication_ref = &communication;
    let events_for_pipeline = Arc::clone(&events);

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            events_for_pipeline.lock().unwrap().push("route");

            let resolution = phase15_resolution(operation, instance.as_str());
            let attempt = phase15_attempt(operation, attempt_id, 1);
            let decision = ControlPlane::new()
                .route_resolved(&resolution, &attempt)
                .expect("security-approved request must have a valid routing decision");

            Ok(futures::executor::block_on(
                communication_ref.send_to_engine(decision.destination(), request.clone()),
            )
            .expect("security-approved routed request must reach the test transport"))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["authenticate", "authorize", "route", "communicate"],
    );
}

#[test]
fn security_rejection_must_not_reach_routing_or_communication() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RejectingAuthenticator,
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"phase15-rejected-credentials".to_vec()),
        },
    ));

    let route_calls = Arc::new(Mutex::new(0usize));
    let route_calls_for_downstream = Arc::clone(&route_calls);

    let (mut context, operation_context) = context_for_attempt(
        "phase15-security-rejection-operation",
        "phase15-security-rejection-node",
        "phase15-security-rejection-attempt",
    );
    let mut request = request_with_target_instance(
        "phase15-security-rejection-message",
        PHASE15_CAPABILITY,
        b"payload",
        operation_context,
        nizaam_core::identity::EngineInstanceId::new("phase15-security-rejection-instance")
            .unwrap(),
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, _request| {
            *route_calls_for_downstream.lock().unwrap() += 1;
            Ok(response(_request, b"must-not-run"))
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));
    assert_eq!(*route_calls.lock().unwrap(), 0);
    assert!(context.security().is_none());
}

#[test]
fn retry_attempt_must_trigger_an_independent_routing_decision() {
    let operation = "phase15-retry-routing-operation";

    let first_attempt = phase15_attempt(operation, "phase15-retry-routing-attempt-1", 1);
    let second_attempt = phase15_attempt(operation, "phase15-retry-routing-attempt-2", 2);

    let first_resolution = phase15_resolution(operation, "phase15-retry-routing-instance-01");
    let second_resolution = phase15_resolution(operation, "phase15-retry-routing-instance-02");

    let first_decision = ControlPlane::new()
        .route_resolved(&first_resolution, &first_attempt)
        .unwrap();
    let second_decision = ControlPlane::new()
        .route_resolved(&second_resolution, &second_attempt)
        .unwrap();

    assert_eq!(
        first_decision.operation_id(),
        second_decision.operation_id()
    );
    assert_ne!(first_decision.attempt_id(), second_decision.attempt_id());
    assert_ne!(first_decision.destination(), second_decision.destination());
    assert_eq!(
        first_decision.destination().as_str(),
        "phase15-retry-routing-instance-01",
    );
    assert_eq!(
        second_decision.destination().as_str(),
        "phase15-retry-routing-instance-02",
    );
}

#[test]
fn routing_failure_must_not_create_an_automatic_retry_attempt() {
    let membership = nizaam_core::control_plane::Membership::new();
    let observations = Observations::new();

    let destination = DestinationRequest::hard_logical(CapabilityRequirement::new(
        CapabilityId::new(PHASE15_CAPABILITY).unwrap(),
    ));

    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();

    let routing_result = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership_snapshot,
        &observation_snapshot,
        &CapabilityId::new(PHASE15_CAPABILITY).unwrap(),
        &ContractDescriptor::new(
            ContractId::new(PHASE15_CONTRACT).unwrap(),
            CapabilityId::new(PHASE15_CAPABILITY).unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        ),
    ));

    assert_eq!(
        routing_result,
        Err(nizaam_core::control_plane::DestinationEligibilityError::NoEligibleDestination)
    );

    let attempt = phase15_attempt(
        "phase15-routing-failure-operation",
        "phase15-routing-failure-attempt-1",
        1,
    );

    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    assert_eq!(
        attempt.operation_id().as_str(),
        "phase15-routing-failure-operation"
    );
    assert_eq!(
        attempt.attempt_id().as_str(),
        "phase15-routing-failure-attempt-1"
    );

    membership
        .register(phase15_registration(
            "phase15-routing-failure-engine",
            "phase15-routing-failure-instance",
        ))
        .unwrap();

    observations
        .update(phase15_healthy_observation(
            "phase15-routing-failure-engine",
            "phase15-routing-failure-instance",
        ))
        .unwrap();

    assert!(membership.contains(
        &nizaam_core::identity::EngineInstanceId::new("phase15-routing-failure-instance").unwrap()
    ));

    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
}

#[test]
fn control_plane_registry_and_runtime_state_must_remain_separate() {
    let registry = EngineRegistry::new();
    let registration =
        phase15_registration("phase15-separation-engine", "phase15-separation-instance");

    registry.register(registration).unwrap();

    let runtime = serving_runtime("phase15-separation-engine", "phase15-separation-instance");

    assert!(registry.contains(runtime.instance_id()));
    assert_eq!(runtime.state(), LifecycleState::Serving);

    runtime.transition(LifecycleState::Draining).unwrap();

    // Registry metadata remains present; runtime lifecycle state changes
    // independently and must be consulted by the runtime admission boundary.
    assert!(registry.contains(runtime.instance_id()));
    assert_eq!(runtime.state(), LifecycleState::Draining);
    assert_eq!(
        runtime.admit_request(),
        Err(RequestAdmissionError::NotServing(LifecycleState::Draining)),
    );
}
