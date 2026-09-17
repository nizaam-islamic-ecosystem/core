//! Execution-attempt identity and lifecycle semantics for retryable operations.
//!
//! An [`Attempt`] represents one concrete execution of a logical operation.
//! Retrying an operation creates another attempt; it does not create another
//! [`OperationId`](crate::identity::OperationId). Policy, retryability,
//! backoff, budgets, idempotency, and execution remain outside this module.

use std::sync::{Arc, Mutex};

use crate::identity::{AttemptId, OperationId};

/// Lifecycle state of one concrete execution attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptLifecycleState {
    /// The attempt exists but execution has not started.
    Created,
    /// The attempt is actively executing.
    Running,
    /// The attempt completed successfully.
    Succeeded,
    /// The attempt completed because execution failed.
    Failed,
    /// The attempt ended because it was cancelled.
    Cancelled,
}

impl AttemptLifecycleState {
    /// Returns whether this lifecycle state is terminal.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    /// Returns whether a direct transition to `next` is valid.
    ///
    /// Repeating the same state is a valid no-op. Terminal states cannot move
    /// back into execution or into another terminal state.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Created)
                | (Self::Running, Self::Running)
                | (Self::Succeeded, Self::Succeeded)
                | (Self::Failed, Self::Failed)
                | (Self::Cancelled, Self::Cancelled)
                | (Self::Created, Self::Running)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
        )
    }
}

/// Error returned when an attempt lifecycle transition is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptLifecycleError {
    from: AttemptLifecycleState,
    to: AttemptLifecycleState,
}

impl AttemptLifecycleError {
    /// Creates an error describing an invalid lifecycle transition.
    pub const fn new(from: AttemptLifecycleState, to: AttemptLifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the state from which the invalid transition was requested.
    pub const fn from(self) -> AttemptLifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub const fn to(self) -> AttemptLifecycleState {
        self.to
    }
}

impl std::fmt::Display for AttemptLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid attempt lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for AttemptLifecycleError {}

/// Error returned when an attempt cannot be created from the supplied metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptCreationError {
    /// Attempt numbers are one-based within an operation.
    InvalidAttemptNumber,
}

impl std::fmt::Display for AttemptCreationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAttemptNumber => {
                formatter.write_str("attempt number must be greater than zero")
            }
        }
    }
}

impl std::error::Error for AttemptCreationError {}

/// Mutable lifecycle state for one execution attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptLifecycle {
    state: AttemptLifecycleState,
}

impl Default for AttemptLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl AttemptLifecycle {
    /// Creates a lifecycle in the `Created` state.
    pub const fn new() -> Self {
        Self {
            state: AttemptLifecycleState::Created,
        }
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> AttemptLifecycleState {
        self.state
    }

    /// Returns whether the lifecycle is terminal.
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Validates and applies a lifecycle transition.
    pub fn transition(&mut self, next: AttemptLifecycleState) -> Result<(), AttemptLifecycleError> {
        if !self.state.can_transition_to(next) {
            return Err(AttemptLifecycleError::new(self.state, next));
        }

        self.state = next;
        Ok(())
    }
}

/// Validates an attempt lifecycle transition without changing any state.
pub const fn can_transition(
    from: AttemptLifecycleState,
    to: AttemptLifecycleState,
) -> Result<(), AttemptLifecycleError> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(AttemptLifecycleError::new(from, to))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AttemptState {
    lifecycle: AttemptLifecycle,
    successor_attempt_id: Option<AttemptId>,
}

/// Error returned when a failed attempt cannot reserve a retry successor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AttemptSuccessorReservationError<E> {
    /// The requested successor reuses the current attempt identity.
    SameAttemptId,
    /// This failed attempt already has an admitted successor.
    SuccessorAlreadyReserved,
    /// The caller's retry-budget reservation failed.
    ReservationFailed(E),
}

