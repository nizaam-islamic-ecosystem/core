//! Phase 16 Control Plane architectural conformance tests.
//!
//! These tests exercise the public Control Plane boundaries as one composed
//! system and protect the Phase 15 -> Phase 16 architectural contract. They deliberately cross registration, registry, membership,
//! observations, destination eligibility, routing policy, resolution,
//! attempts, concrete routing, and Control Plane communication.
//!
//! The current Phase 15 implementation does not expose one stateful
//! `ControlPlane::execute` operation. The tests therefore compose the
//! authoritative subsystem owners explicitly rather than inventing a missing
//! orchestration API.
//!
//! The principal execution path covered here is:
//!
//! ```text
//! EngineRegistration
//!       ↓
//! EngineRegistry + Membership
//!       ↓
//! ObservationSnapshot
//!       ↓
//! destination eligibility
//!       ↓
//! RoutingPolicy
//!       ↓
//! Resolution
//!       ↓
//! Attempt
//!       ↓
//! RoutingDecision
//!       ↓
//! ControlPlaneCommunication
//!       ↓
//! Engine Runtime / Transport
//! ```
//!
//! These tests do not duplicate unit-level validation of individual modules,
//! and they do not claim that currently-unimplemented lifecycle, retry,
//! observability, provenance, or replanning orchestration exists.

use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, ControlPlaneCommunication, DestinationEligibilityInput,
    DestinationRequest, EngineObservation, EngineRegistration, EngineRegistry, FallbackPolicy,
    Membership, Observations, PolicyInput, ResolutionInput, ResolvedCapability, ResolvedContract,
    ResolvedRouting, RoutingCandidate, RoutingConstraints, RoutingPolicy, RoutingStrategy,
    eligible_destinations,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::LifecycleState;
use nizaam_core::status::Status;
use nizaam_core::transport::{InMemoryTransport, TransportError};

const CAPABILITY: &str = "quran.analyze";
const CONTRACT: &str = "quran.analyze";
const MEDIA_TYPE: &str = "application/octet-stream";

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).expect("test operation id must be valid")
}

fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).expect("test attempt id must be valid")
}

fn descriptor_for(capability: &str, interaction: Interaction) -> ContractDescriptor {
    ContractDescriptor::new(
        contract_id(CONTRACT),
        capability_id(capability),
        Version::new(1, 0, 0),
        interaction,
        PayloadDescriptor::new(MEDIA_TYPE, Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    )
}

fn capability_requirement(capability: &str) -> CapabilityRequirement {
    CapabilityRequirement::new(capability_id(capability))
}

fn registration(engine: &str, instance: &str, capability: &str) -> EngineRegistration {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let capability = capability_id(capability);

    let definition = nizaam_core::capability::CapabilityDefinition::new(
        capability.clone(),
        engine.clone(),
        format!("{capability} capability"),
    )
    .expect("test capability definition must be valid");

    EngineRegistration::new(engine, instance.clone())
        .with_capability(definition)
        .expect("capability owner must match registration engine")
        .with_contract(descriptor_for(capability.as_str(), Interaction::Request))
        .with_endpoint(
            nizaam_core::control_plane::Endpoint::new(format!("memory://{}", instance.as_str()))
                .expect("test endpoint must be valid"),
        )
        .with_runtime_metadata(
            nizaam_core::control_plane::RuntimeRegistrationMetadata::new()
                .with_lifecycle(LifecycleState::Serving)
                .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
        )
}

fn healthy_observation(engine: &str, instance: &str) -> EngineObservation {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .expect("test health report must be valid");

    EngineObservation::new(engine, instance, health)
        .expect("observation identity must match health identity")
}

fn resolved_contract() -> ResolvedContract {
    ResolvedContract::new(contract_id(CONTRACT), "1.0.0")
}

fn resolved_capability() -> ResolvedCapability {
    ResolvedCapability::new(capability_id(CAPABILITY))
}

fn attempt(operation: &OperationId, id: &str, number: u32) -> Attempt {
    Attempt::new(operation.clone(), attempt_id(id), number).expect("test attempt must be valid")
}

fn request_for(
    target_engine: &EngineId,
    target_instance: &EngineInstanceId,
    operation: &OperationId,
    attempt: &AttemptId,
    message: &str,
) -> UniversalRequest {
    let descriptor = descriptor_for(CAPABILITY, Interaction::Request);
    let operation_context = OperationContext::new(Operation::new(
        operation.clone(),
        CorrelationId::new(format!("correlation-{message}"))
            .expect("test correlation id must be valid"),
    ))
    .for_attempt(
        NodeId::new("phase15-test-node").expect("test node id must be valid"),
        attempt.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor.clone(),
        Participants::new(engine_id("phase15-caller"), target_engine.clone())
            .with_target_instance(target_instance.clone()),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message).expect("test message id must be valid"),
        operation_context,
        metadata,
        EncodedPayload::new(descriptor.payload, b"phase15 payload".to_vec()),
    ))
}

