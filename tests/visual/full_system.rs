use crate::support::{section, separator, show_arrow, show_identity, step, success};
use futures::executor::block_on;
use nizaam_core::capability::{
    CapabilityDefinition, CapabilityInvocation, CapabilityRegistry, arc_handler, dispatch,
};
use nizaam_core::client::UniversalClient;
use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::control_plane::Endpoint;
use nizaam_core::control_plane::registration::RuntimeRegistrationMetadata;
use nizaam_core::control_plane::{
    CapabilityRequirement, ControlPlane, DestinationEligibilityInput, DestinationRequest,
    EngineObservation, EngineRegistration, Membership, Observations, PolicyInput,
    ResolvedCapability, ResolvedContract, ResolvedRouting, RoutingCandidate, RoutingConstraints,
    RoutingPolicy, RoutingStrategy, eligible_destinations,
};
use nizaam_core::health::{HealthReport, LivenessReport, ReadinessReport};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::retry::Attempt;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityMiddleware,
};
use nizaam_core::status::Status;
use nizaam_core::transport::InMemoryTransport;
use std::sync::{Arc, Mutex};

const CAP: &str = "visual.full.echo";

#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(
        &self,
        _: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Ok(PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("engine-a").unwrap(),
        ))
    }
}
#[derive(Debug)]
struct Allow;
impl Authorizer for Allow {
    fn authorize(
        &self,
        _: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}
#[derive(Clone)]
struct Creds;
impl CredentialExtractor for Creds {
    fn extract(&self, _: &EngineContext, _: &UniversalRequest) -> Option<Vec<u8>> {
        Some(b"opaque-credential".to_vec())
    }
}

fn registration() -> EngineRegistration {
    let engine = EngineId::new("engine-b").unwrap();
    let instance = EngineInstanceId::new("engine-b-instance").unwrap();
    let definition = CapabilityDefinition::new(
        CapabilityId::new(CAP).unwrap(),
        engine.clone(),
        "full-system echo",
    )
    .unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new(CAP).unwrap(),
        CapabilityId::new(CAP).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    EngineRegistration::new(engine, instance.clone())
        .with_capability(definition)
        .unwrap()
        .with_contract(descriptor)
        .with_endpoint(Endpoint::new("memory://engine-b-instance").unwrap())
        .with_runtime_metadata(
            RuntimeRegistrationMetadata::new()
                .with_lifecycle(LifecycleState::Serving)
                .with_readiness(ReadinessReport::from_lifecycle(LifecycleState::Serving)),
        )
}
fn observation() -> EngineObservation {
    let engine = EngineId::new("engine-b").unwrap();
    let instance = EngineInstanceId::new("engine-b-instance").unwrap();
    let health = HealthReport::new(
        engine.clone(),
        LifecycleState::Serving,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(LifecycleState::Serving),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    EngineObservation::new(engine, instance, health).unwrap()
}

#[test]
fn visual_full_system_truthful_execution_narrative() {
    section("NIZAAM CORE — FULL SYSTEM");
    step(1, "Engine A / Engine B initialized");
    let engine_b = EngineId::new("engine-b").unwrap();
    let instance_b = EngineInstanceId::new("engine-b-instance").unwrap();
    let runtime = Arc::new(EngineRuntime::new(engine_b.clone(), instance_b.clone()));
    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime.transition(state).unwrap();
    }
    println!("  Engine B state : {:?}", runtime.state());
    success("concrete runtime initialized and serving");

    step(2, "registration → capability → eligibility → routing");
    let membership = Membership::new();
    let observations = Observations::new();
    let registration = registration();
    membership.register(registration).unwrap();
    observations.update(observation()).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new(CAP).unwrap(),
        CapabilityId::new(CAP).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    let cap = CapabilityId::new(CAP).unwrap();
    let destination = DestinationRequest::hard_logical(CapabilityRequirement::new(cap.clone()));
    let eligible = eligible_destinations(DestinationEligibilityInput::new(
        &destination,
        &membership.snapshot(),
        &observations.snapshot(),
        &cap,
        &descriptor,
    ))
    .unwrap();
    assert_eq!(eligible.len(), 1);
    let candidates: Vec<_> = eligible
        .into_iter()
        .map(|c| RoutingCandidate::new(c.instance_id().clone()))
        .collect();
    let selection = RoutingPolicy::deterministic()
        .evaluate(&PolicyInput::new(&candidates, &RoutingConstraints::new()))
        .unwrap();
    let operation = OperationId::new("visual-full-operation").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("visual-full-attempt").unwrap(),
        1,
    )
    .unwrap();
    let resolution = ControlPlane::new().resolve(nizaam_core::control_plane::ResolutionInput::new(
        operation.clone(),
        ResolvedContract::new(ContractId::new(CAP).unwrap(), "1.0.0"),
        ResolvedCapability::new(cap.clone()),
        ResolvedRouting::new(
            selection.instance_id().clone(),
            RoutingStrategy::Deterministic,
        ),
    ));
    let decision = ControlPlane::new()
        .route_resolved(&resolution, &attempt)
        .unwrap();
    assert_eq!(decision.destination(), &instance_b);
    println!("  selected EngineId         : {engine_b}");
    println!("  selected EngineInstanceId : {}", decision.destination());
    println!("  RoutingDecision operation : {}", decision.operation_id());
    show_arrow("Control Plane", "immutable RoutingDecision");

