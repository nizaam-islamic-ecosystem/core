//! Dependency health observations for the Phase 11 health subsystem.
//!
//! This module represents the currently observed condition of an engine-owned
//! dependency. It does not initialize dependencies, resolve dependency graphs,
//! retry failures, modify runtime lifecycle, or determine overall engine health.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::status::HealthStatus;

pub const MAX_DEPENDENCY_ID_LENGTH: usize = 128;

/// Identifies one dependency for health reporting.
///
/// The identifier is intentionally provider-neutral. Core does not interpret
/// whether the dependency is a database, service, cache, queue, or another
/// engine-specific resource.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct DependencyId(String);

impl DependencyId {
    /// Creates a dependency identifier from a non-empty, bounded string.
    ///
    /// Whitespace at the edges is not trimmed automatically, because the
    /// caller owns the identifier's canonical representation.
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();

        if value.is_empty() || value.len() > MAX_DEPENDENCY_ID_LENGTH {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the dependency identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the identifier and returns its owned string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for DependencyId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value).ok_or("dependency identifier is empty or exceeds the maximum length")
    }
}

/// Describes whether an engine considers a dependency required or optional.
///
/// This is engine-owned metadata. Core does not decide which dependencies are
/// semantically required for a particular engine.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum DependencyRequirement {
    Required,
    Optional,
}

/// A point-in-time observation of one dependency's operational condition.
///
/// The report records identity, engine-provided requirement metadata, observed
/// health status, and observation time. It does not control dependency
/// lifecycle, runtime lifecycle, retry behavior, or request admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyReport {
    id: DependencyId,
    requirement: DependencyRequirement,
    status: HealthStatus,
    observed_at: SystemTime,
}

impl DependencyReport {
    /// Creates a dependency health observation timestamped at the current time.
    #[must_use]
    pub fn new(id: DependencyId, requirement: DependencyRequirement, status: HealthStatus) -> Self {
        Self {
            id,
            requirement,
            status,
            observed_at: SystemTime::now(),
        }
    }

    /// Creates a dependency health observation with an explicit timestamp.
    ///
    /// The timestamp is preserved as supplied. Freshness evaluation belongs to
    /// the higher-level health mechanism.
    #[must_use]
    pub const fn at(
        id: DependencyId,
        requirement: DependencyRequirement,
        status: HealthStatus,
        observed_at: SystemTime,
    ) -> Self {
        Self {
            id,
            requirement,
            status,
            observed_at,
        }
    }

    /// Creates a healthy dependency observation at the current time.
    #[must_use]
    pub fn healthy(id: DependencyId, requirement: DependencyRequirement) -> Self {
        Self::new(id, requirement, HealthStatus::Healthy)
    }

    /// Creates a degraded dependency observation at the current time.
    #[must_use]
    pub fn degraded(id: DependencyId, requirement: DependencyRequirement) -> Self {
        Self::new(id, requirement, HealthStatus::Degraded)
    }

    /// Creates an unhealthy dependency observation at the current time.
    #[must_use]
    pub fn unhealthy(id: DependencyId, requirement: DependencyRequirement) -> Self {
        Self::new(id, requirement, HealthStatus::Unhealthy)
    }

    /// Creates an unknown dependency observation at the current time.
    #[must_use]
    pub fn unknown(id: DependencyId, requirement: DependencyRequirement) -> Self {
        Self::new(id, requirement, HealthStatus::Unknown)
    }

    /// Returns the dependency identifier.
    #[must_use]
    pub fn id(&self) -> &DependencyId {
        &self.id
    }

    /// Returns whether the dependency is required or optional according to the
    /// engine-provided metadata.
    #[must_use]
    pub const fn requirement(&self) -> DependencyRequirement {
        self.requirement
    }

    /// Returns the observed health status.
    #[must_use]
    pub const fn status(&self) -> HealthStatus {
        self.status
    }

