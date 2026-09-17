//! Retry policy, retryability classification, and retry-budget semantics.
//!
//! `RetryPolicy` contains the static constraints that determine whether an
//! additional execution attempt is permitted for a classified outcome.
//! `RetryBudget` contains mutable execution-time accounting and is deliberately
//! separate from the policy itself. Backoff, idempotency, cancellation,
//! deadlines, resource admission, and actual retry execution remain outside
//! this module.

use std::collections::BTreeSet;

use crate::status::Retryability;

/// Retry-specific classification of an attempt outcome.
///
/// These categories do not imply retryability by themselves. The applicable
/// [`RetryPolicy`] and the existing [`Retryability`] metadata must both permit
/// another attempt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FailureCategory {
    /// A failure that may resolve without changing the operation contract.
    Transient,
    /// A failure that is expected to remain invalid without a semantic change.
    Permanent,
    /// Execution ended because cancellation was requested.
    Cancelled,
    /// Execution ended because the applicable deadline was reached.
    Deadline,
    /// Authentication failed.
    Authentication,
    /// Authorization failed.
    Authorization,
    /// Input or contract validation failed.
    Validation,
    /// A configured resource limit was exhausted.
    ResourceExhausted,
    /// A dependency failed or became unavailable.
    Dependency,
    /// A transport-layer failure occurred.
    Transport,
    /// Core/runtime execution failed.
    Engine,
    /// The system cannot determine whether the external outcome completed.
    Unknown,
}

/// Result of evaluating whether another attempt is allowed by one retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    /// The policy permits another attempt, subject to later safety gates.
    Permitted,
    /// The policy does not permit another attempt.
    Denied(RetryDenialReason),
}

/// Reason why a retry is denied by policy-level evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDenialReason {
    /// The supplied attempt number is not a valid one-based attempt ordinal.
    InvalidAttemptNumber,
    /// The failure metadata marks the outcome as non-retryable.
    NonRetryable,
    /// The outcome category is not allowed by this policy.
    CategoryNotAllowed,
    /// The configured retry count has been exhausted.
    MaximumRetriesExceeded,
    /// The configured total-attempt count has been exhausted.
    MaximumAttemptsExceeded,
    /// Unknown outcomes require reconciliation or idempotency handling first.
    UnknownOutcome,
    /// Cancellation has higher authority than ordinary retry policy.
    Cancelled,
    /// The operation deadline takes precedence over further retries.
    Deadline,
}

/// Error returned when a retry policy cannot be constructed from its limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyError {
    /// An operation must permit at least its original attempt.
    InvalidMaxAttempts,
}

impl std::fmt::Display for RetryPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMaxAttempts => {
                formatter.write_str("maximum attempts must be greater than zero")
            }
        }
    }
}

impl std::error::Error for RetryPolicyError {}

/// Static limits and retryability constraints for an operation, node, or
/// capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_retries: u32,
    max_attempts: u32,
    retryable_categories: BTreeSet<FailureCategory>,
}

impl RetryPolicy {
    /// Creates a retry policy with no retryable categories configured.
    ///
    /// `max_retries` counts retries after the original attempt, while
    /// `max_attempts` counts all concrete attempts including the original.
    pub fn new(max_retries: u32, max_attempts: u32) -> Result<Self, RetryPolicyError> {
        if max_attempts == 0 {
            return Err(RetryPolicyError::InvalidMaxAttempts);
        }

        Ok(Self {
            max_retries,
            max_attempts,
            retryable_categories: BTreeSet::new(),
        })
    }

    /// Returns a copy of this policy with one additional retryable category.
    pub fn with_retryable_category(mut self, category: FailureCategory) -> Self {
        self.retryable_categories.insert(category);
        self
    }

    /// Returns a copy of this policy with the supplied retryable categories.
    pub fn with_retryable_categories<I>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = FailureCategory>,
    {
        self.retryable_categories.extend(categories);
        self
    }

