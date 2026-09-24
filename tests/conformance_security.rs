use std::sync::{Arc, Mutex};

use nizaam_core::{
    contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
    },
    control_plane::{
        ControlPlane, ControlPlaneCommunication, DestinationEligibilityInput, DestinationRequest,
        EngineObservation, EngineRegistration, Membership, Observations, ResolutionInput,
        ResolvedCapability, ResolvedContract, ResolvedRouting, RoutingStrategy,
        eligible_destinations,
    },
    health::{HealthReport, LivenessReport, ReadinessReport},
    identity::{
        AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
        OperationId,
    },
    middleware::stages::{Middleware, MiddlewareResult},
    operation::{Operation, OperationContext},
    retry::{Attempt, AttemptLifecycleState},
    runtime::{EngineContext, ExecutionPipeline, LifecycleState},
    security::{
        AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
        AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
        PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
    },
    status::Status,
    streaming::{BackpressureConfig, BackpressurePolicy, Stream, StreamItem},
    transport::InMemoryTransport,
};

fn principal(kind: PrincipalType, id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(kind, PrincipalId::new(id).unwrap())
}

fn user(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::User, id)
}

fn service(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::Service, id)
}

fn operation_context(id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(id).unwrap(),
        CorrelationId::new(format!("{id}-correlation")).unwrap(),
    ))
}

fn context(id: &str) -> EngineContext {
    EngineContext::new(operation_context(id))
}

fn request(id: &str, capability: &str, payload: &[u8]) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new("security.conformance.contract").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("security-conformance-sender").unwrap(),
            EngineId::new("security-conformance-receiver").unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(id).unwrap(),
        operation_context(id),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn response(request: &UniversalRequest) -> UniversalResponse {
    let mut envelope = request.event.envelope.clone();
    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        b"security-ok".to_vec(),
    );
    UniversalResponse::new(envelope, Status::Success)
}

#[derive(Clone)]
struct StaticCredentials(Option<Vec<u8>>);

impl CredentialExtractor for StaticCredentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.0.clone()
    }
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

struct StaticAuthorizer {
    result: Result<AuthorizationDecision, AuthorizationError>,
}

impl Authorizer for StaticAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        self.result
    }
}

struct RecordingAuthorizer {
    expected_principal: PrincipalIdentity,
    expected_calling_service: Option<PrincipalIdentity>,
    expected_capability: CapabilityId,
    observed: Arc<Mutex<Vec<CapabilityId>>>,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.principal(), &self.expected_principal);
        assert_eq!(
            request.calling_service(),
            self.expected_calling_service.as_ref()
        );
        assert_eq!(request.capability(), &self.expected_capability);
        self.observed
            .lock()
            .unwrap()
            .push(request.capability().clone());
        Ok(AuthorizationDecision::Allow)
    }
}

struct RecordingStage {
    events: Arc<Mutex<Vec<&'static str>>>,
    name: &'static str,
}

impl Middleware for RecordingStage {
    fn on_request(
        &self,
        _context: &mut EngineContext,
        _request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        self.events.lock().unwrap().push(self.name);
        MiddlewareResult::Continue
    }
}

#[test]
fn authentication_success_establishes_the_trusted_principal() {
    let expected = user("security-user");
    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(expected.clone()),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"opaque-token".to_vec())),
    );

    let mut context = context("security-auth-success");
    let mut request = request("security-auth-success-message", "security.read", b"opaque");

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );
    assert_eq!(context.security().unwrap().principal(), &expected);
}

#[test]
fn authentication_failure_stops_authorization_and_execution() {
    let authorizer_calls = Arc::new(Mutex::new(0usize));
    let calls = Arc::clone(&authorizer_calls);

    struct CountingAuthorizer {
        calls: Arc<Mutex<usize>>,
    }

    impl Authorizer for CountingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            *self.calls.lock().unwrap() += 1;
            Ok(AuthorizationDecision::Allow)
        }
    }

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Err(AuthenticationError::InvalidCredentials),
        },
        CountingAuthorizer { calls },
        StaticCredentials(Some(b"bad".to_vec())),
    );

    let mut context = context("security-auth-failure");
    let mut request = request("security-auth-failure-message", "security.read", b"opaque");

    assert!(matches!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Reject(_)
    ));
    assert!(context.security().is_none());
    assert_eq!(*authorizer_calls.lock().unwrap(), 0);
}

