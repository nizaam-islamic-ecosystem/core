//! Retry primitives for safe repeat execution of one logical operation.
//!
//! The retry module owns the ordered retry-admission gate: policy evaluation,
//! cancellation/deadline checks, resource admission, external-effect safety,
//! idempotency/safe-repeat approval, observable-output safety, bounded
//! backoff/jitter calculation, retry-budget consumption, and creation of the
//! next attempt. Higher-level execution supplies the domain-owned gate results
//! and remains responsible for obtaining those results and scheduling the
//! admitted attempt.

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

use crate::{
    identity::AttemptId,
    operation::{CancellationToken, Deadline},
    status::Retryability,
};

/// One of the safety gates that must be cleared before a new retry attempt is created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrySafetyGate {
    /// The caller has not requested cancellation of the logical operation.
    Cancellation,
    /// The operation still has enough deadline remaining for another attempt.
    Deadline,
    /// The required execution resources have been admitted.
    ResourceAdmission,
    /// Repeating the external effect is currently considered safe.
    ExternalEffects,
    /// The idempotency/safe-repeat mechanism permits another side-effecting execution.
    Idempotency,
    /// No externally observable stream output has made automatic retry unsafe.
    ObservableOutput,
}

/// Domain-owned retry safety results supplied to the retry-admission API.
///
/// Cancellation and deadline are deliberately not stored here because they are
/// live execution state. The admission API reads both authoritatively at the
/// moment the retry is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrySafetyGates {
    resource_admitted: bool,
    external_effects_safe: bool,
    idempotency_allows_retry: bool,
    observable_output_safe: bool,
}

impl RetrySafetyGates {
    /// Creates the domain-owned retry safety results that remain valid until
    /// the admission request. Cancellation and deadline are read authoritatively
    /// from their live state at admission time.
    pub const fn new(
        resource_admitted: bool,
        external_effects_safe: bool,
        idempotency_allows_retry: bool,
        observable_output_safe: bool,
    ) -> Self {
        Self {
            resource_admitted,
            external_effects_safe,
            idempotency_allows_retry,
            observable_output_safe,
        }
    }

    /// Returns the first gate that currently prevents retry.
    pub fn first_denied(
        &self,
        cancellation: &CancellationToken,
        deadline: Option<Deadline>,
    ) -> Option<RetrySafetyGate> {
        if cancellation.is_cancelled() {
            Some(RetrySafetyGate::Cancellation)
        } else if deadline.is_some_and(Deadline::is_expired) {
            Some(RetrySafetyGate::Deadline)
        } else if !self.resource_admitted {
            Some(RetrySafetyGate::ResourceAdmission)
        } else if !self.external_effects_safe {
            Some(RetrySafetyGate::ExternalEffects)
        } else if !self.idempotency_allows_retry {
            Some(RetrySafetyGate::Idempotency)
        } else if !self.observable_output_safe {
            Some(RetrySafetyGate::ObservableOutput)
        } else {
            None
        }
    }
}

/// Error returned when the complete retry-admission sequence cannot admit a new attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryAdmissionError {
    /// The retry policy rejected the classified outcome.
    Policy(RetryDenialReason),
    /// One of the required safety gates was not cleared.
    SafetyGate(RetrySafetyGate),
    /// The retry budget could not supply another retry opportunity.
    Budget(RetryBudgetError),
    /// Backoff calculation failed before budget consumption.
    Backoff(BackoffError),
    /// The current attempt is not in the failed state required for a retry.
    CurrentAttemptNotFailed(AttemptLifecycleState),
    /// The current attempt number cannot be incremented safely.
    AttemptNumberOverflow,
    /// The supplied successor identity reuses the current attempt identity.
    CurrentAttemptIdReused,
    /// A successor has already been admitted from this failed attempt.
    SuccessorAlreadyReserved,
    /// Construction of the next attempt failed.
    AttemptCreation(AttemptCreationError),
}

impl std::fmt::Display for RetrySafetyGate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Cancellation => "cancellation",
            Self::Deadline => "deadline",
            Self::ResourceAdmission => "resource admission",
            Self::ExternalEffects => "external-effect safety",
            Self::Idempotency => "idempotency",
            Self::ObservableOutput => "observable-output safety",
        };
        formatter.write_str(name)
    }
}

