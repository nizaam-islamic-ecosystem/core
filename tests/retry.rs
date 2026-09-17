use std::time::Duration;

use nizaam_core::{
    artifact::{ArtifactReference, VersionSelector},
    capability::CapabilityDefinition,
    contracts::Version,
    error::{ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, Severity},
    idempotency::{
        IdempotencyIdentity, IdempotencyKey, IdempotencyRecord, IdempotencyReservation,
        IdempotencyScope, IdempotencyState, IdempotencyStateStore, RecordedOutcome,
    },
    identity::{ArtifactId, AttemptId, CapabilityId, CorrelationId, EngineId, NodeId, OperationId},
    observability::CorrelationContext,
    operation::{Operation, OperationContext},
    retry::{
        Attempt, AttemptLifecycleState, BackoffPolicy, FailureCategory, JitterPolicy, RetryBudget,
        RetryDecision, RetryDenialReason, RetryPolicy,
    },
    runtime::EngineContext,
    status::{Retryability, Status},
    streaming::{BackpressureConfig, BackpressurePolicy, Stream, StreamItem},
};

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}

fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).unwrap()
}

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn operation(value: &str) -> Operation {
    Operation::new(
        operation_id(value),
        CorrelationId::new(format!("correlation-{value}")).unwrap(),
    )
}

fn engine_context(value: &str) -> EngineContext {
    EngineContext::new(OperationContext::new(operation(value)))
}

fn attempt(operation_id: OperationId, attempt_id_value: &str, attempt_number: u32) -> Attempt {
    Attempt::new(operation_id, attempt_id(attempt_id_value), attempt_number).unwrap()
}

fn retryable_policy(max_retries: u32, max_attempts: u32) -> RetryPolicy {
    RetryPolicy::new(max_retries, max_attempts)
        .unwrap()
        .with_retryable_category(FailureCategory::Transient)
}

fn error_definition(retryability: Retryability) -> ErrorDefinition {
    ErrorDefinition::new(
        ErrorCode::new("CORE.EXECUTION.001").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        Version::new(1, 0, 0),
        ErrorClass::Execution,
        Severity::Error,
        "retry integration test failure",
        retryability,
    )
    .unwrap()
}

fn idempotency_identity(scope: &str, key: &str) -> IdempotencyIdentity {
    IdempotencyIdentity::new(
        IdempotencyScope::new(scope).unwrap(),
        IdempotencyKey::new(key).unwrap(),
    )
}

fn idempotency_record(
    identity: IdempotencyIdentity,
    operation_id: OperationId,
    capability_id: Option<CapabilityId>,
) -> IdempotencyRecord {
    IdempotencyRecord::new(
        identity,
        operation_id,
        capability_id,
        IdempotencyState::InFlight,
        None,
        None,
        1_000,
    )
}

#[test]
fn attempt_identity_flows_through_engine_context() {
    let operation = operation("retry-operation-1");
    let operation_context = OperationContext::new(operation.clone());
    let engine = EngineContext::new(operation_context);

    let attempt = Attempt::new(
        operation_id("retry-operation-1"),
        attempt_id("attempt-1"),
        1,
    )
    .unwrap();

    let attempt_context = engine.for_attempt(node_id("node-1"), attempt.attempt_id().clone());

    assert_eq!(
        attempt_context.operation().operation.id,
        *attempt.operation_id()
    );
    assert_eq!(
        attempt_context.operation().attempt_id,
        Some(attempt.attempt_id().clone())
    );
    assert_eq!(attempt_context.operation().node_id, Some(node_id("node-1")));
}

#[test]
fn retryability_from_error_definition_is_consumed_by_retry_policy() {
    let retryable_error = error_definition(Retryability::Retryable);
    let policy = retryable_policy(2, 3);

    assert_eq!(
        policy.evaluate(FailureCategory::Transient, retryable_error.retryability, 1,),
        RetryDecision::Permitted
    );

    let non_retryable_error = error_definition(Retryability::NonRetryable);

    assert_eq!(
        policy.evaluate(
            FailureCategory::Transient,
            non_retryable_error.retryability,
            1,
        ),
        RetryDecision::Denied(RetryDenialReason::NonRetryable)
    );
}