    /// Returns the time at which the observation was recorded.
    #[must_use]
    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn dependency(id: &str) -> DependencyId {
        DependencyId::new(id).unwrap()
    }

    #[test]
    fn dependency_id_preserves_value() {
        let id = DependencyId::new("database").unwrap();

        assert_eq!(id.as_str(), "database");
        assert_eq!(id.clone().into_inner(), "database");
    }

    #[test]
    fn dependency_id_rejects_empty_value() {
        assert!(DependencyId::new("").is_none());
    }

    #[test]
    fn dependency_id_rejects_values_over_maximum_length() {
        let value = "a".repeat(MAX_DEPENDENCY_ID_LENGTH + 1);

        assert!(DependencyId::new(value).is_none());
    }

    #[test]
    fn dependency_id_deserialization_preserves_constructor_validation() {
        let valid = serde_json::from_str::<DependencyId>("\"database\"")
            .expect("valid dependency id should deserialize");
        assert_eq!(valid.as_str(), "database");

        assert!(serde_json::from_str::<DependencyId>("\"\"").is_err());
        let oversized = format!("\"{}\"", "x".repeat(MAX_DEPENDENCY_ID_LENGTH + 1));
        assert!(serde_json::from_str::<DependencyId>(&oversized).is_err());
    }

    #[test]
    fn dependency_requirement_variants_are_distinct() {
        assert_ne!(
            DependencyRequirement::Required,
            DependencyRequirement::Optional
        );
    }

    #[test]
    fn report_preserves_all_observation_fields() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(42);
        let report = DependencyReport::at(
            dependency("database"),
            DependencyRequirement::Required,
            HealthStatus::Healthy,
            observed_at,
        );

        assert_eq!(report.id().as_str(), "database");
        assert_eq!(report.requirement(), DependencyRequirement::Required);
        assert_eq!(report.status(), HealthStatus::Healthy);
        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn convenience_constructors_use_expected_statuses() {
        let required = DependencyRequirement::Required;
        let id = dependency("service");

        assert_eq!(
            DependencyReport::healthy(id.clone(), required).status(),
            HealthStatus::Healthy
        );
        assert_eq!(
            DependencyReport::degraded(id.clone(), required).status(),
            HealthStatus::Degraded
        );
        assert_eq!(
            DependencyReport::unhealthy(id.clone(), required).status(),
            HealthStatus::Unhealthy
        );
        assert_eq!(
            DependencyReport::unknown(id, required).status(),
            HealthStatus::Unknown
        );
    }

    #[test]
    fn required_and_optional_metadata_are_preserved() {
        let required =
            DependencyReport::healthy(dependency("required-db"), DependencyRequirement::Required);
        let optional = DependencyReport::unhealthy(
            dependency("optional-cache"),
            DependencyRequirement::Optional,
        );

        assert_eq!(required.requirement(), DependencyRequirement::Required);
        assert_eq!(optional.requirement(), DependencyRequirement::Optional);
        assert_eq!(optional.status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn explicit_timestamp_is_not_recomputed() {
        let observed_at = UNIX_EPOCH + Duration::from_secs(100);
        let report = DependencyReport::at(
            dependency("search"),
            DependencyRequirement::Optional,
            HealthStatus::Degraded,
            observed_at,
        );

        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn new_records_a_current_observation_time() {
        let before = SystemTime::now();
        let report =
            DependencyReport::healthy(dependency("database"), DependencyRequirement::Required);
        let after = SystemTime::now();

        assert!(report.observed_at() >= before);
        assert!(report.observed_at() <= after);
    }

    #[test]
    fn unknown_is_preserved_without_becoming_unhealthy() {
        let report = DependencyReport::unknown(
            dependency("external-service"),
            DependencyRequirement::Required,
        );

        assert_eq!(report.status(), HealthStatus::Unknown);
        assert_ne!(report.status(), HealthStatus::Unhealthy);
    }
}
