use core::fmt;

use crate::contracts::compatibility::compare_versions;
use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::contracts::event::UniversalEvent;
use crate::contracts::request::UniversalRequest;
use crate::contracts::response::UniversalResponse;

/// A structural contract validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyPayload,
    InteractionMismatch,
    EmptyEventType,
    EmptyEventScope,
    CapabilityRequirementMismatch,
    MinimumContractVersionMismatch,
    PayloadDescriptorMismatch,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPayload => "a contract payload must not be empty",
            Self::InteractionMismatch => "the envelope interaction does not match its message kind",
            Self::EmptyEventType => "an event type must not be empty",
            Self::EmptyEventScope => "an event scope must not be empty",
            Self::CapabilityRequirementMismatch => {
                "the required capability does not match the contract"
            }
            Self::MinimumContractVersionMismatch => {
                "the contract version does not meet the minimum required version"
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

    if let Some(minimum_version) = &envelope.metadata.requirements.minimum_contract_version
        && compare_versions(minimum_version, &envelope.metadata.descriptor.version)
            != crate::status::Compatibility::Compatible
    {
        return Err(ValidationError::MinimumContractVersionMismatch);
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

/// Validates the structural invariants of a universal Event.
///
/// Event-specific semantics are limited to the Event interaction marker, the
/// non-empty Event type, and the non-empty Event scope. Generic envelope rules
/// continue to be enforced by [`validate_envelope`].
pub fn validate_event(event: &UniversalEvent) -> Result<(), ValidationError> {
    if event.envelope.metadata.descriptor.interaction != Interaction::Event {
        return Err(ValidationError::InteractionMismatch);
    }

    if event.event_type().trim().is_empty() {
        return Err(ValidationError::EmptyEventType);
    }

    if event.scope().trim().is_empty() {
        return Err(ValidationError::EmptyEventScope);
    }

    validate_envelope(&event.envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::event::UniversalEvent;
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::contracts::request::UniversalRequest;
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, EventId, MessageId, OperationId,
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

    #[test]
    fn request_rejects_a_contract_below_the_minimum_required_version() {
        let descriptor = ContractDescriptor::new(
            ContractId::new("lookup.request").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(1, 2, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("provider").unwrap(),
            ),
        )
        .with_requirements(
            crate::contracts::metadata::RequirementsMetadata::none()
                .requiring_contract_version(Version::new(1, 3, 0)),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("message-2").unwrap(),
            OperationContext::new(Operation::new(
                OperationId::new("operation-2").unwrap(),
                CorrelationId::new("correlation-2").unwrap(),
            )),
            metadata,
            EncodedPayload::new(descriptor.payload, b"payload".to_vec()),
        );

        assert_eq!(
            validate_request(&UniversalRequest::new(envelope)),
            Err(ValidationError::MinimumContractVersionMismatch)
        );
    }

    #[test]
    fn envelope_rejects_empty_payload() {
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
            EncodedPayload::new(descriptor.payload, Vec::<u8>::new()),
        );

        assert_eq!(
            validate_envelope(&envelope),
            Err(ValidationError::EmptyPayload)
        );
    }

    #[test]
    fn response_rejects_non_response_interaction() {
        let descriptor = ContractDescriptor::new(
            ContractId::new("lookup.response").unwrap(),
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

        assert_eq!(
            validate_response(&UniversalResponse::new(
                envelope,
                crate::status::Status::Success
            )),
            Err(ValidationError::InteractionMismatch)
        );
    }

    #[test]
    fn envelope_rejects_payload_descriptor_mismatch() {
        let payload =
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
        let other_payload =
            PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let descriptor = ContractDescriptor::new(
            ContractId::new("lookup.request").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload,
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("provider").unwrap(),
            ),
        );
        // The encoded payload uses a different descriptor than the metadata requires
        let envelope = MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            OperationContext::new(Operation::new(
                OperationId::new("operation-1").unwrap(),
                CorrelationId::new("correlation-1").unwrap(),
            )),
            metadata,
            EncodedPayload::new(other_payload, b"payload".to_vec()),
        );

        assert_eq!(
            validate_envelope(&envelope),
            Err(ValidationError::PayloadDescriptorMismatch)
        );
    }

    #[test]
    fn valid_event_passes_structural_validation() {
        let descriptor = ContractDescriptor::new(
            ContractId::new("event.contract").unwrap(),
            CapabilityId::new("events.read").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Event,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("publisher").unwrap(),
                EngineId::new("subscriber").unwrap(),
            ),
        );
        let envelope = MessageEnvelope::new(
            MessageId::new("event-message-1").unwrap(),
            OperationContext::new(Operation::new(
                OperationId::new("event-operation-1").unwrap(),
                CorrelationId::new("event-correlation-1").unwrap(),
            )),
            metadata,
            EncodedPayload::new(descriptor.payload, b"event payload"),
        );

        let event = UniversalEvent::new(
            envelope,
            EventId::new("event-1").unwrap(),
            "operation.completed",
            "engine:test",
        )
        .unwrap();

        assert_eq!(validate_event(&event), Ok(()));
    }

    #[test]
    fn validation_error_display_messages() {
        assert_eq!(
            ValidationError::EmptyPayload.to_string(),
            "a contract payload must not be empty"
        );
        assert_eq!(
            ValidationError::InteractionMismatch.to_string(),
            "the envelope interaction does not match its message kind"
        );
        assert_eq!(
            ValidationError::EmptyEventType.to_string(),
            "an event type must not be empty"
        );
        assert_eq!(
            ValidationError::EmptyEventScope.to_string(),
            "an event scope must not be empty"
        );
        assert_eq!(
            ValidationError::CapabilityRequirementMismatch.to_string(),
            "the required capability does not match the contract"
        );
        assert_eq!(
            ValidationError::MinimumContractVersionMismatch.to_string(),
            "the contract version does not meet the minimum required version"
        );
        assert_eq!(
            ValidationError::PayloadDescriptorMismatch.to_string(),
            "the payload descriptor does not match the contract descriptor"
        );
    }

    #[test]
    fn validation_error_is_error_trait() {
        fn assert_error<E: std::error::Error>(_: E) {}
        assert_error(ValidationError::EmptyPayload);
        assert_error(ValidationError::InteractionMismatch);
    }
}