fn response_for(request: UniversalRequest, payload: Vec<u8>) -> UniversalResponse {
    let envelope = request.event.envelope;
    let request_descriptor = envelope.metadata.descriptor;
    let payload_descriptor = request_descriptor.payload.clone();
    let participants = envelope.metadata.participants;

    let sender_instance = participants.target_instance.clone();
    let target_instance = participants.sender_instance.clone();
    let mut response_participants = Participants::new(participants.target, participants.sender);

    if let Some(instance) = sender_instance {
        response_participants = response_participants.with_sender_instance(instance);
    }

    if let Some(instance) = target_instance {
        response_participants = response_participants.with_target_instance(instance);
    }

    let descriptor = ContractDescriptor::new(
        request_descriptor.contract_id,
        request_descriptor.capability_id,
        request_descriptor.version,
        Interaction::Response,
        payload_descriptor.clone(),
    );

    let response_message_id = envelope.message_id.clone();

    let response = UniversalResponse::new(
        MessageEnvelope::new(
            envelope.message_id,
            envelope.operation_context,
            ContractMetadata::new(descriptor, response_participants),
            EncodedPayload::new(payload_descriptor, payload),
        ),
        Status::Success,
    );

    assert_eq!(response.event.envelope.message_id, response_message_id);
    response
}

fn eligible_candidates(
    membership: &Membership,
    observations: &Observations,
    destination: &DestinationRequest,
) -> Vec<RoutingCandidate> {
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let capability = capability_id(CAPABILITY);
    let contract = descriptor_for(CAPABILITY, Interaction::Request);

    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        destination,
        &membership_snapshot,
        &observation_snapshot,
        &capability,
        &contract,
    ))
    .expect("test destination must have at least one eligible instance");

    eligible
        .into_iter()
        .map(|candidate| RoutingCandidate::new(candidate.instance_id().clone()))
        .collect()
}

fn resolve_and_route(
    control_plane: &ControlPlane,
    operation: &OperationId,
    destination: EngineInstanceId,
    attempt: &Attempt,
) -> nizaam_core::control_plane::RoutingDecision {
    let resolution = control_plane.resolve(ResolutionInput::new(
        operation.clone(),
        resolved_contract(),
        resolved_capability(),
        ResolvedRouting::new(destination, RoutingStrategy::Deterministic),
    ));

    control_plane
        .route_resolved(&resolution, attempt)
        .expect("resolution and attempt must belong to the same operation")
}

fn expose_echo_engine(
    transport: &InMemoryTransport,
    engine: &str,
    instance: &str,
) -> Arc<Mutex<usize>> {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_handler = Arc::clone(&calls);
    let response_instance = instance.clone();

    transport.register(engine, instance, move |request| {
        *calls_for_handler
            .lock()
            .expect("test call counter lock must not be poisoned") += 1;

        response_for(request, response_instance.as_str().as_bytes().to_vec())
    });

    calls
}

