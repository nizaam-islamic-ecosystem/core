use std::time::Duration;

use nizaam_core::{
    artifact::{ArtifactReference, VersionSelector},
    contracts::{ContractDescriptor, Interaction, PayloadDescriptor, Version},
    control_plane::{
        ControlPlane, ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedRouting,
        RoutingStrategy,
    },
    idempotency::{
        IdempotencyIdentity, IdempotencyKey, IdempotencyRecord, IdempotencyReservation,
        IdempotencyScope, IdempotencyState, IdempotencyStateStore, RecordedOutcome,
    },
    identity::{
        ArtifactId, AttemptId, CapabilityId, ContractId, CorrelationId, EngineInstanceId,
        OperationId,
    },
    operation::{CancellationToken, Operation, OperationContext},
    retry::{
        Attempt, AttemptLifecycleState, BackoffPolicy, FailureCategory, RetryAdmission,
        RetryAdmissionError, RetryAdmissionRequest, RetryBudget, RetryDecision, RetryDenialReason,
        RetryPolicy, RetrySafetyGate, RetrySafetyGates,
    },
    runtime::{Deadline, EngineContext},
    status::{Retryability, Status},
};

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}

fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).unwrap()
}

fn operation(value: &str) -> Operation {
    Operation::new(
        operation_id(value),
        CorrelationId::new(format!("{value}-correlation")).unwrap(),
    )
}

fn engine_context(value: &str) -> EngineContext {
    EngineContext::new(OperationContext::new(operation(value)))
}

fn attempt(operation: &OperationId, id: &str, number: u32) -> Attempt {
    Attempt::new(operation.clone(), attempt_id(id), number).unwrap()
}

fn retryable_policy() -> RetryPolicy {
    RetryPolicy::new(3, 4)
        .unwrap()
        .with_retryable_category(FailureCategory::Transient)
}

fn identity(scope: &str, key: &str) -> IdempotencyIdentity {
    IdempotencyIdentity::new(
        IdempotencyScope::new(scope).unwrap(),
        IdempotencyKey::new(key).unwrap(),
    )
}

fn record(
    identity: IdempotencyIdentity,
    operation_id: OperationId,
    capability: Option<CapabilityId>,
) -> IdempotencyRecord {
    IdempotencyRecord::new(
        identity,
        operation_id,
        capability,
        IdempotencyState::InFlight,
        None,
        None,
        10_000,
    )
}

fn admission<'a>(
    policy: &'a RetryPolicy,
    cancellation: &'a CancellationToken,
    deadline: Option<Deadline>,
) -> RetryAdmission<'a> {
    static NO_BACKOFF: BackoffPolicy = BackoffPolicy::no_backoff();
    RetryAdmission::new(policy, &NO_BACKOFF, cancellation, deadline)
}

fn gates() -> RetrySafetyGates {
    RetrySafetyGates::new(true, true, true, true)
}