    /// Returns the maximum number of retries after the original attempt.
    pub const fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Returns the maximum total number of concrete attempts.
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Returns the effective total-attempt bound implied by both limits.
    ///
    /// `max_retries` is converted to a total-attempt count with saturating
    /// arithmetic so `u32::MAX` cannot overflow.
    pub const fn effective_max_attempts(&self) -> u32 {
        let retry_limit = self.max_retries.saturating_add(1);

        if self.max_attempts < retry_limit {
            self.max_attempts
        } else {
            retry_limit
        }
    }

    /// Returns the categories explicitly permitted by this policy.
    pub fn retryable_categories(&self) -> &BTreeSet<FailureCategory> {
        &self.retryable_categories
    }

    /// Returns whether this policy explicitly permits the supplied category.
    pub fn allows_category(&self, category: FailureCategory) -> bool {
        self.retryable_categories.contains(&category)
    }

    /// Combines this policy with a more restrictive constraint.
    ///
    /// The effective policy is the conservative intersection of both inputs:
    /// numeric limits use the minimum and retryable categories use set
    /// intersection. A less restrictive policy therefore cannot broaden a
    /// restriction imposed by another applicable policy.
    pub fn constrain_with(&self, constraint: &Self) -> Self {
        let retryable_categories = self
            .retryable_categories
            .intersection(&constraint.retryable_categories)
            .copied()
            .collect();

        Self {
            max_retries: self.max_retries.min(constraint.max_retries),
            max_attempts: self.max_attempts.min(constraint.max_attempts),
            retryable_categories,
        }
    }

    /// Evaluates the policy for a classified outcome and one-based attempt
    /// number.
    ///
    /// A permitted result only means that the policy layer allows another
    /// attempt. Cancellation, deadlines, resource capacity, external effects,
    /// idempotency, backoff, and other Phase 13 safety gates are evaluated by
    /// their owning mechanisms before a new attempt is created.
    pub fn evaluate(
        &self,
        category: FailureCategory,
        retryability: Retryability,
        attempt_number: u32,
    ) -> RetryDecision {
        if attempt_number == 0 {
            return RetryDecision::Denied(RetryDenialReason::InvalidAttemptNumber);
        }

        match category {
            FailureCategory::Unknown => {
                return RetryDecision::Denied(RetryDenialReason::UnknownOutcome);
            }
            FailureCategory::Cancelled => {
                return RetryDecision::Denied(RetryDenialReason::Cancelled);
            }
            FailureCategory::Deadline => {
                return RetryDecision::Denied(RetryDenialReason::Deadline);
            }
            _ => {}
        }

        if retryability == Retryability::NonRetryable {
            return RetryDecision::Denied(RetryDenialReason::NonRetryable);
        }

        if !self.allows_category(category) {
            return RetryDecision::Denied(RetryDenialReason::CategoryNotAllowed);
        }

        if attempt_number >= self.max_attempts {
            return RetryDecision::Denied(RetryDenialReason::MaximumAttemptsExceeded);
        }

        if attempt_number > self.max_retries {
            return RetryDecision::Denied(RetryDenialReason::MaximumRetriesExceeded);
        }

        RetryDecision::Permitted
    }
}

/// Mutable retry accounting for one retry budget scope.
///
/// A budget is separate from [`RetryPolicy`]: policy expresses static
/// constraints, while this type tracks consumption during execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryBudget {
    limit: u32,
    consumed: u32,
}

impl RetryBudget {
    /// Creates an unconsumed retry budget with the supplied limit.
    pub const fn new(limit: u32) -> Self {
        Self { limit, consumed: 0 }
    }

    /// Returns the configured retry-budget limit.
    pub const fn limit(&self) -> u32 {
        self.limit
    }

    /// Returns the number of retry opportunities already consumed.
    pub const fn consumed(&self) -> u32 {
        self.consumed
    }

    /// Returns the number of retry opportunities remaining.
    pub const fn remaining(&self) -> u32 {
        self.limit - self.consumed
    }