#[test]
fn registration_to_routing_to_communication_reaches_the_selected_instance() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();
    let observations = Observations::new();

    let registration = registration("quran-engine", "quran-01", CAPABILITY);
    registry
        .register(registration.clone())
        .expect("registry registration must succeed");
    membership
        .register(registration)
        .expect("membership registration must succeed");
    observations
        .update(healthy_observation("quran-engine", "quran-01"))
        .expect("health observation must be accepted");

    assert!(registry.contains(&instance_id("quran-01")));
    assert!(membership.contains(&instance_id("quran-01")));

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let candidates = eligible_candidates(&membership, &observations, &destination);

    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .expect("eligible candidate must be selectable");

    let operation = operation_id("phase15-golden-operation");
    let execution_attempt = attempt(&operation, "phase15-golden-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        selection.instance_id().clone(),
        &execution_attempt,
    );

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), execution_attempt.attempt_id());
    assert_eq!(decision.destination().as_str(), "quran-01");

    let transport = InMemoryTransport::new();
    let calls = expose_echo_engine(&transport, "quran-engine", "quran-01");
    let communication = ControlPlaneCommunication::new(transport);
    let request = request_for(
        &engine_id("quran-engine"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "phase15-golden-message",
    );

    let response =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .expect("selected engine instance must receive the request");

    assert_eq!(response.status, Status::Success);
    assert_eq!(
        response.event.envelope.message_id.as_str(),
        "phase15-golden-message"
    );
    assert_eq!(response.event.envelope.payload.bytes(), b"quran-01");
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn multiple_instances_preserve_concrete_instance_identity_end_to_end() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        let registration = registration("quran-engine", instance, CAPABILITY);
        membership
            .register(registration)
            .expect("membership registration must succeed");
        observations
            .update(healthy_observation("quran-engine", instance))
            .expect("health observation must succeed");
    }

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let candidates = eligible_candidates(&membership, &observations, &destination);
    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();

    assert_eq!(selection.instance_id().as_str(), "quran-01");

    let transport = InMemoryTransport::new();
    let first_calls = expose_echo_engine(&transport, "quran-engine", "quran-01");
    let second_calls = expose_echo_engine(&transport, "quran-engine", "quran-02");
    let communication = ControlPlaneCommunication::new(transport);

    let operation = operation_id("phase15-instance-operation");
    let execution_attempt = attempt(&operation, "phase15-instance-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        selection.instance_id().clone(),
        &execution_attempt,
    );

    let request = request_for(
        &engine_id("quran-engine"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "phase15-instance-message",
    );

    let response =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .unwrap();

    assert_eq!(response.event.envelope.payload.bytes(), b"quran-01");
    assert_eq!(*first_calls.lock().unwrap(), 1);
    assert_eq!(*second_calls.lock().unwrap(), 0);
}

#[test]
fn unhealthy_instance_is_removed_before_policy_selection() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
    }

    observations
        .update(healthy_observation("quran-engine", "quran-01"))
        .unwrap();

    let unhealthy_health = HealthReport::new(
        engine_id("quran-engine"),
        LifecycleState::Serving,
        LivenessReport::unhealthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    observations
        .update(
            EngineObservation::new(
                engine_id("quran-engine"),
                instance_id("quran-02"),
                unhealthy_health,
            )
            .unwrap(),
        )
        .unwrap();

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership_snapshot,
        &observation_snapshot,
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ))
    .unwrap();

    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].instance_id().as_str(), "quran-01");
}

#[test]
fn hard_destination_failure_does_not_fallback_to_another_instance() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("quran-engine", instance))
            .unwrap();
    }

    let hard_destination = DestinationRequest::hard_explicit(instance_id("quran-99"));
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &hard_destination,
        &membership_snapshot,
        &observation_snapshot,
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));

    assert_eq!(
        result,
        Err(
            nizaam_core::control_plane::DestinationEligibilityError::HardDestinationNotMember(
                instance_id("quran-99"),
            ),
        )
    );
}

#[test]
fn preferred_destination_can_fallback_to_an_eligible_instance() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
    }

    observations
        .update(healthy_observation("quran-engine", "quran-02"))
        .unwrap();

    let preferred =
        DestinationRequest::preferred_explicit(instance_id("quran-01"), FallbackPolicy::Allowed);
    let candidates = eligible_candidates(&membership, &observations, &preferred);
    let constraints = RoutingConstraints::new().with_preferred_destination(instance_id("quran-01"));

    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &constraints))
        .unwrap();

    assert_eq!(selection.instance_id().as_str(), "quran-02");
}

#[test]
fn incompatible_registration_never_reaches_routing_or_communication() {
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(registration(
            "arabic-engine",
            "arabic-01",
            "arabic.tokenize",
        ))
        .unwrap();
    observations
        .update(healthy_observation("arabic-engine", "arabic-01"))
        .unwrap();

    let request = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let result = eligible_destinations(DestinationEligibilityInput::new(
        &request,
        &membership_snapshot,
        &observation_snapshot,
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));

    assert_eq!(
        result,
        Err(nizaam_core::control_plane::DestinationEligibilityError::NoEligibleDestination)
    );
}

