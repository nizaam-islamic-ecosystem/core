//! Stream lifecycle state and transition validation.
//!
//! The stream lifecycle is intentionally independent from stream ownership,
//! cancellation signalling, buffering, item semantics, and runtime shutdown.
//! Those mechanisms are composed by the higher-level streaming and runtime
//! modules.
//!
//! The normal stream lifecycle is:
//!
//! ```text
//! CREATED
//!    ↓
//!  OPEN
//!    ├── COMPLETED
//!    ├── CANCELLED
//!    └── FAILED
//! ```
//!
//! `COMPLETED`, `CANCELLED`, and `FAILED` are terminal states. A terminal
//! stream cannot be reopened or moved to a different terminal state.
//!
//! Repeating the same transition is a valid no-op, which makes termination
//! safe to invoke from independent completion, cancellation, deadline, and
//! shutdown paths.

/// Lifecycle state of an application-level stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamLifecycleState {
    /// The stream has been created but has not started active publication.
    Created,

    /// The stream is active and may publish logical items.
    Open,

    /// The producer finished normally and the stream closed successfully.
    Completed,

    /// The stream terminated because cancellation or expiration ended it.
    Cancelled,

    /// The stream terminated because execution failed abnormally.
    Failed,
}

impl StreamLifecycleState {
    /// Returns `true` when this state is terminal.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Returns whether this state may transition directly to `next`.
    ///
    /// Same-state transitions are valid no-ops so that repeated termination
    /// requests remain idempotent.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Created)
                | (Self::Open, Self::Open)
                | (Self::Completed, Self::Completed)
                | (Self::Cancelled, Self::Cancelled)
                | (Self::Failed, Self::Failed)
                | (Self::Created, Self::Open)
                | (Self::Open, Self::Completed)
                | (Self::Open, Self::Cancelled)
                | (Self::Open, Self::Failed)
        )
    }
}

/// Error returned when an invalid stream lifecycle transition is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamLifecycleError {
    from: StreamLifecycleState,
    to: StreamLifecycleState,
}

impl StreamLifecycleError {
    /// Creates an error for an invalid lifecycle transition.
    pub const fn new(from: StreamLifecycleState, to: StreamLifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the state from which the invalid transition was requested.
    pub const fn from(self) -> StreamLifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub const fn to(self) -> StreamLifecycleState {
        self.to
    }
}

impl std::fmt::Display for StreamLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid stream lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for StreamLifecycleError {}

/// Mutable stream lifecycle state machine.
///
/// This type contains no synchronization. The owning `Stream` implementation
/// is responsible for synchronizing lifecycle access when a stream is shared
/// between concurrent producers, consumers, cancellation paths, or shutdown
/// paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamLifecycle {
    state: StreamLifecycleState,
}

impl Default for StreamLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamLifecycle {
    /// Creates a new stream lifecycle in the `Created` state.
    pub const fn new() -> Self {
        Self {
            state: StreamLifecycleState::Created,
        }
    }

    /// Returns the current lifecycle state.
    pub const fn state(&self) -> StreamLifecycleState {
        self.state
    }

    /// Returns `true` when the stream has reached a terminal state.
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Validates and applies a lifecycle transition.
    ///
    /// Same-state transitions are successful no-ops. Invalid transitions
    /// leave the current state unchanged.
    pub fn transition(&mut self, next: StreamLifecycleState) -> Result<(), StreamLifecycleError> {
        if !self.state.can_transition_to(next) {
            return Err(StreamLifecycleError::new(self.state, next));
        }

        self.state = next;
        Ok(())
    }
}

/// Validates a stream lifecycle transition without changing any state.
pub const fn can_transition(
    from: StreamLifecycleState,
    to: StreamLifecycleState,
) -> Result<(), StreamLifecycleError> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(StreamLifecycleError::new(from, to))
    }
}

