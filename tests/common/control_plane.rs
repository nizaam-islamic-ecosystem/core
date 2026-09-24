//! Shared Phase 15/16 Control Plane test helpers used by the E2E and
//! Control Plane conformance integration-test crates.

use std::sync::{Arc, Mutex};

use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, DestinationEligibilityInput, DestinationRequest,
    EngineObservation, EngineRegistration, Membership, Observations, ResolutionInput,
    ResolvedCapability, ResolvedContract, ResolvedRouting, RoutingCandidate, RoutingStrategy,
    eligible_destinations,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::LifecycleState;
use nizaam_core::status::Status;
use nizaam_core::transport::InMemoryTransport;
pub const CAPABILITY: &str = "quran.analyze";
pub const CONTRACT: &str = "quran.analyze";
pub const MEDIA_TYPE: &str = "application/octet-stream";

pub fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

pub fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

pub fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

pub fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

pub fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).expect("test operation id must be valid")
}

pub fn attempt_id(value: &str) -> AttemptId {
    AttemptId::new(value).expect("test attempt id must be valid")
}

pub fn descriptor_for(capability: &str, interaction: Interaction) -> ContractDescriptor {
    ContractDescriptor::new(
        contract_id(CONTRACT),
        capability_id(capability),
        Version::new(1, 0, 0),
        interaction,
        PayloadDescriptor::new(MEDIA_TYPE, Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    )
}

pub fn capability_requirement(capability: &str) -> CapabilityRequirement {
    CapabilityRequirement::new(capability_id(capability))
}

pub fn registration(engine: &str, instance: &str, capability: &str) -> EngineRegistration {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let capability = capability_id(capability);

    let definition = nizaam_core::capability::CapabilityDefinition::new(
        capability.clone(),
        engine.clone(),
        format!("{capability} capability"),
    )
    .expect("test capability definition must be valid");

    EngineRegistration::new(engine, instance.clone())
        .with_capability(definition)
        .expect("capability owner must match registration engine")
        .with_contract(descriptor_for(capability.as_str(), Interaction::Request))
        .with_endpoint(
            nizaam_core::control_plane::Endpoint::new(format!("memory://{}", instance.as_str()))
                .expect("test endpoint must be valid"),
        )
        .with_runtime_metadata(
            nizaam_core::control_plane::RuntimeRegistrationMetadata::new()
                .with_lifecycle(LifecycleState::Serving)
                .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
        )
}

pub fn healthy_observation(engine: &str, instance: &str) -> EngineObservation {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .expect("test health report must be valid");

    EngineObservation::new(engine, instance, health)
        .expect("observation identity must match health identity")
}

pub fn resolved_contract() -> ResolvedContract {
    ResolvedContract::new(contract_id(CONTRACT), "1.0.0")
}

pub fn resolved_capability() -> ResolvedCapability {
    ResolvedCapability::new(capability_id(CAPABILITY))
}

pub fn attempt(operation: &OperationId, id: &str, number: u32) -> Attempt {
    Attempt::new(operation.clone(), attempt_id(id), number).expect("test attempt must be valid")
}

pub fn request_for(
    target_engine: &EngineId,
    target_instance: &EngineInstanceId,
    operation: &OperationId,
    attempt: &AttemptId,
    message: &str,
) -> UniversalRequest {
    let descriptor = descriptor_for(CAPABILITY, Interaction::Request);
    let operation_context = OperationContext::new(Operation::new(
        operation.clone(),
        CorrelationId::new(format!("correlation-{message}"))
            .expect("test correlation id must be valid"),
    ))
    .for_attempt(
        NodeId::new("phase15-test-node").expect("test node id must be valid"),
        attempt.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor.clone(),
        Participants::new(engine_id("phase15-caller"), target_engine.clone())
            .with_target_instance(target_instance.clone()),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message).expect("test message id must be valid"),
        operation_context,
        metadata,
        EncodedPayload::new(descriptor.payload, b"phase15 payload".to_vec()),
    ))
}

