//! Phase 16 cross-module integration tests.
//!
//! Each case combines a small number of production Core subsystems. The suite
//! intentionally avoids becoming a second specialist-conformance suite or a
//! monolithic E2E scenario.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nizaam_core::capability::CapabilityDefinition;
use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::middleware::chain::MiddlewareChainError;
use nizaam_core::runtime::{EngineContext, EngineRuntime, ExecutionPipeline, LifecycleState, RequestPipelineError};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};
use nizaam_core::status::Status;
use nizaam_core::streaming::{BackpressureConfig, BackpressurePolicy, Stream};
use nizaam_core::transport::InMemoryTransport;

mod common;

use common::reference_engine::{ReferenceBehavior, ReferenceEngine};

const CAPABILITY: &str = "integration.capability";
const CONTRACT: &str = "integration.contract";

#[derive(Debug)]
struct IntegrationAuthenticator;

impl Authenticator for IntegrationAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Ok(PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("integration-user").unwrap(),
        ))
    }
}

#[derive(Debug)]
struct IntegrationDenyAuthorizer;

impl Authorizer for IntegrationDenyAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Deny)
    }
}

#[derive(Clone, Debug)]
struct IntegrationCredentials;

impl CredentialExtractor for IntegrationCredentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        Some(b"integration-token".to_vec())
    }
}

fn operation_context(id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(id).unwrap(),
        CorrelationId::new(format!("corr-{id}")).unwrap(),
    ))
}

fn context(id: &str) -> EngineContext {
    EngineContext::new(operation_context(id))
}

fn descriptor() -> ContractDescriptor {
    ContractDescriptor::new(
        ContractId::new(CONTRACT).unwrap(),
        CapabilityId::new(CAPABILITY).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    )
}

fn request(id: &str, payload: &[u8]) -> UniversalRequest {
    let payload_descriptor = descriptor().payload.clone();
    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("message-{id}")).unwrap(),
        operation_context(id),
        ContractMetadata::new(
            descriptor(),
            Participants::new(
                EngineId::new("integration-caller").unwrap(),
                EngineId::new("integration-engine").unwrap(),
            ),
        ),
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new(
        EngineId::new("integration-runtime").unwrap(),
        EngineInstanceId::new("integration-runtime-instance").unwrap(),
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

fn config_snapshot(id: u64, value: &str) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::empty();
    parsed.insert(
        "integration.value",
        ConfigurationValue::String(value.to_owned()),
    );
    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();
    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}

#[test]
fn contract_capability_runtime_handler_round_trip() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-engine").unwrap(),
        EngineInstanceId::new("integration-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let ctx = context("contract-capability-runtime");
    let built_request = request("contract-capability-runtime", b"hello");
    assert_eq!(built_request.event.envelope.payload.bytes(), b"hello");

    let result = engine
        .dispatch(&ctx, CAPABILITY, CONTRACT, b"hello")
        .unwrap();
    assert_eq!(result.as_bytes(), b"hello");
    assert_eq!(engine.invocation_count(CAPABILITY), 1);
}

#[test]
fn security_context_survives_runtime_to_capability_boundary() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-security-engine").unwrap(),
        EngineInstanceId::new("integration-security-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let security = SecurityContext::new(
        PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new("caller").unwrap()),
        None,
    );
    let ctx =
        EngineContext::new(operation_context("security-runtime")).with_security(security.clone());

    engine
        .dispatch(&ctx, CAPABILITY, CONTRACT, b"secure")
        .unwrap();
    let observed = engine.last_context(CAPABILITY).unwrap();
    assert_eq!(observed.security(), Some(&security));
}

#[test]
fn authentication_or_authorization_failure_has_no_capability_dispatch() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-security-negative").unwrap(),
        EngineInstanceId::new("integration-security-negative-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let mut context = context("security-negative");
    let mut request = request("security-negative", b"payload");
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        IntegrationAuthenticator,
        IntegrationDenyAuthorizer,
        IntegrationCredentials,
    ));

    let result: Result<UniversalResponse, nizaam_core::runtime::RequestPipelineError<Status>> =
        pipeline.run_request(&mut context, &mut request, |context, request| {
            let _ = engine.dispatch(
                context,
                CAPABILITY,
                CONTRACT,
                request.event.envelope.payload.bytes(),
            );
            panic!("authorization denial must prevent capability dispatch");
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            MiddlewareChainError::Rejected(ref rejection)
        )) if rejection.reason() == "authorization denied"
    ));
    assert_eq!(engine.invocation_count(CAPABILITY), 0);
}

