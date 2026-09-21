//! Universal logical-message representation for Response interactions.
//!
//! A Response is one interaction form of the common universal message boundary.
//! Its common occurrence identity and semantic metadata are owned by
//! `UniversalEvent`; `Status` remains Response-specific.

use crate::contracts::UniversalEvent;
use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::identity::EventId;
use crate::status::Status;

/// A universal logical message carrying Response interaction semantics.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UniversalResponse {
    pub event: UniversalEvent,
    pub status: Status,
}

impl UniversalResponse {
    /// Creates a Response wrapper and constructs its universal occurrence
    /// internally. Callers do not construct `UniversalEvent` themselves.
    pub fn new(envelope: MessageEnvelope, status: Status) -> Self {
        let event = UniversalEvent::new(envelope, "universal.response", "response", "global")
            .expect("response occurrence metadata is static and valid");

        Self { event, status }
    }

    /// Returns the Response occurrence identity.
    pub fn event_id(&self) -> &EventId {
        self.event.event_id()
    }

    /// Returns the logical message identity carried by the universal occurrence.
    pub fn message_id(&self) -> &crate::identity::MessageId {
        self.event.message_id()
    }

    /// Returns the common universal occurrence boundary.
    pub fn universal_event(&self) -> &UniversalEvent {
        &self.event
    }

    /// Returns whether this logical message declares Response interaction semantics.
    pub fn has_response_interaction(&self) -> bool {
        self.event.envelope.metadata.descriptor.interaction == Interaction::Response
    }

    pub fn event_name(&self) -> &crate::events::EventName {
        self.event.event_name()
    }

    pub fn event_type(&self) -> &str {
        self.event.event_type()
    }

    pub fn event_scope(&self) -> &str {
        self.event.scope()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::envelope::MessageEnvelope;
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

    fn make_envelope(interaction: Interaction) -> MessageEnvelope {
        let desc = ContractDescriptor::new(
            ContractId::new("test.contract").unwrap(),
            CapabilityId::new("test-cap").unwrap(),
            Version::new(1, 0, 0),
            interaction,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let metadata = ContractMetadata::new(
            desc.clone(),
            Participants::new(
                EngineId::new("sender").unwrap(),
                EngineId::new("receiver").unwrap(),
            ),
        );
        let op_ctx = OperationContext::new(Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        ));
        MessageEnvelope::new(
            MessageId::new("msg-1").unwrap(),
            op_ctx,
            metadata,
            EncodedPayload::new(desc.payload, b"test payload"),
        )
    }

    #[test]
    fn response_has_response_interaction() {
        let response =
            UniversalResponse::new(make_envelope(Interaction::Response), Status::Success);

        assert!(response.has_response_interaction());
        assert!(!response.event_id().as_str().is_empty());
        assert_eq!(response.event_type(), "response");
    }

    #[test]
    fn response_does_not_have_request_or_event_interaction() {
        let response = UniversalResponse::new(make_envelope(Interaction::Request), Status::Success);
        assert!(!response.has_response_interaction());

        let response = UniversalResponse::new(make_envelope(Interaction::Event), Status::Success);
        assert!(!response.has_response_interaction());
    }

    #[test]
    fn response_preserves_envelope_status_and_universal_event() {
        let envelope = make_envelope(Interaction::Response);
        let message_id = envelope.message_id.as_str().to_string();

        let response = UniversalResponse::new(envelope, Status::TimedOut);

        assert_eq!(response.event.envelope.message_id.as_str(), message_id);
        assert_eq!(response.status, Status::TimedOut);
        assert!(!response.event_id().as_str().is_empty());
        assert_eq!(response.message_id().as_str(), "msg-1");
        assert_eq!(response.event_name().as_str(), "universal.response");
        assert_eq!(response.event_scope(), "global");
    }

    #[test]
    fn response_serialization_preserves_universal_event() {
        let response =
            UniversalResponse::new(make_envelope(Interaction::Response), Status::Success);

        let encoded = serde_json::to_string(&response).unwrap();
        let decoded: UniversalResponse = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, response);
    }
}
