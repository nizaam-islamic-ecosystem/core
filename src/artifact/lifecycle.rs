//! Lifecycle states and transition validation for artifact versions.
//!
//! The normal lifecycle progression is:
//!
//! Created → Validating → Validated → Published → Superseded → Archived
//!
//! `Revoked` is an exceptional trust/usage state reachable from `Published`
//! or `Superseded`.
//!
//! Not every artifact version must pass through every state.
//!
//! `Superseded` means that a newer version exists. It does not automatically
//! invalidate the superseded version. Exact references remain valid unless the
//! version is explicitly revoked or access policy prevents its use.
//!
//! `Archived` means the version is retained but is no longer active or preferred.
//!
//! `Revoked` means the version is explicitly considered unsafe, invalid, or
//! untrusted under normal usage policy. Revocation does not erase artifact
//! identity or historical existence.
//!
//! `Deleted` is intentionally not a semantic lifecycle state. Physical deletion
//! remains a storage and retention concern.

/// Lifecycle state of an artifact version.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LifecycleState {
    /// The artifact version has been created but has not entered validation.
    Created,

    /// The artifact version is currently undergoing validation.
    Validating,

    /// The artifact version has passed the required generic validation.
    Validated,

    /// The artifact version has been published and is normally resolvable.
    Published,

    /// A newer version exists for the same logical artifact.
    Superseded,

    /// The version is retained but is no longer active or preferred.
    Archived,

    /// The version is explicitly unsafe, invalid, or untrusted for normal use.
    Revoked,
}

/// Error returned when an invalid lifecycle transition is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleError {
    from: LifecycleState,
    to: LifecycleState,
}

impl LifecycleError {
    /// Creates a lifecycle transition error.
    pub fn new(from: LifecycleState, to: LifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the state the transition started from.
    pub fn from(&self) -> LifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub fn to(&self) -> LifecycleState {
        self.to
    }
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid artifact lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for LifecycleError {}

/// Validates whether a lifecycle transition is allowed.
///
/// Same-state transitions are treated as valid no-ops.
pub fn can_transition(from: LifecycleState, to: LifecycleState) -> Result<(), LifecycleError> {
    match (from, to) {
        (state, target) if state == target => Ok(()),

        (LifecycleState::Created, LifecycleState::Validating) => Ok(()),
        (LifecycleState::Validating, LifecycleState::Validated) => Ok(()),
        (LifecycleState::Validated, LifecycleState::Published) => Ok(()),
        (LifecycleState::Published, LifecycleState::Superseded) => Ok(()),
        (LifecycleState::Superseded, LifecycleState::Archived) => Ok(()),
        (LifecycleState::Published, LifecycleState::Revoked) => Ok(()),
        (LifecycleState::Superseded, LifecycleState::Revoked) => Ok(()),

        _ => Err(LifecycleError::new(from, to)),
    }
}

/// Applies a lifecycle transition to a current state.
///
/// This is the single lifecycle transition primitive used by artifact
/// storage and higher-level artifact mechanisms.
pub fn transition(
    from: LifecycleState,
    to: LifecycleState,
) -> Result<LifecycleState, LifecycleError> {
    can_transition(from, to)?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_forward_transitions_are_allowed() {
        assert!(transition(LifecycleState::Created, LifecycleState::Validating).is_ok());
        assert!(transition(LifecycleState::Validating, LifecycleState::Validated).is_ok());
        assert!(transition(LifecycleState::Validated, LifecycleState::Published).is_ok());
        assert!(transition(LifecycleState::Published, LifecycleState::Superseded).is_ok());
        assert!(transition(LifecycleState::Superseded, LifecycleState::Archived).is_ok());
    }

    #[test]
    fn published_version_can_be_revoked() {
        assert!(transition(LifecycleState::Published, LifecycleState::Revoked).is_ok());
    }

    #[test]
    fn superseded_version_can_be_revoked() {
        assert!(transition(LifecycleState::Superseded, LifecycleState::Revoked).is_ok());
    }

    #[test]
    fn same_state_transitions_are_valid_no_ops() {
        let states = [
            LifecycleState::Created,
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Superseded,
            LifecycleState::Archived,
            LifecycleState::Revoked,
        ];

        for state in states {
            assert_eq!(transition(state, state), Ok(state));
        }
    }

    #[test]
    fn backward_transitions_are_rejected() {
        assert!(transition(LifecycleState::Validating, LifecycleState::Created).is_err());
        assert!(transition(LifecycleState::Validated, LifecycleState::Validating).is_err());
        assert!(transition(LifecycleState::Published, LifecycleState::Validated).is_err());
        assert!(transition(LifecycleState::Superseded, LifecycleState::Published).is_err());
        assert!(transition(LifecycleState::Archived, LifecycleState::Superseded).is_err());
    }

    #[test]
    fn invalid_cross_state_transitions_are_rejected() {
        assert!(transition(LifecycleState::Created, LifecycleState::Published).is_err());
        assert!(transition(LifecycleState::Created, LifecycleState::Revoked).is_err());
        assert!(transition(LifecycleState::Validated, LifecycleState::Superseded).is_err());
        assert!(transition(LifecycleState::Validated, LifecycleState::Revoked).is_err());
        assert!(transition(LifecycleState::Archived, LifecycleState::Revoked).is_err());
    }

    #[test]
    fn revoked_is_terminal_for_normal_lifecycle_transitions() {
        let possible_targets = [
            LifecycleState::Created,
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Superseded,
            LifecycleState::Archived,
        ];

        for target in possible_targets {
            assert!(transition(LifecycleState::Revoked, target).is_err());
        }
    }

    #[test]
    fn archived_is_terminal_for_normal_lifecycle_transitions() {
        let possible_targets = [
            LifecycleState::Created,
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Superseded,
            LifecycleState::Revoked,
        ];

        for target in possible_targets {
            assert!(transition(LifecycleState::Archived, target).is_err());
        }
    }

    #[test]
    fn lifecycle_error_preserves_transition_states() {
        let error = LifecycleError::new(LifecycleState::Created, LifecycleState::Published);
        assert_eq!(error.from(), LifecycleState::Created);
        assert_eq!(error.to(), LifecycleState::Published);
    }

    #[test]
    fn lifecycle_error_display_describes_transition() {
        let error = LifecycleError::new(LifecycleState::Created, LifecycleState::Published);
        let message = error.to_string();
        assert!(message.contains("Created"));
        assert!(message.contains("Published"));
        assert!(message.contains("invalid artifact lifecycle transition"));
    }
}