/// Applies a stream lifecycle transition and returns the resulting state.
pub const fn transition(
    from: StreamLifecycleState,
    to: StreamLifecycleState,
) -> Result<StreamLifecycleState, StreamLifecycleError> {
    match can_transition(from, to) {
        Ok(()) => Ok(to),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StreamLifecycle, StreamLifecycleError, StreamLifecycleState, can_transition, transition,
    };

    #[test]
    fn new_lifecycle_starts_created() {
        let lifecycle = StreamLifecycle::new();

        assert_eq!(lifecycle.state(), StreamLifecycleState::Created);
        assert!(!lifecycle.is_terminal());
    }

    #[test]
    fn lifecycle_defaults_to_created() {
        assert_eq!(
            StreamLifecycle::default().state(),
            StreamLifecycleState::Created
        );
    }

    #[test]
    fn valid_transitions_follow_stream_lifecycle() {
        assert!(can_transition(StreamLifecycleState::Created, StreamLifecycleState::Open).is_ok());
        assert!(
            can_transition(StreamLifecycleState::Open, StreamLifecycleState::Completed).is_ok()
        );
        assert!(
            can_transition(StreamLifecycleState::Open, StreamLifecycleState::Cancelled).is_ok()
        );
        assert!(can_transition(StreamLifecycleState::Open, StreamLifecycleState::Failed).is_ok());
    }

    #[test]
    fn same_state_transitions_are_idempotent_no_ops() {
        let states = [
            StreamLifecycleState::Created,
            StreamLifecycleState::Open,
            StreamLifecycleState::Completed,
            StreamLifecycleState::Cancelled,
            StreamLifecycleState::Failed,
        ];

        for state in states {
            assert_eq!(transition(state, state), Ok(state));
        }
    }

    #[test]
    fn invalid_transitions_are_rejected() {
        let invalid_transitions = [
            (
                StreamLifecycleState::Created,
                StreamLifecycleState::Completed,
            ),
            (
                StreamLifecycleState::Created,
                StreamLifecycleState::Cancelled,
            ),
            (StreamLifecycleState::Created, StreamLifecycleState::Failed),
            (StreamLifecycleState::Open, StreamLifecycleState::Created),
            (StreamLifecycleState::Completed, StreamLifecycleState::Open),
            (StreamLifecycleState::Cancelled, StreamLifecycleState::Open),
            (StreamLifecycleState::Failed, StreamLifecycleState::Open),
            (
                StreamLifecycleState::Completed,
                StreamLifecycleState::Cancelled,
            ),
            (
                StreamLifecycleState::Completed,
                StreamLifecycleState::Failed,
            ),
            (
                StreamLifecycleState::Cancelled,
                StreamLifecycleState::Completed,
            ),
            (
                StreamLifecycleState::Cancelled,
                StreamLifecycleState::Failed,
            ),
            (
                StreamLifecycleState::Failed,
                StreamLifecycleState::Completed,
            ),
            (
                StreamLifecycleState::Failed,
                StreamLifecycleState::Cancelled,
            ),
        ];

        for (from, to) in invalid_transitions {
            assert!(
                transition(from, to).is_err(),
                "invalid transition: {from:?} -> {to:?}"
            );
        }
    }

    #[test]
    fn terminal_states_are_terminal() {
        for state in [
            StreamLifecycleState::Completed,
            StreamLifecycleState::Cancelled,
            StreamLifecycleState::Failed,
        ] {
            assert!(state.is_terminal());

            for target in [
                StreamLifecycleState::Created,
                StreamLifecycleState::Open,
                StreamLifecycleState::Completed,
                StreamLifecycleState::Cancelled,
                StreamLifecycleState::Failed,
            ] {
                if target == state {
                    assert!(can_transition(state, target).is_ok());
                } else {
                    assert!(can_transition(state, target).is_err());
                }
            }
        }
    }

    #[test]
    fn non_terminal_states_are_created_and_open_only() {
        assert!(!StreamLifecycleState::Created.is_terminal());
        assert!(!StreamLifecycleState::Open.is_terminal());
    }

    #[test]
    fn lifecycle_transition_updates_state_only_on_success() {
        let mut lifecycle = StreamLifecycle::new();

        lifecycle.transition(StreamLifecycleState::Open).unwrap();
        assert_eq!(lifecycle.state(), StreamLifecycleState::Open);

        lifecycle
            .transition(StreamLifecycleState::Completed)
            .unwrap();
        assert_eq!(lifecycle.state(), StreamLifecycleState::Completed);

        assert!(lifecycle.transition(StreamLifecycleState::Open).is_err());
        assert_eq!(lifecycle.state(), StreamLifecycleState::Completed);
    }

    #[test]
    fn repeated_terminal_transition_is_safe() {
        let mut lifecycle = StreamLifecycle::new();

        lifecycle.transition(StreamLifecycleState::Open).unwrap();
        lifecycle
            .transition(StreamLifecycleState::Cancelled)
            .unwrap();
        lifecycle
            .transition(StreamLifecycleState::Cancelled)
            .unwrap();

        assert_eq!(lifecycle.state(), StreamLifecycleState::Cancelled);
        assert!(lifecycle.is_terminal());
    }

    #[test]
    fn transition_error_preserves_source_and_destination() {
        let error = StreamLifecycleError::new(
            StreamLifecycleState::Completed,
            StreamLifecycleState::Cancelled,
        );

        assert_eq!(error.from(), StreamLifecycleState::Completed);
        assert_eq!(error.to(), StreamLifecycleState::Cancelled);
    }

    #[test]
    fn transition_error_display_describes_the_invalid_transition() {
        let error =
            StreamLifecycleError::new(StreamLifecycleState::Completed, StreamLifecycleState::Open);

        let message = error.to_string();

        assert!(message.contains("invalid stream lifecycle transition"));
        assert!(message.contains("Completed"));
        assert!(message.contains("Open"));
    }
}
