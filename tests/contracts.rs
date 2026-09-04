use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::request::UniversalRequest;
use nizaam_core::contracts::response::UniversalResponse;
use nizaam_core::contracts::validation::{ValidationError, validate_request, validate_response};
use nizaam_core::prelude::*;

fn request() -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("lookup.request").unwrap(),
        CapabilityId::new("lookup").unwrap(),
        Version::new(1, 2, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );
    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("caller").unwrap(),
            EngineId::new("provider").unwrap(),
        ),
    );
    let operation = Operation::new(
        OperationId::new("operation-1").unwrap(),
        CorrelationId::new("correlation-1").unwrap(),
    );
    let envelope = MessageEnvelope::new(
        MessageId::new("message-1").unwrap(),
        OperationContext::new(operation),
        metadata,
        EncodedPayload::new(payload_descriptor, b"opaque payload".to_vec()),
    );

    UniversalRequest::new(envelope)
}

#[test]
fn public_contract_request_validates_structurally() {
    let request = request();

    assert!(request.has_request_interaction());
    assert_eq!(validate_request(&request), Ok(()));
}

#[test]
fn public_contract_validation_rejects_the_wrong_interaction() {
    let mut request = request();
    request.envelope.metadata.descriptor.interaction = Interaction::Response;

    assert_eq!(
        validate_request(&request),
        Err(ValidationError::InteractionMismatch)
    );
}

#[test]
fn public_contract_response_validates_structurally() {
    let mut request = request();
    request.envelope.metadata.descriptor.interaction = Interaction::Response;
    let response = UniversalResponse::new(request.envelope, Status::Success);

    assert!(response.has_response_interaction());
    assert_eq!(validate_response(&response), Ok(()));
}

#[test]
fn public_contract_validation_rejects_an_unmet_capability_requirement() {
    let mut request = request();
    request.envelope.metadata.requirements =
        RequirementsMetadata::none().requiring_capability(CapabilityId::new("other").unwrap());

    assert_eq!(
        validate_request(&request),
        Err(ValidationError::CapabilityRequirementMismatch)
    );
}