    step(3, "UniversalRequest and payload encoding");
    let context = OperationContext::new(Operation::new(
        operation.clone(),
        CorrelationId::new("visual-full-correlation").unwrap(),
    ))
    .for_attempt(
        nizaam_core::identity::NodeId::new("engine-b-node").unwrap(),
        attempt.attempt_id().clone(),
    );
    let request_descriptor = descriptor.clone();
    let request = UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("visual-full-message").unwrap(),
        context,
        ContractMetadata::new(
            request_descriptor.clone(),
            Participants::new(EngineId::new("engine-a").unwrap(), engine_b.clone())
                .with_target_instance(instance_b.clone()),
        ),
        EncodedPayload::new(request_descriptor.payload, b"full-system-payload"),
    ));
    show_identity(
        &operation,
        &request.event.envelope.message_id,
        &request
            .event
            .envelope
            .operation_context
            .operation
            .correlation_id,
        &engine_b,
        &instance_b,
        &cap,
    );
    success("request created with stable operation/message/correlation/capability lineage");

    step(4, "transport → runtime admission → security → capability");
    let registry = Arc::new(CapabilityRegistry::new());
    let invoked = Arc::new(Mutex::new(false));
    let marker = Arc::clone(&invoked);
    registry
        .register(
            CapabilityDefinition::new(cap.clone(), engine_b.clone(), "full-system echo").unwrap(),
            arc_handler(move |_ctx, invocation| {
                *marker.lock().unwrap() = true;
                Ok(nizaam_core::capability::CapabilityOutcome::new(
                    format!("echo:{}", invocation.payload_bytes().len()).into_bytes(),
                ))
            }),
        )
        .unwrap();
    let runtime_for_handler = Arc::clone(&runtime);
    let registry_for_handler = Arc::clone(&registry);
    let transport = InMemoryTransport::new();
    transport.register(engine_b.clone(), instance_b.clone(), move |mut request| {
        runtime_for_handler.admit_request().unwrap();
        let mut context = EngineContext::new(request.event.envelope.operation_context.clone());
        let security = SecurityMiddleware::new(Auth, Allow, Creds);
        assert_eq!(
            security.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );
        let invocation = CapabilityInvocation::new(
            cap.clone(),
            ContractId::new(CAP).unwrap(),
            request.event.envelope.payload.bytes().to_vec(),
        );
        let outcome = dispatch(&registry_for_handler, &context, &invocation)
            .into_outcome()
            .unwrap();
        assert_eq!(outcome.into_bytes(), b"echo:19");
        UniversalResponse::new(request.event.envelope, Status::Success)
    });
    let client = UniversalClient::new(transport);
    let response = block_on(client.send(&instance_b, request)).unwrap();
    assert_eq!(response.status, Status::Success);
    assert!(*invoked.lock().unwrap());
    assert_eq!(
        response.event.envelope.message_id.as_str(),
        "visual-full-message"
    );
    println!("  runtime admission : Serving");
    println!("  middleware        : authentication + authorization");
    println!("  capability       : invoked");
    let _ = response.event.envelope.payload.bytes();
    show_arrow(
        "UniversalRequest",
        "runtime admission → security → capability handler",
    );
    success(
        "intended concrete instance executed the protected capability and returned a valid response",
    );

    step(5, "execution lineage remains coherent");
    assert_eq!(decision.operation_id(), &operation);
    assert_eq!(decision.attempt_id(), attempt.attempt_id());
    assert_eq!(
        response.event.envelope.operation_context.operation.id,
        operation
    );
    println!("  OperationId remains stable across routing and execution");
    println!("  AttemptId remains attached to the execution context");
    success("full-system visual narrative is grounded in current public boundaries");
    separator();
}
