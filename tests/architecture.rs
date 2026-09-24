//! Phase 16 architectural-boundary verification.
//!
//! These tests verify cross-module responsibility separation using only the
//! current public Core API. They intentionally avoid domain-specific semantics.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nizaam_core::artifact::{
    ArtifactReference, ContentDigest, ContentReference, IntegrityProof, verify_integrity,
};
use nizaam_core::capability::{
    CapabilityDefinition, CapabilityOutcome, CapabilityRegistry, arc_handler,
};
use nizaam_core::config::{
    resolution::ConfigurationResolver,
    snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
    validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
};
use nizaam_core::control_plane::dependency::CapabilityRequirement;
use nizaam_core::control_plane::policy::RoutingStrategy;
use nizaam_core::control_plane::resolution::{
    ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedRouting,
};
use nizaam_core::control_plane::{ControlPlane, DestinationRequest};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventName, EventPublisher,
    EventSubscription, Scope,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    ArtifactId, AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId,
    EventId, MessageId, OperationId,
};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::provenance::{ProvenanceRecord, ProvenanceRelation};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState};
use nizaam_core::security::{
    AuthorizationDecision, AuthorizationRequest, Authorizer, PrincipalId, PrincipalIdentity,
    PrincipalType, SecurityContext,
};
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamItem, StreamLifecycleState,
};

fn operation_context(label: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("architecture-operation-{label}")).unwrap(),
        CorrelationId::new(format!("architecture-correlation-{label}")).unwrap(),
    ))
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new(
        EngineId::new("architecture-engine").unwrap(),
        EngineInstanceId::new("architecture-instance").unwrap(),
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

fn resolution(
    operation: &OperationId,
    destination: EngineInstanceId,
) -> nizaam_core::control_plane::resolution::Resolution {
    ControlPlane::new().resolve(ResolutionInput::new(
        operation.clone(),
        ResolvedContract::new(ContractId::new("architecture.contract").unwrap(), "1.0"),
        ResolvedCapability::new(CapabilityId::new("architecture.capability").unwrap()),
        ResolvedRouting::new(destination, RoutingStrategy::Deterministic),
    ))
}

fn configuration(id: u64, value: &str) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(std::collections::BTreeMap::new());
    parsed.insert(
        "architecture.mode".to_owned(),
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
fn identity_roles_remain_distinct() {
    let operation = OperationId::new("architecture-operation-id").unwrap();
    let attempt = AttemptId::new("architecture-attempt-id").unwrap();
    let message = MessageId::new("architecture-message-id").unwrap();
    let event = EventId::new("architecture-event-id").unwrap();
    let capability = CapabilityId::new("architecture-capability-id").unwrap();
    let artifact = ArtifactId::new("architecture-artifact-id").unwrap();
    let contract = ContractId::new("architecture-contract-id").unwrap();

    assert!(!operation.as_str().is_empty());
    assert!(!attempt.as_str().is_empty());
    assert!(!message.as_str().is_empty());
    assert!(!event.as_str().is_empty());
    assert!(!capability.as_str().is_empty());
    assert!(!artifact.as_str().is_empty());
    assert!(!contract.as_str().is_empty());

    let engine = EngineId::new("architecture-engine").unwrap();
    let instance = EngineInstanceId::new("architecture-instance").unwrap();
    assert_eq!(engine.as_str(), "architecture-engine");
    assert_eq!(instance.as_str(), "architecture-instance");
}

#[test]
fn retry_carries_attempt_identity_while_routing_carries_destination() {
    let operation = OperationId::new("architecture-retry-routing").unwrap();
    let attempt =
        Attempt::new(operation.clone(), AttemptId::new("attempt-one").unwrap(), 1).unwrap();
    let decision = ControlPlane::new()
        .route_resolved(
            &resolution(&operation, EngineInstanceId::new("instance-one").unwrap()),
            &attempt,
        )
        .unwrap();

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
    assert_eq!(decision.destination().as_str(), "instance-one");
    assert_eq!(attempt.attempt_number(), 1);
}

#[test]
fn routing_does_not_create_retry_state() {
    let operation = OperationId::new("architecture-routing-no-retry").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("attempt-routing").unwrap(),
        1,
    )
    .unwrap();
    let before = (
        attempt.attempt_id().clone(),
        attempt.attempt_number(),
        attempt.state(),
    );

    let _decision = ControlPlane::new()
        .route_resolved(
            &resolution(
                &operation,
                EngineInstanceId::new("instance-routing").unwrap(),
            ),
            &attempt,
        )
        .unwrap();

    assert_eq!(attempt.attempt_id(), &before.0);
    assert_eq!(attempt.attempt_number(), before.1);
    assert_eq!(attempt.state(), before.2);
}

#[test]
fn control_plane_resolution_does_not_execute_capabilities() {
    let registry = CapabilityRegistry::new();
    let invocations = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&invocations);
    let capability = CapabilityId::new("architecture.execute").unwrap();
    let definition = CapabilityDefinition::new(
        capability.clone(),
        EngineId::new("architecture-capability-engine").unwrap(),
        "architecture capability",
    )
    .unwrap();
    registry
        .register(
            definition,
            arc_handler(move |_context, _invocation| {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok(CapabilityOutcome::new(Vec::new()))
            }),
        )
        .unwrap();

    let operation = OperationId::new("architecture-resolution-only").unwrap();
    let _resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation,
        ResolvedContract::new(ContractId::new("architecture.contract").unwrap(), "1.0"),
        ResolvedCapability::new(capability.clone()),
        ResolvedRouting::new(
            EngineInstanceId::new("architecture-capability-instance").unwrap(),
            RoutingStrategy::Deterministic,
        ),
    ));

    assert_eq!(invocations.load(Ordering::SeqCst), 0);
    assert!(registry.contains(&capability));
}

