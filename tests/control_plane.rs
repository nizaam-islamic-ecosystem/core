//! Phase 15 Control Plane integration tests.
//!
//! These tests exercise the public Control Plane boundary from outside the
//! `control_plane` module. They deliberately compose registration, membership,
//! runtime/readiness observations, destination eligibility, routing policy,
//! immutable resolution, and attempt routing instead of repeating the unit
//! tests owned by the individual Control Plane modules.
//!
//! The Control Plane remains a routing/control boundary:
//!
//! ```text
//! registration + membership + observations
//!              ↓
//!      destination eligibility
//!              ↓
//!        routing policy
//!              ↓
//!          resolution
//!              ↓
//!      existing execution attempt
//!              ↓
//!        routing decision
//! ```
//!
//! No test executes a capability, performs transport, creates a retry, or
//! treats the Control Plane as the owner of those responsibilities.

use nizaam_core::capability::CapabilityDefinition;
use nizaam_core::contracts::{ContractDescriptor, Interaction, PayloadDescriptor, Version};
use nizaam_core::control_plane::ControlPlane;
use nizaam_core::control_plane::capability::require_advertised;
use nizaam_core::control_plane::contract::compare as compare_contracts;
use nizaam_core::control_plane::destination::{
    DestinationEligibilityInput, DestinationRequest, FallbackPolicy, eligible_destinations,
};
use nizaam_core::control_plane::membership::Membership;
use nizaam_core::control_plane::observations::{EngineObservation, Observations};
use nizaam_core::control_plane::policy::{
    PolicyInput, RoutingCandidate, RoutingConstraints, RoutingPolicy,
};
use nizaam_core::control_plane::registration::{
    Endpoint, EngineRegistration, RuntimeRegistrationMetadata,
};
use nizaam_core::control_plane::resolution::{
    ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedProvider, ResolvedRouting,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, EngineId, EngineInstanceId, OperationId,
};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::LifecycleState;
use nizaam_core::status::Compatibility;

const ENGINE_ID: &str = "arabic-engine";
const CAPABILITY_ID: &str = "Arabic.Analyze";
const CONTRACT_ID: &str = "arabic.analyze";

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).expect("test operation id must be valid")
}

fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).expect("test attempt id must be valid")
}

fn contract(version: Version) -> ContractDescriptor {
    ContractDescriptor::new(
        contract_id(CONTRACT_ID),
        capability_id(CAPABILITY_ID),
        version,
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    )
}

fn capability() -> CapabilityDefinition {
    CapabilityDefinition::new(
        capability_id(CAPABILITY_ID),
        engine_id(ENGINE_ID),
        "Arabic Analyze",
    )
    .expect("test capability definition must be valid")
}

fn healthy_registration(instance: &str, offered_contract: Version) -> EngineRegistration {
    let runtime = RuntimeRegistrationMetadata::new()
        .with_version(Version::new(1, 0, 0))
        .with_lifecycle(LifecycleState::Serving)
        .with_readiness(ReadinessReport::ready());

    EngineRegistration::new(engine_id(ENGINE_ID), instance_id(instance))
        .with_capability(capability())
        .expect("capability must belong to the registered engine")
        .with_contract(contract(offered_contract))
        .with_endpoint(
            Endpoint::new(format!("memory://{instance}")).expect("test endpoint must be valid"),
        )
        .with_runtime_metadata(runtime)
}

fn healthy_observation(instance: &str) -> EngineObservation {
    let health = HealthReport::new(
        engine_id(ENGINE_ID),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::ready(),
        Vec::new(),
        Vec::new(),
    )
    .expect("test health report must be valid");

    EngineObservation::new(engine_id(ENGINE_ID), instance_id(instance), health)
        .expect("observation identity must match its health report")
}

fn register(membership: &Membership, observations: &Observations, instance: &str) {
    membership
        .register(healthy_registration(instance, Version::new(1, 0, 0)))
        .expect("test instance registration must succeed");

    observations
        .update(healthy_observation(instance))
        .expect("test observation update must succeed");
}