#[test]
fn retryable_failure_admits_one_successor_attempt() {
    let operation = operation_id("retry-conformance-1");
    let current = attempt(&operation, "retry-conformance-1-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();
    let policy = retryable_policy();

    let (next, delay) = admission(&policy, &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-1-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap();

    assert_eq!(delay, Duration::ZERO);
    assert_eq!(next.operation_id(), &operation);
    assert_eq!(next.attempt_number(), 2);
    assert_eq!(next.state(), AttemptLifecycleState::Created);
    assert_eq!(budget.consumed(), 1);
}

#[test]
fn non_retryable_failure_is_denied_before_budget_consumption() {
    let operation = operation_id("retry-conformance-2");
    let current = attempt(&operation, "retry-conformance-2-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(2);
    let cancellation = CancellationToken::new();

    let result =
        admission(&retryable_policy(), &cancellation, None).admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::NonRetryable,
            next_attempt_id: attempt_id("retry-conformance-2-a2"),
            jitter_source: None,
            safety_gates: gates(),
        });

    assert!(matches!(
        result,
        Err(RetryAdmissionError::Policy(RetryDenialReason::NonRetryable))
    ));
    assert_eq!(budget.consumed(), 0);
}

#[test]
fn retry_budget_is_enforced_and_successor_is_not_reserved_after_failure() {
    let operation = operation_id("retry-conformance-3");
    let current = attempt(&operation, "retry-conformance-3-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(0);
    let cancellation = CancellationToken::new();

    let result =
        admission(&retryable_policy(), &cancellation, None).admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-3-a2"),
            jitter_source: None,
            safety_gates: gates(),
        });

    assert!(matches!(result, Err(RetryAdmissionError::Budget(_))));
    assert_eq!(budget.consumed(), 0);

    let mut later_budget = RetryBudget::new(1);
    let next = admission(&retryable_policy(), &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut later_budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-3-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap()
        .0;

    assert_eq!(next.attempt_number(), 2);
}

#[test]
fn deadline_is_authoritative_for_retry_admission() {
    let operation = operation_id("retry-conformance-4");
    let current = attempt(&operation, "retry-conformance-4-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();
    let expired = Deadline::from_now(Duration::ZERO).unwrap();

    let result = admission(&retryable_policy(), &cancellation, Some(expired)).admit_next(
        RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-4-a2"),
            jitter_source: None,
            safety_gates: gates(),
        },
    );

    assert!(matches!(
        result,
        Err(RetryAdmissionError::SafetyGate(RetrySafetyGate::Deadline))
    ));
    assert_eq!(budget.consumed(), 0);
}

#[test]
fn cancellation_is_authoritative_for_retry_admission() {
    let operation = operation_id("retry-conformance-5");
    let current = attempt(&operation, "retry-conformance-5-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();
    cancellation.cancel();

    let result =
        admission(&retryable_policy(), &cancellation, None).admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-5-a2"),
            jitter_source: None,
            safety_gates: gates(),
        });

    assert!(matches!(
        result,
        Err(RetryAdmissionError::SafetyGate(
            RetrySafetyGate::Cancellation
        ))
    ));
    assert_eq!(budget.consumed(), 0);
}

#[test]
fn retry_preserves_operation_identity_and_changes_attempt_identity() {
    let operation = operation_id("retry-conformance-6");
    let current = attempt(&operation, "retry-conformance-6-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();

    let next = admission(&retryable_policy(), &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-6-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap()
        .0;

    assert_eq!(current.operation_id(), next.operation_id());
    assert_ne!(current.attempt_id(), next.attempt_id());
    assert_eq!(next.attempt_number(), 2);
}

#[test]
fn retry_safety_gates_are_checked_without_consuming_budget() {
    let operation = operation_id("retry-conformance-7");
    let current = attempt(&operation, "retry-conformance-7-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let policy = retryable_policy();
    let cancellation = CancellationToken::new();

    for (gates, expected) in [
        (
            RetrySafetyGates::new(false, true, true, true),
            RetrySafetyGate::ResourceAdmission,
        ),
        (
            RetrySafetyGates::new(true, false, true, true),
            RetrySafetyGate::ExternalEffects,
        ),
        (
            RetrySafetyGates::new(true, true, false, true),
            RetrySafetyGate::Idempotency,
        ),
        (
            RetrySafetyGates::new(true, true, true, false),
            RetrySafetyGate::ObservableOutput,
        ),
    ] {
        let mut budget = RetryBudget::new(1);
        let result = admission(&policy, &cancellation, None).admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-7-successor"),
            jitter_source: None,
            safety_gates: gates,
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::SafetyGate(actual)) if actual == expected
        ));
        assert_eq!(budget.consumed(), 0);
    }
}

