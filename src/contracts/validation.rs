use core::fmt;

use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::contracts::request::UniversalRequest;
use crate::contracts::response::UniversalResponse;

/// A structural contract validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyPayload,
    InteractionMismatch,
    CapabilityRequirementMismatch,
    PayloadDescriptorMismatch,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPayload => "a contract payload must not be empty",
            Self::InteractionMismatch => "the envelope interaction does not match its message kind",
            Self::CapabilityRequirementMismatch => {
                "the required capability does not match the contract"
            }
            Self::PayloadDescriptorMismatch => {
                "the payload descriptor does not match the contract descriptor"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ValidationError {}

pub fn validate_envelope(envelope: &MessageEnvelope) -> Result<(), ValidationError> {
    if envelope.payload.bytes().is_empty() {
        return Err(ValidationError::EmptyPayload);
    }

    if let Some(capability) = &envelope.metadata.requirements.required_capability
        && capability != &envelope.metadata.descriptor.capability_id
    {
        return Err(ValidationError::CapabilityRequirementMismatch);
    }

    if envelope.payload.descriptor() != &envelope.metadata.descriptor.payload {
        return Err(ValidationError::PayloadDescriptorMismatch);
    }

    Ok(())
}

pub fn validate_request(request: &UniversalRequest) -> Result<(), ValidationError> {
    if request.envelope.metadata.descriptor.interaction != Interaction::Request {
        return Err(ValidationError::InteractionMismatch);
    }

    validate_envelope(&request.envelope)
}

pub fn validate_response(response: &UniversalResponse) -> Result<(), ValidationError> {
    if response.envelope.metadata.descriptor.interaction != Interaction::Response {
        return Err(ValidationError::InteractionMismatch);
    }

    validate_envelope(&response.envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::contracts::request::UniversalRequest;
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

    #[test]
    fn valid_request_passes_structural_validation() {
        let descriptor = ContractDescriptor::new(
            ContractId::new("lookup.request").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("provider").unwrap(),
            ),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            OperationContext::new(Operation::new(
                OperationId::new("operation-1").unwrap(),
                CorrelationId::new("correlation-1").unwrap(),
            )),
            metadata,
            EncodedPayload::new(descriptor.payload, b"payload".to_vec()),
        );

        assert_eq!(validate_request(&UniversalRequest::new(envelope)), Ok(()));
    }
}