/// Represents one concrete execution attempt belonging to a logical operation.
///
/// Cloning an `Attempt` clones a handle to the same attempt lifecycle. It does
/// not create a second attempt or increment the attempt number.
#[derive(Clone)]
pub struct Attempt {
    operation_id: OperationId,
    attempt_id: AttemptId,
    attempt_number: u32,
    state: Arc<Mutex<AttemptState>>,
}

impl std::fmt::Debug for Attempt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Attempt")
            .field("operation_id", &self.operation_id)
            .field("attempt_id", &self.attempt_id)
            .field("attempt_number", &self.attempt_number)
            .field("state", &self.state())
            .finish()
    }
}

impl Attempt {
    /// Creates an attempt in the `Created` state.
    ///
    /// Attempt numbers are one-based and must increase within the owning
    /// operation as retries create subsequent attempts.
    pub fn new(
        operation_id: OperationId,
        attempt_id: AttemptId,
        attempt_number: u32,
    ) -> Result<Self, AttemptCreationError> {
        if attempt_number == 0 {
            return Err(AttemptCreationError::InvalidAttemptNumber);
        }

        Ok(Self {
            operation_id,
            attempt_id,
            attempt_number,
            state: Arc::new(Mutex::new(AttemptState {
                lifecycle: AttemptLifecycle::new(),
                successor_attempt_id: None,
            })),
        })
    }

