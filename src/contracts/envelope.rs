use crate::contracts::descriptor::EncodedPayload;
use crate::contracts::metadata::ContractMetadata;
use crate::identity::MessageId;
use crate::operation::OperationContext;

/// The common identity, context, metadata, and opaque payload of a message.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MessageEnvelope {
    pub message_id: MessageId,
    pub operation_context: OperationContext,
    pub metadata: ContractMetadata,
    pub payload: EncodedPayload,
}

impl MessageEnvelope {
    pub fn new(
        message_id: MessageId,
        operation_context: OperationContext,
        metadata: ContractMetadata,
        payload: EncodedPayload,
    ) -> Self {
        Self {
            message_id,
            operation_context,
            metadata,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

    fn make_envelope() -> MessageEnvelope {
        let desc = ContractDescriptor::new(
            ContractId::new("test.contract").unwrap(),
            CapabilityId::new("test-cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
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
    fn envelope_preserves_all_fields() {
        let env = make_envelope();
        assert_eq!(env.message_id.as_str(), "msg-1");
        assert_eq!(env.operation_context.operation.id.as_str(), "op-1");
        assert_eq!(env.payload.bytes(), b"test payload");
    }

    #[test]
    fn envelope_fields_are_accessible() {
        let env = make_envelope();
        let _: &MessageId = &env.message_id;
        let _: &OperationContext = &env.operation_context;
        let _: &ContractMetadata = &env.metadata;
        let _: &EncodedPayload = &env.payload;
    }
}
