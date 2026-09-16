//! Capability health observations for the Phase 11 health subsystem.
//!
//! This module reports the operational condition of an existing Core
//! capability. It does not replace the Phase 6 capability registry or dispatch
//! mechanisms, does not own engine lifecycle, and does not determine overall
//! engine readiness.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::status::HealthStatus;
use crate::identity::CapabilityId;

/// A point-in-time observation of one capability's operational condition.
///
/// Registration and invocation remain owned by the Phase 6 Capability System.
/// This report only associates a capability identity with a health status and
/// the time at which that status was observed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityHealthReport {
    capability_id: CapabilityId,
    status: HealthStatus,
    observed_at: SystemTime,
}

impl CapabilityHealthReport {
    /// Creates a capability health observation timestamped at the current time.
    #[must_use]
    pub fn new(capability_id: CapabilityId, status: HealthStatus) -> Self {
        Self {
            capability_id,
            status,
            observed_at: SystemTime::now(),
        }
    }

    /// Creates a capability health observation with an explicit observation
    /// timestamp.
    ///
    /// Freshness is intentionally not evaluated here. The shared health
    /// mechanism will own freshness and stale-observation policy.
    #[must_use]
    pub const fn at(
        capability_id: CapabilityId,
        status: HealthStatus,
        observed_at: SystemTime,
    ) -> Self {
        Self {
            capability_id,
            status,
            observed_at,
        }
    }

    /// Creates a healthy capability observation at the current time.
    #[must_use]
    pub fn healthy(capability_id: CapabilityId) -> Self {
        Self::new(capability_id, HealthStatus::Healthy)
    }

    /// Creates a degraded capability observation at the current time.
    #[must_use]
    pub fn degraded(capability_id: CapabilityId) -> Self {
        Self::new(capability_id, HealthStatus::Degraded)
    }

    /// Creates an unhealthy capability observation at the current time.
    #[must_use]
    pub fn unhealthy(capability_id: CapabilityId) -> Self {
        Self::new(capability_id, HealthStatus::Unhealthy)
    }

    /// Creates an unknown capability observation at the current time.
    #[must_use]
    pub fn unknown(capability_id: CapabilityId) -> Self {
        Self::new(capability_id, HealthStatus::Unknown)
    }

    /// Returns the identity of the observed capability.
    #[must_use]
    pub const fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }

    /// Returns the observed health status.
    #[must_use]
    pub const fn status(&self) -> HealthStatus {
        self.status
    }

    /// Returns the time at which this capability health observation was
    /// recorded.
    #[must_use]
    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn capability_id(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    #[test]
    fn report_preserves_capability_identity() {
        let report = CapabilityHealthReport::healthy(capability_id("quran.search"));

        assert_eq!(report.capability_id().as_str(), "quran.search");
    }

    #[test]
    fn report_preserves_observed_status() {
        let report = CapabilityHealthReport::at(
            capability_id("arabic.nahw"),
            HealthStatus::Degraded,
            UNIX_EPOCH + Duration::from_secs(42),
        );

        assert_eq!(report.status(), HealthStatus::Degraded);
    }

    #[test]
    fn explicit_timestamp_is_preserved() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(84);
        let report = CapabilityHealthReport::at(
            capability_id("knowledge.query"),
            HealthStatus::Healthy,
            observed_at,
        );

        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn convenience_constructors_use_expected_statuses() {
        let id = capability_id("test.capability");

        assert_eq!(
            CapabilityHealthReport::healthy(id.clone()).status(),
            HealthStatus::Healthy
        );
        assert_eq!(
            CapabilityHealthReport::degraded(id.clone()).status(),
            HealthStatus::Degraded
        );
        assert_eq!(
            CapabilityHealthReport::unhealthy(id.clone()).status(),
            HealthStatus::Unhealthy
        );
        assert_eq!(
            CapabilityHealthReport::unknown(id).status(),
            HealthStatus::Unknown
        );
    }

    #[test]
    fn new_records_a_current_observation_time() {
        let before = SystemTime::now();
        let report = CapabilityHealthReport::healthy(capability_id("test.capability"));
        let after = SystemTime::now();

        assert!(report.observed_at() >= before);
        assert!(report.observed_at() <= after);
    }

    #[test]
    fn unknown_is_distinct_from_unhealthy() {
        let unknown = CapabilityHealthReport::unknown(capability_id("external.capability"));
        let unhealthy = CapabilityHealthReport::unhealthy(capability_id("external.capability"));

        assert_eq!(unknown.status(), HealthStatus::Unknown);
        assert_eq!(unhealthy.status(), HealthStatus::Unhealthy);
        assert_ne!(unknown.status(), unhealthy.status());
    }

    #[test]
    fn capability_health_does_not_depend_on_engine_lifecycle() {
        let report = CapabilityHealthReport::healthy(capability_id("example.capability"));

        assert_eq!(report.status(), HealthStatus::Healthy);
        assert_eq!(report.capability_id().as_str(), "example.capability");
    }
}
