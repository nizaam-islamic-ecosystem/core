use crate::contracts::descriptor::EncodedPayload;
use crate::contracts::metadata::ContractMetadata;
use crate::identity::MessageId;
use crate::operation::OperationContext;

/// The common identity, context, metadata, and opaque payload of a message.
#[derive(Clone, Debug, Eq, PartialEq)]
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