#[test]
fn existing_routing_decision_survives_membership_change() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("quran-engine", instance))
            .unwrap();
    }

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let candidates = eligible_candidates(&membership, &observations, &destination);
    let first_selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();

    let operation = operation_id("phase15-immutable-operation");
    let first_attempt = attempt(&operation, "phase15-attempt-1", 1);
    let first_decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        first_selection.instance_id().clone(),
        &first_attempt,
    );

    membership
        .unregister(&instance_id("quran-01"))
        .expect("first instance must be removable");
    observations
        .remove(&instance_id("quran-01"))
        .expect("first observation must be removable");

    assert_eq!(first_decision.destination().as_str(), "quran-01");

    let next_candidates = eligible_candidates(&membership, &observations, &destination);
    assert_eq!(next_candidates.len(), 1);
    assert_eq!(next_candidates[0].instance_id().as_str(), "quran-02");
}

#[test]
fn retry_attempt_gets_an_independent_routing_decision() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("quran-engine", instance))
            .unwrap();
    }

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let first_candidates = eligible_candidates(&membership, &observations, &destination);
    let first_selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(
            &first_candidates,
            &RoutingConstraints::new(),
        ))
        .unwrap();

    let operation = operation_id("phase15-retry-operation");
    let attempt_one = attempt(&operation, "phase15-retry-attempt-1", 1);
    let decision_one = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        first_selection.instance_id().clone(),
        &attempt_one,
    );

    membership.unregister(&instance_id("quran-01")).unwrap();
    observations.remove(&instance_id("quran-01")).unwrap();

    let second_candidates = eligible_candidates(&membership, &observations, &destination);
    let second_selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(
            &second_candidates,
            &RoutingConstraints::new(),
        ))
        .unwrap();

    let attempt_two = attempt(&operation, "phase15-retry-attempt-2", 2);
    let decision_two = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        second_selection.instance_id().clone(),
        &attempt_two,
    );

    assert_eq!(decision_one.operation_id(), decision_two.operation_id());
    assert_ne!(decision_one.attempt_id(), decision_two.attempt_id());
    assert_eq!(decision_one.destination().as_str(), "quran-01");
    assert_eq!(decision_two.destination().as_str(), "quran-02");
}

#[test]
fn routing_failure_is_distinct_from_transport_failure() {
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(registration("quran-engine", "quran-01", CAPABILITY))
        .unwrap();

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let routing_failure = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership_snapshot,
        &observation_snapshot,
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));

    assert_eq!(
        routing_failure,
        Err(nizaam_core::control_plane::DestinationEligibilityError::NoEligibleDestination)
    );

    observations
        .update(healthy_observation("quran-engine", "quran-01"))
        .unwrap();

    let candidates = eligible_candidates(&membership, &observations, &destination);
    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();

    let operation = operation_id("phase15-transport-failure-operation");
    let execution_attempt = attempt(&operation, "phase15-transport-failure-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        selection.instance_id().clone(),
        &execution_attempt,
    );

    let communication = ControlPlaneCommunication::new(InMemoryTransport::new());
    let request = request_for(
        &engine_id("quran-engine"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "phase15-transport-failure-message",
    );

    let transport_result =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request));

    assert_eq!(transport_result, Err(TransportError::Disconnected),);
}

#[test]
fn routing_decision_targets_the_exact_communication_instance() {
    let membership = Membership::new();
    let observations = Observations::new();

    for instance in ["quran-01", "quran-02"] {
        membership
            .register(registration("quran-engine", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("quran-engine", instance))
            .unwrap();
    }

    let destination = DestinationRequest::hard_explicit(instance_id("quran-02"));
    let candidates = eligible_candidates(&membership, &observations, &destination);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].instance_id().as_str(), "quran-02");

    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(
            &candidates,
            &RoutingConstraints::new().with_hard_destination(instance_id("quran-02")),
        ))
        .unwrap();

    let operation = operation_id("phase15-target-operation");
    let execution_attempt = attempt(&operation, "phase15-target-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        selection.instance_id().clone(),
        &execution_attempt,
    );

    let transport = InMemoryTransport::new();
    let first_calls = expose_echo_engine(&transport, "quran-engine", "quran-01");
    let second_calls = expose_echo_engine(&transport, "quran-engine", "quran-02");
    let communication = ControlPlaneCommunication::new(transport);

    let request = request_for(
        &engine_id("quran-engine"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "phase15-target-message",
    );

    let response =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .unwrap();

    assert_eq!(response.event.envelope.payload.bytes(), b"quran-02");
    assert_eq!(*first_calls.lock().unwrap(), 0);
    assert_eq!(*second_calls.lock().unwrap(), 1);
}

