//! Common health assessment vocabulary for Nizaam Core.
//!
//! `HealthStatus` describes an operational health assessment. It is deliberately
//! independent from runtime lifecycle state, liveness checks, readiness checks,
//! diagnostics, dependency policy, and lifecycle control.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The coarse-grained health assessment used by the Health subsystem.
///
/// `Unknown` means that health could not be established from the available
/// observation. It is intentionally distinct from `Unhealthy`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum HealthStatus {
    /// The observed component is operating normally.
    Healthy,
    /// The component is operating, but one or more observed conditions are
    /// outside the normal state without being an outright failure.
    Degraded,
    /// The observed component is known to be unhealthy or unavailable.
    Unhealthy,
    /// The available observations are insufficient to establish health.
    Unknown,
}

impl HealthStatus {
    /// Returns `true` only for [`HealthStatus::Healthy`].
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Returns `true` only for [`HealthStatus::Degraded`].
    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::Degraded)
    }

    /// Returns `true` only for [`HealthStatus::Unhealthy`].
    pub const fn is_unhealthy(self) -> bool {
        matches!(self, Self::Unhealthy)
    }

    /// Returns `true` only for [`HealthStatus::Unknown`].
    pub const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Healthy => "HEALTHY",
            Self::Degraded => "DEGRADED",
            Self::Unhealthy => "UNHEALTHY",
            Self::Unknown => "UNKNOWN",
        };

        f.write_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::HealthStatus;

    #[test]
    fn variants_are_distinct() {
        assert_ne!(HealthStatus::Healthy, HealthStatus::Degraded);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unknown);
        assert_ne!(HealthStatus::Degraded, HealthStatus::Unhealthy);
        assert_ne!(HealthStatus::Degraded, HealthStatus::Unknown);
        assert_ne!(HealthStatus::Unhealthy, HealthStatus::Unknown);
    }

    #[test]
    fn classification_helpers_match_their_variants() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(HealthStatus::Degraded.is_degraded());
        assert!(HealthStatus::Unhealthy.is_unhealthy());
        assert!(HealthStatus::Unknown.is_unknown());

        assert!(!HealthStatus::Healthy.is_degraded());
        assert!(!HealthStatus::Degraded.is_unhealthy());
        assert!(!HealthStatus::Unhealthy.is_unknown());
        assert!(!HealthStatus::Unknown.is_healthy());
    }

    #[test]
    fn status_is_copy_and_cloneable() {
        let original = HealthStatus::Degraded;
        let copied = original;
        let cloned = original;

        assert_eq!(original, copied);
        assert_eq!(original, cloned);
    }

    #[test]
    fn display_uses_stable_uppercase_names() {
        assert_eq!(HealthStatus::Healthy.to_string(), "HEALTHY");
        assert_eq!(HealthStatus::Degraded.to_string(), "DEGRADED");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "UNHEALTHY");
        assert_eq!(HealthStatus::Unknown.to_string(), "UNKNOWN");
    }
}
