//! Phase 2 boundary for universal contracts, envelopes, and compatibility.

pub mod compatibility;
pub mod descriptor;
pub mod envelope;
pub mod event;
pub mod metadata;
pub mod request;
pub mod response;
pub mod validation;

pub use descriptor::{
    ContractDescriptor, EncodedPayload, EncodingError, Interaction, InvalidDescriptor,
    PayloadCodec, PayloadDescriptor, RawPayloadCodec, Version,
};
pub use envelope::MessageEnvelope;
pub use event::{UniversalEvent, UniversalEventError};
pub use metadata::{ContractMetadata, ExecutionMetadata, Participants, RequirementsMetadata};
pub use request::UniversalRequest;
pub use response::UniversalResponse;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, EventId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

    fn make_envelope(interaction: Interaction) -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("test.contract").unwrap(),
            CapabilityId::new("test-capability").unwrap(),
            Version::new(1, 0, 0),
            interaction,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let payload = EncodedPayload::new(descriptor.payload.clone(), b"test payload".to_vec());

        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("sender").unwrap(),
                EngineId::new("receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            operation_context,
            metadata,
            payload,
        )
    }

    fn make_event() -> UniversalEvent {
        UniversalEvent::new(
            make_envelope(Interaction::Event),
            EventId::new("event-1").unwrap(),
            "operation.completed",
            "engine:test",
        )
        .unwrap()
    }

    #[test]
    fn contracts_root_reexports_the_complete_universal_message_surface() {
        let _: Version = Version::new(1, 0, 0);
        let _: Interaction = Interaction::Request;
        let _: PayloadDescriptor =
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

        let event = make_event();
        let request = UniversalRequest::new(make_envelope(Interaction::Request));
        let response = UniversalResponse::new(
            make_envelope(Interaction::Response),
            crate::status::Status::Success,
        );

        assert!(event.has_event_interaction());
        assert!(request.has_request_interaction());
        assert!(response.has_response_interaction());
    }

    #[test]
    fn request_response_and_event_remain_distinct_interactions() {
        let request = UniversalRequest::new(make_envelope(Interaction::Request));
        let response = UniversalResponse::new(
            make_envelope(Interaction::Response),
            crate::status::Status::Success,
        );
        let event = make_event();

        assert!(request.has_request_interaction());
        assert!(
            !request
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Response)
        );
        assert!(
            !request
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Event)
        );

        assert!(response.has_response_interaction());
        assert!(
            !response
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Request)
        );
        assert!(
            !response
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Event)
        );

        assert!(event.has_event_interaction());
        assert!(
            !event
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Request)
        );
        assert!(
            !event
                .envelope
                .metadata
                .descriptor
                .interaction
                .eq(&Interaction::Response)
        );
    }

    #[test]
    fn universal_event_composes_with_message_envelope_and_preserves_context_and_payload() {
        let event = make_event();

        assert_eq!(event.event_id().as_str(), "event-1");
        assert_eq!(event.message_id().as_str(), "message-1");
        assert_eq!(
            event.envelope.operation_context.operation.id.as_str(),
            "operation-1"
        );
        assert_eq!(
            event
                .envelope
                .operation_context
                .operation
                .correlation_id
                .as_str(),
            "correlation-1"
        );
        assert_eq!(event.envelope.payload.bytes(), b"test payload");
        assert_eq!(event.event_type(), "operation.completed");
        assert_eq!(event.scope(), "engine:test");
    }

    #[test]
    fn event_and_message_identity_remain_distinct_at_the_contract_boundary() {
        let event = make_event();

        let event_id = event.event_id();
        let message_id = event.message_id();

        assert_eq!(event_id.as_str(), "event-1");
        assert_eq!(message_id.as_str(), "message-1");
        assert_ne!(event_id.as_str(), message_id.as_str());

        let _: &EventId = event_id;
        let _: &MessageId = message_id;
    }

    #[test]
    fn universal_event_rejects_request_and_response_envelopes() {
        let event_id = EventId::new("event-1").unwrap();

        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Request),
                event_id.clone(),
                "operation.completed",
                "engine:test",
            ),
            Err(UniversalEventError::WrongInteraction(Interaction::Request))
        );

        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Response),
                event_id,
                "operation.completed",
                "engine:test",
            ),
            Err(UniversalEventError::WrongInteraction(Interaction::Response))
        );
    }

    #[test]
    fn contract_validation_accepts_a_well_formed_event_envelope() {
        let event = make_event();

        assert_eq!(validation::validate_envelope(&event.envelope), Ok(()));
    }

    #[test]
    fn universal_event_requires_event_specific_metadata() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                EventId::new("event-1").unwrap(),
                "   ",
                "engine:test",
            ),
            Err(UniversalEventError::EmptyEventType)
        );

        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                EventId::new("event-2").unwrap(),
                "operation.completed",
                " ",
            ),
            Err(UniversalEventError::EmptyEventScope)
        );
    }

    #[test]
    fn event_validation_composes_with_generic_envelope_validation() {
        let event = make_event();

        assert_eq!(validation::validate_event(&event), Ok(()));
        assert_eq!(validation::validate_envelope(&event.envelope), Ok(()));
    }

    #[test]
    fn compatibility_keeps_event_interaction_in_the_contract_comparison() {
        let event_descriptor = make_envelope(Interaction::Event)
            .metadata
            .descriptor
            .clone();

        let request_descriptor = make_envelope(Interaction::Request)
            .metadata
            .descriptor
            .clone();

        assert_eq!(
            compatibility::compare_contracts(&event_descriptor, &request_descriptor),
            crate::status::Compatibility::Incompatible
        );
    }

    #[test]
    fn module_paths_and_root_reexports_compose_without_duplicate_contract_surfaces() {
        let root_event = make_event();

        let module_event = event::UniversalEvent::new(
            make_envelope(Interaction::Event),
            EventId::new("event-2").unwrap(),
            "operation.failed",
            "engine:other",
        )
        .unwrap();

        assert_eq!(root_event.event_id().as_str(), "event-1");
        assert_eq!(module_event.event_id().as_str(), "event-2");

        let _: descriptor::Interaction = Interaction::Event;
        let _: envelope::MessageEnvelope = root_event.envelope.clone();
        let _: metadata::ContractMetadata = root_event.envelope.metadata.clone();
        let _: request::UniversalRequest =
            UniversalRequest::new(make_envelope(Interaction::Request));
        let _: response::UniversalResponse = UniversalResponse::new(
            make_envelope(Interaction::Response),
            crate::status::Status::Success,
        );
    }
}