fn eligible_for(
    membership: &Membership,
    observations: &Observations,
    request: &DestinationRequest,
    offered_contract: &ContractDescriptor,
) -> Vec<EngineInstanceId> {
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();

    eligible_destinations(DestinationEligibilityInput::new(
        request,
        &membership_snapshot,
        &observation_snapshot,
        &capability_id(CAPABILITY_ID),
        offered_contract,
    ))
    .expect("test destination must have at least one eligible instance")
    .into_iter()
    .map(|destination| destination.instance_id().clone())
    .collect()
}

fn select(candidates: &[EngineInstanceId], constraints: &RoutingConstraints) -> EngineInstanceId {
    let routing_candidates: Vec<RoutingCandidate> = candidates
        .iter()
        .cloned()
        .map(RoutingCandidate::new)
        .collect();

    RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&routing_candidates, constraints))
        .expect("test routing policy must select a candidate")
        .into_instance_id()
}

fn resolve_and_route(
    operation: &OperationId,
    attempt: &Attempt,
    destination: EngineInstanceId,
) -> nizaam_core::control_plane::routing::RoutingDecision {
    let resolution = ControlPlane::new().resolve(
        ResolutionInput::new(
            operation.clone(),
            ResolvedContract::new(contract_id(CONTRACT_ID), "1.0.0"),
            ResolvedCapability::new(capability_id(CAPABILITY_ID)),
            ResolvedRouting::new(
                destination,
                nizaam_core::control_plane::policy::RoutingStrategy::Deterministic,
            ),
        )
        .with_provider(ResolvedProvider::new(engine_id(ENGINE_ID))),
    );

    ControlPlane::new()
        .route_resolved(&resolution, attempt)
        .expect("existing attempt must be routable")
}

#[test]
fn logical_capability_request_crosses_real_control_plane_stages_to_one_concrete_instance() {
    let membership = Membership::new();
    let observations = Observations::new();

    register(&membership, &observations, "arabic-01");
    register(&membership, &observations, "arabic-02");

    let request = DestinationRequest::hard_logical(
        nizaam_core::control_plane::dependency::CapabilityRequirement::new(capability_id(
            CAPABILITY_ID,
        )),
    );

    let eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );

    assert_eq!(
        eligible,
        vec![instance_id("arabic-01"), instance_id("arabic-02")]
    );

    let selected = select(&eligible, &RoutingConstraints::new());

    let operation = operation_id("phase15-logical-operation");
    let attempt = Attempt::new(operation.clone(), attempt_id("phase15-logical-attempt"), 1)
        .expect("test attempt must be valid");

    let decision = resolve_and_route(&operation, &attempt, selected.clone());

    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
    assert_eq!(decision.destination(), &selected);
    assert_eq!(decision.destination(), &instance_id("arabic-01"));
}

#[test]
fn multiple_registered_instances_share_one_logical_capability_without_collapsing_instance_identity()
{
    let membership = Membership::new();
    let observations = Observations::new();

    register(&membership, &observations, "arabic-01");
    register(&membership, &observations, "arabic-02");
    register(&membership, &observations, "arabic-03");

    let request = DestinationRequest::hard_logical(
        nizaam_core::control_plane::dependency::CapabilityRequirement::new(capability_id(
            CAPABILITY_ID,
        )),
    );

    let eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );

    assert_eq!(eligible.len(), 3);
    assert_ne!(instance_id("arabic-01"), instance_id("arabic-02"));
    assert_ne!(instance_id("arabic-02"), instance_id("arabic-03"));

    let selected = select(&eligible, &RoutingConstraints::new());
    assert!(eligible.contains(&selected));

    let logical_engine = engine_id(ENGINE_ID);
    assert_eq!(
        membership
            .snapshot()
            .instances_for_engine(&logical_engine)
            .count(),
        3
    );
}