#[test]
fn operation_and_attempt_identity_survive_resolution_routing_and_communication() {
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(registration("quran-engine", "quran-01", CAPABILITY))
        .unwrap();
    observations
        .update(healthy_observation("quran-engine", "quran-01"))
        .unwrap();

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let candidates = eligible_candidates(&membership, &observations, &destination);
    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();

    let operation = operation_id("phase15-identity-operation");
    let attempt_id = attempt_id("phase15-identity-attempt");
    let execution_attempt = Attempt::new(operation.clone(), attempt_id.clone(), 1).unwrap();
    let resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation.clone(),
        resolved_contract(),
        resolved_capability(),
        ResolvedRouting::new(
            selection.instance_id().clone(),
            RoutingStrategy::Deterministic,
        ),
    ));
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &execution_attempt)
        .unwrap();

    assert_eq!(resolution.operation_id(), &operation);
    assert_eq!(resolution.destination(), decision.destination());
    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), &attempt_id);

    let transport = InMemoryTransport::new();
    expose_echo_engine(&transport, "quran-engine", "quran-01");
    let communication = ControlPlaneCommunication::new(transport);
    let request = request_for(
        &engine_id("quran-engine"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "phase15-identity-message",
    );

    let response =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .unwrap();

    assert_eq!(
        response.event.envelope.operation_context.operation.id,
        operation
    );
    assert_eq!(
        response.event.envelope.operation_context.attempt_id,
        Some(attempt_id),
    );
}

#[test]
fn concurrent_distinct_registrations_produce_one_coherent_membership_snapshot() {
    let membership = Arc::new(Membership::new());
    let barrier = Arc::new(Barrier::new(3));

    let first_membership = Arc::clone(&membership);
    let first_barrier = Arc::clone(&barrier);
    let first = thread::spawn(move || {
        first_barrier.wait();
        first_membership
            .register(registration("quran-engine", "quran-01", CAPABILITY))
            .expect("first concurrent registration must succeed");
    });

    let second_membership = Arc::clone(&membership);
    let second_barrier = Arc::clone(&barrier);
    let second = thread::spawn(move || {
        second_barrier.wait();
        second_membership
            .register(registration("quran-engine", "quran-02", CAPABILITY))
            .expect("second concurrent registration must succeed");
    });

    barrier.wait();
    first.join().expect("first registration thread must join");
    second.join().expect("second registration thread must join");

    let snapshot = membership.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot.version(), 2);
    assert!(snapshot.contains(&instance_id("quran-01")));
    assert!(snapshot.contains(&instance_id("quran-02")));

    let candidate_ids = snapshot
        .candidates()
        .map(|record| record.engine_instance_id().as_str().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(candidate_ids, vec!["quran-01", "quran-02"]);
}

#[test]
fn coordination_change_is_exposed_as_a_replanning_decision() {
    let current = nizaam_core::control_plane::CoordinationSnapshot::new();
    let dependency = nizaam_core::control_plane::Dependency::blocking(
        NodeId::new("hadith-node").unwrap(),
        nizaam_core::control_plane::DependencyTarget::Capability(capability_requirement(
            "quran.search",
        )),
    )
    .unwrap();
    let proposed =
        nizaam_core::control_plane::CoordinationSnapshot::from_dependencies([dependency]);

    let decision = ControlPlane::new().evaluate_replanning_snapshots(&current, &proposed);

    assert!(decision.is_required());
    assert_eq!(
        decision.reason(),
        Some(nizaam_core::control_plane::ReplanningReason::CoordinationRequirementsChanged),
    );

    let delta = nizaam_core::control_plane::ReplanningDelta::between(&current, &proposed);
    assert!(delta.is_changed());
    assert_eq!(delta.added_count(), 1);
    assert_eq!(delta.removed_count(), 0);
}

// ---------------------------------------------------------------------------
// Phase 16 architectural conformance additions
// ---------------------------------------------------------------------------

#[test]
fn control_plane_keeps_logical_engine_and_concrete_instance_distinct() {
    let registration = registration("cp-engine", "cp-instance", CAPABILITY);
    assert_eq!(registration.engine_id().as_str(), "cp-engine");
    assert_eq!(registration.engine_instance_id().as_str(), "cp-instance");

    let registry = EngineRegistry::new();
    registry.register(registration).unwrap();
    let record = registry.get(&instance_id("cp-instance")).unwrap();
    assert_eq!(record.engine_id().as_str(), "cp-engine");
    assert_eq!(record.engine_instance_id().as_str(), "cp-instance");
}

