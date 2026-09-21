//! Phase 15 Control Plane routing integration tests.
//!
//! These tests exercise the public routing boundary from outside the
//! `control_plane` module. Routing consumes an already-resolved destination
//! and an already-created execution attempt; it does not perform discovery,
//! destination selection, retry creation, transport, or execution.

use nizaam_core::control_plane::policy::RoutingStrategy;
use nizaam_core::control_plane::resolution::{
    Resolution, ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedProvider,
    ResolvedRouting,
};
use nizaam_core::control_plane::routing::{Router, RoutingError, RoutingInput};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, NodeId,
    OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::retry::{Attempt, AttemptLifecycleState};
use std::sync::{Arc, Barrier};

const CONTRACT_ID: &str = "quran.analyze";
const CONTRACT_VERSION: &str = "1.0";
const CAPABILITY_ID: &str = "quran.analyze";

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).expect("test operation id must be valid")
}

fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).expect("test attempt id must be valid")
}

fn engine_instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

fn resolution_for(operation: &OperationId, destination: &EngineInstanceId) -> Resolution {
    Resolution::resolve(ResolutionInput::new(
        operation.clone(),
        ResolvedContract::new(contract_id(CONTRACT_ID), CONTRACT_VERSION),
        ResolvedCapability::new(capability_id(CAPABILITY_ID)),
        ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
    ))
}

fn attempt_for(operation: &OperationId, id: &AttemptId, number: u32) -> Attempt {
    Attempt::new(operation.clone(), id.clone(), number).expect("test attempt must be valid")
}

fn context_for(operation: &OperationId, id: &AttemptId) -> OperationContext {
    let operation_context = Operation::new(
        operation.clone(),
        CorrelationId::new("routing-correlation").expect("test correlation id must be valid"),
    );

    OperationContext::new(operation_context).for_attempt(
        NodeId::new("routing-node").expect("test node id must be valid"),
        id.clone(),
    )
}

#[test]
fn resolved_destination_reaches_routing_decision_unchanged() {
    let operation = operation_id("routing-operation-01");
    let attempt_id = attempt_id("routing-attempt-01");
    let destination = engine_instance_id("quran-01");

    let resolution = resolution_for(&operation, &destination);
    let attempt = attempt_for(&operation, &attempt_id, 1);

    let decision =
        Router::route_resolved(&resolution, &attempt).expect("resolved interaction must route");

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), &attempt_id);
    assert_eq!(decision.destination(), &destination);
}

#[test]
fn router_binds_the_resolved_destination_without_selecting_another_destination() {
    let operation = operation_id("routing-operation-02");
    let attempt_id = attempt_id("routing-attempt-02");
    let destination = engine_instance_id("arabic-02");

    let resolution = resolution_for(&operation, &destination);
    let attempt = attempt_for(&operation, &attempt_id, 1);

    let decision = Router::new()
        .route(RoutingInput::new(&resolution, &attempt))
        .expect("router must bind a valid resolution to its attempt");

    assert_eq!(decision.destination().as_str(), "arabic-02");
}

#[test]
fn operation_identity_mismatch_is_rejected() {
    let resolution_operation = operation_id("routing-resolution-operation");
    let attempt_operation = operation_id("routing-attempt-operation");
    let destination = engine_instance_id("quran-03");

    let resolution = resolution_for(&resolution_operation, &destination);
    let attempt = attempt_for(&attempt_operation, &attempt_id("routing-attempt-03"), 1);

    let result = Router::route_resolved(&resolution, &attempt);

    assert!(matches!(
        result,
        Err(RoutingError::OperationMismatch {
            resolution_operation_id,
            attempt_operation_id,
        }) if resolution_operation_id == resolution_operation
            && attempt_operation_id == attempt_operation
    ));
}

#[test]
fn attempt_context_mismatch_is_rejected() {
    let operation = operation_id("routing-operation-04");
    let resolution_attempt = attempt_id("resolution-attempt-04");
    let routing_attempt = attempt_id("routing-attempt-04");

    let resolution = Resolution::resolve(
        ResolutionInput::new(
            operation.clone(),
            ResolvedContract::new(contract_id(CONTRACT_ID), CONTRACT_VERSION),
            ResolvedCapability::new(capability_id(CAPABILITY_ID)),
            ResolvedRouting::new(
                engine_instance_id("quran-04"),
                RoutingStrategy::Deterministic,
            ),
        )
        .with_context(context_for(&operation, &resolution_attempt)),
    );

    let attempt = attempt_for(&operation, &routing_attempt, 1);
    let result = Router::route_resolved(&resolution, &attempt);

    assert!(matches!(
        result,
        Err(RoutingError::AttemptMismatch {
            resolution_attempt_id,
            routing_attempt_id,
        }) if resolution_attempt_id == resolution_attempt
            && routing_attempt_id == routing_attempt
    ));
}