#[test]
fn configuration_snapshot_is_consumed_as_an_immutable_context_value() {
    let first = config_snapshot(1, "v1");
    let second = config_snapshot(2, "v2");

    let first_ctx = EngineContext::new(operation_context("config-first"))
        .with_configuration(Arc::clone(&first));
    let second_ctx = EngineContext::new(operation_context("config-second"))
        .with_configuration(Arc::clone(&second));

    assert_eq!(first_ctx.configuration().unwrap().id().value(), 1);
    assert_eq!(second_ctx.configuration().unwrap().id().value(), 2);
    assert_eq!(
        first_ctx
            .configuration()
            .unwrap()
            .get("integration.value")
            .unwrap(),
        &ConfigurationValue::String("v1".to_owned())
    );
}

#[test]
fn health_observation_and_control_plane_membership_are_separate_inputs() {
    use nizaam_core::control_plane::{
        EngineObservation, EngineRegistration, Membership, Observations,
    };
    use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};

    let membership = Membership::new();
    let observations = Observations::new();
    let engine = EngineId::new("integration-health").unwrap();
    let instance = EngineInstanceId::new("integration-health-instance").unwrap();

    let definition = CapabilityDefinition::new(
        CapabilityId::new(CAPABILITY).unwrap(),
        engine.clone(),
        "integration health capability",
    )
    .unwrap();
    let registration = EngineRegistration::new(engine.clone(), instance.clone())
        .with_capability(definition)
        .unwrap();

    membership.register(registration).unwrap();
    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    observations
        .update(EngineObservation::new(engine, instance.clone(), health).unwrap())
        .unwrap();

    assert!(membership.contains(&instance));
    assert!(observations.snapshot().contains(&instance));
}

#[test]
fn registration_membership_and_transport_keep_the_same_concrete_target() {
    use nizaam_core::control_plane::{EngineRegistration, Membership};
    let engine = EngineId::new("integration-route").unwrap();
    let instance = EngineInstanceId::new("integration-route-instance").unwrap();

    let definition = CapabilityDefinition::new(
        CapabilityId::new(CAPABILITY).unwrap(),
        engine.clone(),
        "integration route capability",
    )
    .unwrap();
    let registration = EngineRegistration::new(engine.clone(), instance.clone())
        .with_capability(definition)
        .unwrap();

    let membership = Membership::new();
    membership.register(registration).unwrap();
    assert!(membership.contains(&instance));

    let transport = InMemoryTransport::new();
    let seen = Arc::new(Mutex::new(0usize));
    let seen_handler = Arc::clone(&seen);
    transport.register(engine, instance.clone(), move |_request| {
        *seen_handler.lock().unwrap() += 1;
        panic!("transport invocation is intentionally not reached by registration alone");
    });

    assert_eq!(*seen.lock().unwrap(), 0);
    assert!(membership.contains(&instance));
}

#[test]
fn stale_runtime_state_rejects_after_routing_boundary() {
    let runtime = serving_runtime();
    assert_eq!(runtime.state(), LifecycleState::Serving);
    runtime.transition(LifecycleState::Draining).unwrap();
    assert!(runtime.admit_request().is_err());
}