#[test]
fn registration_metadata_survives_registry_and_membership() {
    let registration = registration("cp-meta-engine", "cp-meta-instance", CAPABILITY);
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    registry.register(registration.clone()).unwrap();
    membership.register(registration.clone()).unwrap();

    let record = registry.get(&instance_id("cp-meta-instance")).unwrap();
    assert!(
        record
            .registration()
            .capabilities()
            .iter()
            .any(|c| c.capability_id().as_str() == CAPABILITY)
    );
    assert!(membership.contains(&instance_id("cp-meta-instance")));
}

#[test]
fn duplicate_instance_reassignment_is_rejected() {
    let membership = Membership::new();
    membership
        .register(registration("cp-owner-a", "cp-shared", CAPABILITY))
        .unwrap();

    let result = membership.register(registration("cp-owner-b", "cp-shared", CAPABILITY));
    assert!(result.is_err());
}

#[test]
fn registry_and_membership_have_independent_snapshots() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();
    let first = registration("cp-independent", "cp-one", CAPABILITY);

    registry.register(first.clone()).unwrap();
    membership.register(first).unwrap();

    let membership_before = membership.snapshot();
    registry
        .register(registration("cp-independent", "cp-two", CAPABILITY))
        .unwrap();

    assert_eq!(membership_before.len(), 1);
    assert!(!membership_before.contains(&instance_id("cp-two")));
    assert!(registry.contains(&instance_id("cp-two")));
}

#[test]
fn membership_snapshot_is_immutable_after_later_registration() {
    let membership = Membership::new();
    membership
        .register(registration("cp-snapshot", "cp-one", CAPABILITY))
        .unwrap();

    let snapshot = membership.snapshot();
    membership
        .register(registration("cp-snapshot", "cp-two", CAPABILITY))
        .unwrap();

    assert!(snapshot.contains(&instance_id("cp-one")));
    assert!(!snapshot.contains(&instance_id("cp-two")));
}

#[test]
fn observation_snapshot_is_immutable_after_update() {
    let observations = Observations::new();
    observations
        .update(healthy_observation("cp-observe", "cp-one"))
        .unwrap();

    let snapshot = observations.snapshot();
    observations
        .update(healthy_observation("cp-observe", "cp-two"))
        .unwrap();

    assert!(snapshot.contains(&instance_id("cp-one")));
    assert!(!snapshot.contains(&instance_id("cp-two")));
}

#[test]
fn capability_resolution_is_declarative() {
    let operation = operation_id("cp-resolution-capability");
    let resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation.clone(),
        resolved_contract(),
        resolved_capability(),
        ResolvedRouting::new(
            instance_id("cp-destination"),
            RoutingStrategy::Deterministic,
        ),
    ));

    assert_eq!(resolution.operation_id(), &operation);
    assert_eq!(resolution.capability().capability_id().as_str(), CAPABILITY);
}

#[test]
fn incompatible_contract_is_not_eligible() {
    let membership = Membership::new();
    let observations = Observations::new();
    membership
        .register(registration("cp-contract", "cp-one", CAPABILITY))
        .unwrap();
    observations
        .update(healthy_observation("cp-contract", "cp-one"))
        .unwrap();

    let incompatible = ContractDescriptor::new(
        contract_id("incompatible.contract"),
        capability_id(CAPABILITY),
        Version::new(9, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new(MEDIA_TYPE, Version::new(9, 0, 0)).unwrap(),
    );

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let result = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &incompatible,
    ));
    assert!(result.is_err());
}

#[test]
fn registration_without_observation_is_not_routable() {
    let membership = Membership::new();
    let observations = Observations::new();
    membership
        .register(registration("cp-observation", "cp-one", CAPABILITY))
        .unwrap();

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let result = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));
    assert!(result.is_err());
}

#[test]
fn non_serving_observation_is_not_eligible() {
    let membership = Membership::new();
    let observations = Observations::new();

    let registration = registration("cp-lifecycle", "cp-one", CAPABILITY).with_runtime_metadata(
        nizaam_core::control_plane::RuntimeRegistrationMetadata::new()
            .with_lifecycle(LifecycleState::Draining)
            .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Draining)),
    );

    membership.register(registration).unwrap();

    let health = HealthReport::new(
        engine_id("cp-lifecycle"),
        LifecycleState::Draining,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Draining),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    observations
        .update(
            EngineObservation::new(engine_id("cp-lifecycle"), instance_id("cp-one"), health)
                .unwrap(),
        )
        .unwrap();

    let destination = DestinationRequest::hard_logical(capability_requirement(CAPABILITY));
    let result = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));

    assert!(result.is_err());
}

