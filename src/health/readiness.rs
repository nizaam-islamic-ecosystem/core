//! Readiness observations for the Phase 11 health subsystem.
//!
//! Readiness reports whether a runtime is currently in the Phase 8 state that
//! permits normal work. It observes the existing runtime lifecycle rather than
//! creating a second lifecycle or request-admission mechanism.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::status::HealthStatus;
use crate::runtime::lifecycle::LifecycleState;

/// Readiness state derived from the runtime's authoritative lifecycle state.
///
/// `Ready` means the runtime is in `SERVING` and may accept normal work.
/// `NotReady` means readiness is known to be unavailable. `Unknown` means the
/// current readiness condition cannot be established by the observing layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ReadinessState {
    Ready,
    NotReady,
    Unknown,
}

impl ReadinessState {
    /// Returns whether the observed state permits normal work.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns the health status associated with this readiness observation.
    ///
    /// Readiness and health are distinct concepts. `Ready` maps to
    /// `Healthy`, while `NotReady` and `Unknown` retain their separate
    /// operational meanings through the readiness state itself.
    #[must_use]
    pub const fn health_status(self) -> HealthStatus {
        match self {
            Self::Ready => HealthStatus::Healthy,
            Self::NotReady => HealthStatus::Unhealthy,
            Self::Unknown => HealthStatus::Unknown,
        }
    }
}

impl From<LifecycleState> for ReadinessState {
    fn from(state: LifecycleState) -> Self {
        match state {
            LifecycleState::Created
            | LifecycleState::Starting
            | LifecycleState::Configuring
            | LifecycleState::Dependencies
            | LifecycleState::Capabilities
            | LifecycleState::Registering
            | LifecycleState::Ready
            | LifecycleState::Draining
            | LifecycleState::Stopped => Self::NotReady,
            LifecycleState::Serving => Self::Ready,
        }
    }
}

/// A point-in-time observation of runtime readiness.
///
/// The report stores only the readiness result and observation time. It does
/// not store or mutate lifecycle state, and it does not perform request
/// admission, dependency evaluation, capability evaluation, or heartbeat
/// scheduling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadinessReport {
    state: ReadinessState,
    observed_at: SystemTime,
}

impl ReadinessReport {
    /// Creates a readiness report timestamped at the current system time.
    #[must_use]
    pub fn new(state: ReadinessState) -> Self {
        Self {
            state,
            observed_at: SystemTime::now(),
        }
    }

    /// Creates a readiness report with an explicitly supplied observation time.
    ///
    /// This is useful when a producer already has an observation timestamp and
    /// for deterministic tests. It does not evaluate freshness.
    #[must_use]
    pub const fn at(state: ReadinessState, observed_at: SystemTime) -> Self {
        Self { state, observed_at }
    }

    /// Creates a readiness report from the authoritative Phase 8 lifecycle
    /// state using the current time as the observation timestamp.
    #[must_use]
    pub fn from_lifecycle(state: LifecycleState) -> Self {
        Self::new(ReadinessState::from(state))
    }

    /// Creates a readiness report from the authoritative Phase 8 lifecycle
    /// state with an explicit observation timestamp.
    #[must_use]
    pub fn at_lifecycle(state: LifecycleState, observed_at: SystemTime) -> Self {
        Self::at(ReadinessState::from(state), observed_at)
    }

    /// Creates a ready observation at the current time.
    #[must_use]
    pub fn ready() -> Self {
        Self::new(ReadinessState::Ready)
    }

    /// Creates a not-ready observation at the current time.
    #[must_use]
    pub fn not_ready() -> Self {
        Self::new(ReadinessState::NotReady)
    }

    /// Creates an unknown readiness observation at the current time.
    #[must_use]
    pub fn unknown() -> Self {
        Self::new(ReadinessState::Unknown)
    }

    /// Returns the observed readiness state.
    #[must_use]
    pub const fn state(&self) -> ReadinessState {
        self.state
    }

    /// Returns the time at which this readiness observation was recorded.
    #[must_use]
    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }

    /// Returns whether this observation reports readiness for normal work.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.state.is_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn lifecycle_mapping_matches_phase_eight_admission_boundary() {
        assert_eq!(
            ReadinessState::from(LifecycleState::Created),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Starting),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Configuring),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Dependencies),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Capabilities),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Registering),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Ready),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Serving),
            ReadinessState::Ready
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Draining),
            ReadinessState::NotReady
        );
        assert_eq!(
            ReadinessState::from(LifecycleState::Stopped),
            ReadinessState::NotReady
        );
    }

    #[test]
    fn only_serving_is_ready_for_normal_work() {
        assert!(!ReadinessReport::from_lifecycle(LifecycleState::Ready).is_ready());
        assert!(ReadinessReport::from_lifecycle(LifecycleState::Serving).is_ready());
        assert!(!ReadinessReport::from_lifecycle(LifecycleState::Draining).is_ready());
        assert!(!ReadinessReport::from_lifecycle(LifecycleState::Stopped).is_ready());
    }

    #[test]
    fn explicit_timestamp_is_preserved() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(42);
        let report = ReadinessReport::at(ReadinessState::Ready, observed_at);

        assert_eq!(report.state(), ReadinessState::Ready);
        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn explicit_lifecycle_timestamp_is_preserved() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(84);
        let report = ReadinessReport::at_lifecycle(LifecycleState::Serving, observed_at);

        assert_eq!(report.state(), ReadinessState::Ready);
        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn convenience_constructors_use_expected_states() {
        assert_eq!(ReadinessReport::ready().state(), ReadinessState::Ready);
        assert_eq!(
            ReadinessReport::not_ready().state(),
            ReadinessState::NotReady
        );
        assert_eq!(ReadinessReport::unknown().state(), ReadinessState::Unknown);
    }

    #[test]
    fn new_records_a_current_observation_time() {
        let before = SystemTime::now();
        let report = ReadinessReport::new(ReadinessState::Ready);
        let after = SystemTime::now();

        assert!(report.observed_at() >= before);
        assert!(report.observed_at() <= after);
    }

    #[test]
    fn unknown_is_not_treated_as_ready() {
        let report = ReadinessReport::unknown();

        assert!(!report.is_ready());
        assert_eq!(report.state().health_status(), HealthStatus::Unknown);
    }

    #[test]
    fn readiness_does_not_change_runtime_lifecycle() {
        let mut lifecycle = crate::runtime::lifecycle::Lifecycle::new();
        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Configuring).unwrap();

        let _report = ReadinessReport::from_lifecycle(lifecycle.state());

        assert_eq!(lifecycle.state(), LifecycleState::Configuring);
    }
}