pub fn response_for(request: UniversalRequest, payload: Vec<u8>) -> UniversalResponse {
    let envelope = request.event.envelope;
    let request_descriptor = envelope.metadata.descriptor;
    let payload_descriptor = request_descriptor.payload.clone();
    let participants = envelope.metadata.participants;

    let sender_instance = participants.target_instance.clone();
    let target_instance = participants.sender_instance.clone();
    let mut response_participants = Participants::new(participants.target, participants.sender);

    if let Some(instance) = sender_instance {
        response_participants = response_participants.with_sender_instance(instance);
    }

    if let Some(instance) = target_instance {
        response_participants = response_participants.with_target_instance(instance);
    }

    let descriptor = ContractDescriptor::new(
        request_descriptor.contract_id,
        request_descriptor.capability_id,
        request_descriptor.version,
        Interaction::Response,
        payload_descriptor.clone(),
    );

    let response_message_id = envelope.message_id.clone();

    let response = UniversalResponse::new(
        MessageEnvelope::new(
            envelope.message_id,
            envelope.operation_context,
            ContractMetadata::new(descriptor, response_participants),
            EncodedPayload::new(payload_descriptor, payload),
        ),
        Status::Success,
    );

    assert_eq!(response.event.envelope.message_id, response_message_id);
    response
}

pub fn eligible_candidates(
    membership: &Membership,
    observations: &Observations,
    destination: &DestinationRequest,
) -> Vec<RoutingCandidate> {
    let membership_snapshot = membership.snapshot();
    let observation_snapshot = observations.snapshot();
    let capability = capability_id(CAPABILITY);
    let contract = descriptor_for(CAPABILITY, Interaction::Request);

    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        destination,
        &membership_snapshot,
        &observation_snapshot,
        &capability,
        &contract,
    ))
    .expect("test destination must have at least one eligible instance");

    eligible
        .into_iter()
        .map(|candidate| RoutingCandidate::new(candidate.instance_id().clone()))
        .collect()
}

pub fn resolve_and_route(
    control_plane: &ControlPlane,
    operation: &OperationId,
    destination: EngineInstanceId,
    attempt: &Attempt,
) -> nizaam_core::control_plane::RoutingDecision {
    let resolution = control_plane.resolve(ResolutionInput::new(
        operation.clone(),
        resolved_contract(),
        resolved_capability(),
        ResolvedRouting::new(destination, RoutingStrategy::Deterministic),
    ));

    control_plane
        .route_resolved(&resolution, attempt)
        .expect("resolution and attempt must belong to the same operation")
}

pub fn expose_echo_engine(
    transport: &InMemoryTransport,
    engine: &str,
    instance: &str,
) -> Arc<Mutex<usize>> {
    let engine = engine_id(engine);
    let instance = instance_id(instance);
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_handler = Arc::clone(&calls);
    let response_instance = instance.clone();

    transport.register(engine, instance, move |request| {
        *calls_for_handler
            .lock()
            .expect("test call counter lock must not be poisoned") += 1;

        response_for(request, response_instance.as_str().as_bytes().to_vec())
    });

    calls
}

