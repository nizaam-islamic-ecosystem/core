//! Phase 11 integration tests for health across Core module boundaries.
//!
//! Health implementation files contain their own unit tests and
//! `health/mod.rs` covers composition internal to the health subsystem.
//! These tests exercise the health boundary against the existing runtime,
//! lifecycle, capability, dependency, and execution-context contracts.

use nizaam_core::health::{
    CapabilityHealthReport, DependencyId, DependencyReport, DependencyRequirement, HealthReport,
    HealthStatus, LivenessReport, ReadinessReport, ReadinessState,
};
use nizaam_core::identity::{CapabilityId, EngineId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState};

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        nizaam_core::identity::OperationId::new("health-operation").unwrap(),
        nizaam_core::identity::CorrelationId::new("health-correlation").unwrap(),
    ))
}

fn engine_context() -> EngineContext {
    EngineContext::new(operation_context())
}

fn engine_id() -> EngineId {
    EngineId::new("phase11-health-engine").unwrap()
}

fn healthy_dependency(id: &str, requirement: DependencyRequirement) -> DependencyReport {
    DependencyReport::healthy(DependencyId::new(id).unwrap(), requirement)
}

fn healthy_capability(id: &str) -> CapabilityHealthReport {
    CapabilityHealthReport::healthy(CapabilityId::new(id).unwrap())
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new();

    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime
            .transition(state)
            .expect("runtime should reach the serving lifecycle state");
    }

    runtime
}

fn report_for(
    lifecycle: LifecycleState,
    liveness: LivenessReport,
    readiness: ReadinessReport,
    dependencies: Vec<DependencyReport>,
    capabilities: Vec<CapabilityHealthReport>,
) -> HealthReport {
    HealthReport::new(
        engine_id(),
        lifecycle,
        liveness,
        readiness,
        dependencies,
        capabilities,
    )
    .expect("health report should be valid")
}

#[test]
fn health_report_reflects_the_runtime_lifecycle_state() {
    let runtime = serving_runtime();
    let state = runtime.state();

    let report = report_for(
        state,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(state),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(report.lifecycle(), LifecycleState::Serving);
    assert_eq!(report.readiness().state(), ReadinessState::Ready);
}

#[test]
fn health_observation_does_not_change_runtime_lifecycle_state() {
    let runtime = serving_runtime();
    let before = runtime.state();

    let _report = report_for(
        before,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(before),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(runtime.state(), before);
}

#[test]
fn liveness_and_readiness_are_independent_health_observations() {
    let runtime = serving_runtime();

    let liveness = LivenessReport::healthy();
    let readiness = ReadinessReport::from_lifecycle(runtime.state());

    let report = report_for(
        runtime.state(),
        liveness.clone(),
        readiness.clone(),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(report.liveness(), &liveness);
    assert_eq!(report.readiness(), &readiness);
}

#[test]
fn readiness_tracks_the_authoritative_runtime_lifecycle() {
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
        LifecycleState::Serving,
    ] {
        let readiness = ReadinessReport::from_lifecycle(state);

        if state == LifecycleState::Serving {
            assert!(readiness.is_ready());
            assert_eq!(readiness.state(), ReadinessState::Ready);
        } else {
            assert!(!readiness.is_ready());
            assert_eq!(readiness.state(), ReadinessState::NotReady);
        }
    }
}

#[test]
fn draining_lifecycle_state_is_visible_without_becoming_automatically_unhealthy() {
    let report = report_for(
        LifecycleState::Draining,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Draining),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(report.lifecycle(), LifecycleState::Draining);
    assert_eq!(report.readiness().state(), ReadinessState::NotReady);
    assert_ne!(report.overall(), HealthStatus::Unhealthy);
}

#[test]
fn health_report_includes_real_capability_health_observations() {
    let capability = healthy_capability("phase11.capability");

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        vec![capability.clone()],
    );

    assert_eq!(report.capabilities(), &[capability]);
    assert_eq!(
        report
            .capability(&CapabilityId::new("phase11.capability").unwrap())
            .unwrap()
            .status(),
        HealthStatus::Healthy
    );
}

#[test]
fn unhealthy_capability_is_reflected_in_aggregate_health() {
    let capability =
        CapabilityHealthReport::unhealthy(CapabilityId::new("phase11.capability").unwrap());

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        vec![capability],
    );

    assert_eq!(report.overall(), HealthStatus::Degraded);
}