#[test]
fn authentication_subsystem_failure_is_distinct_from_invalid_credentials() {
    let failed = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Err(AuthenticationError::Failed),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut context = context("security-auth-subsystem");
    let mut request = request(
        "security-auth-subsystem-message",
        "security.read",
        b"opaque",
    );

    assert!(matches!(
        failed.on_request(&mut context, &mut request),
        MiddlewareResult::Fail(_)
    ));
    assert!(context.security().is_none());
}

#[test]
fn authorization_allow_and_deny_remain_distinct() {
    let allow = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("allow-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut allow_context = context("security-allow");
    let mut allow_request = request("security-allow-message", "security.read", b"payload");
    assert_eq!(
        allow.on_request(&mut allow_context, &mut allow_request),
        MiddlewareResult::Continue
    );

    let deny = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("deny-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Deny),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut deny_context = context("security-deny");
    let mut deny_request = request("security-deny-message", "security.read", b"payload");
    assert!(matches!(
        deny.on_request(&mut deny_context, &mut deny_request),
        MiddlewareResult::Reject(_)
    ));
}

#[test]
fn authorization_failure_fails_closed() {
    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("failure-user")),
        },
        StaticAuthorizer {
            result: Err(AuthorizationError::Failed),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut context = context("security-authorizer-failure");
    let mut request = request(
        "security-authorizer-failure-message",
        "security.read",
        b"payload",
    );

    assert!(matches!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Fail(_)
    ));
}

#[test]
fn principal_and_calling_service_are_preserved_together() {
    let principal = user("delegated-user");
    let calling_service = service("gateway");

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        RecordingAuthorizer {
            expected_principal: principal.clone(),
            expected_calling_service: Some(calling_service.clone()),
            expected_capability: CapabilityId::new("security.read").unwrap(),
            observed: Arc::new(Mutex::new(Vec::new())),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut context = context("security-delegation").with_security(SecurityContext::new(
        service("existing-principal"),
        Some(calling_service.clone()),
    ));
    let mut request = request("security-delegation-message", "security.read", b"payload");

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );

    let security = context.security().unwrap();
    assert_eq!(security.principal(), &principal);
    assert_eq!(security.calling_service(), Some(&calling_service));
}

#[test]
fn child_context_preserves_security_information() {
    let principal = user("child-user");
    let calling_service = service("child-gateway");

    let parent = context("security-child").with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));
    let child = parent.child();

    assert_eq!(child.security().unwrap().principal(), &principal);
    assert_eq!(
        child.security().unwrap().calling_service(),
        Some(&calling_service)
    );
    assert_eq!(child.operation(), parent.operation());
}

#[test]
fn authorization_uses_capability_identity_without_interpreting_payload() {
    let capability = CapabilityId::new("security.opaque").unwrap();
    let observed = Arc::new(Mutex::new(Vec::new()));

    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("opaque-user")),
        },
        RecordingAuthorizer {
            expected_principal: user("opaque-user"),
            expected_calling_service: None,
            expected_capability: capability.clone(),
            observed: Arc::clone(&observed),
        },
        StaticCredentials(Some(b"credential".to_vec())),
    );

    let mut context = context("security-opaque");
    let mut request = request(
        "security-opaque-message",
        "security.opaque",
        b"domain payload that Core must not interpret",
    );

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );
    assert_eq!(*observed.lock().unwrap(), vec![capability]);
}

#[test]
fn security_middleware_order_is_authenticate_then_authorize_then_downstream() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_auth = Arc::clone(&events);
    let events_authorize = Arc::clone(&events);

    struct OrderedAuthenticator {
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Authenticator for OrderedAuthenticator {
        fn authenticate(
            &self,
            _request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            self.events.lock().unwrap().push("authenticate");
            Ok(user("ordered-user"))
        }
    }

    struct OrderedAuthorizer {
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Authorizer for OrderedAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            self.events.lock().unwrap().push("authorize");
            Ok(AuthorizationDecision::Allow)
        }
    }

    let middleware = SecurityMiddleware::new(
        OrderedAuthenticator {
            events: events_auth,
        },
        OrderedAuthorizer {
            events: events_authorize,
        },
        StaticCredentials(Some(b"ordered-token".to_vec())),
    );

    let mut context = context("security-order");
    let mut request = request("security-order-message", "security.read", b"payload");
    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );
    events.lock().unwrap().push("downstream");

    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["authenticate", "authorize", "downstream"]
    );
}