pub fn request_with_payload(
    target_engine: &EngineId,
    target_instance: &EngineInstanceId,
    operation: &OperationId,
    attempt: &AttemptId,
    message: &str,
    payload: &[u8],
) -> UniversalRequest {
    let descriptor = descriptor_for(CAPABILITY, Interaction::Request);
    let operation_context = OperationContext::new(Operation::new(
        operation.clone(),
        CorrelationId::new(format!("correlation-{message}"))
            .expect("test correlation id must be valid"),
    ))
    .for_attempt(
        NodeId::new("phase16-e2e-node").expect("test node id must be valid"),
        attempt.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor.clone(),
        Participants::new(engine_id("phase16-e2e-caller"), target_engine.clone())
            .with_target_instance(target_instance.clone()),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message).expect("test message id must be valid"),
        operation_context,
        metadata,
        EncodedPayload::new(descriptor.payload, payload.to_vec()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_control_plane_fixtures_are_executable() {
        assert_eq!(CAPABILITY, CONTRACT);
        assert_eq!(MEDIA_TYPE, "application/octet-stream");

        let engine = engine_id("helper-engine");
        let instance = instance_id("helper-instance");
        let capability = capability_id(CAPABILITY);
        let contract = contract_id(CONTRACT);
        let operation = operation_id("helper-operation");
        let attempt = attempt(&operation, "helper-attempt", 1);
        let attempt_identity = attempt_id("helper-attempt-identity");

        assert_eq!(engine.as_str(), "helper-engine");
        assert_eq!(instance.as_str(), "helper-instance");
        assert_eq!(capability.as_str(), CAPABILITY);
        assert_eq!(contract.as_str(), CONTRACT);
        assert_eq!(attempt.attempt_id().as_str(), "helper-attempt");
        assert_eq!(attempt_identity.as_str(), "helper-attempt-identity");

        let descriptor = descriptor_for(CAPABILITY, Interaction::Request);
        assert_eq!(descriptor.capability_id.as_str(), CAPABILITY);

        let requirement = capability_requirement(CAPABILITY);
        assert_eq!(requirement.capability_id().as_str(), CAPABILITY);

        let registration = registration("helper-engine", "helper-instance", CAPABILITY);
        assert_eq!(registration.engine_id(), &engine);
        assert_eq!(registration.engine_instance_id(), &instance);

        let observation = healthy_observation("helper-engine", "helper-instance");
        assert_eq!(observation.engine_id(), &engine);
        assert_eq!(observation.engine_instance_id(), &instance);

        let resolved_contract: ResolvedContract = resolved_contract();
        let resolved_capability: ResolvedCapability = resolved_capability();
        assert_eq!(resolved_contract.contract_id().as_str(), CONTRACT);
        assert_eq!(resolved_capability.capability_id().as_str(), CAPABILITY);

        let membership = Membership::new();
        let observations = Observations::new();
        membership
            .register(registration)
            .expect("helper registration must succeed");
        observations
            .update(observation)
            .expect("helper observation must succeed");

        let destination = DestinationRequest::hard_logical(requirement);
        let candidates = eligible_candidates(&membership, &observations, &destination);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].instance_id(), &instance);

        let decision =
            resolve_and_route(&ControlPlane::new(), &operation, instance.clone(), &attempt);
        assert_eq!(decision.destination(), &instance);

        let request = request_for(
            &engine,
            &instance,
            &operation,
            attempt.attempt_id(),
            "helper-response-message",
        );
        let response = response_for(request, b"helper-response".to_vec());
        assert_eq!(response.status, Status::Success);
        assert!(response.has_response_interaction());

        let payload_request = request_with_payload(
            &engine,
            &instance,
            &operation,
            attempt.attempt_id(),
            "helper-payload-message",
            b"helper-payload",
        );
        assert!(payload_request.has_request_interaction());
        assert_eq!(
            payload_request.event.envelope.payload.bytes(),
            b"helper-payload"
        );

        let transport = InMemoryTransport::new();
        let calls = expose_echo_engine(&transport, "helper-engine", "helper-instance");
        let communication = nizaam_core::control_plane::ControlPlaneCommunication::new(transport);
        let request = request_with_payload(
            &engine,
            &instance,
            &operation,
            attempt.attempt_id(),
            "helper-communication-message",
            b"helper-communication",
        );
        let received =
            futures::executor::block_on(communication.send_to_engine(&instance, request))
                .expect("helper communication must reach the registered instance");

        assert_eq!(received.status, Status::Success);
        assert_eq!(received.event.envelope.payload.bytes(), b"helper-instance");
        assert_eq!(*calls.lock().expect("helper call counter lock"), 1);
    }
}