#[test]
fn retry_does_not_reuse_current_attempt_identity_or_admit_two_successors() {
    let operation = operation_id("retry-conformance-8");
    let current = attempt(&operation, "retry-conformance-8-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let policy = retryable_policy();
    let cancellation = CancellationToken::new();

    let mut budget = RetryBudget::new(2);
    let reused = admission(&policy, &cancellation, None).admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &current,
        category: FailureCategory::Transient,
        retryability: Retryability::Retryable,
        next_attempt_id: current.attempt_id().clone(),
        jitter_source: None,
        safety_gates: gates(),
    });
    assert!(matches!(
        reused,
        Err(RetryAdmissionError::CurrentAttemptIdReused)
    ));
    assert_eq!(budget.consumed(), 0);

    let first = admission(&policy, &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-8-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap()
        .0;

    let second = admission(&policy, &cancellation, None).admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &current,
        category: FailureCategory::Transient,
        retryability: Retryability::Retryable,
        next_attempt_id: attempt_id("retry-conformance-8-a3"),
        jitter_source: None,
        safety_gates: gates(),
    });

    assert!(matches!(
        second,
        Err(RetryAdmissionError::SuccessorAlreadyReserved)
    ));
    assert_eq!(first.operation_id(), current.operation_id());
    assert_eq!(budget.consumed(), 1);
}

#[test]
fn unknown_outcome_is_not_treated_as_ordinary_retryable_failure() {
    let policy = retryable_policy();

    assert_eq!(
        policy.evaluate(FailureCategory::Unknown, Retryability::Retryable, 1),
        RetryDecision::Denied(RetryDenialReason::UnknownOutcome)
    );
}

#[test]
fn idempotency_duplicate_preserves_completed_logical_result() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "retry-key");
    let operation = operation_id("retry-idempotency-operation");
    let capability = CapabilityId::new("retry.capability").unwrap();

    let initial = record(id.clone(), operation.clone(), Some(capability.clone()));
    assert!(matches!(
        store.reserve(initial, 1).unwrap(),
        IdempotencyReservation::Created(_)
    ));

    let outcome = RecordedOutcome::new(Status::Success, None);
    let completed = store
        .transition(
            &id,
            IdempotencyState::Succeeded,
            Some(outcome.clone()),
            Some("result://retry/1".into()),
        )
        .unwrap();

    let duplicate = record(id.clone(), operation.clone(), Some(capability.clone()));
    let reservation = store.reserve(duplicate, 2).unwrap();

    let IdempotencyReservation::Duplicate(existing) = reservation else {
        panic!("expected duplicate reservation");
    };

    assert_eq!(existing, completed);
    assert_eq!(existing.outcome(), Some(&outcome));
    assert_eq!(existing.result_reference(), Some("result://retry/1"));
}

#[test]
fn idempotency_conflict_is_distinct_from_duplicate() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "conflict-key");
    let capability = CapabilityId::new("retry.capability").unwrap();

    store
        .reserve(
            record(
                id.clone(),
                operation_id("operation-a"),
                Some(capability.clone()),
            ),
            1,
        )
        .unwrap();

    let conflict = store
        .reserve(record(id, operation_id("operation-b"), Some(capability)), 2)
        .unwrap();

    assert!(matches!(conflict, IdempotencyReservation::Conflict(_)));
}

#[test]
fn retry_and_idempotency_share_operation_identity_without_becoming_one_mechanism() {
    let operation = operation_id("retry-conformance-9");
    let id = identity("retry-scope", "logical-action");
    let capability = CapabilityId::new("retry.capability").unwrap();
    let store = IdempotencyStateStore::new();

    store
        .reserve(
            record(id.clone(), operation.clone(), Some(capability.clone())),
            1,
        )
        .unwrap();

    let current = attempt(&operation, "retry-conformance-9-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();
    let next = admission(&retryable_policy(), &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-9-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap()
        .0;

    let stored = store.get(&id).unwrap().unwrap();
    assert_eq!(stored.operation_id(), next.operation_id());
    assert_eq!(stored.capability_id(), Some(&capability));
    assert_ne!(current.attempt_id(), next.attempt_id());
}

#[test]
fn duplicate_in_flight_submission_is_not_admitted_as_a_new_logical_action() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "in-flight-key");
    let operation = operation_id("retry-inflight-operation");
    let capability = CapabilityId::new("retry.capability").unwrap();

    let first = record(id.clone(), operation.clone(), Some(capability.clone()));
    assert!(matches!(
        store.reserve(first, 1).unwrap(),
        IdempotencyReservation::Created(_)
    ));

    let duplicate = record(id.clone(), operation, Some(capability));
    assert!(matches!(
        store.reserve(duplicate, 2).unwrap(),
        IdempotencyReservation::Duplicate(_)
    ));
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn completed_duplicate_reuses_existing_logical_outcome() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "completed-key");
    let operation = operation_id("retry-completed-operation");
    let capability = CapabilityId::new("retry.capability").unwrap();

    store
        .reserve(
            record(id.clone(), operation.clone(), Some(capability.clone())),
            1,
        )
        .unwrap();

    let completed = store
        .transition(
            &id,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some("result://completed/1".into()),
        )
        .unwrap();

    let duplicate = store
        .reserve(record(id, operation, Some(capability)), 2)
        .unwrap();

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        panic!("expected completed duplicate");
    };

    assert_eq!(existing, completed);
    assert_eq!(existing.result_reference(), Some("result://completed/1"));
}

