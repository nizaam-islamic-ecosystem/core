//! Phase 15 end-to-end integration tests.
//!
//! These tests exercise the public Control Plane boundaries as one composed
//! system. They deliberately cross registration, registry, membership,
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