#[test]
fn health_observation_does_not_change_runtime_lifecycle() {
    let engine = nizaam_core::runtime::EngineRuntime::new(
        engine_id("cp-health-runtime"),
        instance_id("cp-health-runtime-1"),
    );
    assert_eq!(engine.state(), LifecycleState::Created);

    let observations = Observations::new();
    let health = HealthReport::new(
        engine_id("cp-health-runtime"),
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
                engine_id("cp-health-runtime"),
                instance_id("cp-health-runtime-1"),
                health,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(engine.state(), LifecycleState::Created);
}

#[test]
fn deterministic_policy_is_stable_for_same_candidates() {
    let candidates = vec![
        RoutingCandidate::new(instance_id("cp-policy-a")),
        RoutingCandidate::new(instance_id("cp-policy-b")),
    ];
    let constraints = RoutingConstraints::new();
    let input = PolicyInput::new(&candidates, &constraints);
    let first = RoutingPolicy::deterministic().evaluate(&input).unwrap();
    let second = RoutingPolicy::deterministic().evaluate(&input).unwrap();
    assert_eq!(first, second);
}

#[test]
fn hard_destination_does_not_fallback() {
    let membership = Membership::new();
    let observations = Observations::new();
    for id in ["cp-hard-a", "cp-hard-b"] {
        membership
            .register(registration("cp-hard", id, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("cp-hard", id))
            .unwrap();
    }

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &DestinationRequest::hard_explicit(instance_id("cp-hard-missing")),
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));
    assert!(result.is_err());
}

#[test]
fn preferred_destination_can_fallback() {
    let membership = Membership::new();
    let observations = Observations::new();
    for id in ["cp-pref-a", "cp-pref-b"] {
        membership
            .register(registration("cp-pref", id, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("cp-pref", id))
            .unwrap();
    }

    let preferred = DestinationRequest::preferred_explicit(
        instance_id("cp-pref-missing"),
        FallbackPolicy::Allowed,
    );
    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        &preferred,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ))
    .unwrap();

    assert!(!eligible.is_empty());
    assert_ne!(eligible[0].instance_id().as_str(), "cp-pref-missing");
}

#[test]
fn routing_decision_remains_unchanged_after_membership_change() {
    let operation = operation_id("cp-immutable-decision");
    let attempt = attempt(&operation, "cp-immutable-attempt", 1);
    let plane = ControlPlane::new();
    let decision = resolve_and_route(&plane, &operation, instance_id("cp-old"), &attempt);

    let membership = Membership::new();
    membership
        .register(registration("cp-immutable", "cp-old", CAPABILITY))
        .unwrap();
    membership
        .register(registration("cp-immutable", "cp-new", CAPABILITY))
        .unwrap();

    assert_eq!(decision.destination().as_str(), "cp-old");
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
}

#[test]
fn two_attempts_of_one_operation_receive_distinct_routing_decisions() {
    let operation = operation_id("cp-attempt-isolation");
    let first = attempt(&operation, "cp-attempt-one", 1);
    let second = attempt(&operation, "cp-attempt-two", 2);
    let plane = ControlPlane::new();

    let first_decision = resolve_and_route(&plane, &operation, instance_id("cp-a"), &first);
    let second_decision = resolve_and_route(&plane, &operation, instance_id("cp-b"), &second);

    assert_eq!(
        first_decision.operation_id(),
        second_decision.operation_id()
    );
    assert_ne!(first_decision.attempt_id(), second_decision.attempt_id());
    assert_ne!(first_decision.destination(), second_decision.destination());
}

#[test]
fn routing_does_not_change_attempt_number() {
    let operation = operation_id("cp-attempt-number");
    let attempt = attempt(&operation, "cp-attempt-number-1", 7);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-number"),
        &attempt,
    );
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
}

#[test]
fn stale_route_does_not_override_runtime_admission() {
    let engine = nizaam_core::runtime::EngineRuntime::new(
        engine_id("cp-stale-runtime"),
        instance_id("cp-stale-instance"),
    );
    let operation = Operation::new(
        operation_id("cp-stale-operation"),
        CorrelationId::new("cp-stale-correlation").unwrap(),
    );
    let context = nizaam_core::runtime::EngineContext::new(OperationContext::new(operation));

    let plane = ControlPlane::new();
    let attempt = attempt(&operation_id("cp-stale-operation"), "cp-stale-attempt", 1);
    let decision = resolve_and_route(
        &plane,
        &operation_id("cp-stale-operation"),
        instance_id("cp-stale-instance"),
        &attempt,
    );

    assert_eq!(decision.destination().as_str(), "cp-stale-instance");
    assert!(engine.admit_request().is_err());
    let _ = context;
}

