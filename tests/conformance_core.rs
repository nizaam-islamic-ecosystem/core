//! Phase 16 foundational Core conformance tests.
//!
//! These tests exercise cross-module contracts through the public Core API.
//! They intentionally avoid asserting concrete values for generated identities.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse,
};
use nizaam_core::health::{HealthReport, HealthStatus, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, EventId,
    MessageId, NodeId, OperationId,
};
use nizaam_core::operation::{CancellationToken, Deadline, Operation, OperationContext};
use nizaam_core::prelude::{Status, Version};
use nizaam_core::provenance::ProvenanceContext;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState};
use nizaam_core::security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext};
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamError, StreamItem,
};

fn operation_context(name: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("phase16-core-operation-{name}")).unwrap(),
        CorrelationId::new(format!("phase16-core-correlation-{name}")).unwrap(),
    ))
}

fn request(name: &str, payload: &[u8]) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("phase16.core.contract").unwrap(),
        CapabilityId::new("phase16.core.echo").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("phase16-core-sender").unwrap(),
            EngineId::new("phase16-core-receiver").unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("phase16-core-message-{name}")).unwrap(),
        operation_context(name),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
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

fn serving_runtime(name: &str) -> EngineRuntime {
    let runtime = EngineRuntime::new(
        EngineId::new(format!("phase16-core-engine-{name}")).unwrap(),
        EngineInstanceId::new(format!("phase16-core-instance-{name}")).unwrap(),
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

fn configuration_snapshot(value: &str, id: u64) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(BTreeMap::new());
    parsed.insert(
        "phase16.core.configuration",
        ConfigurationValue::String(value.to_owned()),
    );

    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();

    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}

fn security_context() -> SecurityContext {
    let principal = PrincipalIdentity::new(
        PrincipalType::User,
        PrincipalId::new("phase16-core-user").unwrap(),
    );
    let calling_service = PrincipalIdentity::new(
        PrincipalType::Service,
        PrincipalId::new("phase16-core-service").unwrap(),
    );

    SecurityContext::new(principal, Some(calling_service))
}

#[test]
fn universal_event_exposes_constructor_owned_event_identity() {
    let request = request("event-identity", b"payload");
    let event_id: &EventId = request.event_id();

    assert!(!event_id.as_str().is_empty());
}

#[test]
fn universal_request_and_response_each_obtain_their_event_identity_from_construction() {
    let request = request("request-response-identity", b"request");
    let response = response(&request, b"response");

    assert!(!request.event_id().as_str().is_empty());
    assert!(!response.event_id().as_str().is_empty());
}

#[test]
fn message_and_event_id_types_remain_separate_at_request_boundary() {
    let request = request("identity-types", b"payload");

    let _: &MessageId = request.message_id();
    let _: &EventId = request.event_id();

    assert!(!request.message_id().as_str().is_empty());
    assert!(!request.event_id().as_str().is_empty());
}

#[test]
fn operation_correlation_and_attempt_identity_slots_remain_separate() {
    let context = operation_context("attempt-boundary");
    let attempt = context.clone().for_attempt(
        NodeId::new("phase16-core-node").unwrap(),
        AttemptId::new("phase16-core-attempt").unwrap(),
    );

    assert!(!context.operation.id.as_str().is_empty());
    assert!(!context.operation.correlation_id.as_str().is_empty());
    assert!(attempt.node_id.is_some());
    assert!(attempt.attempt_id.is_some());
}

#[test]
fn explicit_engine_and_instance_identities_remain_separate() {
    let engine_id = EngineId::new("phase16-core-engine").unwrap();
    let instance_id = EngineInstanceId::new("phase16-core-engine-instance").unwrap();

    assert_eq!(engine_id.as_str(), "phase16-core-engine");
    assert_eq!(instance_id.as_str(), "phase16-core-engine-instance");
}

#[test]
fn request_constructor_preserves_operation_and_correlation_context() {
    let request = request("context-preservation", b"payload");
    let context = &request.event.envelope.operation_context;

    assert!(!context.operation.id.as_str().is_empty());
    assert!(!context.operation.correlation_id.as_str().is_empty());
    assert!(context.node_id.is_none());
    assert!(context.attempt_id.is_none());
}

#[test]
fn request_constructor_preserves_payload_descriptor_and_opaque_payload() {
    let request = request("descriptor-preservation", b"opaque-bytes");
    let descriptor = &request.event.envelope.metadata.descriptor;

    assert_eq!(descriptor.interaction, Interaction::Request);
    assert_eq!(descriptor.payload.media_type(), "application/octet-stream");
    assert_eq!(*descriptor.payload.schema_version(), Version::new(1, 0, 0));
    assert_eq!(request.event.envelope.payload.bytes(), b"opaque-bytes");
}

#[test]
fn response_construction_changes_response_semantics_without_replacing_payload_metadata() {
    let request = request("response-metadata", b"request-body");
    let response = response(&request, b"response-body");

    assert_eq!(response.status, Status::Success);
    assert_eq!(
        response.event.envelope.metadata.descriptor.interaction,
        Interaction::Response
    );
    assert_eq!(
        response
            .event
            .envelope
            .metadata
            .descriptor
            .payload
            .media_type(),
        "application/octet-stream"
    );
    assert_eq!(
        *response
            .event
            .envelope
            .metadata
            .descriptor
            .payload
            .schema_version(),
        Version::new(1, 0, 0)
    );
    assert_eq!(response.event.envelope.payload.bytes(), b"response-body");
}

#[test]
fn child_context_preserves_operation_and_security_provenance_and_configuration() {
    let configuration = configuration_snapshot("v1", 1);
    let parent = EngineContext::new(operation_context("child-context"))
        .with_security(security_context())
        .with_provenance(ProvenanceContext::new().with_attribute("source", "phase16"))
        .with_configuration(Arc::clone(&configuration));

    let child = parent.child();

    assert!(!child.operation().operation.id.as_str().is_empty());
    assert!(
        !child
            .operation()
            .operation
            .correlation_id
            .as_str()
            .is_empty()
    );
    assert!(child.operation().node_id.is_none());
    assert!(child.operation().attempt_id.is_none());
    assert_eq!(child.security(), parent.security());
    assert_eq!(child.provenance(), parent.provenance());
    assert_eq!(child.configuration(), parent.configuration());
    assert!(!child.cancellation().is_cancelled());
}

#[test]
fn attempt_context_preserves_operation_and_carries_attempt_scope() {
    let engine = EngineContext::new(operation_context("attempt-context"));
    let attempt = engine.for_attempt(
        NodeId::new("phase16-core-attempt-node").unwrap(),
        AttemptId::new("phase16-core-attempt-id").unwrap(),
    );

    assert!(!attempt.operation().operation.id.as_str().is_empty());
    assert!(attempt.operation().node_id.is_some());
    assert!(attempt.operation().attempt_id.is_some());
}

#[test]
fn configuration_snapshot_remains_stable_across_clone_and_child_context() {
    let snapshot = configuration_snapshot("immutable-v1", 7);
    let parent = EngineContext::new(operation_context("configuration-stability"))
        .with_configuration(Arc::clone(&snapshot));
    let child = parent.child();
    let clone = parent.clone();

    assert!(parent.configuration().is_some());
    assert!(child.configuration().is_some());
    assert!(clone.configuration().is_some());
    assert_eq!(
        snapshot
            .get("phase16.core.configuration")
            .and_then(ConfigurationValue::as_string),
        Some("immutable-v1")
    );
}

#[test]
fn valid_runtime_lifecycle_reaches_serving() {
    let runtime = serving_runtime("valid-lifecycle");

    assert_eq!(runtime.state(), LifecycleState::Serving);
    assert!(runtime.admit_request().is_ok());
}

#[test]
fn invalid_runtime_transition_is_rejected_without_mutating_state() {
    let runtime = EngineRuntime::new(
        EngineId::new("phase16-core-invalid-engine").unwrap(),
        EngineInstanceId::new("phase16-core-invalid-instance").unwrap(),
    );

    assert!(runtime.transition(LifecycleState::Ready).is_err());
    assert_eq!(runtime.state(), LifecycleState::Created);
}

#[test]
fn stopped_runtime_is_terminal_and_does_not_accept_new_requests() {
    let runtime = serving_runtime("stopped-terminal");

    runtime.transition(LifecycleState::Draining).unwrap();
    assert!(runtime.admit_request().is_err());
    assert!(runtime.shutdown().is_ok());
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.admit_request().is_err());
    assert!(runtime.transition(LifecycleState::Serving).is_err());
}

