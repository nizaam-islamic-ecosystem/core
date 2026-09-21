//! System-level admission for Control Plane communication requests.
//!
//! Admission answers one narrow question:
//!
//! "Can this request enter the Control Plane communication path?"
//!
//! Admission does not:
//! - select a destination,
//! - resolve a capability provider,
//! - check engine readiness,
//! - perform authentication or authorization,
//! - execute capability handlers,
//! - invoke transport,
//! - create retry attempts,
//! - interpret application payloads.
//!
//! Structural and contract invariants are delegated to the existing contract
//! validation layer so that the Control Plane does not duplicate protocol
//! validation rules.

use crate::contracts::request::UniversalRequest;
use crate::contracts::validation::{ValidationError, validate_request};

/// Failure returned when a request cannot enter the Control Plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    /// The request violates an existing contract invariant.
    Contract(ValidationError),
}

impl core::fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Contract(error) => {
                write!(formatter, "Control Plane admission rejected: {error}")
            }
        }
    }
}

impl std::error::Error for AdmissionError {}

impl From<ValidationError> for AdmissionError {
    fn from(error: ValidationError) -> Self {
        Self::Contract(error)
    }
}

/// Admits a universal request into the Control Plane communication path.
///
/// This function performs only system-level admission. It intentionally does
/// not resolve capabilities, select destinations, inspect runtime readiness,
/// authorize execution, or invoke transport/execution mechanisms.
///
/// The request is borrowed immutably because admission must not modify
/// operation, attempt, routing, or payload state.
pub fn admit(request: &UniversalRequest) -> Result<(), AdmissionError> {
    validate_request(request).map_err(AdmissionError::Contract)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::metadata::{ContractMetadata, Participants, RequirementsMetadata};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

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

        let envelope = crate::contracts::envelope::MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            OperationContext::new(operation),
            metadata,
            EncodedPayload::new(payload_descriptor, b"opaque payload".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    #[test]
    fn valid_request_is_admitted() {
        assert_eq!(admit(&request()), Ok(()));
    }

    #[test]
    fn response_interaction_is_rejected() {
        let mut request = request();

        request.event.envelope.metadata.descriptor.interaction = Interaction::Response;

        assert_eq!(
            admit(&request),
            Err(AdmissionError::Contract(
                ValidationError::InteractionMismatch
            ))
        );
    }

    #[test]
    fn empty_payload_is_rejected() {
        let mut request = request();

        let descriptor = request.event.envelope.payload.descriptor().clone();

        request.event.envelope.payload = EncodedPayload::new(descriptor, Vec::<u8>::new());

        assert_eq!(
            admit(&request),
            Err(AdmissionError::Contract(ValidationError::EmptyPayload))
        );
    }

    #[test]
    fn capability_requirement_mismatch_is_rejected() {
        let mut request = request();

        request.event.envelope.metadata.requirements = RequirementsMetadata::none()
            .requiring_capability(CapabilityId::new("different-capability").unwrap());

        assert_eq!(
            admit(&request),
            Err(AdmissionError::Contract(
                ValidationError::CapabilityRequirementMismatch
            ))
        );
    }

    #[test]
    fn minimum_contract_version_mismatch_is_rejected() {
        let mut request = request();

        request.event.envelope.metadata.requirements =
            RequirementsMetadata::none().requiring_contract_version(Version::new(2, 0, 0));

        assert_eq!(
            admit(&request),
            Err(AdmissionError::Contract(
                ValidationError::MinimumContractVersionMismatch
            ))
        );
    }

    #[test]
    fn payload_descriptor_mismatch_is_rejected() {
        let mut request = request();

        let mismatched_descriptor =
            PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();

        request.event.envelope.payload =
            EncodedPayload::new(mismatched_descriptor, b"opaque payload".to_vec());

        assert_eq!(
            admit(&request),
            Err(AdmissionError::Contract(
                ValidationError::PayloadDescriptorMismatch
            ))
        );
    }

    #[test]
    fn admission_does_not_create_an_attempt() {
        let request = request();

        assert!(
            request
                .event
                .envelope
                .operation_context
                .attempt_id
                .is_none()
        );

        assert_eq!(admit(&request), Ok(()));

        assert!(
            request
                .event
                .envelope
                .operation_context
                .attempt_id
                .is_none()
        );
    }

    #[test]
    fn admission_does_not_modify_the_request() {
        let request = request();
        let before = request.clone();

        assert_eq!(admit(&request), Ok(()));
        assert_eq!(request, before);
    }

    #[test]
    fn admission_preserves_explicit_engine_instances() {
        let mut request = request();

        let sender_instance = crate::identity::EngineInstanceId::new("caller-01").unwrap();
        let target_instance = crate::identity::EngineInstanceId::new("provider-01").unwrap();

        request.event.envelope.metadata.participants.sender_instance =
            Some(sender_instance.clone());
        request.event.envelope.metadata.participants.target_instance =
            Some(target_instance.clone());

        assert_eq!(admit(&request), Ok(()));

        assert_eq!(
            request
                .event
                .envelope
                .metadata
                .participants
                .sender_instance
                .as_ref(),
            Some(&sender_instance)
        );
        assert_eq!(
            request
                .event
                .envelope
                .metadata
                .participants
                .target_instance
                .as_ref(),
            Some(&target_instance)
        );
    }
}