impl std::fmt::Display for RetryAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Policy(reason) => write!(formatter, "retry policy denied admission: {reason:?}"),
            Self::SafetyGate(gate) => {
                write!(formatter, "retry safety gate denied admission: {gate}")
            }
            Self::Budget(error) => write!(formatter, "retry budget denied admission: {error}"),
            Self::Backoff(error) => write!(formatter, "retry backoff calculation failed: {error}"),
            Self::CurrentAttemptNotFailed(state) => write!(
                formatter,
                "retry requires a failed current attempt, got {state:?}"
            ),
            Self::AttemptNumberOverflow => {
                formatter.write_str("retry attempt number cannot be incremented")
            }
            Self::CurrentAttemptIdReused => {
                formatter.write_str("retry successor must use a new attempt identity")
            }
            Self::SuccessorAlreadyReserved => {
                formatter.write_str("a successor is already reserved for this failed attempt")
            }
            Self::AttemptCreation(error) => {
                write!(formatter, "retry attempt creation failed: {error}")
            }
        }
    }
}

impl std::error::Error for RetryAdmissionError {}

/// Ordered retry-admission coordinator.
///
/// The coordinator reads cancellation and deadline state authoritatively at
/// admission time and receives a fresh set of domain-owned gate results for each
/// admission request. In return, callers get one API that cannot consume the
/// retry budget or create the next attempt until every required safety boundary
/// has passed.
/// Inputs required for one retry-admission request.
///
/// The request groups the retry budget, current attempt, failure classification,
/// retryability, successor identity, jitter source, and domain-owned safety
/// results into one explicit admission request.
pub struct RetryAdmissionRequest<'a> {
    pub budget: &'a mut RetryBudget,
    pub current_attempt: &'a Attempt,
    pub category: FailureCategory,
    pub retryability: Retryability,
    pub next_attempt_id: AttemptId,
    pub jitter_source: Option<&'a mut dyn JitterSource>,
    pub safety_gates: RetrySafetyGates,
}

pub struct RetryAdmission<'a> {
    policy: &'a RetryPolicy,
    backoff: &'a BackoffPolicy,
    cancellation: CancellationToken,
    deadline: Option<Deadline>,
}

impl<'a> RetryAdmission<'a> {
    /// Creates an admission coordinator from the applicable policy, backoff,
    /// and live operation execution state. Domain-owned safety gates are supplied
    /// separately to each admission request.
    pub fn new(
        policy: &'a RetryPolicy,
        backoff: &'a BackoffPolicy,
        cancellation: &CancellationToken,
        deadline: Option<Deadline>,
    ) -> Self {
        Self {
            policy,
            backoff,
            cancellation: cancellation.clone(),
            deadline,
        }
    }