#[test]
fn health_report_includes_real_dependency_observations() {
    let dependency = healthy_dependency("phase11.database", DependencyRequirement::Required);

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        vec![dependency.clone()],
        Vec::new(),
    );

    assert_eq!(report.dependencies(), &[dependency]);
    assert_eq!(
        report
            .dependency(&DependencyId::new("phase11.database").unwrap())
            .unwrap()
            .requirement(),
        DependencyRequirement::Required
    );
}

#[test]
fn failed_required_dependency_is_reflected_in_aggregate_health() {
    let dependency = DependencyReport::unhealthy(
        DependencyId::new("phase11.database").unwrap(),
        DependencyRequirement::Required,
    );

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        vec![dependency],
        Vec::new(),
    );

    assert_eq!(report.overall(), HealthStatus::Unhealthy);
}

#[test]
fn required_unknown_dependency_is_reflected_as_unknown() {
    let dependency = DependencyReport::unknown(
        DependencyId::new("phase11.database").unwrap(),
        DependencyRequirement::Required,
    );

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        vec![dependency],
        Vec::new(),
    );

    assert_eq!(report.overall(), HealthStatus::Unknown);
}

#[test]
fn optional_unknown_dependency_does_not_hide_a_known_runtime_state() {
    let dependency = DependencyReport::unknown(
        DependencyId::new("phase11.cache").unwrap(),
        DependencyRequirement::Optional,
    );

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        vec![dependency],
        Vec::new(),
    );

    assert_eq!(report.overall(), HealthStatus::Degraded);
}

#[test]
fn health_aggregation_is_deterministic_for_identical_observations() {
    let dependencies = vec![
        healthy_dependency("cache", DependencyRequirement::Optional),
        healthy_dependency("database", DependencyRequirement::Required),
    ];

    let capabilities = vec![
        healthy_capability("phase11.search"),
        healthy_capability("phase11.tafsir"),
    ];

    let observed_at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);

    let liveness = LivenessReport::at(HealthStatus::Healthy, observed_at);
    let readiness = ReadinessReport::at(ReadinessState::Ready, observed_at);

    let first = report_for(
        LifecycleState::Serving,
        liveness.clone(),
        readiness.clone(),
        dependencies.clone(),
        capabilities.clone(),
    );

    let second = report_for(
        LifecycleState::Serving,
        liveness,
        readiness,
        dependencies,
        capabilities,
    );

    assert_eq!(first.engine_id(), second.engine_id());
    assert_eq!(first.lifecycle(), second.lifecycle());
    assert_eq!(first.liveness(), second.liveness());
    assert_eq!(first.readiness(), second.readiness());
    assert_eq!(first.dependencies(), second.dependencies());
    assert_eq!(first.capabilities(), second.capabilities());
    assert_eq!(first.overall(), second.overall());
}

#[test]
fn health_observation_does_not_start_runtime_work() {
    let runtime = EngineRuntime::new();
    let before = runtime.state();

    let _report = report_for(
        before,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(before),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(runtime.state(), before);
    assert!(!runtime.shutdown_token().is_cancelled());
}

#[test]
fn health_can_observe_a_real_engine_runtime() {
    let runtime = serving_runtime();

    let report = report_for(
        runtime.state(),
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(runtime.state()),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(report.lifecycle(), runtime.state());
    assert_eq!(report.readiness().state(), ReadinessState::Ready);
}

#[test]
fn health_observation_does_not_replace_engine_execution_context() {
    let context = engine_context();
    let operation_id = context.operation().operation.id.clone();
    let correlation_id = context.operation().operation.correlation_id.clone();

    let report = report_for(
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    );

    assert_eq!(report.overall(), HealthStatus::Healthy);
    assert_eq!(context.operation().operation.id, operation_id);
    assert_eq!(context.operation().operation.correlation_id, correlation_id);
}

#[test]
fn complete_health_report_observes_runtime_dependencies_and_capabilities() {
    let runtime = serving_runtime();

    let dependencies = vec![
        healthy_dependency("cache", DependencyRequirement::Optional),
        healthy_dependency("database", DependencyRequirement::Required),
    ];
    let capabilities = vec![
        healthy_capability("phase11.search"),
        healthy_capability("phase11.tafsir"),
    ];

    let report = report_for(
        runtime.state(),
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(runtime.state()),
        dependencies,
        capabilities,
    );

    assert_eq!(report.engine_id().as_str(), "phase11-health-engine");
    assert_eq!(report.lifecycle(), LifecycleState::Serving);
    assert_eq!(report.liveness().status(), HealthStatus::Healthy);
    assert_eq!(report.readiness().state(), ReadinessState::Ready);
    assert_eq!(report.dependencies().len(), 2);
    assert_eq!(report.capabilities().len(), 2);
    assert_eq!(report.overall(), HealthStatus::Healthy);
}
