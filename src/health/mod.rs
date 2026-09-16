pub mod capabilities;
pub mod dependencies;
pub mod liveness;
pub mod readiness;
pub mod report;
pub mod status;

pub use capabilities::CapabilityHealthReport;
pub use dependencies::{DependencyId, DependencyReport, DependencyRequirement};
pub use liveness::LivenessReport;
pub use readiness::{ReadinessReport, ReadinessState};
pub use report::{HealthReport, HealthReportError};
pub use status::HealthStatus;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CapabilityId, EngineId};
    use crate::runtime::lifecycle::LifecycleState;
    use std::time::SystemTime;

    fn engine_id() -> EngineId {
        EngineId::new("health-module-test-engine").expect("valid engine id")
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

    fn capability(id: &str, status: HealthStatus) -> CapabilityHealthReport {
        CapabilityHealthReport::new(CapabilityId::new(id).expect("valid capability id"), status)
    }

    #[test]
    fn health_modules_compose_into_a_complete_report() {
        let lifecycle = LifecycleState::Serving;
        let liveness = LivenessReport::healthy();
        let readiness = ReadinessReport::from_lifecycle(lifecycle);

        let dependencies = vec![
            dependency(
                "cache",
                DependencyRequirement::Optional,
                HealthStatus::Healthy,
            ),
            dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Healthy,
            ),
        ];

        let capabilities = vec![
            capability("quran.search", HealthStatus::Healthy),
            capability("quran.tafsir", HealthStatus::Healthy),
        ];

        let report = HealthReport::new(
            engine_id(),
            lifecycle,
            liveness.clone(),
            readiness.clone(),
            dependencies,
            capabilities,
        )
        .expect("health observations should form a valid report");

        assert_eq!(report.engine_id().as_str(), "health-module-test-engine");
        assert_eq!(report.lifecycle(), LifecycleState::Serving);
        assert_eq!(report.liveness(), &liveness);
        assert_eq!(report.readiness(), &readiness);
        assert_eq!(report.overall(), HealthStatus::Healthy);

        assert_eq!(report.dependencies().len(), 2);
        assert_eq!(report.capabilities().len(), 2);

        assert_eq!(
            report
                .dependency(&DependencyId::new("database").unwrap())
                .expect("database dependency")
                .status(),
            HealthStatus::Healthy
        );

        assert_eq!(
            report
                .capability(&CapabilityId::new("quran.search").unwrap())
                .expect("search capability")
                .status(),
            HealthStatus::Healthy
        );
    }

    #[test]
    fn readiness_tracks_the_phase_eight_serving_boundary() {
        assert_eq!(
            ReadinessState::from(LifecycleState::Serving),
            ReadinessState::Ready
        );

        for state in [
            LifecycleState::Created,
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
            LifecycleState::Registering,
            LifecycleState::Ready,
            LifecycleState::Draining,
            LifecycleState::Stopped,
        ] {
            assert_eq!(
                ReadinessState::from(state),
                ReadinessState::NotReady,
                "unexpected ready state: {state:?}"
            );
        }
    }

    #[test]
    fn draining_remains_observable_without_becoming_an_automatic_failure() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Draining,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Draining),
            Vec::new(),
            Vec::new(),
        )
        .expect("draining report should be valid");

        assert_eq!(report.lifecycle(), LifecycleState::Draining);
        assert_eq!(report.liveness().status(), HealthStatus::Healthy);
        assert_eq!(report.readiness().state(), ReadinessState::NotReady);
        assert_ne!(report.overall(), HealthStatus::Unhealthy);
    }

    #[test]
    fn unknown_observations_are_not_promoted_to_healthy() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::unknown(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Healthy,
            )],
            vec![capability("quran.search", HealthStatus::Healthy)],
        )
        .expect("report should remain structurally valid");

        assert_eq!(report.overall(), HealthStatus::Unknown);
        assert_ne!(report.overall(), HealthStatus::Healthy);
    }

    #[test]
    fn optional_dependency_degradation_propagates_to_overall_health() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
            vec![dependency(
                "cache",
                DependencyRequirement::Optional,
                HealthStatus::Degraded,
            )],
            Vec::new(),
        )
        .expect("report should remain structurally valid");

        assert_eq!(report.overall(), HealthStatus::Degraded);
    }

    #[test]
    fn required_dependency_failure_propagates_to_unhealthy() {
        let report = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
            vec![dependency(
                "database",
                DependencyRequirement::Required,
                HealthStatus::Unhealthy,
            )],
            Vec::new(),
        )
        .expect("report should remain structurally valid");

        assert_eq!(report.overall(), HealthStatus::Unhealthy);
    }

    #[test]
    fn aggregate_timestamp_can_be_preserved_across_the_health_module() {
        let observed_at = SystemTime::UNIX_EPOCH;

        let report = HealthReport::at(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
            Vec::new(),
            Vec::new(),
            observed_at,
        )
        .expect("report should remain structurally valid");

        assert_eq!(report.observed_at(), observed_at);
    }

    #[test]
    fn duplicate_component_identity_is_rejected_by_the_health_module() {
        let duplicate_dependency = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
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
        );

        assert!(matches!(
            duplicate_dependency,
            Err(HealthReportError::DuplicateDependency(_))
        ));

        let duplicate_capability = HealthReport::new(
            engine_id(),
            LifecycleState::Serving,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(LifecycleState::Serving),
            Vec::new(),
            vec![
                capability("quran.search", HealthStatus::Healthy),
                capability("quran.search", HealthStatus::Degraded),
            ],
        );

        assert!(matches!(
            duplicate_capability,
            Err(HealthReportError::DuplicateCapability(_))
        ));
    }
}
