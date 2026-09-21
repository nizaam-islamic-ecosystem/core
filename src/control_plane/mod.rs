//! Control Plane module root.
//!
//! Phase 15 provides the communication-focused coordination boundary between
//! logical requests and concrete Engine Runtime destinations.
//!
//! The module root has three responsibilities:
//!
//! 1. declare the Control Plane modules;
//! 2. expose the intended public Control Plane API;
//! 3. provide Level 2 module-composition tests.
//!
//! The individual modules remain responsible for their own domain boundaries.
//! `mod.rs` does not introduce another planner, resolver, router, transport
//! layer, retry system, health system, security system, or execution system.
//!
//! The composition boundary is:
//!
//! ```text
//! EngineRegistration
//!         ↓
//! Membership
//!         ↓
//! Contract / Capability identification
//!         ↓
//! Provider candidates
//!         ↓
//! Destination semantics
//!         ↓
//! eligible candidates
//!         ↓
//! RoutingPolicy
//!         ↓
//! Resolution
//!         ↓
//! existing Attempt
//!         ↓
//! RoutingDecision
//!         ↓
//! communication / transport
//!         ↓
//! Engine Runtime
//! ```
//!
//! Global coordination is composed separately:
//!
//! ```text
//! Dependency
//!     ↓
//! CoordinationSnapshot
//!     ↓
//! Replanner
//!     ↓
//! Plan / PlanVersion
//! ```
//!
//! Level 2 tests in this module verify that these independently-owned pieces
//! compose correctly. They do not replace the Level 1 unit tests contained in
//! each implementation file or the Level 3 repository integration tests under
//! `tests/`.

pub mod admission;
pub mod capability;
pub mod communication;
pub mod context;
pub mod contract;
pub mod dependency;
pub mod destination;
pub mod failure;
pub mod lifecycle;
pub mod membership;
pub mod observations;
pub mod plan;
pub mod plane;
pub mod policy;
pub mod provider;
pub mod registration;
pub mod registry;
pub mod replanning;
pub mod resolution;
pub mod routing;

// -----------------------------------------------------------------------------
// Public Control Plane API
// -----------------------------------------------------------------------------
//
// Re-export the primary boundary types rather than glob-importing every module.
// Several modules intentionally expose similarly named helpers such as
// `identify`, so glob re-exports would create an ambiguous public namespace.
//
// The individual modules remain publicly accessible as:
//
//     control_plane::registration::EngineRegistration
//     control_plane::membership::Membership
//     control_plane::policy::RoutingPolicy
//     ...
//
// These re-exports provide convenient access to the principal composition
// objects without changing ownership of the underlying types.

pub use admission::{AdmissionError, admit};

pub use capability::{CapabilityResolutionError, CapabilityResult};

pub use communication::ControlPlaneCommunication;

pub use context::{
    ContextAcceptance, ContextAcceptanceStatus, ContextError, ContextPackage, ContextRequest,
    ContextRequirement, ContextRequirementLevel,
};

pub use contract::{ContractResolutionError, ContractResult};

pub use dependency::{
    CapabilityRequirement, CapabilityRequirementError, ConditionReference, ConditionReferenceError,
    Dependency, DependencyKind, DependencyTarget, DependencyValidationError,
};

pub use destination::{
    Destination, DestinationCandidate, DestinationEligibilityError, DestinationEligibilityInput,
    DestinationRequest, DestinationResult, DestinationStrength, DestinationValidationError,
    EligibleDestination, ExplicitDestination, FallbackPolicy, LogicalDestination,
    eligible_destinations,
};

pub use failure::{ControlPlaneFailure, ControlPlaneFailureKind, ControlPlaneResult};

pub use membership::{
    Membership, MembershipError, MembershipRecord, MembershipResult, MembershipSnapshot,
};

pub use observations::{
    EngineObservation, ObservationError, ObservationResult, ObservationSnapshot, Observations,
};

pub use plan::{
    InvalidPlanStateTransition, Plan, PlanBuildError, PlanBuilder, PlanNode,
    PlanNodeValidationError, PlanState, PlanValidationError, PlanVersion,
};

pub use plane::ControlPlane;