#[test]
fn matching_attempt_context_is_accepted() {
    let operation = operation_id("routing-operation-05");
    let attempt_id = attempt_id("routing-attempt-05");
    let destination = engine_instance_id("quran-05");

    let resolution = Resolution::resolve(
        ResolutionInput::new(
            operation.clone(),
            ResolvedContract::new(contract_id(CONTRACT_ID), CONTRACT_VERSION),
            ResolvedCapability::new(capability_id(CAPABILITY_ID)),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::CapacityAware),
        )
        .with_context(context_for(&operation, &attempt_id)),
    );

    let attempt = attempt_for(&operation, &attempt_id, 1);
    let decision =
        Router::route_resolved(&resolution, &attempt).expect("matching attempt context must route");

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), &attempt_id);
    assert_eq!(decision.destination(), &destination);
}

#[test]
fn resolution_without_attempt_context_is_routable() {
    let operation = operation_id("routing-operation-06");
    let attempt_id = attempt_id("routing-attempt-06");
    let destination = engine_instance_id("quran-06");

    let resolution = resolution_for(&operation, &destination);
    assert!(resolution.context().is_none());

    let attempt = attempt_for(&operation, &attempt_id, 1);
    let decision = Router::route_resolved(&resolution, &attempt)
        .expect("attempt may bind to context-free resolution");

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), &attempt_id);
    assert_eq!(decision.destination(), &destination);
}

#[test]
fn existing_routing_decision_remains_stable_when_later_resolution_changes() {
    let operation = operation_id("routing-operation-07");
    let first_attempt_id = attempt_id("routing-attempt-07a");
    let second_attempt_id = attempt_id("routing-attempt-07b");

    let first_destination = engine_instance_id("engine-a");
    let second_destination = engine_instance_id("engine-b");

    let first_resolution = resolution_for(&operation, &first_destination);
    let first_attempt = attempt_for(&operation, &first_attempt_id, 1);

    let first_decision = Router::route_resolved(&first_resolution, &first_attempt)
        .expect("first routing decision must succeed");

    let later_resolution = resolution_for(&operation, &second_destination);
    let second_attempt = attempt_for(&operation, &second_attempt_id, 2);

    let second_decision = Router::route_resolved(&later_resolution, &second_attempt)
        .expect("second routing decision must succeed");

    assert_eq!(first_decision.destination(), &first_destination);
    assert_eq!(first_decision.attempt_id(), &first_attempt_id);

    assert_eq!(second_decision.destination(), &second_destination);
    assert_eq!(second_decision.attempt_id(), &second_attempt_id);

    assert_ne!(first_decision, second_decision);
}

#[test]
fn separate_attempts_for_one_operation_receive_independent_decisions() {
    let operation = operation_id("routing-operation-08");

    let attempt_one_id = attempt_id("routing-attempt-08a");
    let attempt_two_id = attempt_id("routing-attempt-08b");

    let destination_one = engine_instance_id("engine-08a");
    let destination_two = engine_instance_id("engine-08b");

    let resolution_one = resolution_for(&operation, &destination_one);
    let resolution_two = resolution_for(&operation, &destination_two);

    let attempt_one = attempt_for(&operation, &attempt_one_id, 1);
    let attempt_two = attempt_for(&operation, &attempt_two_id, 2);

    let decision_one =
        Router::route_resolved(&resolution_one, &attempt_one).expect("first attempt must route");
    let decision_two =
        Router::route_resolved(&resolution_two, &attempt_two).expect("second attempt must route");

    assert_eq!(decision_one.operation_id(), &operation);
    assert_eq!(decision_two.operation_id(), &operation);

    assert_eq!(decision_one.attempt_id(), &attempt_one_id);
    assert_eq!(decision_two.attempt_id(), &attempt_two_id);

    assert_eq!(decision_one.destination(), &destination_one);
    assert_eq!(decision_two.destination(), &destination_two);

    assert_ne!(decision_one.attempt_id(), decision_two.attempt_id());
    assert_ne!(decision_one.destination(), decision_two.destination());
}