#[test]
fn security_success_is_required_before_routed_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let route_events = Arc::clone(&events);
    let stage_events = Arc::clone(&events);

    let pipeline = ExecutionPipeline::new()
        .with_middleware(SecurityMiddleware::new(
            StaticAuthenticator {
                result: Ok(user("route-user")),
            },
            StaticAuthorizer {
                result: Ok(AuthorizationDecision::Allow),
            },
            StaticCredentials(Some(b"route-token".to_vec())),
        ))
        .with_middleware(RecordingStage {
            events: stage_events,
            name: "recording-stage",
        });

    let mut context = context("security-route");
    let mut request = request("security-route-message", "security.route", b"payload");

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, move |context, request| {
            route_events.lock().unwrap().push("route");
            assert!(context.security().is_some());
            Ok(response(request))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["recording-stage", "route"]
    );
}

#[test]
fn authenticated_request_can_reach_control_plane_and_transport_only_after_security() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let target = EngineInstanceId::new("security-route-instance").unwrap();
    let operation = "security-route-cross-system";
    let attempt_id = AttemptId::new("security-route-attempt").unwrap();

    let transport = InMemoryTransport::new();
    let transport_events = Arc::clone(&events);
    let engine = EngineId::new("security-route-engine").unwrap();
    transport.register(engine, target.clone(), move |request| {
        transport_events.lock().unwrap().push("transport");
        response(&request)
    });
    let communication = ControlPlaneCommunication::new(transport);

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("cross-system-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"cross-system-token".to_vec())),
    ));

    let mut context = EngineContext::new(operation_context(operation));
    let mut request = request(
        "security-cross-system-message",
        "security.route",
        b"payload",
    );
    request.event.envelope.operation_context = operation_context(operation).for_attempt(
        nizaam_core::identity::NodeId::new("security-node").unwrap(),
        attempt_id.clone(),
    );
    request.event.envelope.metadata.participants = request
        .event
        .envelope
        .metadata
        .participants
        .clone()
        .with_target_instance(target.clone());

    let communication_ref = &communication;
    let events_for_route = Arc::clone(&events);

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            events_for_route.lock().unwrap().push("route");

            let resolution = ControlPlane::new().resolve(ResolutionInput::new(
                OperationId::new(operation).unwrap(),
                ResolvedContract::new(ContractId::new("security.route.contract").unwrap(), "1.0.0"),
                ResolvedCapability::new(CapabilityId::new("security.route").unwrap()),
                ResolvedRouting::new(target.clone(), RoutingStrategy::Deterministic),
            ));

            let attempt =
                Attempt::new(OperationId::new(operation).unwrap(), attempt_id.clone(), 1).unwrap();

            let decision = ControlPlane::new()
                .route_resolved(&resolution, &attempt)
                .unwrap();

            Ok(futures::executor::block_on(
                communication_ref.send_to_engine(decision.destination(), request.clone()),
            )
            .unwrap())
        });

    assert!(result.is_ok());
    assert_eq!(events.lock().unwrap().as_slice(), ["route", "transport"]);
}

#[test]
fn security_rejection_does_not_enter_control_plane_or_transport() {
    let route_calls = Arc::new(Mutex::new(0usize));
    let transport_calls = Arc::new(Mutex::new(0usize));

    let route_calls_for_handler = Arc::clone(&route_calls);
    let transport_calls_for_handler = Arc::clone(&transport_calls);

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Err(AuthenticationError::InvalidCredentials),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"bad-token".to_vec())),
    ));

    let mut context = context("security-route-reject");
    let mut request = request(
        "security-route-reject-message",
        "security.route",
        b"payload",
    );

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, move |_context, _request| {
            *route_calls_for_handler.lock().unwrap() += 1;
            *transport_calls_for_handler.lock().unwrap() += 1;
            Ok(response(_request))
        });

    assert!(result.is_err());
    assert_eq!(*route_calls.lock().unwrap(), 0);
    assert_eq!(*transport_calls.lock().unwrap(), 0);
    assert!(context.security().is_none());
}