pub use policy::{
    DestinationConstraint, PolicyError, PolicyInput, PolicySelection, RoutingCandidate,
    RoutingConstraints, RoutingPolicy, RoutingStrategy,
};

pub use provider::{
    ProviderDefinitionError, ProviderDescriptor, ProviderResolutionError, ProviderResult,
};

pub use registration::{
    Endpoint, EngineRegistration, RegistrationResult, RegistrationValidationError,
    RuntimeRegistrationMetadata,
};

pub use registry::{EngineRegistry, RegistryError, RegistryRecord, RegistryResult};

pub use replanning::{
    CoordinationSnapshot, Replanner, ReplanningDecision, ReplanningDelta, ReplanningInput,
    ReplanningReason,
};

pub use resolution::{
    Resolution, ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedProvider,
    ResolvedRouting,
};

pub use routing::{Router, RoutingDecision, RoutingError, RoutingInput};

// -----------------------------------------------------------------------------
// Level 2 — module-composition tests
// -----------------------------------------------------------------------------
//
// These tests deliberately live at the Control Plane module root.
//
// Level 1 tests remain in each individual source file.
// Level 3 tests belong under tests/.
//
// The tests below verify composition:
//
// registration
//     → membership
//     → contract/capability
//     → provider
//     → destination
//     → policy
//     → resolution
//     → routing
//     → global coordination
//
// They do not invoke capability handlers or transport.

#[cfg(test)]
mod tests {
    use super::*;

