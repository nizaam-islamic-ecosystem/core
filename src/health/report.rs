use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::identity::{CapabilityId, EngineId};
use crate::runtime::lifecycle::LifecycleState;

use super::capabilities::CapabilityHealthReport;
use super::dependencies::{DependencyId, DependencyReport, DependencyRequirement};
use super::liveness::LivenessReport;
use super::readiness::{ReadinessReport, ReadinessState};
use super::status::HealthStatus;

/// A point-in-time aggregate of an engine's operational health observations.
#[derive(Clone, Debug, PartialEq)]
pub struct HealthReport {
    engine_id: EngineId,
    lifecycle: LifecycleState,
    liveness: LivenessReport,
    readiness: ReadinessReport,
    dependencies: Vec<DependencyReport>,
    capabilities: Vec<CapabilityHealthReport>,
    overall: HealthStatus,
    observed_at: SystemTime,
}

impl HealthReport {
    /// Constructs a health report from component observations.
    ///
    /// Dependencies and capabilities are normalized into deterministic order,
    /// and duplicate identities are rejected.
    pub fn new(
        engine_id: EngineId,
        lifecycle: LifecycleState,
        liveness: LivenessReport,
        readiness: ReadinessReport,
        dependencies: Vec<DependencyReport>,
        capabilities: Vec<CapabilityHealthReport>,
    ) -> Result<Self, HealthReportError> {
        let dependencies = normalize_dependencies(dependencies)?;
        let capabilities = normalize_capabilities(capabilities)?;

        let overall = aggregate_health(
            lifecycle,
            &liveness,
            &readiness,
            &dependencies,
            &capabilities,
        );

        Ok(Self {
            engine_id,
            lifecycle,
            liveness,
            readiness,
            dependencies,
            capabilities,
            overall,
            observed_at: SystemTime::now(),
        })
    }

    /// Constructs a report with an explicit aggregate observation timestamp.
    pub fn at(
        engine_id: EngineId,
        lifecycle: LifecycleState,
        liveness: LivenessReport,
        readiness: ReadinessReport,
        dependencies: Vec<DependencyReport>,
        capabilities: Vec<CapabilityHealthReport>,
        observed_at: SystemTime,
    ) -> Result<Self, HealthReportError> {
        let dependencies = normalize_dependencies(dependencies)?;
        let capabilities = normalize_capabilities(capabilities)?;

        let overall = aggregate_health(
            lifecycle,
            &liveness,
            &readiness,
            &dependencies,
            &capabilities,
        );

        Ok(Self {
            engine_id,
            lifecycle,
            liveness,
            readiness,
            dependencies,
            capabilities,
            overall,
            observed_at,
        })
    }

    /// Returns the engine this report describes.
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the lifecycle state observed for the engine.
    pub const fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }

    /// Returns the liveness observation.
    pub const fn liveness(&self) -> &LivenessReport {
        &self.liveness
    }

    /// Returns the readiness observation.
    pub const fn readiness(&self) -> &ReadinessReport {
        &self.readiness
    }

    /// Returns the dependency observations in deterministic order.
    pub fn dependencies(&self) -> &[DependencyReport] {
        &self.dependencies
    }

    /// Returns the capability observations in deterministic order.
    pub fn capabilities(&self) -> &[CapabilityHealthReport] {
        &self.capabilities
    }

    /// Returns the derived overall health assessment.
    pub const fn overall(&self) -> HealthStatus {
        self.overall
    }

    /// Returns the time at which this aggregate snapshot was assembled.
    pub const fn observed_at(&self) -> SystemTime {
        self.observed_at
    }

    /// Returns a dependency observation by identity.
    pub fn dependency(&self, id: &DependencyId) -> Option<&DependencyReport> {
        self.dependencies
            .binary_search_by(|report| report.id().cmp(id))
            .ok()
            .map(|index| &self.dependencies[index])
    }

    /// Returns a capability observation by identity.
    pub fn capability(&self, id: &CapabilityId) -> Option<&CapabilityHealthReport> {
        self.capabilities
            .binary_search_by(|report| report.capability_id().cmp(id))
            .ok()
            .map(|index| &self.capabilities[index])
    }
}

/// Construction failures for an aggregate health report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthReportError {
    DuplicateDependency(DependencyId),
    DuplicateCapability(CapabilityId),
}

