use crate::support::{section, show_arrow, show_identity, step, success};
use futures::executor::block_on;
use nizaam_core::client::UniversalClient;
use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::status::Status;
use nizaam_core::transport::InMemoryTransport;

fn request(engine: &EngineId, instance: &EngineInstanceId) -> UniversalRequest {
    let descriptor = ContractDescriptor::new(
        ContractId::new("visual.echo").unwrap(),
        CapabilityId::new("visual.echo").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    let operation = OperationId::new("visual-communication-operation").unwrap();
    let context = OperationContext::new(Operation::new(
        operation,
        CorrelationId::new("visual-correlation").unwrap(),
    ));
    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("visual-communication-message").unwrap(),
        context,
        ContractMetadata::new(
            descriptor.clone(),
            Participants::new(EngineId::new("engine-a").unwrap(), engine.clone())
                .with_target_instance(instance.clone()),
        ),
        EncodedPayload::new(descriptor.payload, b"hello-engine-b"),
    ))
}

#[test]
fn visual_engine_to_engine_communication() {
    section("NIZAAM CORE — ENGINE COMMUNICATION");
    let engine_a = EngineId::new("engine-a").unwrap();
    let engine_b = EngineId::new("engine-b").unwrap();
    let instance_b = EngineInstanceId::new("engine-b-instance").unwrap();
    step(1, "initialize concrete identities");
    println!("  caller    : {engine_a}");
    println!("  target    : {engine_b}");
    println!("  instance  : {instance_b}");

    step(2, "construct universal request");
    let req = request(&engine_b, &instance_b);
    let operation = req.event.envelope.operation_context.operation.id.clone();
    let message = req.event.envelope.message_id.clone();
    let correlation = req
        .event
        .envelope
        .operation_context
        .operation
        .correlation_id
        .clone();
    show_identity(
        &operation,
        &message,
        &correlation,
        &engine_b,
        &instance_b,
        &req.event.envelope.metadata.descriptor.capability_id,
    );
    show_arrow("Engine A", "UniversalRequest");

    step(3, "send through UniversalClient / InMemoryTransport");
    let transport = InMemoryTransport::new();
    let target = instance_b.clone();
    transport.register(engine_b.clone(), instance_b.clone(), move |request| {
        let envelope = request.event.envelope;
        UniversalResponse::new(envelope, Status::Success)
    });
    let client = UniversalClient::new(transport);
    let response = block_on(client.send(&instance_b, req)).unwrap();
    assert_eq!(response.status, Status::Success);
    assert_eq!(response.event.envelope.message_id, message);
    assert_eq!(response.event.envelope.payload.bytes(), b"hello-engine-b");
    assert_eq!(target.as_str(), "engine-b-instance");
    show_arrow("UniversalClient", "Engine B handler");
    success("request reached the intended concrete instance and returned a structural response");

    step(4, "implementation boundary");
    println!("  Current transport path returns the handler response directly.");
    println!("  Response framing/reassembly is demonstrated separately by message_framing.");
    success(
        "visual narrative does not claim a symmetric response-framing loop that the current transport does not implement",
    );
}