#[test]
fn membership_change_affects_new_resolution_without_mutating_an_existing_resolution() {
    let membership = Membership::new();
    let observations = Observations::new();

    register(&membership, &observations, "arabic-01");
    register(&membership, &observations, "arabic-02");

    let request = DestinationRequest::hard_logical(
        nizaam_core::control_plane::dependency::CapabilityRequirement::new(capability_id(
            CAPABILITY_ID,
        )),
    );

    let first_eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );
    let first_destination = select(&first_eligible, &RoutingConstraints::new());

    let operation = operation_id("phase15-membership-operation");
    let attempt = Attempt::new(
        operation.clone(),
        attempt_id("phase15-membership-attempt"),
        1,
    )
    .expect("test attempt must be valid");

    let existing_decision = resolve_and_route(&operation, &attempt, first_destination.clone());

    membership
        .unregister(&instance_id("arabic-01"))
        .expect("registered instance must be removable");

    observations
        .remove(&instance_id("arabic-01"))
        .expect("existing observation must be removable");

    let second_eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );
    let second_destination = select(&second_eligible, &RoutingConstraints::new());

    assert_eq!(existing_decision.destination(), &instance_id("arabic-01"));
    assert_eq!(second_destination, instance_id("arabic-02"));
    assert_ne!(existing_decision.destination(), &second_destination);
}

#[test]
fn hard_destination_does_not_silently_fallback_when_the_requested_instance_is_unavailable() {
    let membership = Membership::new();
    let observations = Observations::new();

    register(&membership, &observations, "arabic-01");
    register(&membership, &observations, "arabic-02");

    membership
        .unregister(&instance_id("arabic-01"))
        .expect("hard-target instance must be removable");

    observations
        .remove(&instance_id("arabic-01"))
        .expect("hard-target observation must be removable");

    let request = DestinationRequest::hard_explicit(instance_id("arabic-01"));

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &request,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY_ID),
        &contract(Version::new(1, 0, 0)),
    ));

    assert!(matches!(
        result,
        Err(
            nizaam_core::control_plane::destination::DestinationEligibilityError::HardDestinationNotMember(
                ref id
            )
        ) if id == &instance_id("arabic-01")
    ));
}

#[test]
fn preferred_destination_can_fallback_to_another_eligible_instance() {
    let membership = Membership::new();
    let observations = Observations::new();

    register(&membership, &observations, "arabic-01");
    register(&membership, &observations, "arabic-02");

    let request =
        DestinationRequest::preferred_explicit(instance_id("arabic-01"), FallbackPolicy::Allowed);

    let eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );

    let preferred_constraints =
        RoutingConstraints::new().with_preferred_destination(instance_id("arabic-01"));

    assert_eq!(
        select(&eligible, &preferred_constraints),
        instance_id("arabic-01")
    );

    membership
        .unregister(&instance_id("arabic-01"))
        .expect("preferred instance must be removable");

    observations
        .remove(&instance_id("arabic-01"))
        .expect("preferred observation must be removable");

    let fallback_eligible = eligible_for(
        &membership,
        &observations,
        &request,
        &contract(Version::new(1, 0, 0)),
    );

    assert_eq!(
        select(&fallback_eligible, &preferred_constraints),
        instance_id("arabic-02")
    );
}

#[test]
fn incompatible_contract_prevents_an_instance_from_becoming_an_eligible_destination() {
    let membership = Membership::new();
    let observations = Observations::new();

    membership
        .register(healthy_registration("arabic-01", Version::new(2, 0, 0)))
        .expect("registration with a valid contract must succeed");

    observations
        .update(healthy_observation("arabic-01"))
        .expect("observation update must succeed");

    let required = contract(Version::new(1, 0, 0));
    let offered = contract(Version::new(2, 0, 0));

    assert_eq!(
        compare_contracts(&required, &offered),
        Compatibility::Incompatible
    );

    let binding = [capability()];
    let capability_matches = require_advertised(&capability_id(CAPABILITY_ID), &binding)
        .expect("capability advertisement itself should match");

    assert_eq!(capability_matches.len(), 1);

    let request = DestinationRequest::hard_logical(
        nizaam_core::control_plane::dependency::CapabilityRequirement::new(capability_id(
            CAPABILITY_ID,
        )),
    );

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &request,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY_ID),
        &required,
    ));

    assert!(matches!(
        result,
        Err(
            nizaam_core::control_plane::destination::DestinationEligibilityError::NoEligibleDestination
        )
    ));
}

