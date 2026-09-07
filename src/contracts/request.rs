use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;

/// A universal request envelope carrying an opaque capability payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniversalRequest {
    pub envelope: MessageEnvelope,
}

impl UniversalRequest {
    pub fn new(envelope: MessageEnvelope) -> Self {
        Self { envelope }
    }

    pub fn has_request_interaction(&self) -> bool {
        self.envelope.metadata.descriptor.interaction == Interaction::Request
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
    }

    #[test]
    fn request_does_not_have_response_or_event_interaction() {
        let req = UniversalRequest::new(make_envelope(Interaction::Response));
        assert!(!req.has_request_interaction());

        let req = UniversalRequest::new(make_envelope(Interaction::Event));
        assert!(!req.has_request_interaction());
    }

    #[test]
    fn request_preserves_envelope() {
        let env = make_envelope(Interaction::Request);
        let env_id = env.message_id.as_str().to_string();
        let req = UniversalRequest::new(env);
        assert_eq!(req.envelope.message_id.as_str(), env_id);
    }
}
