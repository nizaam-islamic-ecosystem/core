//! Control Plane composition boundary.
//!
//! `plane.rs` is the public composition point for the Phase 15 Control Plane.
//!
//! It intentionally contains very little domain logic. Its responsibility is
//! to expose one coherent Control Plane boundary while delegating actual
//! planning, replanning, resolution, and routing semantics to their owning
//! modules.
//!
//! The intended flow is:
//!
//! ```text
//! global coordination requirements
//!             ↓
//!          plan.rs
//!             ↓
//!       replanning.rs
//!             ↓
//!        resolution.rs
//!             ↓
//!          routing.rs
//!             ↓
//!       communication
//!             ↓
//!      Engine Runtime
//!             ↓
//!         capability
//! ```
//!
//! `ControlPlane` does not:
//! - execute capabilities;
//! - interpret domain workflows;
//! - own engine-local state;
//! - own the authoritative registry;
//! - implement health/readiness;
//! - create execution attempts;
//! - implement retry policy;
//! - perform transport;
//! - route individual transport frames;
//! - mutate an already-issued routing decision;
//! - create a competing security system;
//! - create a competing observability system;
//! - silently wait for destinations;
//! - silently introduce fallback behavior.
//!
//! The Control Plane remains communication-focused. The target Engine Runtime
//! remains authoritative for local request admission and capability execution.

use super::replanning::{Replanner, ReplanningDecision, ReplanningInput};
use super::resolution::{Resolution, ResolutionInput};
use super::routing::{Router, RoutingDecision, RoutingError, RoutingInput};

/// The stateless Control Plane composition boundary.
///
/// `ControlPlane` does not own authoritative discovery, health, retry,
/// transport, execution, or domain state. It composes the independently owned
/// Phase 15 mechanisms.
///
/// Keeping this type stateless also avoids creating a second global source of
/// truth for registry, membership, health, or routing state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlPlane;

impl ControlPlane {
    /// Creates a stateless Control Plane.
    pub const fn new() -> Self {
        Self
    }

    /// Evaluates whether a proposed global coordination state requires a new
    /// plan version.
    ///
    /// The actual replanning semantics remain owned by `replanning.rs`.
    /// `ControlPlane` only exposes that mechanism through the composed
    /// boundary.
    pub fn evaluate_replanning(&self, input: ReplanningInput<'_>) -> ReplanningDecision {
        Replanner::new().evaluate(input)
    }

    /// Convenience form of [`Self::evaluate_replanning`].
    pub fn evaluate_replanning_snapshots(
        &self,
        current: &super::replanning::CoordinationSnapshot,
        proposed: &super::replanning::CoordinationSnapshot,
    ) -> ReplanningDecision {
        Replanner::evaluate_snapshots(current, proposed)
    }

    /// Resolves an already-constructed Control Plane resolution input.
    ///
    /// Resolution remains responsible for:
    ///
    /// - preserving the operation identity;
    /// - carrying the selected provider/routing information;
    /// - producing the immutable `Resolution` snapshot.
    ///
    /// This method does not execute or route the resolved capability.
    pub fn resolve(&self, input: ResolutionInput) -> Resolution {
        Resolution::resolve(input)
    }

    /// Routes an already-resolved interaction for an already-created attempt.
    ///
    /// Attempt creation and retry policy remain outside the Control Plane.
    /// `routing.rs` remains responsible for binding the immutable resolution
    /// to the concrete execution attempt.
    pub fn route(&self, input: RoutingInput<'_>) -> Result<RoutingDecision, RoutingError> {
        Router::new().route(input)
    }