#[test]
fn stale_routing_decision_cannot_bypass_runtime_admission() {
    let runtime = serving_runtime();
    let operation = OperationId::new("architecture-stale-route").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("stale-route-attempt").unwrap(),
        1,
    )
    .unwrap();
    let decision = ControlPlane::new()
        .route_resolved(
            &resolution(
                &operation,
                EngineInstanceId::new("architecture-instance").unwrap(),
            ),
            &attempt,
        )
        .unwrap();
    assert_eq!(decision.destination().as_str(), "architecture-instance");

    runtime.transition(LifecycleState::Draining).unwrap();
    assert!(runtime.admit_request().is_err());
}

#[test]
fn health_observation_does_not_mutate_runtime_lifecycle() {
    let runtime = serving_runtime();
    let health = HealthReport::new(
        EngineId::new("architecture-engine").unwrap(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::ready(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let observation = nizaam_core::control_plane::observations::EngineObservation::new(
        EngineId::new("architecture-engine").unwrap(),
        EngineInstanceId::new("architecture-instance").unwrap(),
        health,
    )
    .unwrap();
    let observations = nizaam_core::control_plane::observations::Observations::new();
    observations.update(observation).unwrap();

    assert_eq!(runtime.state(), LifecycleState::Serving);
    assert_eq!(observations.len(), 1);
}

#[test]
fn events_are_local_delivery_infrastructure() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let subscription = EventSubscription::new(
        EventName::new("architecture.event").unwrap(),
        "architecture.event",
        Scope::new("engine:architecture").unwrap(),
        move |_event: &Event| {
            sender.send(()).unwrap();
        },
        &owner,
    )
    .unwrap();
    let subscription = publisher.subscribe(subscription).unwrap();
    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(2, 1, 2).unwrap(), owner.clone()).unwrap();
    let handle = dispatcher.register(subscription).unwrap();
    let event = Event::new(
        EventId::new("architecture-event-id").unwrap(),
        EventName::new("architecture.event").unwrap(),
        "architecture.event",
        Scope::new("engine:architecture").unwrap(),
    )
    .unwrap();
    let publication = publisher.publish(event).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    dispatcher.shutdown();
}

#[test]
fn streaming_items_remain_distinct_from_transport_frames() {
    let context = EngineContext::new(operation_context("stream-framing"));
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(4, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 1)).unwrap();
    stream.publish(StreamItem::final_item(1, 2)).unwrap();

    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 0);
    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 1);
    assert_eq!(stream.state(), StreamLifecycleState::Completed);

    let frames = nizaam_core::transport::encode_message(9, b"logical payload").unwrap();
    let (_, payload) = nizaam_core::transport::decode_frame(&frames[0]).unwrap();
    assert_eq!(payload, b"logical payload");
}