#[test]
fn routing_does_not_mutate_attempt_lifecycle_or_attempt_number() {
    let operation = operation_id("routing-operation-09");
    let attempt_id = attempt_id("routing-attempt-09");

    let resolution = resolution_for(&operation, &engine_instance_id("engine-09"));
    let attempt = attempt_for(&operation, &attempt_id, 1);

    assert_eq!(attempt.attempt_id(), &attempt_id);
    assert_eq!(attempt.attempt_number(), 1);
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);

    let _decision =
        Router::route_resolved(&resolution, &attempt).expect("valid attempt must route");

    assert_eq!(attempt.attempt_id(), &attempt_id);
    assert_eq!(attempt.attempt_number(), 1);
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
}

#[test]
fn provider_identity_does_not_replace_the_resolved_concrete_destination() {
    let operation = operation_id("routing-operation-10");
    let attempt_id = attempt_id("routing-attempt-10");
    let provider = EngineId::new("quran-provider").expect("test engine id must be valid");
    let destination = engine_instance_id("quran-provider-01");

    let resolution = Resolution::resolve(
        ResolutionInput::new(
            operation.clone(),
            ResolvedContract::new(contract_id(CONTRACT_ID), CONTRACT_VERSION),
            ResolvedCapability::new(capability_id(CAPABILITY_ID)),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
        )
        .with_provider(ResolvedProvider::new(provider.clone())),
    );

    assert_eq!(
        resolution
            .provider()
            .expect("provider must be preserved")
            .engine_id(),
        &provider
    );
    assert_eq!(resolution.destination(), &destination);

    let attempt = attempt_for(&operation, &attempt_id, 1);
    let decision = Router::route_resolved(&resolution, &attempt)
        .expect("provider-backed resolution must route");

    assert_eq!(decision.destination(), &destination);
    assert_ne!(
        decision.destination().as_str(),
        resolution
            .provider()
            .expect("provider must exist")
            .engine_id()
            .as_str()
    );
}

#[test]
fn routing_decision_into_parts_preserves_all_routing_identity() {
    let operation = operation_id("routing-operation-11");
    let attempt_id = attempt_id("routing-attempt-11");
    let destination = engine_instance_id("engine-11");

    let resolution = resolution_for(&operation, &destination);
    let attempt = attempt_for(&operation, &attempt_id, 1);

    let decision =
        Router::route_resolved(&resolution, &attempt).expect("valid routing input must succeed");

    let (resolved_operation, resolved_attempt, resolved_destination) = decision.into_parts();

    assert_eq!(resolved_operation, operation);
    assert_eq!(resolved_attempt, attempt_id);
    assert_eq!(resolved_destination, destination);
}

#[test]
fn concurrent_independent_routing_decisions_preserve_their_identities() {
    const ROUTING_COUNT: usize = 8;

    let router = Router::new();
    let barrier = Arc::new(Barrier::new(ROUTING_COUNT));
    let router = Arc::new(router);

    let mut handles = Vec::with_capacity(ROUTING_COUNT);

    for index in 0..ROUTING_COUNT {
        let router = Arc::clone(&router);
        let barrier = Arc::clone(&barrier);

        handles.push(std::thread::spawn(move || {
            let operation = operation_id(&format!("routing-operation-{index:02}"));
            let attempt_id = attempt_id(&format!("routing-attempt-{index:02}"));
            let destination = engine_instance_id(&format!("engine-{index:02}"));

            let resolution = resolution_for(&operation, &destination);
            let attempt = attempt_for(&operation, &attempt_id, 1);

            barrier.wait();

            let decision = router
                .route(RoutingInput::new(&resolution, &attempt))
                .expect("independent routing decision must succeed");

            (operation, attempt_id, destination, decision)
        }));
    }

    for (index, handle) in handles.into_iter().enumerate() {
        let (operation, attempt_id, destination, decision) =
            handle.join().expect("routing worker must not panic");

        assert_eq!(operation.as_str(), format!("routing-operation-{index:02}"));
        assert_eq!(attempt_id.as_str(), format!("routing-attempt-{index:02}"));
        assert_eq!(destination.as_str(), format!("engine-{index:02}"));

        assert_eq!(decision.operation_id(), &operation);
        assert_eq!(decision.attempt_id(), &attempt_id);
        assert_eq!(decision.destination(), &destination);
    }
}