    /// Returns whether the budget has no retry opportunities remaining.
    pub const fn is_exhausted(&self) -> bool {
        self.consumed >= self.limit
    }

    /// Consumes one retry opportunity.
    pub fn try_consume(&mut self) -> Result<(), RetryBudgetError> {
        if self.is_exhausted() {
            return Err(RetryBudgetError::Exhausted);
        }

        self.consumed += 1;
        Ok(())
    }
}

/// Error returned when a retry budget cannot satisfy another consumption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryBudgetError {
    /// The configured number of retry opportunities has already been consumed.
    Exhausted,
}

impl std::fmt::Display for RetryBudgetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted => formatter.write_str("retry budget exhausted"),
        }
    }
}

impl std::error::Error for RetryBudgetError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn retryable_policy(max_retries: u32, max_attempts: u32) -> RetryPolicy {
        RetryPolicy::new(max_retries, max_attempts)
            .unwrap()
            .with_retryable_categories([
                FailureCategory::Transient,
                FailureCategory::Dependency,
                FailureCategory::Transport,
            ])
    }

    #[test]
    fn retry_policy_rejects_zero_max_attempts() {
        assert_eq!(
            RetryPolicy::new(0, 0),
            Err(RetryPolicyError::InvalidMaxAttempts)
        );
    }

    #[test]
    fn retry_policy_starts_without_retryable_categories() {
        let policy = RetryPolicy::new(3, 4).unwrap();

        assert_eq!(policy.max_retries(), 3);
        assert_eq!(policy.max_attempts(), 4);
        assert_eq!(policy.effective_max_attempts(), 4);
        assert!(policy.retryable_categories().is_empty());
    }

    #[test]
    fn retryable_categories_can_be_added() {
        let policy = RetryPolicy::new(3, 4)
            .unwrap()
            .with_retryable_category(FailureCategory::Transient)
            .with_retryable_category(FailureCategory::Transport);

        assert!(policy.allows_category(FailureCategory::Transient));
        assert!(policy.allows_category(FailureCategory::Transport));
        assert!(!policy.allows_category(FailureCategory::Validation));
    }

    #[test]
    fn retry_is_permitted_for_allowed_retryable_category() {
        let policy = retryable_policy(2, 3);

        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 1,),
            RetryDecision::Permitted
        );
    }

    #[test]
    fn non_retryable_metadata_overrides_allowed_category() {
        let policy = retryable_policy(2, 3);

        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::NonRetryable, 1,),
            RetryDecision::Denied(RetryDenialReason::NonRetryable)
        );
    }

    #[test]
    fn disallowed_category_is_rejected() {
        let policy = retryable_policy(2, 3);

        assert_eq!(
            policy.evaluate(FailureCategory::Validation, Retryability::Retryable, 1,),
            RetryDecision::Denied(RetryDenialReason::CategoryNotAllowed)
        );
    }

    #[test]
    fn unknown_outcome_is_rejected() {
        let policy = RetryPolicy::new(3, 4)
            .unwrap()
            .with_retryable_category(FailureCategory::Unknown);

        assert_eq!(
            policy.evaluate(FailureCategory::Unknown, Retryability::Retryable, 1),
            RetryDecision::Denied(RetryDenialReason::UnknownOutcome)
        );
    }

    #[test]
    fn cancellation_and_deadline_are_rejected() {
        let policy = retryable_policy(3, 4);

        assert_eq!(
            policy.evaluate(FailureCategory::Cancelled, Retryability::Retryable, 1,),
            RetryDecision::Denied(RetryDenialReason::Cancelled)
        );
        assert_eq!(
            policy.evaluate(FailureCategory::Deadline, Retryability::Retryable, 1,),
            RetryDecision::Denied(RetryDenialReason::Deadline)
        );
    }

    #[test]
    fn maximum_retries_is_enforced() {
        let policy = retryable_policy(1, 10);

        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 1,),
            RetryDecision::Permitted
        );
        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 2,),
            RetryDecision::Denied(RetryDenialReason::MaximumRetriesExceeded)
        );
    }

    #[test]
    fn maximum_attempts_is_enforced() {
        let policy = retryable_policy(10, 2);

        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 1,),
            RetryDecision::Permitted
        );
        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 2,),
            RetryDecision::Denied(RetryDenialReason::MaximumAttemptsExceeded)
        );
    }

    #[test]
    fn effective_attempt_limit_uses_the_strictest_bound() {
        let retries_stricter = RetryPolicy::new(2, 10).unwrap();
        assert_eq!(retries_stricter.effective_max_attempts(), 3);

        let attempts_stricter = RetryPolicy::new(10, 2).unwrap();
        assert_eq!(attempts_stricter.effective_max_attempts(), 2);
    }

    #[test]
    fn combining_policies_is_conservative() {
        let operation = RetryPolicy::new(5, 6)
            .unwrap()
            .with_retryable_categories([FailureCategory::Transient, FailureCategory::Dependency]);
        let capability = RetryPolicy::new(1, 2)
            .unwrap()
            .with_retryable_categories([FailureCategory::Transient, FailureCategory::Transport]);

        let effective = operation.constrain_with(&capability);

        assert_eq!(effective.max_retries(), 1);
        assert_eq!(effective.max_attempts(), 2);
        assert_eq!(
            effective
                .retryable_categories()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![FailureCategory::Transient]
        );
        assert!(effective.allows_category(FailureCategory::Transient));
        assert!(!effective.allows_category(FailureCategory::Dependency));
        assert!(!effective.allows_category(FailureCategory::Transport));
    }

    #[test]
    fn empty_category_intersection_forbids_retry() {
        let first = RetryPolicy::new(2, 3)
            .unwrap()
            .with_retryable_category(FailureCategory::Transient);
        let second = RetryPolicy::new(2, 3)
            .unwrap()
            .with_retryable_category(FailureCategory::Transport);

        let effective = first.constrain_with(&second);

        assert!(effective.retryable_categories().is_empty());
        assert_eq!(
            effective.evaluate(FailureCategory::Transient, Retryability::Retryable, 1,),
            RetryDecision::Denied(RetryDenialReason::CategoryNotAllowed)
        );
    }

    #[test]
    fn invalid_zero_attempt_number_is_rejected() {
        let policy = retryable_policy(2, 3);

        assert_eq!(
            policy.evaluate(FailureCategory::Transient, Retryability::Retryable, 0,),
            RetryDecision::Denied(RetryDenialReason::InvalidAttemptNumber)
        );
    }

    #[test]
    fn maximum_retry_count_does_not_overflow() {
        let policy = RetryPolicy::new(u32::MAX, u32::MAX).unwrap();

        assert_eq!(policy.effective_max_attempts(), u32::MAX);
    }

    #[test]
    fn retry_budget_tracks_consumption() {
        let mut budget = RetryBudget::new(2);

        assert_eq!(budget.limit(), 2);
        assert_eq!(budget.consumed(), 0);
        assert_eq!(budget.remaining(), 2);
        assert!(!budget.is_exhausted());

        budget.try_consume().unwrap();
        assert_eq!(budget.consumed(), 1);
        assert_eq!(budget.remaining(), 1);

        budget.try_consume().unwrap();
        assert_eq!(budget.consumed(), 2);
        assert_eq!(budget.remaining(), 0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn retry_budget_rejects_consumption_after_exhaustion() {
        let mut budget = RetryBudget::new(1);
        budget.try_consume().unwrap();

        assert_eq!(budget.try_consume(), Err(RetryBudgetError::Exhausted));
        assert_eq!(budget.consumed(), 1);
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn zero_retry_budget_is_immediately_exhausted() {
        let mut budget = RetryBudget::new(0);

        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining(), 0);
        assert_eq!(budget.try_consume(), Err(RetryBudgetError::Exhausted));
    }
}