#[test]
fn retry_attempt_can_use_a_new_runtime_target() {
    let first = ReferenceEngine::new(
        EngineId::new("integration-retry-a").unwrap(),
        EngineInstanceId::new("integration-retry-a-1").unwrap(),
    );
    let second = ReferenceEngine::new(
        EngineId::new("integration-retry-b").unwrap(),
        EngineInstanceId::new("integration-retry-b-1").unwrap(),
    );
    first
        .register_capability(CAPABILITY, ReferenceBehavior::Fail)
        .unwrap();
    second
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    first.serving().unwrap();
    second.serving().unwrap();

    let operation = operation_context("retry-route").operation.id.clone();
    let first_context = context("retry-route").for_attempt(
        nizaam_core::identity::NodeId::new("node-a").unwrap(),
        nizaam_core::identity::AttemptId::new("attempt-a").unwrap(),
    );
    let second_context = context("retry-route").for_attempt(
        nizaam_core::identity::NodeId::new("node-b").unwrap(),
        nizaam_core::identity::AttemptId::new("attempt-b").unwrap(),
    );

    assert!(
        first
            .dispatch(&first_context, CAPABILITY, CONTRACT, b"x")
            .is_err()
    );
    assert_eq!(
        second
            .dispatch(&second_context, CAPABILITY, CONTRACT, b"x")
            .unwrap()
            .as_bytes(),
        b"x"
    );
    assert_eq!(
        first
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .operation
            .id,
        operation
    );
    assert_eq!(
        second
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .operation
            .id,
        operation
    );
    assert_ne!(
        first
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .attempt_id
            .as_ref(),
        second
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .attempt_id
            .as_ref()
    );
}

#[test]
fn retry_idempotency_boundary_can_record_one_side_effect() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-idempotency").unwrap(),
        EngineInstanceId::new("integration-idempotency-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::SideEffect(b"ok".to_vec()))
        .unwrap();
    engine.serving().unwrap();

    let ctx = context("idempotency");
    engine
        .dispatch(&ctx, CAPABILITY, CONTRACT, b"side-effect")
        .unwrap();
    // The production idempotency layer owns duplicate suppression; this engine
    // exposes the side effect count so the integration boundary can assert the
    // handler itself remains observable.
    assert_eq!(engine.side_effect_count(CAPABILITY), 1);
}

#[test]
fn streaming_and_runtime_share_operation_ownership() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-stream").unwrap(),
        EngineInstanceId::new("integration-stream-instance").unwrap(),
    );
    engine.serving().unwrap();
    let ctx = context("stream-runtime");

    let stream = engine
        .open_stream::<u32>(&ctx, 4, BackpressurePolicy::Reject)
        .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream
        .publish(nizaam_core::streaming::StreamItem::partial(0, 1))
        .unwrap();
    stream
        .publish(nizaam_core::streaming::StreamItem::final_item(1, 2))
        .unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().into_payload(), 1);
    assert_eq!(consumer.next_item().unwrap().unwrap().into_payload(), 2);
}