#[test]
fn security_rejection_does_not_create_a_retry_attempt() {
    use nizaam_core::retry::{
        BackoffPolicy, FailureCategory, RetryAdmission, RetryAdmissionRequest, RetryBudget,
        RetryPolicy, RetrySafetyGates,
    };
    use nizaam_core::status::Retryability;

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Err(AuthenticationError::InvalidCredentials),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"bad-token".to_vec())),
    ));

    let mut context = EngineContext::new(operation_context("security-retry-boundary"))
        .with_security(SecurityContext::new(user("preexisting"), None));
    let mut request = request("security-retry-message", "security.retry", b"payload");

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |_context, _request| {
            panic!("security rejection must prevent execution");
        });

    assert!(result.is_err());

    let operation = OperationId::new("security-retry-boundary").unwrap();
    let attempt = Attempt::new(
        operation,
        AttemptId::new("security-retry-attempt").unwrap(),
        1,
    )
    .unwrap();
    attempt.start().unwrap();
    attempt.fail().unwrap();

    let policy = RetryPolicy::new(3, 4).unwrap();
    let backoff = BackoffPolicy::no_backoff();
    let cancellation = nizaam_core::operation::CancellationToken::new();
    let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
    let mut budget = RetryBudget::new(2);

    let retry = admission.admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &attempt,
        category: FailureCategory::Unknown,
        retryability: Retryability::NonRetryable,
        next_attempt_id: AttemptId::new("security-retry-successor").unwrap(),
        jitter_source: None,
        safety_gates: RetrySafetyGates::new(true, true, true, true),
    });

    assert!(retry.is_err());
    assert_eq!(budget.consumed(), 0);
    assert_eq!(attempt.state(), AttemptLifecycleState::Failed);
}

#[test]
fn retry_attempt_context_preserves_security_context() {
    let principal = user("retry-user");
    let calling_service = service("retry-gateway");
    let parent = EngineContext::new(operation_context("security-retry-context")).with_security(
        SecurityContext::new(principal.clone(), Some(calling_service.clone())),
    );

    let first = parent.for_attempt(
        nizaam_core::identity::NodeId::new("node-a").unwrap(),
        AttemptId::new("attempt-a").unwrap(),
    );
    let second = parent.for_attempt(
        nizaam_core::identity::NodeId::new("node-b").unwrap(),
        AttemptId::new("attempt-b").unwrap(),
    );

    assert_eq!(first.security(), second.security());
    assert_eq!(
        first.operation().operation.id,
        second.operation().operation.id
    );
    assert_ne!(first.operation().attempt_id, second.operation().attempt_id);
}

#[test]
fn stale_routing_decision_cannot_override_runtime_admission() {
    let runtime = nizaam_core::runtime::EngineRuntime::new(
        EngineId::new("security-runtime").unwrap(),
        EngineInstanceId::new("security-runtime-instance").unwrap(),
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
    assert_eq!(runtime.admit_request(), Ok(()));

    let operation = OperationId::new("security-stale-route").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("security-stale-attempt").unwrap(),
        1,
    )
    .unwrap();
    let resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation,
        ResolvedContract::new(ContractId::new("security.stale.contract").unwrap(), "1.0.0"),
        ResolvedCapability::new(CapabilityId::new("security.stale").unwrap()),
        ResolvedRouting::new(
            EngineInstanceId::new("security-runtime-instance").unwrap(),
            RoutingStrategy::Deterministic,
        ),
    ));
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();

    runtime.transition(LifecycleState::Draining).unwrap();
    assert_eq!(decision.destination().as_str(), "security-runtime-instance");
    assert_eq!(
        runtime.admit_request(),
        Err(nizaam_core::runtime::RequestAdmissionError::NotServing(
            LifecycleState::Draining
        ))
    );
}

#[test]
fn explicit_destination_does_not_bypass_security_middleware() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("explicit-target-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Deny),
        },
        StaticCredentials(Some(b"token".to_vec())),
    ));

    let mut context = context("security-explicit-target");
    let mut request = request(
        "security-explicit-target-message",
        "security.target",
        b"payload",
    );

    request.event.envelope.metadata.participants = request
        .event
        .envelope
        .metadata
        .participants
        .clone()
        .with_target_instance(EngineInstanceId::new("explicit-target").unwrap());

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |_context, _request| {
            panic!("explicit destination must not bypass security");
        });

    assert!(result.is_err());
}