impl std::fmt::Display for HealthReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateDependency(id) => {
                write!(f, "duplicate dependency health report: {}", id.as_str())
            }
            Self::DuplicateCapability(id) => {
                write!(f, "duplicate capability health report: {}", id.as_str())
            }
        }
    }
}

impl std::error::Error for HealthReportError {}

fn normalize_dependencies(
    dependencies: Vec<DependencyReport>,
) -> Result<Vec<DependencyReport>, HealthReportError> {
    let mut normalized = BTreeMap::new();

    for report in dependencies {
        let id = report.id().clone();

        if normalized.insert(id.clone(), report).is_some() {
            return Err(HealthReportError::DuplicateDependency(id));
        }
    }

    Ok(normalized.into_values().collect())
}

fn normalize_capabilities(
    capabilities: Vec<CapabilityHealthReport>,
) -> Result<Vec<CapabilityHealthReport>, HealthReportError> {
    let mut normalized = BTreeMap::new();

    for report in capabilities {
        let id = report.capability_id().clone();

        if normalized.insert(id.clone(), report).is_some() {
            return Err(HealthReportError::DuplicateCapability(id));
        }
    }

    Ok(normalized.into_values().collect())
}

fn aggregate_health(
    lifecycle: LifecycleState,
    liveness: &LivenessReport,
    readiness: &ReadinessReport,
    dependencies: &[DependencyReport],
    capabilities: &[CapabilityHealthReport],
) -> HealthStatus {
    let liveness_status = liveness.status();

    if liveness_status.is_unknown() {
        return HealthStatus::Unknown;
    }

    if liveness_status.is_unhealthy() {
        return HealthStatus::Unhealthy;
    }

    let mut has_unknown = false;
    let mut has_degraded = liveness_status.is_degraded();

    for dependency in dependencies {
        match dependency.status() {
            HealthStatus::Unhealthy
                if dependency.requirement() == DependencyRequirement::Required =>
            {
                return HealthStatus::Unhealthy;
            }
            HealthStatus::Unhealthy | HealthStatus::Degraded => {
                has_degraded = true;
            }
            HealthStatus::Unknown => {
                if dependency.requirement() == DependencyRequirement::Required {
                    has_unknown = true;
                } else {
                    has_degraded = true;
                }
            }
            HealthStatus::Healthy => {}
        }
    }

    for capability in capabilities {
        match capability.status() {
            HealthStatus::Unhealthy => has_degraded = true,
            HealthStatus::Degraded => has_degraded = true,
            HealthStatus::Unknown => has_unknown = true,
            HealthStatus::Healthy => {}
        }
    }

    if has_unknown {
        return HealthStatus::Unknown;
    }

    if has_degraded {
        return HealthStatus::Degraded;
    }

    // A known "not ready" report is not by itself a health failure while an
    // engine is intentionally outside the serving state, such as draining.
    match readiness.state() {
        ReadinessState::Unknown => {
            if matches!(lifecycle, LifecycleState::Serving) {
                HealthStatus::Unknown
            } else {
                HealthStatus::Degraded
            }
        }
        ReadinessState::NotReady if lifecycle == LifecycleState::Serving => HealthStatus::Degraded,
        ReadinessState::Ready | ReadinessState::NotReady => HealthStatus::Healthy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::capabilities::CapabilityHealthReport;
    use crate::health::dependencies::{DependencyId, DependencyRequirement};
    use crate::health::liveness::LivenessReport;
    use crate::health::readiness::ReadinessReport;
    use crate::health::status::HealthStatus;
    use crate::identity::{CapabilityId, EngineId};
    use crate::runtime::lifecycle::LifecycleState;

    fn engine_id() -> EngineId {
        EngineId::new("report-test-engine").expect("valid engine id")
    }

    fn capability(id: &str, status: HealthStatus) -> CapabilityHealthReport {
        CapabilityHealthReport::new(CapabilityId::new(id).expect("valid capability id"), status)
    }

    fn dependency(
        id: &str,
        requirement: DependencyRequirement,
        status: HealthStatus,
    ) -> DependencyReport {
        DependencyReport::new(
            DependencyId::new(id).expect("valid dependency id"),
            requirement,
            status,
        )
    }

    fn serving_liveness() -> LivenessReport {
        LivenessReport::healthy()
    }

    fn serving_readiness() -> ReadinessReport {
        ReadinessReport::from_lifecycle(LifecycleState::Serving)
    }

    #[test]
    fn preserves_engine_and_component_observations() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Healthy,
            )],
            vec![capability("quran.search", HealthStatus::Healthy)],
        )
        .expect("valid report");

        assert_eq!(report.engine_id().as_str(), "report-test-engine");
        assert_eq!(report.lifecycle(), LifecycleState::Serving);
        assert_eq!(report.overall(), HealthStatus::Healthy);
        assert_eq!(report.dependencies().len(), 1);
        assert_eq!(report.capabilities().len(), 1);
        assert!(
            report
                .dependency(&DependencyId::new("database").unwrap())
                .is_some()
        );
        assert!(
            report
                .capability(&CapabilityId::new("quran.search").unwrap())
                .is_some()
        );
    }

    #[test]
    fn normalizes_dependencies_and_capabilities_deterministically() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![
                dependency(
                    "zeta",
                    DependencyRequirement::Optional,
                    HealthStatus::Healthy,
                ),
                dependency(
                    "alpha",
                    DependencyRequirement::Optional,
                    HealthStatus::Healthy,
                ),
            ],
            vec![
                capability("zeta.capability", HealthStatus::Healthy),
                capability("alpha.capability", HealthStatus::Healthy),
            ],
        )
        .expect("valid report");

        assert_eq!(report.dependencies()[0].id().as_str(), "alpha");
        assert_eq!(report.dependencies()[1].id().as_str(), "zeta");
        assert_eq!(
            report.capabilities()[0].capability_id().as_str(),
            "alpha.capability"
        );
        assert_eq!(
            report.capabilities()[1].capability_id().as_str(),
            "zeta.capability"
        );
    }

    #[test]
    fn rejects_duplicate_dependencies() {
        let error = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![
                dependency(
                    "database",
                    DependencyRequirement::Required,
                    HealthStatus::Healthy,
                ),
                dependency(
                    "database",
                    DependencyRequirement::Required,
                    HealthStatus::Degraded,
                ),
            ],
            Vec::new(),
        )
        .expect_err("duplicate dependency must fail");

        assert!(matches!(error, HealthReportError::DuplicateDependency(_)));
    }

    #[test]
    fn rejects_duplicate_capabilities() {
        let error = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            Vec::new(),
            vec![
                capability("search", HealthStatus::Healthy),
                capability("search", HealthStatus::Degraded),
            ],
        )
        .expect_err("duplicate capability must fail");

        assert!(matches!(error, HealthReportError::DuplicateCapability(_)));
    }

    #[test]
    fn healthy_when_all_required_components_are_healthy() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Healthy,
            )],
            vec![capability("search", HealthStatus::Healthy)],
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Healthy);
    }

    #[test]
    fn degraded_for_optional_dependency_failure() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![dependency(
                "cache",
                DependencyRequirement::Optional,
                HealthStatus::Unhealthy,
            )],
            Vec::new(),
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Degraded);
    }

    #[test]
    fn unhealthy_for_required_dependency_failure() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Unhealthy,
            )],
            Vec::new(),
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Unhealthy);
    }

    #[test]
    fn unknown_for_required_dependency_with_unknown_status() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Unknown,
            )],
            Vec::new(),
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Unknown);
    }

    #[test]
    fn unknown_liveness_makes_overall_unknown() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::unknown(),
            serving_readiness(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Unknown);
    }

    #[test]
    fn unhealthy_liveness_makes_overall_unhealthy() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::unhealthy(),
            serving_readiness(),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid report");

        assert_eq!(report.overall(), HealthStatus::Unhealthy);
    }

    #[test]
    fn draining_with_healthy_liveness_is_not_automatically_unhealthy() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Draining,
            serving_liveness(),
            ReadinessReport::from_lifecycle(LifecycleState::Draining),
            Vec::new(),
            Vec::new(),
        )
        .expect("valid report");

        assert_ne!(report.overall(), HealthStatus::Unhealthy);
    }

    #[test]
    fn explicit_timestamp_is_preserved() {
        let observed_at = SystemTime::UNIX_EPOCH;

        let report = HealthReport::at(
            engine_id(),
            LifecycleState::Serving,
            serving_liveness(),
            serving_readiness(),
            Vec::new(),
            Vec::new(),
            observed_at,
        )
        .expect("valid report");

        assert_eq!(report.observed_at(), observed_at);
    }
}