#[test]
fn unknown_idempotency_outcome_remains_distinct_until_reconciled() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "unknown-key");
    let operation = operation_id("retry-unknown-operation");

    store
        .reserve(record(id.clone(), operation, None), 1)
        .unwrap();

    let unknown = store
        .transition(&id, IdempotencyState::Unknown, None, None)
        .unwrap();

    assert_eq!(*unknown.state(), IdempotencyState::Unknown);
    assert!(unknown.outcome().is_none());
    assert!(unknown.result_reference().is_none());
}

#[test]
fn retry_after_response_loss_can_protect_one_logical_side_effect() {
    let store = IdempotencyStateStore::new();
    let id = identity("retry-scope", "response-loss");
    let operation = operation_id("retry-response-loss-operation");
    let capability = CapabilityId::new("retry.side-effect").unwrap();

    let mut side_effects = 0usize;

    let first = record(id.clone(), operation.clone(), Some(capability.clone()));
    if matches!(
        store.reserve(first, 1).unwrap(),
        IdempotencyReservation::Created(_)
    ) {
        side_effects += 1;
    }

    store
        .transition(
            &id,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some("result://response-loss/1".into()),
        )
        .unwrap();

    let retry = record(id.clone(), operation.clone(), Some(capability));
    if matches!(
        store.reserve(retry, 2).unwrap(),
        IdempotencyReservation::Created(_)
    ) {
        side_effects += 1;
    }

    assert_eq!(side_effects, 1);
    assert!(matches!(
        store.lookup(&id, 2).unwrap(),
        nizaam_core::idempotency::IdempotencyLookup::Present(_)
    ));
}

#[test]
fn retry_creates_a_new_control_plane_routing_decision_for_the_new_attempt() {
    let operation = operation_id("retry-routing-conformance");
    let first = attempt(&operation, "retry-routing-a1", 1);
    let second = attempt(&operation, "retry-routing-a2", 2);

    let contract =
        ResolvedContract::new(ContractId::new("retry.routing.contract").unwrap(), "1.0.0");
    let capability =
        ResolvedCapability::new(CapabilityId::new("retry.routing.capability").unwrap());

    let first_resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation.clone(),
        contract.clone(),
        capability.clone(),
        ResolvedRouting::new(
            EngineInstanceId::new("retry-routing-instance-a").unwrap(),
            RoutingStrategy::Deterministic,
        ),
    ));
    let second_resolution = ControlPlane::new().resolve(ResolutionInput::new(
        operation.clone(),
        contract,
        capability,
        ResolvedRouting::new(
            EngineInstanceId::new("retry-routing-instance-b").unwrap(),
            RoutingStrategy::Deterministic,
        ),
    ));

    let first_decision = ControlPlane::new()
        .route_resolved(&first_resolution, &first)
        .unwrap();
    let second_decision = ControlPlane::new()
        .route_resolved(&second_resolution, &second)
        .unwrap();

    assert_eq!(
        first_decision.operation_id(),
        second_decision.operation_id()
    );
    assert_ne!(first_decision.attempt_id(), second_decision.attempt_id());
    assert_ne!(first_decision.destination(), second_decision.destination());
}