#[test]
fn sequential_retry_progression_uses_attempt_budget_and_backoff() {
    let operation = operation_id("retry-operation-2");
    let policy = retryable_policy(3, 4);
    let backoff = BackoffPolicy::new(
        Duration::from_millis(50),
        Duration::from_millis(200),
        JitterPolicy::None,
    )
    .unwrap();
    let mut budget = RetryBudget::new(2);

    let first = attempt(operation.clone(), "attempt-1", 1);
    first.start().unwrap();
    first.fail().unwrap();

    assert_eq!(first.state(), AttemptLifecycleState::Failed);

    assert_eq!(
        policy.evaluate(
            FailureCategory::Transient,
            Retryability::Retryable,
            first.attempt_number(),
        ),
        RetryDecision::Permitted
    );

    budget.try_consume().unwrap();

    assert_eq!(backoff.delay(1, None).unwrap(), Duration::from_millis(50));

    let second = attempt(operation.clone(), "attempt-2", 2);
    second.start().unwrap();
    second.fail().unwrap();

    assert_eq!(second.state(), AttemptLifecycleState::Failed);

    assert_eq!(
        policy.evaluate(
            FailureCategory::Transient,
            Retryability::Retryable,
            second.attempt_number(),
        ),
        RetryDecision::Permitted
    );

    budget.try_consume().unwrap();

    assert_eq!(backoff.delay(2, None).unwrap(), Duration::from_millis(100));

    let third = attempt(operation.clone(), "attempt-3", 3);
    third.start().unwrap();
    third.succeed().unwrap();

    assert_eq!(third.state(), AttemptLifecycleState::Succeeded);
    assert_eq!(first.operation_id(), third.operation_id());
    assert_eq!(first.attempt_number(), 1);
    assert_eq!(second.attempt_number(), 2);
    assert_eq!(third.attempt_number(), 3);
    assert_ne!(first.attempt_id(), third.attempt_id());
    assert_eq!(budget.consumed(), 2);
}

#[test]
fn exhausted_retry_budget_prevents_next_attempt() {
    let operation = operation_id("retry-operation-3");
    let policy = retryable_policy(3, 4);
    let mut budget = RetryBudget::new(1);

    let first = attempt(operation.clone(), "attempt-1", 1);
    first.start().unwrap();
    first.fail().unwrap();

    assert_eq!(
        policy.evaluate(
            FailureCategory::Transient,
            Retryability::Retryable,
            first.attempt_number(),
        ),
        RetryDecision::Permitted
    );

    budget.try_consume().unwrap();

    assert!(budget.is_exhausted());
    assert_eq!(budget.remaining(), 0);

    assert!(budget.try_consume().is_err());
}

#[test]
fn operation_deadline_prevents_retry() {
    let engine = engine_context("retry-operation-4")
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::ZERO).unwrap());

    let attempt = Attempt::new(
        engine.operation().operation.id.clone(),
        attempt_id("attempt-1"),
        1,
    )
    .unwrap();

    let attempt_context = engine.for_attempt(node_id("node-1"), attempt.attempt_id().clone());

    assert!(attempt_context.is_expired());

    let policy = retryable_policy(2, 3);

    assert_eq!(
        policy.evaluate(
            FailureCategory::Deadline,
            Retryability::Retryable,
            attempt.attempt_number(),
        ),
        RetryDecision::Denied(RetryDenialReason::Deadline)
    );
}

#[test]
fn cancellation_prevents_retry() {
    let engine = engine_context("retry-operation-5");

    let attempt = Attempt::new(
        engine.operation().operation.id.clone(),
        attempt_id("attempt-1"),
        1,
    )
    .unwrap();

    let attempt_context = engine.for_attempt(node_id("node-1"), attempt.attempt_id().clone());

    engine.cancellation().cancel();

    assert!(attempt_context.cancellation().is_cancelled());

    attempt.start().unwrap();
    attempt.cancel().unwrap();

    let policy = retryable_policy(2, 3);

    assert_eq!(
        policy.evaluate(
            FailureCategory::Cancelled,
            Retryability::Retryable,
            attempt.attempt_number(),
        ),
        RetryDecision::Denied(RetryDenialReason::Cancelled)
    );
}