#[test]
fn preferred_fallback_does_not_bypass_security_boundary() {
    let membership = Membership::new();
    let observations = Observations::new();

    let preferred = EngineInstanceId::new("security-preferred").unwrap();
    let fallback = EngineInstanceId::new("security-fallback").unwrap();
    let engine = EngineId::new("security-fallback-engine").unwrap();
    let capability = CapabilityId::new("security.fallback").unwrap();

    let definition = nizaam_core::capability::CapabilityDefinition::new(
        capability.clone(),
        engine.clone(),
        "security fallback capability",
    )
    .unwrap();

    membership
        .register(
            EngineRegistration::new(engine.clone(), fallback.clone())
                .with_capability(definition)
                .unwrap()
                .with_contract(
                    request(
                        "security-fallback-registration",
                        "security.fallback",
                        b"payload",
                    )
                    .event
                    .envelope
                    .metadata
                    .descriptor,
                )
                .with_endpoint(
                    nizaam_core::control_plane::Endpoint::new("memory://security-fallback")
                        .unwrap(),
                )
                .with_runtime_metadata(
                    nizaam_core::control_plane::RuntimeRegistrationMetadata::new()
                        .with_lifecycle(LifecycleState::Serving)
                        .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
                ),
        )
        .unwrap();

    let health = HealthReport::new(
        engine,
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    observations
        .update(
            EngineObservation::new(
                EngineId::new("security-fallback-engine").unwrap(),
                fallback.clone(),
                health,
            )
            .unwrap(),
        )
        .unwrap();

    let destination = DestinationRequest::preferred_explicit(
        preferred,
        nizaam_core::control_plane::FallbackPolicy::Allowed,
    );
    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability,
        &request(
            "security-fallback-contract",
            "security.fallback",
            b"payload",
        )
        .event
        .envelope
        .metadata
        .descriptor,
    ))
    .unwrap();
    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].instance_id(), &fallback);

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("fallback-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Deny),
        },
        StaticCredentials(Some(b"token".to_vec())),
    ));

    let mut context = context("security-fallback");
    let mut request = request("security-fallback-message", "security.fallback", b"payload");
    request.event.envelope.metadata.participants = request
        .event
        .envelope
        .metadata
        .participants
        .clone()
        .with_target_instance(fallback);

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |_context, _request| {
            panic!("fallback resolution must not bypass authorization");
        });

    assert!(result.is_err());
}

#[test]
fn stream_execution_retains_security_context() {
    let principal = user("stream-user");
    let context = EngineContext::new(operation_context("security-stream"))
        .with_security(SecurityContext::new(principal.clone(), None));

    let stream: Stream<u32> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(
        stream.context().security(),
        Some(&SecurityContext::new(principal, None))
    );
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 7)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7);
    assert!(stream.has_observable_output());
}

#[test]
fn cancellation_and_deadline_do_not_turn_security_denial_into_success() {
    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("signal-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Deny),
        },
        StaticCredentials(Some(b"token".to_vec())),
    );

    let mut context = context("security-signals");
    context.cancellation().cancel();
    let mut request = request("security-signals-message", "security.signal", b"payload");

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);
    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |_context, _request| {
            panic!("cancelled security request must not succeed");
        });

    assert!(result.is_err());
}

#[test]
fn credential_material_does_not_enter_security_context_or_request_payload() {
    let secret = b"super-secret-credential";
    let middleware = SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user("redaction-user")),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(secret.to_vec())),
    );

    let mut context = context("security-redaction");
    let mut request = request(
        "security-redaction-message",
        "security.redaction",
        b"safe-payload",
    );

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );

    assert!(context.security().is_some());
    assert!(
        !request
            .event
            .envelope
            .payload
            .bytes()
            .windows(secret.len())
            .any(|window| window == secret)
    );
}

#[test]
fn event_subscription_authorization_can_use_the_existing_security_identity() {
    let security = SecurityContext::new(user("event-user"), Some(service("event-gateway")));
    let capability = CapabilityId::new("events.read").unwrap();

    let subscription = nizaam_core::events::EventSubscription::new(
        nizaam_core::events::EventName::new("security.event").unwrap(),
        "security.event",
        nizaam_core::events::Scope::new("engine:security").unwrap(),
        |_event: &nizaam_core::events::Event| {},
        &nizaam_core::operation::CancellationToken::new(),
    )
    .unwrap()
    .with_security_context(security.clone())
    .with_authorizer(Arc::new(StaticAuthorizer {
        result: Ok(AuthorizationDecision::Allow),
    }))
    .requiring_capability(capability.clone());

    assert_eq!(subscription.security_context(), Some(&security));
    assert_eq!(subscription.authorization_capability(), Some(&capability));
    assert_eq!(
        subscription.authorization_decision().unwrap(),
        AuthorizationDecision::Allow
    );
}

#[test]
fn authenticated_security_context_survives_into_downstream_execution() {
    let principal = user("downstream-user");
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        StaticAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        StaticCredentials(Some(b"token".to_vec())),
    ));

    let mut context = context("security-downstream");
    let mut request = request(
        "security-downstream-message",
        "security.execute",
        b"payload",
    );

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |context, _request| {
            assert_eq!(context.security().unwrap().principal(), &principal);
            Ok(response(_request))
        });

    assert!(result.is_ok());
}