    use crate::capability::CapabilityDefinition;
    use crate::contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest, Version,
    };
    use crate::identity::{
        AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
        NodeId, OperationId, PlanId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::retry::Attempt;
    use crate::runtime::lifecycle::LifecycleState;

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn attempt_id(value: &str) -> AttemptId {
        AttemptId::new(value).unwrap()
    }

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).unwrap()
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).unwrap()
    }

    fn capability_id(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    fn contract_id(value: &str) -> ContractId {
        ContractId::new(value).unwrap()
    }

    fn provider_id(value: &str) -> String {
        value.to_owned()
    }

    fn node_id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn plan_id(value: &str) -> PlanId {
        PlanId::new(value).unwrap()
    }

    fn operation(value: &str) -> Operation {
        Operation::new(
            operation_id(value),
            CorrelationId::new(format!("correlation-{value}")).unwrap(),
        )
    }

    fn operation_context(value: &str) -> OperationContext {
        OperationContext::new(operation(value))
    }

    fn payload_descriptor() -> PayloadDescriptor {
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap()
    }

    fn contract_descriptor(contract: &str, capability: &str) -> ContractDescriptor {
        ContractDescriptor::new(
            contract_id(contract),
            capability_id(capability),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload_descriptor(),
        )
    }

    fn capability_definition(engine: &str, capability: &str) -> CapabilityDefinition {
        CapabilityDefinition::new(capability_id(capability), engine_id(engine), capability).unwrap()
    }

    fn registration(engine: &str, instance: &str, capability: &str) -> EngineRegistration {
        EngineRegistration::new(engine_id(engine), instance_id(instance))
            .with_capability(capability_definition(engine, capability))
            .unwrap()
            .with_contract(contract_descriptor("quran.analyze", capability))
    }

    fn attempt(operation: &OperationId, attempt: &str, number: u32) -> Attempt {
        Attempt::new(operation.clone(), attempt_id(attempt), number).unwrap()
    }

    fn resolved_contract() -> ResolvedContract {
        ResolvedContract::new(contract_id("quran.analyze"), "1.0.0")
    }

    fn resolved_capability() -> ResolvedCapability {
        ResolvedCapability::new(capability_id("quran.analyze"))
    }

    fn resolution(operation: &OperationId, destination: &EngineInstanceId) -> Resolution {
        Resolution::resolve(ResolutionInput::new(
            operation.clone(),
            resolved_contract(),
            resolved_capability(),
            ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic),
        ))
    }

    fn request() -> UniversalRequest {
        let descriptor = contract_descriptor("lookup.request", "lookup");

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(engine_id("caller"), engine_id("provider")),
        );

        let envelope = MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            operation_context("admission-operation"),
            metadata,
            EncodedPayload::new(descriptor.payload.clone(), b"opaque payload".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    fn capability_requirement(capability: &str) -> CapabilityRequirement {
        CapabilityRequirement::new(capability_id(capability))
    }

    fn dependency(source: &str, capability: &str) -> Dependency {
        Dependency::blocking(
            node_id(source),
            DependencyTarget::Capability(capability_requirement(capability)),
        )
        .unwrap()
    }

    // -------------------------------------------------------------------------
    // 1. Module-root sanity
    // -------------------------------------------------------------------------

    #[test]
    fn control_plane_module_exposes_stateless_facade() {
        let first = ControlPlane::new();
        let second = ControlPlane::new();

        assert_eq!(first, second);
        assert_eq!(ControlPlane, first);
    }

    // -------------------------------------------------------------------------
    // 2. Admission → contract → capability
    // -------------------------------------------------------------------------

    #[test]
    fn admission_contract_and_capability_compose() {
        let request = request();

        admit(&request).unwrap();

        let contract = contract::identify_request(&request);
        assert_eq!(contract.contract_id, contract_id("lookup.request"));

        let capability = capability::identify_request(&request);
        assert_eq!(capability, &capability_id("lookup"));
    }

    #[test]
    fn contract_compatibility_and_capability_advertisement_compose() {
        let required = contract_descriptor("quran.analyze", "quran.analyze");

        let offered = required.clone();

        contract::require_compatible(&required, &offered).unwrap();

        let advertised = capability_definition("QuranEngine", "quran.analyze");

        let advertised_capabilities = [advertised];

        let matches = capability::require_advertised(
            &capability_id("quran.analyze"),
            &advertised_capabilities,
        )
        .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].capability_id(), &capability_id("quran.analyze"));
    }

    // -------------------------------------------------------------------------
    // 3. Registration → membership
    // -------------------------------------------------------------------------

    #[test]
    fn registration_flows_into_membership_snapshot() {
        let membership = Membership::new();

        let registration = registration("ArabicEngine", "arabic-01", "arabic.analyze");

        membership.register(registration).unwrap();

        let snapshot = membership.snapshot();

        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.contains(&instance_id("arabic-01")));

        let record = snapshot.get(&instance_id("arabic-01")).unwrap();

        assert_eq!(record.engine_id(), &engine_id("ArabicEngine"));

        assert_eq!(record.engine_instance_id(), &instance_id("arabic-01"));
    }

    #[test]
    fn one_logical_engine_can_have_multiple_membership_instances() {
        let membership = Membership::new();

        membership
            .register(registration("ArabicEngine", "arabic-01", "arabic.analyze"))
            .unwrap();

        membership
            .register(registration("ArabicEngine", "arabic-02", "arabic.analyze"))
            .unwrap();

        membership
            .register(registration("ArabicEngine", "arabic-03", "arabic.analyze"))
            .unwrap();

        let snapshot = membership.snapshot();

        assert_eq!(snapshot.len(), 3);
        assert_eq!(
            snapshot
                .instances_for_engine(&engine_id("ArabicEngine"))
                .count(),
            3
        );
    }

    // -------------------------------------------------------------------------
    // 4. Membership → capability/provider resolution
    // -------------------------------------------------------------------------

    #[test]
    fn membership_advertisement_can_feed_capability_resolution() {
        let membership = Membership::new();

        membership
            .register(registration("QuranEngine", "quran-01", "quran.analyze"))
            .unwrap();

        membership
            .register(registration("QuranEngine", "quran-02", "quran.analyze"))
            .unwrap();

        let snapshot = membership.snapshot();

        let advertised = snapshot
            .candidates()
            .flat_map(|record| record.registration().capabilities())
            .cloned()
            .collect::<Vec<_>>();

        let matches =
            capability::require_advertised(&capability_id("quran.analyze"), &advertised).unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn capability_and_contract_can_feed_provider_resolution() {
        let capability = capability_id("quran.analyze");
        let contract = contract_descriptor("quran.analyze", "quran.analyze");

        let provider = ProviderDescriptor::new(
            provider_id("quran-provider"),
            engine_id("QuranEngine"),
            vec![capability.clone()],
        )
        .unwrap()
        .with_contract(contract.clone())
        .unwrap();

        let providers = vec![provider];

        let matches = provider::require_compatible(&capability, &contract, &providers).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].owning_engine(), &engine_id("QuranEngine"));
    }

    // -------------------------------------------------------------------------
    // 5. Dependency → destination
    // -------------------------------------------------------------------------

    #[test]
    fn global_dependency_can_become_logical_destination_requirement() {
        let requirement = capability_requirement("quran.search");

        requirement.validate().unwrap();

        let destination = Destination::logical(requirement.clone());

        assert!(destination.is_logical());
        assert!(!destination.is_explicit());

        let logical = destination.logical_destination().unwrap();

        assert_eq!(
            logical.requirement().capability_id(),
            &capability_id("quran.search")
        );
    }

    #[test]
    fn explicit_destination_preserves_concrete_instance_identity() {
        let destination = Destination::explicit(instance_id("quran-02"));

        assert!(destination.is_explicit());
        assert_eq!(destination.instance_id(), Some(&instance_id("quran-02")));

        let request = DestinationRequest::hard_explicit(instance_id("quran-02"));

        request.validate().unwrap();

        assert!(request.is_hard());
        assert!(!request.allows_fallback());
    }

    // -------------------------------------------------------------------------
    // 6. Membership snapshot → policy
    // -------------------------------------------------------------------------

    #[test]
    fn membership_snapshot_can_supply_policy_candidates() {
        let membership = Membership::new();

        membership
            .register(registration("ArabicEngine", "arabic-01", "arabic.analyze"))
            .unwrap();

        membership
            .register(registration("ArabicEngine", "arabic-02", "arabic.analyze"))
            .unwrap();

        let snapshot = membership.snapshot();

        let candidates = snapshot
            .candidates()
            .map(|record| RoutingCandidate::new(record.engine_instance_id().clone()))
            .collect::<Vec<_>>();

        let constraints = RoutingConstraints::new();

        let input = PolicyInput::new(&candidates, &constraints);

        let selection = RoutingPolicy::deterministic().evaluate(&input).unwrap();

        assert_eq!(selection.instance_id(), &instance_id("arabic-01"));
    }

    #[test]
    fn hard_destination_is_not_replaced_by_policy_fallback() {
        let candidates = vec![
            RoutingCandidate::new(instance_id("arabic-01")),
            RoutingCandidate::new(instance_id("arabic-02")),
        ];

        let constraints = RoutingConstraints::new().with_hard_destination(instance_id("arabic-99"));

        let input = PolicyInput::new(&candidates, &constraints);

        let result = RoutingPolicy::deterministic().evaluate(&input);

        assert_eq!(
            result,
            Err(PolicyError::HardDestinationUnavailable(instance_id(
                "arabic-99"
            )))
        );
    }

    #[test]
    fn preferred_destination_can_fall_back_to_an_eligible_candidate() {
        let candidates = vec![
            RoutingCandidate::new(instance_id("arabic-01")),
            RoutingCandidate::new(instance_id("arabic-02")),
        ];

        let constraints =
            RoutingConstraints::new().with_preferred_destination(instance_id("arabic-99"));

        let input = PolicyInput::new(&candidates, &constraints);

        let selection = RoutingPolicy::deterministic().evaluate(&input).unwrap();

        assert_eq!(selection.instance_id(), &instance_id("arabic-01"));
    }

    // -------------------------------------------------------------------------
    // 7. Policy → resolution → routing
    // -------------------------------------------------------------------------

    #[test]
    fn policy_resolution_and_routing_compose_for_one_attempt() {
        let operation = operation_id("operation-routing-1");
        let destination = instance_id("arabic-01");

        let candidates = vec![
            RoutingCandidate::new(destination.clone()),
            RoutingCandidate::new(instance_id("arabic-02")),
        ];

        let constraints = RoutingConstraints::new();

        let policy_input = PolicyInput::new(&candidates, &constraints);

        let selection = RoutingPolicy::deterministic()
            .evaluate(&policy_input)
            .unwrap();

        let resolved = Resolution::resolve(ResolutionInput::new(
            operation.clone(),
            resolved_contract(),
            resolved_capability(),
            ResolvedRouting::new(selection.into_instance_id(), RoutingStrategy::Deterministic),
        ));

        let execution_attempt = attempt(&operation, "attempt-1", 1);

        let decision = Router::route_resolved(&resolved, &execution_attempt).unwrap();

        assert_eq!(decision.operation_id(), &operation);

        assert_eq!(decision.attempt_id(), execution_attempt.attempt_id());

        assert_eq!(decision.destination(), &destination);
    }

    // -------------------------------------------------------------------------
    // 8. Resolution snapshot / attempt stability
    // -------------------------------------------------------------------------

    #[test]
    fn routing_decision_remains_stable_after_later_state_changes() {
        let operation = operation_id("stable-operation");
        let first_destination = instance_id("arabic-01");

        let resolved = resolution(&operation, &first_destination);

        let execution_attempt = attempt(&operation, "attempt-1", 1);

        let decision = Router::route_resolved(&resolved, &execution_attempt).unwrap();

        // Simulate later Control Plane state changing elsewhere.
        let later_destination = instance_id("arabic-02");

        assert_ne!(later_destination, *decision.destination());

        // The already-issued decision remains immutable.
        assert_eq!(decision.destination(), &first_destination);
    }

    #[test]
    fn a_new_attempt_can_receive_a_new_routing_decision() {
        let operation = operation_id("retry-operation");

        let first_resolution = resolution(&operation, &instance_id("arabic-01"));

        let first_attempt = attempt(&operation, "attempt-1", 1);

        let first_decision = Router::route_resolved(&first_resolution, &first_attempt).unwrap();

        let second_resolution = resolution(&operation, &instance_id("arabic-02"));

        let second_attempt = attempt(&operation, "attempt-2", 2);

        let second_decision = Router::route_resolved(&second_resolution, &second_attempt).unwrap();

        assert_eq!(first_decision.attempt_id(), &attempt_id("attempt-1"));

        assert_eq!(first_decision.destination(), &instance_id("arabic-01"));

        assert_eq!(second_decision.attempt_id(), &attempt_id("attempt-2"));

        assert_eq!(second_decision.destination(), &instance_id("arabic-02"));
    }

    // -------------------------------------------------------------------------
    // 9. Context propagation boundary
    // -------------------------------------------------------------------------

    #[test]
    fn context_request_preserves_existing_operation_context() {
        let operation = operation_context("context-operation");

        let requirement =
            ContextRequirement::new("caller.locale", ContextRequirementLevel::Required).unwrap();

        let request = ContextRequest::new(operation.clone(), requirement);

        assert_eq!(request.operation(), &operation);

        assert_eq!(request.requirement().name(), "caller.locale");
    }

    #[test]
    fn context_package_remains_opaque_to_control_plane() {
        let package = ContextPackage::new().with_entry("locale", "ar").unwrap();

        assert_eq!(package.entry("locale"), Some("ar"));

        assert!(!package.is_empty());
    }

    // -------------------------------------------------------------------------
    // 10. Lifecycle → routing eligibility
    // -------------------------------------------------------------------------

    #[test]
    fn lifecycle_serving_state_composes_with_routing_eligibility() {
        assert!(lifecycle::is_routable(LifecycleState::Serving));

        assert!(!lifecycle::is_routable(LifecycleState::Ready));

        assert!(!lifecycle::is_routable(LifecycleState::Draining));

        assert!(!lifecycle::is_routable(LifecycleState::Stopped));
    }

    // -------------------------------------------------------------------------
    // 11. Global dependency → replanning
    // -------------------------------------------------------------------------

    #[test]
    fn unchanged_global_coordination_does_not_require_replanning() {
        let dependency = dependency("hadith", "quran.search");

        let current = CoordinationSnapshot::from_dependencies([dependency.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([dependency]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(decision, ReplanningDecision::NotRequired);
    }

    #[test]
    fn changed_global_coordination_requires_replanning() {
        let current =
            CoordinationSnapshot::from_dependencies([dependency("hadith", "quran.search")]);

        let proposed =
            CoordinationSnapshot::from_dependencies([dependency("hadith", "quran.analyze")]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert!(decision.is_required());

        let delta = ReplanningDelta::between(&current, &proposed);

        assert!(delta.is_changed());
        assert_eq!(delta.added_count(), 1);
        assert_eq!(delta.removed_count(), 1);
    }

    // -------------------------------------------------------------------------
    // 12. Replanning → plan construction
    // -------------------------------------------------------------------------

    #[test]
    fn changed_coordination_can_feed_plan_construction() {
        let dependency = dependency("quran", "hadith.search");

        let snapshot = CoordinationSnapshot::from_dependencies([dependency.clone()]);

        assert_eq!(snapshot.len(), 1);

        let node = PlanNode::new(node_id("quran"), capability_requirement("quran.analyze"));

        let plan = Plan::builder(plan_id("plan-1"), operation_id("operation-plan"))
            .add_node(node)
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(plan.version(), PlanVersion::FIRST);

        assert_eq!(plan.state(), PlanState::Draft);

        assert_eq!(plan.nodes().len(), 1);

        assert_eq!(plan.dependencies().len(), 1);
    }

    // -------------------------------------------------------------------------
    // 13. Membership snapshot coherence
    // -------------------------------------------------------------------------

    #[test]
    fn routing_consumers_can_use_one_immutable_membership_snapshot() {
        let membership = Membership::new();

        membership
            .register(registration("QuranEngine", "quran-01", "quran.search"))
            .unwrap();

        membership
            .register(registration("QuranEngine", "quran-02", "quran.search"))
            .unwrap();

        let snapshot = membership.snapshot();

        let original_version = snapshot.version();

        // Mutate live membership after taking the snapshot.
        membership
            .register(registration("QuranEngine", "quran-03", "quran.search"))
            .unwrap();

        // The original snapshot remains coherent and unchanged.
        assert_eq!(snapshot.version(), original_version);

        assert_eq!(snapshot.len(), 2);

        assert!(!snapshot.contains(&instance_id("quran-03")));

        // The new live snapshot observes the later state.
        let updated = membership.snapshot();

        assert_eq!(updated.len(), 3);

        assert!(updated.contains(&instance_id("quran-03")));
    }

    // -------------------------------------------------------------------------
    // 14. Facade composition boundary
    // -------------------------------------------------------------------------

    #[test]
    fn control_plane_facade_delegates_replanning() {
        let current = CoordinationSnapshot::new();

        let proposed =
            CoordinationSnapshot::from_dependencies([dependency("quran", "hadith.search")]);

        let plane = ControlPlane::new();

        let decision = plane.evaluate_replanning_snapshots(&current, &proposed);

        assert!(decision.is_required());
    }

    #[test]
    fn control_plane_facade_delegates_routing() {
        let operation = operation_id("facade-routing");

        let resolved = resolution(&operation, &instance_id("quran-01"));

        let execution_attempt = attempt(&operation, "attempt-1", 1);

        let decision = ControlPlane::new()
            .route_resolved(&resolved, &execution_attempt)
            .unwrap();

        assert_eq!(decision.operation_id(), &operation);

        assert_eq!(decision.attempt_id(), &attempt_id("attempt-1"));

        assert_eq!(decision.destination(), &instance_id("quran-01"));
    }

    // -------------------------------------------------------------------------
    // 15. Negative ownership tests
    // -------------------------------------------------------------------------

    #[test]
    fn routing_does_not_create_a_new_attempt() {
        let operation = operation_id("attempt-ownership");

        let resolved = resolution(&operation, &instance_id("quran-01"));

        let existing_attempt = attempt(&operation, "attempt-7", 7);

        let decision = Router::route_resolved(&resolved, &existing_attempt).unwrap();

        assert_eq!(decision.attempt_id(), existing_attempt.attempt_id());

        assert_eq!(existing_attempt.attempt_number(), 7);
    }

    #[test]
    fn destination_semantics_do_not_select_an_instance() {
        let destination = Destination::logical(capability_requirement("hadith.search"));

        assert!(destination.instance_id().is_none());

        assert!(destination.logical_destination().is_some());
    }
}