#[test]
fn retry_attempt_identity_reaches_observability_correlation() {
    let engine = engine_context("retry-operation-6");

    let first = engine.for_attempt(node_id("node-1"), attempt_id("attempt-1"));
    let second = engine.for_attempt(node_id("node-1"), attempt_id("attempt-2"));

    let first_correlation = CorrelationContext::from_operation_context(first.operation());
    let second_correlation = CorrelationContext::from_operation_context(second.operation());

    assert_eq!(
        first_correlation.operation_id(),
        second_correlation.operation_id()
    );
    assert_eq!(
        first_correlation.operation_id(),
        Some(&operation_id("retry-operation-6"))
    );
    assert_eq!(
        first_correlation.attempt_id(),
        Some(&attempt_id("attempt-1"))
    );
    assert_eq!(
        second_correlation.attempt_id(),
        Some(&attempt_id("attempt-2"))
    );
    assert_ne!(
        first_correlation.attempt_id(),
        second_correlation.attempt_id()
    );
}

#[test]
fn retry_preserves_one_idempotent_logical_operation_identity_capability_and_result() {
    let operation = operation_id("retry-operation-7");
    let identity = idempotency_identity("service-a", "request-1");

    let capability = CapabilityDefinition::new(
        CapabilityId::new("capability.retryable-operation").unwrap(),
        EngineId::new("engine-retry").unwrap(),
        "Retryable Operation",
    )
    .unwrap()
    .with_version(Version::new(1, 0, 0));

    let capability_id = capability.capability_id().clone();

    let artifact_reference = ArtifactReference::with_selector(
        ArtifactId::new("artifact.retry-result").unwrap(),
        VersionSelector::exact("v1"),
    );

    assert!(artifact_reference.is_exact());
    assert!(artifact_reference.is_valid());

    let artifact_result_reference = serde_json::to_string(&artifact_reference).unwrap();

    let store = IdempotencyStateStore::new();

    let record = idempotency_record(
        identity.clone(),
        operation.clone(),
        Some(capability_id.clone()),
    );

    assert_eq!(
        store.reserve(record.clone(), 100).unwrap(),
        IdempotencyReservation::Created(record)
    );

    let first_attempt = attempt(operation.clone(), "attempt-1", 1);
    first_attempt.start().unwrap();
    first_attempt.fail().unwrap();

    let completed = store
        .transition(
            &identity,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some(artifact_result_reference.clone()),
        )
        .unwrap();

    assert_eq!(*completed.state(), IdempotencyState::Succeeded);
    assert_eq!(completed.capability_id(), Some(&capability_id));
    assert_eq!(
        completed.result_reference(),
        Some(artifact_result_reference.as_str())
    );

    let second_attempt = attempt(operation.clone(), "attempt-2", 2);

    assert_eq!(first_attempt.operation_id(), second_attempt.operation_id());
    assert_ne!(first_attempt.attempt_id(), second_attempt.attempt_id());

    let repeated_record = idempotency_record(identity, operation, Some(capability_id.clone()));

    let repeated_reservation = store.reserve(repeated_record, 200).unwrap();

    let IdempotencyReservation::Duplicate(existing) = repeated_reservation else {
        panic!("expected the repeated logical submission to be classified as a duplicate");
    };

    assert_eq!(existing, completed);
    assert_eq!(existing.operation_id(), second_attempt.operation_id());
    assert_eq!(existing.capability_id(), Some(&capability_id));
    assert_eq!(
        existing.result_reference(),
        Some(artifact_result_reference.as_str())
    );
    assert_eq!(store.len().unwrap(), 1);
}

#[test]
fn externally_observable_stream_output_is_visible_at_retry_boundary() {
    let engine = engine_context("retry-operation-8");

    let attempt_context = engine.for_attempt(node_id("node-1"), attempt_id("attempt-1"));

    let stream: Stream<u32> = Stream::new(
        &attempt_context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 42)).unwrap();

    assert!(!stream.has_observable_output());

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &42);

    assert!(stream.has_observable_output());

    stream.fail().unwrap();

    assert!(stream.has_observable_output());

    let correlation = CorrelationContext::from_operation_context(attempt_context.operation());

    let encoded = serde_json::to_string(&correlation).unwrap();
    let decoded: CorrelationContext = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, correlation);

    assert_eq!(
        correlation.operation_id().unwrap().as_str(),
        "retry-operation-8"
    );
    assert_eq!(correlation.attempt_id().unwrap().as_str(), "attempt-1");

    assert_eq!(
        stream.state(),
        nizaam_core::streaming::StreamLifecycleState::Failed
    );
    assert_eq!(
        consumer.next_item(),
        Err(nizaam_core::streaming::StreamError::Failed)
    );
}
