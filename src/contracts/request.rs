use crate::contracts::UniversalEvent;
use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::identity::EventId;

/// A universal logical message carrying Request interaction semantics.
///
/// The common occurrence boundary is owned by `UniversalEvent`; request-specific
/// behavior remains on this wrapper.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UniversalRequest {
    pub event: UniversalEvent,
}

impl UniversalRequest {
    /// Creates a Request wrapper and constructs its universal occurrence
    /// internally. Callers do not construct `UniversalEvent` themselves.
    pub fn new(envelope: MessageEnvelope) -> Self {
        let event = UniversalEvent::new(envelope, "universal.request", "request", "global")
            .expect("request occurrence metadata is static and valid");

        Self { event }
    }

    /// Returns the Request occurrence identity.
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

    /// Returns the canonical name of the request event.
    pub fn event_name(&self) -> &crate::events::EventName {
        self.event.event_name()
    }

    /// Returns the request event type.
    pub fn event_type(&self) -> &str {
        self.event.event_type()
    }

    /// Returns the request event scope.
    pub fn event_scope(&self) -> &str {
        self.event.scope()
    }

    /// Returns whether this logical message declares Request interaction semantics.
    pub fn has_request_interaction(&self) -> bool {
        self.event.envelope.metadata.descriptor.interaction == Interaction::Request
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
    fn request_has_request_interaction() {
        let req = UniversalRequest::new(make_envelope(Interaction::Request));

        assert!(req.has_request_interaction());
        assert!(!req.event_id().as_str().is_empty());
        assert_eq!(req.universal_event().event_type(), "request");
    }

    #[test]
    fn request_does_not_have_response_or_event_interaction() {
        let req = UniversalRequest::new(make_envelope(Interaction::Response));
        assert!(!req.has_request_interaction());

        let req = UniversalRequest::new(make_envelope(Interaction::Event));
        assert!(!req.has_request_interaction());
    }

    #[test]
    fn request_preserves_envelope_and_universal_event() {
        let env = make_envelope(Interaction::Request);
        let env_id = env.message_id.as_str().to_string();

        let req = UniversalRequest::new(env);

        assert_eq!(req.event.envelope.message_id.as_str(), env_id);
        assert_eq!(req.message_id().as_str(), "msg-1");
        assert!(!req.event_id().as_str().is_empty());
        assert_eq!(req.event_name().as_str(), "universal.request");
        assert_eq!(req.event_type(), "request");
        assert_eq!(req.event_scope(), "global");
    }

    #[test]
    fn request_serialization_preserves_universal_event() {
        let req = UniversalRequest::new(make_envelope(Interaction::Request));

        let encoded = serde_json::to_string(&req).unwrap();
        let decoded: UniversalRequest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, req);
    }
}