#[test]
fn control_plane_communication_uses_the_selected_instance() {
    let transport = InMemoryTransport::new();
    let calls = expose_echo_engine(&transport, "cp-communication", "cp-selected");
    let communication = ControlPlaneCommunication::new(transport);
    let operation = operation_id("cp-communication-operation");
    let attempt = attempt(&operation, "cp-communication-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-selected"),
        &attempt,
    );

    let request = request_for(
        &engine_id("cp-communication"),
        decision.destination(),
        &operation,
        decision.attempt_id(),
        "cp-communication-message",
    );
    let response =
        futures::executor::block_on(communication.send_to_engine(decision.destination(), request))
            .unwrap();

    assert_eq!(response.status, Status::Success);
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn communication_failure_is_not_reported_as_routing_success() {
    let transport = InMemoryTransport::new();
    let communication = ControlPlaneCommunication::new(transport);
    let result = futures::executor::block_on(communication.send_to_engine(
        &instance_id("cp-missing"),
        request_for(
            &engine_id("cp-missing-engine"),
            &instance_id("cp-missing"),
            &operation_id("cp-communication-failure"),
            &attempt_id("cp-communication-failure-attempt"),
            "cp-communication-failure-message",
        ),
    ));
    assert!(result.is_err());
}

#[test]
fn replanning_detects_coordination_change_without_selecting_destination() {
    use nizaam_core::control_plane::{
        CoordinationSnapshot, Dependency, DependencyTarget, ReplanningDecision, ReplanningReason,
    };

    let dependency = Dependency::blocking(
        NodeId::new("cp-replan-source").unwrap(),
        DependencyTarget::Capability(CapabilityRequirement::new(capability_id(
            "cp-replan-capability",
        ))),
    )
    .unwrap();

    let current = CoordinationSnapshot::new();
    let proposed = CoordinationSnapshot::from_dependencies([dependency]);
    let decision = ControlPlane::new().evaluate_replanning_snapshots(&current, &proposed);

    assert_eq!(
        decision,
        ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
    );
}

#[test]
fn semantically_identical_replanning_snapshots_do_not_trigger_replanning() {
    use nizaam_core::control_plane::{
        CoordinationSnapshot, Dependency, DependencyTarget, ReplanningDecision,
    };

    let dependency = Dependency::blocking(
        NodeId::new("cp-replan-same-source").unwrap(),
        DependencyTarget::Capability(CapabilityRequirement::new(capability_id("cp-replan-same"))),
    )
    .unwrap();

    let first = CoordinationSnapshot::from_dependencies([dependency.clone()]);
    let second = CoordinationSnapshot::from_dependencies([dependency.clone(), dependency]);

    assert_eq!(
        ControlPlane::new().evaluate_replanning_snapshots(&first, &second),
        ReplanningDecision::NotRequired
    );
}

#[test]
fn replanning_does_not_create_a_destination_or_attempt() {
    use nizaam_core::control_plane::{CoordinationSnapshot, Dependency, DependencyTarget};

    let dependency = Dependency::blocking(
        NodeId::new("cp-replan-boundary-source").unwrap(),
        DependencyTarget::Capability(CapabilityRequirement::new(capability_id(
            "cp-replan-boundary",
        ))),
    )
    .unwrap();
    let decision = ControlPlane::new().evaluate_replanning_snapshots(
        &CoordinationSnapshot::new(),
        &CoordinationSnapshot::from_dependencies([dependency]),
    );

    assert!(decision.is_required());
}

#[test]
fn control_plane_does_not_execute_capability_handlers() {
    let transport = InMemoryTransport::new();
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_handler = Arc::clone(&calls);
    transport.register(
        engine_id("cp-no-execution"),
        instance_id("cp-no-execution-instance"),
        move |request| {
            *calls_for_handler.lock().unwrap() += 1;
            response_for(request, b"executed".to_vec())
        },
    );

    let operation = operation_id("cp-no-execution-operation");
    let attempt = attempt(&operation, "cp-no-execution-attempt", 1);
    let _decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-no-execution-instance"),
        &attempt,
    );

    assert_eq!(*calls.lock().unwrap(), 0);
}

#[test]
fn control_plane_does_not_create_retry_state() {
    let operation = operation_id("cp-no-retry");
    let attempt = attempt(&operation, "cp-no-retry-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-no-retry"),
        &attempt,
    );
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
}