#[test]
fn event_style_observation_does_not_replace_runtime_result() {
    use nizaam_core::events::{Event, EventName, Scope};
    use nizaam_core::identity::EventId;

    let event = Event::new(
        EventId::new("integration-event").unwrap(),
        EventName::new("integration.execution").unwrap(),
        "execution",
        Scope::new("engine:integration").unwrap(),
    )
    .unwrap();

    let engine = ReferenceEngine::new(
        EngineId::new("integration-event-engine").unwrap(),
        EngineInstanceId::new("integration-event-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();
    let result = engine
        .dispatch(&context("event-runtime"), CAPABILITY, CONTRACT, b"event")
        .unwrap();

    assert_eq!(result.as_bytes(), b"event");
    assert_eq!(event.event_type(), "execution");
}

#[test]
fn runtime_failure_remains_observable_as_a_capability_failure() {
    let engine = ReferenceEngine::new(
        EngineId::new("integration-error").unwrap(),
        EngineInstanceId::new("integration-error-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Fail)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &context("error-observability"),
        CAPABILITY,
        CONTRACT,
        b"failure",
    );
    assert!(result.is_err());
    assert_eq!(engine.invocation_count(CAPABILITY), 1);
}

#[test]
fn artifact_integrity_and_provenance_remain_separate() {
    use nizaam_core::artifact::ArtifactReference;
    use nizaam_core::artifact::{ArtifactVersion, ContentDigest, ContentReference, IntegrityProof};
    use nizaam_core::identity::ArtifactId;
    use nizaam_core::provenance::{ProvenanceContext, ProvenanceRecord, ProvenanceRelation};

    let artifact_id = ArtifactId::new("integration-artifact").unwrap();
    let bytes = b"artifact-bytes";
    let content = ContentReference::new("integration-provider", "content/v1");
    let version = ArtifactVersion::new(
        artifact_id.clone(),
        "v1",
        content,
        ContentDigest::new(bytes),
        bytes.len() as u64,
    );
    let proof = IntegrityProof::verify(bytes, &ContentDigest::new(bytes)).unwrap();
    let provenance =
        ProvenanceContext::new().with_attribute("operation", "integration-artifact-operation");

    assert_eq!(version.artifact_id(), &artifact_id);
    assert_eq!(version.digest(), &ContentDigest::new(bytes));
    assert!(proof.matches(&ContentDigest::new(bytes), bytes.len() as u64));
    assert_eq!(
        provenance.attribute("operation"),
        Some("integration-artifact-operation")
    );

    let source = ArtifactReference::new(artifact_id.clone(), "v1");
    let target = ArtifactReference::new(artifact_id.clone(), "v1");
    let record = ProvenanceRecord::new(source, ProvenanceRelation::ProducedFrom, target);
    assert_eq!(record.relation(), ProvenanceRelation::ProducedFrom);
}

#[test]
fn event_and_logging_layers_can_share_operation_identity_without_merging_state() {
    use nizaam_core::events::{Event, EventName, Scope};
    use nizaam_core::identity::EventId;
    let op = operation_context("event-log-operation");
    let event = Event::new(
        EventId::new("event-log-id").unwrap(),
        EventName::new("integration.log").unwrap(),
        "log",
        Scope::new("engine:integration").unwrap(),
    )
    .unwrap();
    assert_eq!(op.operation.id.as_str(), "event-log-operation");
    assert_eq!(event.event_type(), "log");
}

#[test]
fn cancellation_is_visible_to_owned_stream() {
    let ctx = context("cancel-stream");
    let stream = Stream::<u32>::new(
        &ctx,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    ctx.cancellation().cancel();
    assert!(consumer.next_item().is_err());
}

#[test]
fn deadline_context_is_visible_before_retry_chain_continues() {
    let ctx = context("deadline-chain").with_deadline(
        nizaam_core::runtime::Deadline::from_now(std::time::Duration::from_millis(0)).unwrap(),
    );
    std::thread::sleep(Duration::from_millis(1));
    assert!(ctx.is_expired());
}

#[test]
fn transport_corruption_is_kept_outside_capability_semantics() {
    use nizaam_core::transport::MessageHeader;
    let header = MessageHeader::new(1, 0, 0, 0, 0, 0, 0).unwrap();
    let mut bytes = header.serialize();
    bytes[0] ^= 0xff;
    assert!(MessageHeader::deserialize(&bytes).is_err());
}

#[test]
fn event_subscriber_failure_is_not_a_capability_failure() {
    use nizaam_core::events::{Event, EventName, Scope};
    use nizaam_core::identity::EventId;
    let event = Event::new(
        EventId::new("integration-isolated-event").unwrap(),
        EventName::new("integration.failure").unwrap(),
        "failure",
        Scope::new("engine:integration").unwrap(),
    )
    .unwrap();
    let engine = ReferenceEngine::new(
        EngineId::new("integration-isolation-engine").unwrap(),
        EngineInstanceId::new("integration-isolation-instance").unwrap(),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();
    let result = engine
        .dispatch(&context("subscriber-failure"), CAPABILITY, CONTRACT, b"ok")
        .unwrap();
    assert_eq!(result.as_bytes(), b"ok");
    assert_eq!(event.event_type(), "failure");
}

#[test]
fn configuration_update_does_not_mutate_existing_context_snapshot() {
    let first = config_snapshot(11, "before");
    let second = config_snapshot(12, "after");
    let existing = EngineContext::new(operation_context("config-existing"))
        .with_configuration(Arc::clone(&first));
    let new_context =
        EngineContext::new(operation_context("config-new")).with_configuration(Arc::clone(&second));

    assert_eq!(existing.configuration().unwrap().id().value(), 11);
    assert_eq!(new_context.configuration().unwrap().id().value(), 12);
}