#[test]
fn health_observation_does_not_mutate_runtime_lifecycle() {
    let runtime = serving_runtime("health-boundary");
    let before = runtime.state();

    let report = HealthReport::new(
        runtime.engine_id().clone(),
        runtime.state(),
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(report.overall(), HealthStatus::Healthy);
    assert_eq!(runtime.state(), before);
}

#[test]
fn parent_cancellation_reaches_owned_child_context() {
    let parent = EngineContext::new(operation_context("parent-cancel"));
    let child = parent.child();

    assert!(!parent.cancellation().is_cancelled());
    assert!(!child.cancellation().is_cancelled());

    parent.cancellation().cancel();

    assert!(parent.cancellation().is_cancelled());
    assert!(child.cancellation().is_cancelled());
}

#[test]
fn child_cancellation_does_not_cancel_parent_or_sibling() {
    let parent = EngineContext::new(operation_context("child-cancel"));
    let first = parent.child();
    let second = parent.child();

    first.cancellation().cancel();

    assert!(!parent.cancellation().is_cancelled());
    assert!(first.cancellation().is_cancelled());
    assert!(!second.cancellation().is_cancelled());
}

#[test]
fn child_deadline_cannot_extend_parent_deadline() {
    let parent_deadline = Deadline::from_now(Duration::from_secs(5)).unwrap();
    let later_deadline = Deadline::from_now(Duration::from_secs(30)).unwrap();

    let parent =
        EngineContext::new(operation_context("deadline-boundary")).with_deadline(parent_deadline);
    let child = parent.child_with_deadline(later_deadline);

    assert_eq!(child.deadline(), Some(parent_deadline));
}

#[test]
fn terminal_stream_rejects_new_publication() {
    let context = EngineContext::new(operation_context("terminal-stream"));
    let stream: Stream<u32> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    stream.publish(StreamItem::final_item(0, 1)).unwrap();

    let result = stream.publish(StreamItem::partial(1, 2));
    assert!(matches!(
        result,
        Err(StreamError::NotOpen | StreamError::Lifecycle(_))
    ));
}

#[test]
fn cancellation_token_child_scope_remains_directional() {
    let parent = CancellationToken::new();
    let child = parent.child_token();
    let sibling = parent.child_token();

    child.cancel();

    assert!(!parent.is_cancelled());
    assert!(child.is_cancelled());
    assert!(!sibling.is_cancelled());
}
