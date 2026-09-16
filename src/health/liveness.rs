//! Liveness observations for the Phase 11 health subsystem.
//!
//! Liveness answers whether a runtime component is alive enough to be considered
//! operational. It is intentionally independent from readiness, lifecycle
//! control, dependency health, and heartbeat scheduling.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::status::HealthStatus;

/// A point-in-time observation of a component's liveness.
///
/// The observation timestamp is part of the reported state so that a higher
/// level health mechanism can later evaluate freshness without requiring this
/// type to own timers, heartbeat scheduling, or lifecycle control.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LivenessReport {
    status: HealthStatus,
    observed_at: SystemTime,
}

impl LivenessReport {
    /// Creates a liveness report timestamped at the current system time.
    #[must_use]
    pub fn new(status: HealthStatus) -> Self {
        Self {
            status,
            observed_at: SystemTime::now(),
        }
    }

    /// Creates a liveness report with an explicitly supplied observation time.
    ///
    /// This is useful when a producer already has a timestamp and for
    /// deterministic tests. It does not perform freshness evaluation.
    #[must_use]
    pub const fn at(status: HealthStatus, observed_at: SystemTime) -> Self {
        Self {
            status,
            observed_at,
        }
    }

    /// Creates a healthy liveness observation at the current time.
    #[must_use]
    pub fn healthy() -> Self {
        Self::new(HealthStatus::Healthy)
    }

    /// Creates a degraded liveness observation at the current time.
    #[must_use]
    pub fn degraded() -> Self {
        Self::new(HealthStatus::Degraded)
    }

    /// Creates an unhealthy liveness observation at the current time.
    #[must_use]
    pub fn unhealthy() -> Self {
        Self::new(HealthStatus::Unhealthy)
    }

    /// Creates an unknown liveness observation at the current time.
    #[must_use]
    pub fn unknown() -> Self {
        Self::new(HealthStatus::Unknown)
    }

    /// Returns the observed liveness status.
    #[must_use]
    pub const fn status(&self) -> HealthStatus {
        self.status
    }

    /// Returns the time at which this liveness observation was recorded.
    #[must_use]
    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn explicit_timestamp_is_preserved() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(42);
        let report = LivenessReport::at(HealthStatus::Healthy, observed_at);

        assert_eq!(report.status(), HealthStatus::Healthy);
        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn convenience_constructors_use_expected_statuses() {
        assert_eq!(LivenessReport::healthy().status(), HealthStatus::Healthy);
        assert_eq!(LivenessReport::degraded().status(), HealthStatus::Degraded);
        assert_eq!(
            LivenessReport::unhealthy().status(),
            HealthStatus::Unhealthy
        );
        assert_eq!(LivenessReport::unknown().status(), HealthStatus::Unknown);
    }

    #[test]
    fn new_records_a_current_observation_time() {
        let before = SystemTime::now();
        let report = LivenessReport::new(HealthStatus::Healthy);
        let after = SystemTime::now();

        assert!(report.observed_at() >= before);
        assert!(report.observed_at() <= after);
    }

    #[test]
    fn liveness_contains_no_readiness_state() {
        let report = LivenessReport::healthy();

        assert_eq!(report.status(), HealthStatus::Healthy);
    }

    #[test]
    fn reports_with_identical_data_are_equal() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(100);
        let first = LivenessReport::at(HealthStatus::Degraded, observed_at);
        let second = LivenessReport::at(HealthStatus::Degraded, observed_at);

        assert_eq!(first, second);
    }
}
