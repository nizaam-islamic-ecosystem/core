use crate::support::{section, show_arrow, step, success};
use nizaam_core::capability::CapabilityDefinition;
use nizaam_core::contracts::descriptor::{
    ContractDescriptor, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::control_plane::Endpoint;
use nizaam_core::control_plane::registration::RuntimeRegistrationMetadata;
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, DestinationEligibilityInput, DestinationRequest,
    EngineObservation, EngineRegistration, EngineRegistry, Membership, Observations, PolicyInput,
    ResolvedCapability, ResolvedContract, ResolvedRouting, RoutingCandidate, RoutingConstraints,
    RoutingPolicy, RoutingStrategy, eligible_destinations,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, EngineId, EngineInstanceId, OperationId,
};

use nizaam_core::retry::Attempt;
use nizaam_core::runtime::LifecycleState;

const CAPABILITY: &str = "visual.route";
const CONTRACT: &str = "visual.route";

fn registration(instance: &str) -> EngineRegistration {
    let engine = EngineId::new("visual-engine").unwrap();
    let instance_id = EngineInstanceId::new(instance).unwrap();
    let capability = CapabilityId::new(CAPABILITY).unwrap();
    let definition =
        CapabilityDefinition::new(capability, engine.clone(), "visual capability").unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new(CONTRACT).unwrap(),
        CapabilityId::new(CAPABILITY).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    EngineRegistration::new(engine, instance_id.clone())
        .with_capability(definition)
        .unwrap()
        .with_contract(descriptor)
        .with_endpoint(Endpoint::new(format!("memory://{instance}")).unwrap())
        .with_runtime_metadata(
            RuntimeRegistrationMetadata::new()
                .with_lifecycle(LifecycleState::Serving)
                .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
        )
}

fn observation(instance: &str) -> EngineObservation {
    let engine = EngineId::new("visual-engine").unwrap();
    let instance_id = EngineInstanceId::new(instance).unwrap();
    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    EngineObservation::new(engine, instance_id, health).unwrap()
}

#[test]
fn visual_control_plane_coordination_path() {
    section("NIZAAM CORE — CONTROL PLANE");
    let registry = EngineRegistry::new();
    let membership = Membership::new();
    let observations = Observations::new();
    let registration = registration("visual-engine-01");
    registry.register(registration.clone()).unwrap();
    membership.register(registration).unwrap();
    observations
        .update(observation("visual-engine-01"))
        .unwrap();

    step(1, "registration and metadata");
    assert!(registry.contains(&EngineInstanceId::new("visual-engine-01").unwrap()));
    success("logical EngineId and concrete EngineInstanceId are registered separately");
    show_arrow("Registration", "Registry / membership");

    step(2, "capability and contract resolution inputs");
    let destination = DestinationRequest::hard_logical(CapabilityRequirement::new(
        CapabilityId::new(CAPABILITY).unwrap(),
    ));
    let capability = CapabilityId::new(CAPABILITY).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new(CONTRACT).unwrap(),
        capability.clone(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability,
        &descriptor,
    ))
    .unwrap();
    assert_eq!(eligible.len(), 1);
    success("eligible destination exists without invoking a capability handler");
    show_arrow("Capability / contract", "Eligibility");

    step(3, "deterministic routing policy");
    let candidates: Vec<_> = eligible
        .into_iter()
        .map(|candidate| RoutingCandidate::new(candidate.instance_id().clone()))
        .collect();
    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();
    println!("  selected instance : {}", selection.instance_id());
    assert_eq!(selection.instance_id().as_str(), "visual-engine-01");
    show_arrow("Eligible candidates", "Immutable policy selection");

    step(4, "immutable RoutingDecision");
    let operation = OperationId::new("visual-control-operation").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("visual-control-attempt").unwrap(),
        1,
    )
    .unwrap();
    let resolution = ControlPlane::new().resolve(nizaam_core::control_plane::ResolutionInput::new(
        operation.clone(),
        ResolvedContract::new(ContractId::new(CONTRACT).unwrap(), "1.0.0"),
        ResolvedCapability::new(CapabilityId::new(CAPABILITY).unwrap()),
        ResolvedRouting::new(
            selection.instance_id().clone(),
            RoutingStrategy::Deterministic,
        ),
    ));
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();
    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.destination().as_str(), "visual-engine-01");
    println!("  OperationId       : {}", decision.operation_id());
    println!("  AttemptId         : {}", decision.attempt_id());
    println!("  Destination       : {}", decision.destination());
    show_arrow("Routing policy", "Universal client / communication");
    success(
        "Control Plane stops at a concrete routing decision; it does not execute the capability",
    );
}