    /// Convenience form of [`Self::route`].
    pub fn route_resolved(
        &self,
        resolution: &Resolution,
        attempt: &crate::retry::Attempt,
    ) -> Result<RoutingDecision, RoutingError> {
        Router::route_resolved(resolution, attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::control_plane::dependency::{
        CapabilityRequirement, Dependency, DependencyKind, DependencyTarget,
    };
    use crate::control_plane::policy::RoutingStrategy;
    use crate::control_plane::replanning::CoordinationSnapshot;
    use crate::control_plane::resolution::{
        ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedRouting,
    };
    use crate::identity::{
        AttemptId, CapabilityId, ContractId, CorrelationId, EngineInstanceId, NodeId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::retry::Attempt;

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn attempt_id(value: &str) -> AttemptId {
        AttemptId::new(value).unwrap()
    }

    fn engine_instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).unwrap()
    }

    fn capability(value: &str) -> CapabilityRequirement {
        CapabilityRequirement::new(CapabilityId::new(value).unwrap())
    }

    fn dependency(source: &str, capability_name: &str, kind: DependencyKind) -> Dependency {
        let source = NodeId::new(source).unwrap();
        let target = DependencyTarget::Capability(capability(capability_name));

        match kind {
            DependencyKind::Blocking => Dependency::blocking(source, target).unwrap(),
            DependencyKind::NonBlocking => Dependency::non_blocking(source, target).unwrap(),
            DependencyKind::Conditional => Dependency::conditional(
                source,
                target,
                crate::control_plane::dependency::ConditionReference::new("condition").unwrap(),
            )
            .unwrap(),
        }
    }

    fn contract() -> ResolvedContract {
        ResolvedContract::new(ContractId::new("quran.analyze").unwrap(), "1.0")
    }

    fn resolved_capability() -> ResolvedCapability {
        ResolvedCapability::new(CapabilityId::new("quran.analyze").unwrap())
    }

    fn resolution_for(operation: &OperationId, destination: &EngineInstanceId) -> Resolution {
        Resolution::resolve(ResolutionInput::new(
            operation.clone(),
            contract(),
            resolved_capability(),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
        ))
    }

    fn attempt_for(operation: &OperationId, attempt: &AttemptId) -> Attempt {
        Attempt::new(operation.clone(), attempt.clone(), 1).unwrap()
    }

    #[test]
    fn creates_stateless_control_plane() {
        let first = ControlPlane::new();
        let second = ControlPlane::new();

        assert_eq!(first, second);
    }

    #[test]
    fn default_control_plane_is_equivalent_to_new() {
        assert_eq!(ControlPlane, ControlPlane::new());
    }

    #[test]
    fn replanning_is_delegated_through_control_plane() {
        let current = CoordinationSnapshot::new();

        let proposed = CoordinationSnapshot::from_dependencies([dependency(
            "hadith",
            "quran.search",
            DependencyKind::Blocking,
        )]);

        let plane = ControlPlane::new();

        let decision = plane.evaluate_replanning_snapshots(&current, &proposed);

        assert!(decision.is_required());
    }

    #[test]
    fn identical_coordination_state_does_not_require_replanning() {
        let requirement = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([requirement.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([requirement]);

        let decision = ControlPlane::new().evaluate_replanning_snapshots(&current, &proposed);

        assert_eq!(decision, ReplanningDecision::NotRequired);
    }

    #[test]
    fn resolution_is_delegated_through_control_plane() {
        let operation = operation_id("operation-1");
        let destination = engine_instance_id("engine-01");

        let input = ResolutionInput::new(
            operation.clone(),
            contract(),
            resolved_capability(),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
        );

        let resolution = ControlPlane::new().resolve(input);

        assert_eq!(resolution.operation_id(), &operation);

        assert_eq!(resolution.destination(), &destination);
    }

    #[test]
    fn routing_is_delegated_through_control_plane() {
        let operation = operation_id("operation-2");
        let attempt = attempt_id("attempt-2");
        let destination = engine_instance_id("engine-02");

        let resolution = resolution_for(&operation, &destination);

        let execution_attempt = attempt_for(&operation, &attempt);

        let decision = ControlPlane::new()
            .route_resolved(&resolution, &execution_attempt)
            .unwrap();

        assert_eq!(decision.operation_id(), &operation);

        assert_eq!(decision.attempt_id(), &attempt);

        assert_eq!(decision.destination(), &destination);
    }

    #[test]
    fn routing_preserves_existing_attempt_identity() {
        let operation = operation_id("operation-3");
        let attempt_id = attempt_id("attempt-3");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-03"));

        let attempt = attempt_for(&operation, &attempt_id);

        let decision = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap();

        assert_eq!(decision.attempt_id(), &attempt_id);

        assert_eq!(attempt.attempt_id(), &attempt_id);

        assert_eq!(attempt.attempt_number(), 1);
    }

    #[test]
    fn control_plane_does_not_change_attempt_lifecycle() {
        let operation = operation_id("operation-4");
        let attempt_id = attempt_id("attempt-4");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-04"));

        let attempt = attempt_for(&operation, &attempt_id);

        assert_eq!(
            attempt.state(),
            crate::retry::AttemptLifecycleState::Created
        );

        let _decision = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap();

        assert_eq!(
            attempt.state(),
            crate::retry::AttemptLifecycleState::Created
        );
    }

    #[test]
    fn control_plane_does_not_change_resolution() {
        let operation = operation_id("operation-5");
        let destination = engine_instance_id("engine-05");

        let resolution = resolution_for(&operation, &destination);

        let before_operation = resolution.operation_id().clone();

        let before_destination = resolution.destination().clone();

        let attempt = attempt_for(&operation, &attempt_id("attempt-5"));

        let _decision = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap();

        assert_eq!(resolution.operation_id(), &before_operation);

        assert_eq!(resolution.destination(), &before_destination);
    }

    #[test]
    fn control_plane_does_not_select_a_new_destination() {
        let operation = operation_id("operation-6");
        let destination = engine_instance_id("selected-engine");

        let resolution = resolution_for(&operation, &destination);

        let attempt = attempt_for(&operation, &attempt_id("attempt-6"));

        let decision = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap();

        assert_eq!(decision.destination().as_str(), "selected-engine");
    }

    #[test]
    fn control_plane_preserves_routing_errors() {
        let resolution_operation = operation_id("operation-resolution");

        let attempt_operation = operation_id("operation-attempt");

        let resolution = resolution_for(&resolution_operation, &engine_instance_id("engine-07"));

        let attempt = attempt_for(&attempt_operation, &attempt_id("attempt-7"));

        let error = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap_err();

        assert_eq!(
            error,
            RoutingError::OperationMismatch {
                resolution_operation_id: resolution_operation,
                attempt_operation_id: attempt_operation,
            }
        );
    }

    #[test]
    fn control_plane_preserves_attempt_context_validation() {
        let operation = operation_id("operation-8");

        let attempt_in_resolution = attempt_id("attempt-resolution");

        let operation_context = Operation::new(
            operation.clone(),
            CorrelationId::new("correlation-8").unwrap(),
        );

        let context = OperationContext::new(operation_context).for_attempt(
            NodeId::new("node-8").unwrap(),
            attempt_in_resolution.clone(),
        );

        let resolution = Resolution::resolve(
            ResolutionInput::new(
                operation.clone(),
                contract(),
                resolved_capability(),
                ResolvedRouting::new(
                    engine_instance_id("engine-08"),
                    RoutingStrategy::Deterministic,
                ),
            )
            .with_context(context),
        );

        let routing_attempt = attempt_for(&operation, &attempt_id("attempt-routing"));

        let error = ControlPlane::new()
            .route_resolved(&resolution, &routing_attempt)
            .unwrap_err();

        assert_eq!(
            error,
            RoutingError::AttemptMismatch {
                resolution_attempt_id: attempt_in_resolution,
                routing_attempt_id: attempt_id("attempt-routing"),
            }
        );
    }

    #[test]
    fn same_inputs_produce_same_control_plane_results() {
        let operation = operation_id("operation-9");

        let destination = engine_instance_id("engine-09");

        let resolution = resolution_for(&operation, &destination);

        let attempt = attempt_for(&operation, &attempt_id("attempt-9"));

        let plane = ControlPlane::new();

        let first = plane.route_resolved(&resolution, &attempt).unwrap();

        let second = plane.route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn control_plane_instances_do_not_share_mutable_state() {
        let operation = operation_id("operation-10");

        let destination = engine_instance_id("engine-10");

        let resolution = resolution_for(&operation, &destination);

        let attempt = attempt_for(&operation, &attempt_id("attempt-10"));

        let first_plane = ControlPlane::new();

        let second_plane = ControlPlane::new();

        let first = first_plane.route_resolved(&resolution, &attempt).unwrap();

        let second = second_plane.route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn control_plane_does_not_create_a_retry_attempt() {
        let operation = operation_id("operation-11");

        let attempt_id = attempt_id("attempt-11");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-11"));

        let attempt = attempt_for(&operation, &attempt_id);

        let _decision = ControlPlane::new()
            .route_resolved(&resolution, &attempt)
            .unwrap();

        assert_eq!(attempt.attempt_id(), &attempt_id);

        assert_eq!(attempt.attempt_number(), 1);
    }

    #[test]
    fn control_plane_composes_replanning_then_resolution_then_routing() {
        let operation = operation_id("operation-12");

        let attempt_id = attempt_id("attempt-12");

        let destination = engine_instance_id("engine-12");

        let old_requirement = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let new_requirement = dependency("hadith", "arabic.analyze", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([old_requirement.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([old_requirement, new_requirement]);

        let plane = ControlPlane::new();

        let replanning = plane.evaluate_replanning_snapshots(&current, &proposed);

        assert!(replanning.is_required());

        let resolution_input = ResolutionInput::new(
            operation.clone(),
            contract(),
            resolved_capability(),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
        );

        let resolution = plane.resolve(resolution_input);

        let attempt = attempt_for(&operation, &attempt_id);

        let route = plane.route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(route.operation_id(), &operation);

        assert_eq!(route.attempt_id(), &attempt_id);

        assert_eq!(route.destination(), &destination);
    }
}