#[test]
fn artifacts_and_provenance_keep_reference_history_separate() {
    let artifact_id = ArtifactId::new("architecture-artifact").unwrap();
    let source = ArtifactReference::new(artifact_id.clone(), "v1");
    let target = ArtifactReference::new(artifact_id, "v2");
    let content = ContentReference::new("test-provider", "artifact/v1");
    let bytes = b"architecture-content";
    let digest = ContentDigest::new(bytes);
    let proof = IntegrityProof::verify(bytes, &digest).unwrap();

    assert!(content.is_valid());
    assert!(verify_integrity(bytes, &digest).is_ok());
    assert!(proof.matches(&digest, bytes.len() as u64));

    let record = ProvenanceRecord::new(source.clone(), ProvenanceRelation::DerivedFrom, target);
    assert_eq!(record.source(), &source);
    assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
}

#[test]
fn observability_context_does_not_replace_runtime_authority() {
    let runtime = serving_runtime();
    let context = EngineContext::new(operation_context("observability"));
    assert!(!context.operation().operation.id.as_str().is_empty());
    assert_eq!(runtime.state(), LifecycleState::Serving);
}

#[derive(Debug)]
struct AllowAuthorizer;

impl Authorizer for AllowAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, nizaam_core::security::AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}

#[test]
fn security_context_is_distinct_from_credentials() {
    let principal = PrincipalIdentity::new(
        PrincipalType::User,
        PrincipalId::new("architecture-user").unwrap(),
    );
    let context = SecurityContext::new(principal.clone(), None);
    let capability = CapabilityId::new("architecture.secure").unwrap();
    let request =
        AuthorizationRequest::new(context.principal(), context.calling_service(), &capability);

    assert_eq!(
        AllowAuthorizer.authorize(&request).unwrap(),
        AuthorizationDecision::Allow
    );
    assert_eq!(context.principal(), &principal);
}

#[test]
fn configuration_snapshots_do_not_mutate_existing_runtime_contexts() {
    let first = configuration(100, "first");
    let context = EngineContext::new(operation_context("configuration"))
        .with_configuration(Arc::clone(&first));
    let second = configuration(101, "second");
    let later = EngineContext::new(operation_context("configuration-later"))
        .with_configuration(Arc::clone(&second));

    assert_eq!(context.configuration(), Some(first.as_ref()));
    assert_eq!(
        context.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(100)
    );
    assert_eq!(later.configuration(), Some(second.as_ref()));
    assert_eq!(
        later.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(101)
    );
}

#[test]
fn domain_leakage_guard_checks_library_module_boundary() {
    let lib = include_str!("../src/lib.rs");
    assert!(!lib.contains("pub mod quran"));
    assert!(!lib.contains("pub mod hadith"));
    assert!(!lib.contains("pub mod arabic"));
    assert!(!lib.contains("use crate::quran"));
    assert!(!lib.contains("use crate::hadith"));
    assert!(!lib.contains("use crate::arabic"));
}

#[test]
fn explicit_destination_api_keeps_fallback_as_control_plane_policy() {
    let request = DestinationRequest::preferred_explicit(
        EngineInstanceId::new("architecture-preferred-instance").unwrap(),
        nizaam_core::control_plane::FallbackPolicy::Allowed,
    );
    assert!(request.allows_fallback());
    assert_eq!(
        request.explicit_instance_id().unwrap().as_str(),
        "architecture-preferred-instance"
    );
    let logical = DestinationRequest::hard_logical(CapabilityRequirement::new(
        CapabilityId::new("architecture.capability").unwrap(),
    ));
    assert!(logical.is_hard());
}
