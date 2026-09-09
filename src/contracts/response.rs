use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::status::Status;

/// A universal response envelope carrying an opaque capability payload.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UniversalResponse {
    pub envelope: MessageEnvelope,
    pub status: Status,
}

impl UniversalResponse {
    pub fn new(envelope: MessageEnvelope, status: Status) -> Self {
        Self { envelope, status }
    }

    pub fn has_response_interaction(&self) -> bool {
        self.envelope.metadata.descriptor.interaction == Interaction::Response
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
    use crate::status::Status;

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
        let resp = UniversalResponse::new(make_envelope(Interaction::Response), Status::Success);
        assert!(resp.has_response_interaction());
    }

    #[test]
    fn response_does_not_have_request_or_event_interaction() {
        let resp = UniversalResponse::new(make_envelope(Interaction::Request), Status::Success);
        assert!(!resp.has_response_interaction());

        let resp = UniversalResponse::new(make_envelope(Interaction::Event), Status::Success);
        assert!(!resp.has_response_interaction());
    }

    #[test]
    fn response_preserves_envelope_and_status() {
        let env = make_envelope(Interaction::Response);
        let env_id = env.message_id.as_str().to_string();
        let resp = UniversalResponse::new(env, Status::TimedOut);
        assert_eq!(resp.envelope.message_id.as_str(), env_id);
        assert_eq!(resp.status, Status::TimedOut);
    }
}