#[test]
fn each_authorized_retry_attempt_receives_an_independent_control_plane_route() {
    let operation = operation_id("phase15-retry-operation");

    let attempt_one = Attempt::new(operation.clone(), attempt_id("phase15-attempt-1"), 1)
        .expect("first attempt must be valid");

    let attempt_two = Attempt::new(operation.clone(), attempt_id("phase15-attempt-2"), 2)
        .expect("second attempt must be valid");

    let first_decision = resolve_and_route(&operation, &attempt_one, instance_id("arabic-01"));

    let second_decision = resolve_and_route(&operation, &attempt_two, instance_id("arabic-02"));

    assert_eq!(first_decision.operation_id(), &operation);
    assert_eq!(second_decision.operation_id(), &operation);

    assert_ne!(first_decision.attempt_id(), second_decision.attempt_id());

    assert_eq!(first_decision.destination(), &instance_id("arabic-01"));
    assert_eq!(second_decision.destination(), &instance_id("arabic-02"));
}

#[test]
fn routing_preserves_attempt_identity_and_rejects_an_operation_mismatch() {
    let resolution_operation = operation_id("phase15-resolution-operation");
    let attempt_operation = operation_id("phase15-attempt-operation");

    let resolution = ControlPlane::new().resolve(ResolutionInput::new(
        resolution_operation.clone(),
        ResolvedContract::new(contract_id(CONTRACT_ID), "1.0.0"),
        ResolvedCapability::new(capability_id(CAPABILITY_ID)),
        ResolvedRouting::new(
            instance_id("arabic-01"),
            nizaam_core::control_plane::policy::RoutingStrategy::Deterministic,
        ),
    ));

    let matching_attempt = Attempt::new(
        resolution_operation.clone(),
        attempt_id("phase15-matching-attempt"),
        1,
    )
    .expect("matching attempt must be valid");

    let decision = ControlPlane::new()
        .route_resolved(&resolution, &matching_attempt)
        .expect("matching operation must route successfully");

    assert_eq!(decision.operation_id(), &resolution_operation);
    assert_eq!(decision.attempt_id(), matching_attempt.attempt_id());
    assert_eq!(decision.destination(), &instance_id("arabic-01"));

    let mismatched_attempt = Attempt::new(
        attempt_operation.clone(),
        attempt_id("phase15-mismatched-attempt"),
        1,
    )
    .expect("mismatched attempt must still be structurally valid");

    let result = ControlPlane::new().route_resolved(&resolution, &mismatched_attempt);

    assert!(matches!(
        result,
        Err(
            nizaam_core::control_plane::routing::RoutingError::OperationMismatch {
                resolution_operation_id,
                attempt_operation_id,
            }
        ) if resolution_operation_id == resolution_operation
            && attempt_operation_id == attempt_operation
    ));
}

#[test]
fn control_plane_resolution_preserves_the_selected_provider_without_executing_it() {
    let operation = operation_id("phase15-provider-operation");
    let provider = engine_id("arabic-provider");

    let resolution = ControlPlane::new().resolve(
        ResolutionInput::new(
            operation.clone(),
            ResolvedContract::new(contract_id(CONTRACT_ID), "1.0.0"),
            ResolvedCapability::new(capability_id(CAPABILITY_ID)),
            ResolvedRouting::new(
                instance_id("arabic-01"),
                nizaam_core::control_plane::policy::RoutingStrategy::Deterministic,
            ),
        )
        .with_provider(ResolvedProvider::new(provider.clone())),
    );

    assert_eq!(
        resolution
            .provider()
            .expect("provider must be preserved")
            .engine_id(),
        &provider
    );
    assert_eq!(resolution.operation_id(), &operation);
    assert_eq!(resolution.destination(), &instance_id("arabic-01"));
}
