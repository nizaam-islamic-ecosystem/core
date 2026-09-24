use crate::support::{section, show_arrow, step, success};
use nizaam_core::idempotency::{
    IdempotencyIdentity, IdempotencyKey, IdempotencyRecord, IdempotencyReservation,
    IdempotencyScope, IdempotencyState, IdempotencyStateStore, RecordedOutcome,
};
use nizaam_core::identity::{AttemptId, CapabilityId, OperationId};
use nizaam_core::operation::CancellationToken;
use nizaam_core::retry::{
    Attempt, BackoffPolicy, FailureCategory, RetryAdmission, RetryAdmissionRequest, RetryBudget,
    RetryPolicy, RetrySafetyGates,
};
use nizaam_core::status::{Retryability, Status};

#[test]
fn visual_retry_identity_and_idempotency_boundary() {
    section("NIZAAM CORE — RETRY + IDEMPOTENCY");
    let operation = OperationId::new("visual-logical-operation").unwrap();
    let first_id = AttemptId::new("visual-attempt-1").unwrap();
    let second_id = AttemptId::new("visual-attempt-2").unwrap();
    let first = Attempt::new(operation.clone(), first_id.clone(), 1).unwrap();
    first.start().unwrap();
    first.fail().unwrap();

    step(1, "retry admission");
    let policy = RetryPolicy::new(2, 3)
        .unwrap()
        .with_retryable_category(FailureCategory::Transient);
    let backoff = BackoffPolicy::no_backoff();
    let cancellation = CancellationToken::new();
    let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
    let mut budget = RetryBudget::new(1);
    let (second, delay) = admission
        .admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &first,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: second_id.clone(),
            jitter_source: None,
            safety_gates: RetrySafetyGates::new(true, true, true, true),
        })
        .unwrap();
    assert_eq!(second.operation_id(), &operation);
    assert_ne!(second.attempt_id(), first.attempt_id());
    assert_eq!(second.attempt_number(), 2);
    assert_eq!(delay, std::time::Duration::ZERO);
    println!("  OperationId = SAME  : {}", operation);
    println!("  AttemptId   = A1    : {}", first.attempt_id());
    println!("  AttemptId   = A2    : {}", second.attempt_id());
    println!("  Retry budget used   : {}", budget.consumed());
    show_arrow("Attempt 1 failure", "Retry admission → Attempt 2");
    success("retry changes attempt identity while preserving the logical operation");

    step(2, "independent idempotency identity");
    let identity = IdempotencyIdentity::new(
        IdempotencyScope::new("visual-service").unwrap(),
        IdempotencyKey::new("visual-request-1").unwrap(),
    );
    let capability = CapabilityId::new("visual.side_effect").unwrap();
    let store = IdempotencyStateStore::new();
    let record = IdempotencyRecord::new(
        identity.clone(),
        operation.clone(),
        Some(capability.clone()),
        IdempotencyState::InFlight,
        None,
        None,
        10_000,
    );
    assert!(matches!(
        store.reserve(record, 1).unwrap(),
        IdempotencyReservation::Created(_)
    ));
    let duplicate = IdempotencyRecord::new(
        identity.clone(),
        operation.clone(),
        Some(capability.clone()),
        IdempotencyState::InFlight,
        None,
        None,
        10_000,
    );
    assert!(matches!(
        store.reserve(duplicate, 2).unwrap(),
        IdempotencyReservation::Duplicate(_)
    ));
    println!("  Idempotency scope : {}", identity.scope());
    println!("  Idempotency key   : {}", identity.key());
    assert_eq!(store.len().unwrap(), 1);
    success("repeated logical submission is recognized without conflating idempotency with retry");

    step(3, "terminal outcome and conflict");
    let outcome = RecordedOutcome::new(Status::Success, None);
    let completed = store
        .transition(
            &identity,
            IdempotencyState::Succeeded,
            Some(outcome),
            Some("result:visual-1".to_owned()),
        )
        .unwrap();
    assert_eq!(*completed.state(), IdempotencyState::Succeeded);
    let conflict = IdempotencyRecord::new(
        identity.clone(),
        OperationId::new("different-operation").unwrap(),
        Some(capability),
        IdempotencyState::InFlight,
        None,
        None,
        10_000,
    );
    assert!(matches!(
        store.reserve(conflict, 3).unwrap(),
        IdempotencyReservation::Conflict(_)
    ));
    println!("  terminal state   : {:?}", completed.state());
    println!("  result reference  : {:?}", completed.result_reference());
    success("terminal state is separate from retry policy and incompatible reuse is rejected");
}