#[test]
fn retry_layer_does_not_choose_a_destination_by_itself() {
    let operation = operation_id("retry-no-routing");
    let current = attempt(&operation, "retry-no-routing-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();

    let (next, _) = admission(&retryable_policy(), &cancellation, None)
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-no-routing-a2"),
            jitter_source: None,
            safety_gates: gates(),
        })
        .unwrap();

    assert_eq!(next.operation_id(), &operation);
    assert_eq!(next.attempt_number(), 2);
}

#[test]
fn routing_failure_does_not_mutate_retry_attempt_state() {
    let operation = operation_id("retry-routing-failure");
    let attempt = attempt(&operation, "retry-routing-failure-a1", 1);

    let contract = ContractDescriptor::new(
        ContractId::new("retry.failure.contract").unwrap(),
        CapabilityId::new("retry.failure.capability").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    let destination = nizaam_core::control_plane::DestinationRequest::hard_logical(
        nizaam_core::control_plane::CapabilityRequirement::new(
            CapabilityId::new("retry.failure.capability").unwrap(),
        ),
    );

    let membership = nizaam_core::control_plane::Membership::new();
    let observations = nizaam_core::control_plane::Observations::new();

    let result = nizaam_core::control_plane::eligible_destinations(
        nizaam_core::control_plane::DestinationEligibilityInput::new(
            &destination,
            &membership.snapshot(),
            &observations.snapshot(),
            &CapabilityId::new("retry.failure.capability").unwrap(),
            &contract,
        ),
    );

    assert!(matches!(
        result,
        Err(nizaam_core::control_plane::DestinationEligibilityError::NoEligibleDestination)
    ));
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    assert_eq!(attempt.attempt_number(), 1);
}

#[test]
fn artifact_result_reference_can_be_recorded_without_duplication() {
    let artifact = ArtifactReference::with_selector(
        ArtifactId::new("retry.artifact").unwrap(),
        VersionSelector::exact("v1"),
    );
    assert!(artifact.is_exact());
    assert!(artifact.is_valid());

    let store = IdempotencyStateStore::new();
    let id = identity("artifact-scope", "artifact-key");
    let operation = operation_id("artifact-retry-operation");
    let capability = CapabilityId::new("artifact-producing").unwrap();

    store
        .reserve(
            record(id.clone(), operation.clone(), Some(capability.clone())),
            1,
        )
        .unwrap();

    let reference = serde_json::to_string(&artifact).unwrap();
    let completed = store
        .transition(
            &id,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some(reference.clone()),
        )
        .unwrap();

    let duplicate = store
        .reserve(record(id, operation, Some(capability)), 2)
        .unwrap();

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        panic!("expected artifact-producing duplicate");
    };

    assert_eq!(existing, completed);
    assert_eq!(existing.result_reference(), Some(reference.as_str()));
}

#[test]
fn streaming_output_blocks_retry_when_observable() {
    let operation = operation("retry-conformance-10");
    let context = engine_context("retry-conformance-10").for_attempt(
        nizaam_core::identity::NodeId::new("retry-node").unwrap(),
        attempt_id("retry-conformance-10-a1"),
    );

    let stream = nizaam_core::streaming::Stream::new(
        &context,
        nizaam_core::streaming::BackpressureConfig::new(
            2,
            nizaam_core::streaming::BackpressurePolicy::Reject,
        )
        .unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream
        .publish(nizaam_core::streaming::StreamItem::partial(0, 7))
        .unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7);
    assert!(stream.has_observable_output());
    stream.fail().unwrap();

    let current = attempt(&operation.id, "retry-conformance-10-a1", 1);
    current.start().unwrap();
    current.fail().unwrap();

    let mut budget = RetryBudget::new(1);
    let cancellation = CancellationToken::new();
    let result =
        admission(&retryable_policy(), &cancellation, None).admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("retry-conformance-10-a2"),
            jitter_source: None,
            safety_gates: RetrySafetyGates::new(true, true, true, !stream.has_observable_output()),
        });

    assert!(matches!(
        result,
        Err(RetryAdmissionError::SafetyGate(
            RetrySafetyGate::ObservableOutput
        ))
    ));
}
