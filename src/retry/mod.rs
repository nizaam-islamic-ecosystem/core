//! Retry primitives for safe repeat execution of one logical operation.
//!
//! The retry module composes attempt identity/lifecycle, retry policy and budget
//! evaluation, and bounded backoff/jitter calculation. Higher-level execution
//! is responsible for cancellation, deadlines, resources, idempotency,
//! external effects, and creation/scheduling of subsequent attempts.

pub mod attempt;
pub mod backoff;
pub mod policy;

pub use attempt::{
    Attempt, AttemptCreationError, AttemptLifecycle, AttemptLifecycleError, AttemptLifecycleState,
    can_transition,
};
pub use backoff::{BackoffError, BackoffPolicy, BackoffPolicyError, JitterPolicy, JitterSource};
pub use policy::{
    FailureCategory, RetryBudget, RetryBudgetError, RetryDecision, RetryDenialReason, RetryPolicy,
    RetryPolicyError,
};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        identity::{AttemptId, OperationId},
        status::Retryability,
    };

    use super::*;

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn attempt_id(value: &str) -> AttemptId {
        AttemptId::new(value).unwrap()
    }

    fn retryable_policy(max_retries: u32, max_attempts: u32) -> RetryPolicy {
        RetryPolicy::new(max_retries, max_attempts)
            .unwrap()
            .with_retryable_category(FailureCategory::Transient)
    }

    #[derive(Clone, Copy, Debug)]
    struct FixedJitterSource {
        sample: u64,
    }

    impl JitterSource for FixedJitterSource {
        fn next_u64(&mut self) -> u64 {
            self.sample
        }
    }

    #[test]
    fn module_exports_all_retry_primitives() {
        let operation = operation_id("retry-module-operation");
        let attempt = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
        let policy = retryable_policy(2, 3);
        let budget = RetryBudget::new(2);
        let backoff = BackoffPolicy::new(
            Duration::from_millis(100),
            Duration::from_millis(1_000),
            JitterPolicy::None,
        )
        .unwrap();

        assert_eq!(attempt.operation_id(), &operation);
        assert_eq!(policy.max_retries(), 2);
        assert_eq!(budget.remaining(), 2);
        assert_eq!(backoff.delay(1, None).unwrap(), Duration::from_millis(100));
    }

    #[test]
    fn failed_first_attempt_can_create_a_second_attempt_under_same_operation() {
        let operation = operation_id("operation-1");
        let first = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
        first.start().unwrap();
        first.fail().unwrap();

        let policy = retryable_policy(2, 3);
        assert_eq!(
            policy.evaluate(
                FailureCategory::Transient,
                Retryability::Retryable,
                first.attempt_number(),
            ),
            RetryDecision::Permitted
        );

        let second = Attempt::new(operation.clone(), attempt_id("attempt-2"), 2).unwrap();

        assert_eq!(first.operation_id(), second.operation_id());
        assert_ne!(first.attempt_id(), second.attempt_id());
        assert_eq!(first.attempt_number(), 1);
        assert_eq!(second.attempt_number(), 2);
        assert_eq!(first.state(), AttemptLifecycleState::Failed);
        assert_eq!(second.state(), AttemptLifecycleState::Created);
    }

    #[test]
    fn permitted_retry_consumes_budget_before_second_attempt() {
        let operation = operation_id("operation-2");
        let first = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
        first.start().unwrap();
        first.fail().unwrap();

        let policy = retryable_policy(2, 3);
        let mut budget = RetryBudget::new(1);

        assert_eq!(
            policy.evaluate(
                FailureCategory::Transient,
                Retryability::Retryable,
                first.attempt_number(),
            ),
            RetryDecision::Permitted
        );
        assert_eq!(budget.consumed(), 0);

        budget.try_consume().unwrap();
        assert_eq!(budget.consumed(), 1);
        assert_eq!(budget.remaining(), 0);

        let second = Attempt::new(operation, attempt_id("attempt-2"), 2).unwrap();
        assert_eq!(second.state(), AttemptLifecycleState::Created);
    }

    #[test]
    fn denied_retry_does_not_consume_budget_or_enter_a_new_attempt() {
        let first = Attempt::new(operation_id("operation-3"), attempt_id("attempt-1"), 1).unwrap();
        first.start().unwrap();
        first.fail().unwrap();

        let policy = RetryPolicy::new(2, 3)
            .unwrap()
            .with_retryable_category(FailureCategory::Transient);
        let budget = RetryBudget::new(1);

        let decision = policy.evaluate(
            FailureCategory::Validation,
            Retryability::Retryable,
            first.attempt_number(),
        );

        assert_eq!(
            decision,
            RetryDecision::Denied(RetryDenialReason::CategoryNotAllowed)
        );
        assert_eq!(budget.consumed(), 0);
    }

    #[test]
    fn backoff_uses_retry_number_between_attempts() {
        let backoff = BackoffPolicy::new(
            Duration::from_millis(100),
            Duration::from_millis(500),
            JitterPolicy::None,
        )
        .unwrap();

        assert_eq!(backoff.delay(1, None).unwrap(), Duration::from_millis(100));
        assert_eq!(backoff.delay(2, None).unwrap(), Duration::from_millis(200));
        assert_eq!(backoff.delay(3, None).unwrap(), Duration::from_millis(400));
        assert_eq!(backoff.delay(4, None).unwrap(), Duration::from_millis(500));
    }

    #[test]
    fn policy_and_backoff_form_a_bounded_sequential_retry_flow() {
        let operation = operation_id("operation-4");
        let policy = retryable_policy(3, 4);
        let backoff = BackoffPolicy::new(
            Duration::from_millis(50),
            Duration::from_millis(150),
            JitterPolicy::None,
        )
        .unwrap();
        let mut budget = RetryBudget::new(3);

        let first = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
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
        assert_eq!(backoff.delay(1, None).unwrap(), Duration::from_millis(50));

        let second = Attempt::new(operation.clone(), attempt_id("attempt-2"), 2).unwrap();
        second.start().unwrap();
        second.fail().unwrap();

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

        let third = Attempt::new(operation.clone(), attempt_id("attempt-3"), 3).unwrap();
        third.start().unwrap();
        third.succeed().unwrap();

        assert_eq!(third.operation_id(), &operation);
        assert_eq!(third.state(), AttemptLifecycleState::Succeeded);
        assert_eq!(budget.remaining(), 1);
    }

    #[test]
    fn maximum_attempts_stops_the_next_attempt() {
        let policy = retryable_policy(10, 2);
        let first = Attempt::new(operation_id("operation-5"), attempt_id("attempt-1"), 1).unwrap();
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

        let second =
            Attempt::new(first.operation_id().clone(), attempt_id("attempt-2"), 2).unwrap();
        second.start().unwrap();
        second.fail().unwrap();

        assert_eq!(
            policy.evaluate(
                FailureCategory::Transient,
                Retryability::Retryable,
                second.attempt_number(),
            ),
            RetryDecision::Denied(RetryDenialReason::MaximumAttemptsExceeded)
        );
    }

    #[test]
    fn unknown_outcome_cannot_enter_the_retry_flow_even_when_policy_lists_it() {
        let policy = RetryPolicy::new(3, 4)
            .unwrap()
            .with_retryable_category(FailureCategory::Unknown);

        assert_eq!(
            policy.evaluate(FailureCategory::Unknown, Retryability::Retryable, 1,),
            RetryDecision::Denied(RetryDenialReason::UnknownOutcome)
        );
    }

    #[test]
    fn cancellation_ends_attempt_and_blocks_retry() {
        let attempt =
            Attempt::new(operation_id("operation-6"), attempt_id("attempt-1"), 1).unwrap();
        attempt.start().unwrap();
        attempt.cancel().unwrap();

        let policy = retryable_policy(2, 3);

        assert_eq!(attempt.state(), AttemptLifecycleState::Cancelled);
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
    fn jittered_backoff_remains_bounded_for_a_permitted_retry() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::new(
            Duration::from_millis(100),
            Duration::from_millis(400),
            JitterPolicy::Full,
        )
        .unwrap();
        let first = Attempt::new(operation_id("operation-7"), attempt_id("attempt-1"), 1).unwrap();
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

        let mut source = FixedJitterSource {
            sample: u64::MAX / 2,
        };
        let delay = backoff.delay(1, Some(&mut source)).unwrap();

        assert!(delay <= Duration::from_millis(100));
        assert!(delay > Duration::ZERO);
    }

    #[test]
    fn second_attempt_can_complete_after_first_attempt_fails() {
        let operation = operation_id("operation-8");
        let first = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
        first.start().unwrap();
        first.fail().unwrap();

        let policy = retryable_policy(1, 2);
        assert_eq!(
            policy.evaluate(
                FailureCategory::Transient,
                Retryability::Retryable,
                first.attempt_number(),
            ),
            RetryDecision::Permitted
        );

        let second = Attempt::new(operation, attempt_id("attempt-2"), 2).unwrap();
        second.start().unwrap();
        second.succeed().unwrap();

        assert_eq!(first.state(), AttemptLifecycleState::Failed);
        assert_eq!(second.state(), AttemptLifecycleState::Succeeded);
        assert!(second.is_terminal());
    }
}