    /// Evaluates the complete retry-admission sequence and, only after every
    /// gate passes, consumes one retry-budget opportunity and creates the next
    /// attempt under the same logical operation identity.
    ///
    /// The returned delay is the bounded delay before executing the admitted
    /// attempt. Actual waiting/scheduling remains the caller's responsibility.
    pub fn admit_next(
        &self,
        request: RetryAdmissionRequest<'_>,
    ) -> Result<(Attempt, std::time::Duration), RetryAdmissionError> {
        let RetryAdmissionRequest {
            budget,
            current_attempt,
            category,
            retryability,
            next_attempt_id,
            jitter_source,
            safety_gates,
        } = request;
        match self
            .policy
            .evaluate(category, retryability, current_attempt.attempt_number())
        {
            RetryDecision::Permitted => {}
            RetryDecision::Denied(reason) => return Err(RetryAdmissionError::Policy(reason)),
        }

        if let Some(gate) = safety_gates.first_denied(&self.cancellation, self.deadline) {
            return Err(RetryAdmissionError::SafetyGate(gate));
        }

        let current_state = current_attempt.state();
        if current_state != AttemptLifecycleState::Failed {
            return Err(RetryAdmissionError::CurrentAttemptNotFailed(current_state));
        }

        let next_attempt_number = current_attempt
            .attempt_number()
            .checked_add(1)
            .ok_or(RetryAdmissionError::AttemptNumberOverflow)?;

        // Construct the successor before reserving the attempt-state successor
        // claim or consuming retry budget. This keeps constructor/backoff
        // failures side-effect free.
        let attempt = Attempt::new(
            current_attempt.operation_id().clone(),
            next_attempt_id.clone(),
            next_attempt_number,
        )
        .map_err(RetryAdmissionError::AttemptCreation)?;

        let delay = self
            .backoff
            .delay(current_attempt.attempt_number(), jitter_source)
            .map_err(RetryAdmissionError::Backoff)?;

        // Recheck live cancellation/deadline state while holding the attempt-state
        // mutex, immediately before budget reservation and successor-claim commit.
        current_attempt
            .try_reserve_successor(&next_attempt_id, || {
                if let Some(gate) = safety_gates.first_denied(&self.cancellation, self.deadline)
                {
                    return Err(RetryAdmissionError::SafetyGate(gate));
                }

                budget
                    .try_consume()
                    .map_err(RetryAdmissionError::Budget)
            })
            .map_err(|error| match error {
                crate::retry::attempt::AttemptSuccessorReservationError::SameAttemptId => {
                    RetryAdmissionError::CurrentAttemptIdReused
                }
                crate::retry::attempt::AttemptSuccessorReservationError::SuccessorAlreadyReserved => {
                    RetryAdmissionError::SuccessorAlreadyReserved
                }
                crate::retry::attempt::AttemptSuccessorReservationError::ReservationFailed(error) => {
                    error
                }
            })?;

        Ok((attempt, delay))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::{
        identity::{AttemptId, OperationId},
        operation::{CancellationToken, Deadline},
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

    fn all_safety_gates_clear() -> RetrySafetyGates {
        RetrySafetyGates::new(true, true, true, true)
    }

    fn failed_attempt(operation: &str, attempt: &str) -> Attempt {
        let attempt = Attempt::new(operation_id(operation), attempt_id(attempt), 1).unwrap();
        attempt.start().unwrap();
        attempt.fail().unwrap();
        attempt
    }

    #[test]
    fn retry_admission_requires_every_static_safety_gate_before_budget_or_attempt() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let current = failed_attempt("admission-gates", "attempt-1");
        let cases = [
            (
                RetrySafetyGate::ResourceAdmission,
                RetrySafetyGates::new(false, true, true, true),
            ),
            (
                RetrySafetyGate::ExternalEffects,
                RetrySafetyGates::new(true, false, true, true),
            ),
            (
                RetrySafetyGate::Idempotency,
                RetrySafetyGates::new(true, true, false, true),
            ),
            (
                RetrySafetyGate::ObservableOutput,
                RetrySafetyGates::new(true, true, true, false),
            ),
        ];

        let cancellation = CancellationToken::new();

        for (index, (expected_gate, safety_gates)) in cases.iter().enumerate() {
            let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
            let mut budget = RetryBudget::new(1);
            let result = admission.admit_next(RetryAdmissionRequest {
                budget: &mut budget,
                current_attempt: &current,
                category: FailureCategory::Transient,
                retryability: Retryability::Retryable,
                next_attempt_id: attempt_id(&format!("attempt-{}", index + 2)),
                jitter_source: None,
                safety_gates: *safety_gates,
            });

            assert!(matches!(
                result,
                Err(RetryAdmissionError::SafetyGate(actual_gate)) if actual_gate == *expected_gate
            ));
            assert_eq!(budget.consumed(), 0);
        }
    }

    #[test]
    fn retry_admission_checks_live_cancellation_state_at_admission_time() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-live-cancellation", "attempt-1");
        cancellation.cancel();
        let mut budget = RetryBudget::new(1);

        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-2"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
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
    fn retry_admission_checks_live_deadline_at_admission_time() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let deadline = Deadline::at(Instant::now());
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, Some(deadline));
        let current = failed_attempt("admission-live-deadline", "attempt-1");
        let mut budget = RetryBudget::new(1);

        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-2"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::SafetyGate(RetrySafetyGate::Deadline))
        ));
        assert_eq!(budget.consumed(), 0);
    }

    #[test]
    fn retry_admission_rechecks_domain_gate_results_for_each_admission() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-fresh-gates", "attempt-1");

        let mut first_budget = RetryBudget::new(1);
        let (first, _) = admission
            .admit_next(RetryAdmissionRequest {
                budget: &mut first_budget,
                current_attempt: &current,
                category: FailureCategory::Transient,
                retryability: Retryability::Retryable,
                next_attempt_id: attempt_id("attempt-2"),
                jitter_source: None,
                safety_gates: all_safety_gates_clear(),
            })
            .unwrap();

        assert_eq!(first_budget.consumed(), 1);
        assert_eq!(first.attempt_number(), 2);

        let mut second_budget = RetryBudget::new(1);
        let changed_gates = RetrySafetyGates::new(false, true, true, true);
        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut second_budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-3"),
            jitter_source: None,
            safety_gates: changed_gates,
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::SafetyGate(
                RetrySafetyGate::ResourceAdmission
            ))
        ));
        assert_eq!(second_budget.consumed(), 0);
    }

    #[test]
    fn retry_admission_creates_next_attempt_after_all_gates_pass() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::new(
            Duration::from_millis(100),
            Duration::from_millis(500),
            JitterPolicy::None,
        )
        .unwrap();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-success", "attempt-1");
        let mut budget = RetryBudget::new(1);

        let (next, delay) = admission
            .admit_next(RetryAdmissionRequest {
                budget: &mut budget,
                current_attempt: &current,
                category: FailureCategory::Transient,
                retryability: Retryability::Retryable,
                next_attempt_id: attempt_id("attempt-2"),
                jitter_source: None,
                safety_gates: all_safety_gates_clear(),
            })
            .unwrap();

        assert_eq!(delay, Duration::from_millis(100));
        assert_eq!(budget.consumed(), 1);
        assert_eq!(next.operation_id(), current.operation_id());
        assert_ne!(next.attempt_id(), current.attempt_id());
        assert_eq!(next.attempt_number(), 2);
        assert_eq!(next.state(), AttemptLifecycleState::Created);
    }

    #[test]
    fn retry_admission_does_not_consume_budget_when_backoff_fails() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::new(
            Duration::from_millis(100),
            Duration::from_millis(500),
            JitterPolicy::Full,
        )
        .unwrap();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-backoff-error", "attempt-1");
        let mut budget = RetryBudget::new(1);

        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-2"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::Backoff(
                BackoffError::JitterSourceRequired
            ))
        ));
        assert_eq!(budget.consumed(), 0);
    }

    #[test]
    fn retry_admission_rejects_a_non_failed_current_attempt_without_consuming_budget() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current =
            Attempt::new(operation_id("admission-state"), attempt_id("attempt-1"), 1).unwrap();
        let mut budget = RetryBudget::new(1);

        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-2"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::CurrentAttemptNotFailed(
                AttemptLifecycleState::Created
            ))
        ));
        assert_eq!(budget.consumed(), 0);
    }

    #[test]
    fn retry_admission_rejects_reused_current_attempt_id_without_consuming_budget() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-reused-id", "attempt-1");
        let mut budget = RetryBudget::new(1);

        let result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: current.attempt_id().clone(),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            result,
            Err(RetryAdmissionError::CurrentAttemptIdReused)
        ));
        assert_eq!(budget.consumed(), 0);
    }

    #[test]
    fn retry_admission_allows_only_one_successor_from_a_failed_attempt() {
        let policy = retryable_policy(3, 4);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-single-successor", "attempt-1");
        let mut budget = RetryBudget::new(2);

        let (first_successor, _) = admission
            .admit_next(RetryAdmissionRequest {
                budget: &mut budget,
                current_attempt: &current,
                category: FailureCategory::Transient,
                retryability: Retryability::Retryable,
                next_attempt_id: attempt_id("attempt-2"),
                jitter_source: None,
                safety_gates: all_safety_gates_clear(),
            })
            .unwrap();

        let second_result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-3"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            second_result,
            Err(RetryAdmissionError::SuccessorAlreadyReserved)
        ));
        assert_eq!(budget.consumed(), 1);
        assert_eq!(first_successor.attempt_number(), 2);
    }

    #[test]
    fn retry_admission_keeps_successor_unclaimed_when_budget_is_exhausted() {
        let policy = retryable_policy(2, 3);
        let backoff = BackoffPolicy::no_backoff();
        let cancellation = CancellationToken::new();
        let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
        let current = failed_attempt("admission-budget-rollback", "attempt-1");
        let mut exhausted_budget = RetryBudget::new(0);

        let first_result = admission.admit_next(RetryAdmissionRequest {
            budget: &mut exhausted_budget,
            current_attempt: &current,
            category: FailureCategory::Transient,
            retryability: Retryability::Retryable,
            next_attempt_id: attempt_id("attempt-2"),
            jitter_source: None,
            safety_gates: all_safety_gates_clear(),
        });

        assert!(matches!(
            first_result,
            Err(RetryAdmissionError::Budget(RetryBudgetError::Exhausted))
        ));
        assert_eq!(exhausted_budget.consumed(), 0);

        let mut available_budget = RetryBudget::new(1);
        let (next, _) = admission
            .admit_next(RetryAdmissionRequest {
                budget: &mut available_budget,
                current_attempt: &current,
                category: FailureCategory::Transient,
                retryability: Retryability::Retryable,
                next_attempt_id: attempt_id("attempt-2"),
                jitter_source: None,
                safety_gates: all_safety_gates_clear(),
            })
            .unwrap();

        assert_eq!(available_budget.consumed(), 1);
        assert_eq!(next.attempt_number(), 2);
    }
}
