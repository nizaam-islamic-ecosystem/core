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
    ContractDescriptor, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, ControlPlaneCommunication, DestinationEligibilityInput,
    DestinationRequest, EngineObservation, EngineRegistry, FallbackPolicy, Membership,
    Observations, PolicyInput, ResolutionInput, ResolvedCapability, ResolvedContract,
    ResolvedRouting, RoutingCandidate, RoutingConstraints, RoutingPolicy, RoutingStrategy,
    eligible_destinations,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{CorrelationId, NodeId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::LifecycleState;
use nizaam_core::status::Status;
use nizaam_core::transport::{InMemoryTransport, TransportError};

mod common;
use common::control_plane::*;

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
    let resolved_contract: ResolvedContract = resolved_contract();
    let resolved_capability: ResolvedCapability = resolved_capability();
    let operation_model = Operation::new(
        operation.clone(),
        CorrelationId::new("cp-resolution-capability-correlation")
            .expect("test correlation id must be valid"),
    );
    let operation_context: OperationContext = OperationContext::new(operation_model).for_attempt(
        NodeId::new("cp-resolution-node").expect("test node id must be valid"),
        attempt_id("cp-resolution-attempt"),
    );

    let resolution = ControlPlane::new().resolve(
        ResolutionInput::new(
            operation.clone(),
            resolved_contract,
            resolved_capability,
            ResolvedRouting::new(
                instance_id("cp-destination"),
                RoutingStrategy::Deterministic,
            ),
        )
        .with_context(operation_context.clone()),
    );

    assert_eq!(resolution.operation_id(), &operation);
    assert_eq!(resolution.contract().contract_id().as_str(), CONTRACT);
    assert_eq!(resolution.capability().capability_id().as_str(), CAPABILITY);
    assert_eq!(resolution.context(), Some(&operation_context));
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

    assert_eq!(
        result,
        Err(nizaam_core::control_plane::DestinationEligibilityError::NoEligibleDestination)
    );
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
fn routing_decision_preserves_attempt_identity_and_number() {
    let operation = operation_id("cp-attempt-number");
    let attempt = attempt(&operation, "cp-attempt-number-1", 7);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-number"),
        &attempt,
    );

    assert_eq!(attempt.attempt_number(), 7);
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
    assert_eq!(attempt.attempt_number(), 7);
}

#[test]
fn stale_route_does_not_override_runtime_admission() {
    let engine = nizaam_core::runtime::EngineRuntime::new(
        engine_id("cp-stale-runtime"),
        instance_id("cp-stale-instance"),
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
        engine.transition(state).unwrap();
    }

    let operation = operation_id("cp-stale-operation");
    let attempt = attempt(&operation, "cp-stale-attempt", 1);
    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-stale-instance"),
        &attempt,
    );

    assert_eq!(decision.destination().as_str(), "cp-stale-instance");
    assert_eq!(engine.admit_request(), Ok(()));

    engine.transition(LifecycleState::Draining).unwrap();
    assert_eq!(
        engine.admit_request(),
        Err(nizaam_core::runtime::RequestAdmissionError::NotServing(
            LifecycleState::Draining
        ))
    );
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
fn control_plane_resolution_does_not_invoke_transport_handlers() {
    let transport = InMemoryTransport::new();
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_handler = Arc::clone(&calls);
    let engine = engine_id("cp-no-execution");
    let instance = instance_id("cp-no-execution-instance");

    transport.register(engine.clone(), instance.clone(), move |request| {
        *calls_for_handler.lock().unwrap() += 1;
        response_for(request, b"executed".to_vec())
    });

    let _communication = ControlPlaneCommunication::new(transport);
    let operation = operation_id("cp-no-execution-operation");
    let attempt = attempt(&operation, "cp-no-execution-attempt", 1);
    let decision = resolve_and_route(&ControlPlane::new(), &operation, instance.clone(), &attempt);

    assert_eq!(decision.destination(), &instance);
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[test]
fn routing_resolution_preserves_existing_attempt_state() {
    let operation = operation_id("cp-no-retry");
    let attempt = attempt(&operation, "cp-no-retry-attempt", 1);
    assert_eq!(
        attempt.state(),
        nizaam_core::retry::AttemptLifecycleState::Created
    );

    let decision = resolve_and_route(
        &ControlPlane::new(),
        &operation,
        instance_id("cp-no-retry"),
        &attempt,
    );

    assert_eq!(decision.attempt_id(), attempt.attempt_id());
    assert_eq!(
        attempt.state(),
        nizaam_core::retry::AttemptLifecycleState::Created
    );
}