    /// Returns the logical operation this attempt belongs to.
    pub const fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the unique identity of this concrete attempt.
    pub const fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }

    /// Returns the one-based ordinal of this attempt within its operation.
    pub const fn attempt_number(&self) -> u32 {
        self.attempt_number
    }

    /// Returns the current attempt lifecycle state.
    pub fn state(&self) -> AttemptLifecycleState {
        let state = self.state.lock().expect("attempt state lock poisoned");
        state.lifecycle.state()
    }

    /// Returns whether the attempt has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.state().is_terminal()
    }

    /// Starts a created attempt.
    pub fn start(&self) -> Result<(), AttemptLifecycleError> {
        let mut state = self.state.lock().expect("attempt state lock poisoned");
        state.lifecycle.transition(AttemptLifecycleState::Running)
    }

    /// Marks a running attempt as successfully completed.
    pub fn succeed(&self) -> Result<(), AttemptLifecycleError> {
        let mut state = self.state.lock().expect("attempt state lock poisoned");
        state.lifecycle.transition(AttemptLifecycleState::Succeeded)
    }

    /// Marks a running attempt as failed.
    pub fn fail(&self) -> Result<(), AttemptLifecycleError> {
        let mut state = self.state.lock().expect("attempt state lock poisoned");
        state.lifecycle.transition(AttemptLifecycleState::Failed)
    }

    /// Marks a running attempt as cancelled.
    pub fn cancel(&self) -> Result<(), AttemptLifecycleError> {
        let mut state = self.state.lock().expect("attempt state lock poisoned");
        state.lifecycle.transition(AttemptLifecycleState::Cancelled)
    }

    /// Atomically reserves the single successor that may be admitted from this
    /// failed attempt.
    ///
    /// The retry-budget reservation is performed while the same attempt-state
    /// mutex is held. The successor claim is committed only when the budget
    /// reservation succeeds, so a failed budget reservation leaves the attempt
    /// available for a later retry admission.
    pub(crate) fn try_reserve_successor<E, F>(
        &self,
        successor_id: &AttemptId,
        reserve_budget: F,
    ) -> Result<(), AttemptSuccessorReservationError<E>>
    where
        F: FnOnce() -> Result<(), E>,
    {
        let mut state = self.state.lock().expect("attempt state lock poisoned");

        if self.attempt_id == *successor_id {
            return Err(AttemptSuccessorReservationError::SameAttemptId);
        }

        if state.successor_attempt_id.is_some() {
            return Err(AttemptSuccessorReservationError::SuccessorAlreadyReserved);
        }

        reserve_budget().map_err(AttemptSuccessorReservationError::ReservationFailed)?;

        state.successor_attempt_id = Some(successor_id.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn attempt_id(value: &str) -> AttemptId {
        AttemptId::new(value).unwrap()
    }

    #[test]
    fn lifecycle_starts_created_and_is_not_terminal() {
        let lifecycle = AttemptLifecycle::new();

        assert_eq!(lifecycle.state(), AttemptLifecycleState::Created);
        assert!(!lifecycle.is_terminal());
    }

    #[test]
    fn lifecycle_default_starts_created() {
        assert_eq!(
            AttemptLifecycle::default().state(),
            AttemptLifecycleState::Created
        );
    }

    #[test]
    fn lifecycle_accepts_the_documented_execution_path() {
        let mut lifecycle = AttemptLifecycle::new();

        lifecycle
            .transition(AttemptLifecycleState::Running)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Succeeded)
            .unwrap();

        assert_eq!(lifecycle.state(), AttemptLifecycleState::Succeeded);
    }

    #[test]
    fn running_attempt_can_end_failed() {
        let mut lifecycle = AttemptLifecycle::new();

        lifecycle
            .transition(AttemptLifecycleState::Running)
            .unwrap();
        lifecycle.transition(AttemptLifecycleState::Failed).unwrap();

        assert_eq!(lifecycle.state(), AttemptLifecycleState::Failed);
    }

    #[test]
    fn running_attempt_can_end_cancelled() {
        let mut lifecycle = AttemptLifecycle::new();

        lifecycle
            .transition(AttemptLifecycleState::Running)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Cancelled)
            .unwrap();

        assert_eq!(lifecycle.state(), AttemptLifecycleState::Cancelled);
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(AttemptLifecycleState::Succeeded.is_terminal());
        assert!(AttemptLifecycleState::Failed.is_terminal());
        assert!(AttemptLifecycleState::Cancelled.is_terminal());
        assert!(!AttemptLifecycleState::Created.is_terminal());
        assert!(!AttemptLifecycleState::Running.is_terminal());
    }

    #[test]
    fn created_to_cancelled_is_rejected() {
        let error = can_transition(
            AttemptLifecycleState::Created,
            AttemptLifecycleState::Cancelled,
        )
        .unwrap_err();

        assert_eq!(error.from(), AttemptLifecycleState::Created);
        assert_eq!(error.to(), AttemptLifecycleState::Cancelled);
    }

    #[test]
    fn terminal_to_running_is_rejected() {
        for terminal in [
            AttemptLifecycleState::Succeeded,
            AttemptLifecycleState::Failed,
            AttemptLifecycleState::Cancelled,
        ] {
            let error = can_transition(terminal, AttemptLifecycleState::Running).unwrap_err();
            assert_eq!(error.from(), terminal);
            assert_eq!(error.to(), AttemptLifecycleState::Running);
        }
    }

    #[test]
    fn terminal_states_cannot_transition_to_each_other() {
        let terminal_states = [
            AttemptLifecycleState::Succeeded,
            AttemptLifecycleState::Failed,
            AttemptLifecycleState::Cancelled,
        ];

        for from in terminal_states {
            for to in terminal_states {
                if from != to {
                    assert!(can_transition(from, to).is_err());
                }
            }
        }
    }

    #[test]
    fn repeating_the_same_state_is_a_no_op() {
        let mut lifecycle = AttemptLifecycle::new();

        lifecycle
            .transition(AttemptLifecycleState::Created)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Running)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Running)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Succeeded)
            .unwrap();
        lifecycle
            .transition(AttemptLifecycleState::Succeeded)
            .unwrap();

        assert_eq!(lifecycle.state(), AttemptLifecycleState::Succeeded);
    }

    #[test]
    fn lifecycle_error_has_clear_display_text() {
        let error = AttemptLifecycleError::new(
            AttemptLifecycleState::Succeeded,
            AttemptLifecycleState::Running,
        );

        assert_eq!(
            error.to_string(),
            "invalid attempt lifecycle transition from Succeeded to Running"
        );
    }

    #[test]
    fn creation_rejects_zero_attempt_number() {
        let result = Attempt::new(operation_id("operation-1"), attempt_id("attempt-1"), 0);

        assert!(matches!(
            result,
            Err(AttemptCreationError::InvalidAttemptNumber)
        ));
    }

    #[test]
    fn creation_preserves_operation_attempt_and_number() {
        let attempt =
            Attempt::new(operation_id("operation-1"), attempt_id("attempt-1"), 1).unwrap();

        assert_eq!(attempt.operation_id().as_str(), "operation-1");
        assert_eq!(attempt.attempt_id().as_str(), "attempt-1");
        assert_eq!(attempt.attempt_number(), 1);
        assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    }

    #[test]
    fn creation_accepts_later_one_based_attempt_numbers() {
        let attempt =
            Attempt::new(operation_id("operation-2"), attempt_id("attempt-7"), 7).unwrap();

        assert_eq!(attempt.attempt_number(), 7);
    }

    #[test]
    fn lifecycle_methods_follow_the_attempt_state_machine() {
        let attempt =
            Attempt::new(operation_id("operation-3"), attempt_id("attempt-1"), 1).unwrap();

        attempt.start().unwrap();
        assert_eq!(attempt.state(), AttemptLifecycleState::Running);

        attempt.fail().unwrap();
        assert_eq!(attempt.state(), AttemptLifecycleState::Failed);
        assert!(attempt.is_terminal());
    }

    #[test]
    fn terminal_method_calls_are_idempotent_for_the_same_terminal_state() {
        let attempt =
            Attempt::new(operation_id("operation-4"), attempt_id("attempt-1"), 1).unwrap();

        attempt.start().unwrap();
        attempt.succeed().unwrap();
        attempt.succeed().unwrap();

        assert_eq!(attempt.state(), AttemptLifecycleState::Succeeded);
    }

    #[test]
    fn attempt_clone_shares_lifecycle_state() {
        let attempt =
            Attempt::new(operation_id("operation-5"), attempt_id("attempt-1"), 1).unwrap();
        let clone = attempt.clone();

        clone.start().unwrap();

        assert_eq!(attempt.state(), AttemptLifecycleState::Running);
        assert_eq!(clone.state(), AttemptLifecycleState::Running);
        assert_eq!(attempt.attempt_id(), clone.attempt_id());
        assert_eq!(attempt.attempt_number(), clone.attempt_number());
    }

    #[test]
    fn different_attempts_can_belong_to_one_operation() {
        let operation = operation_id("operation-6");
        let first = Attempt::new(operation.clone(), attempt_id("attempt-1"), 1).unwrap();
        let second = Attempt::new(operation.clone(), attempt_id("attempt-2"), 2).unwrap();

        assert_eq!(first.operation_id(), second.operation_id());
        assert_ne!(first.attempt_id(), second.attempt_id());
        assert_ne!(first.attempt_number(), second.attempt_number());
    }

    #[test]
    fn different_operations_have_distinct_operation_identity() {
        let first = Attempt::new(operation_id("operation-1"), attempt_id("attempt-1"), 1).unwrap();
        let second = Attempt::new(operation_id("operation-2"), attempt_id("attempt-2"), 1).unwrap();

        assert_ne!(first.operation_id(), second.operation_id());
    }

    #[test]
    fn cancellation_only_applies_from_running() {
        let attempt =
            Attempt::new(operation_id("operation-7"), attempt_id("attempt-1"), 1).unwrap();

        assert_eq!(
            attempt.cancel().unwrap_err(),
            AttemptLifecycleError::new(
                AttemptLifecycleState::Created,
                AttemptLifecycleState::Cancelled,
            )
        );
        assert_eq!(attempt.state(), AttemptLifecycleState::Created);

        attempt.start().unwrap();
        attempt.cancel().unwrap();
        assert_eq!(attempt.state(), AttemptLifecycleState::Cancelled);
    }

    #[test]
    fn success_and_failure_only_apply_from_running() {
        let attempt =
            Attempt::new(operation_id("operation-8"), attempt_id("attempt-1"), 1).unwrap();

        assert!(attempt.succeed().is_err());
        assert!(attempt.fail().is_err());
        assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    }
}
