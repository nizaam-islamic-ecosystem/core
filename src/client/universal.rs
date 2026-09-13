//! Phase 7 boundary for the universal engine client mechanism.
//!
//! The universal client provides a high-level interface for sending universal
//! requests through an abstract transport and receiving universal responses.
//! Typed capability clients build on the universal client rather than
//! creating separate transport stacks.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineId;
use crate::transport::{BoxedFuture, Transport, TransportError};

/// A universal engine client.
///
/// Provides a simple interface for sending universal requests to peer engines
/// via an abstract transport. Typed capability clients build on top of this
/// client instead of creating their own transport stacks.
pub struct UniversalClient<T>
where
    T: Transport,
{
    transport: T,
}

impl<T> UniversalClient<T>
where
    T: Transport,
{
    /// Creates a new universal client backed by the given transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Sends a universal request to the specified target and awaits the response.
    ///
    /// # Returns
    ///
    /// A boxed future yielding the universal response or a transport error.
    pub fn send(
        &self,
        target: &EngineId,
        request: UniversalRequest,
    ) -> BoxedFuture<UniversalResponse, TransportError> {
        self.transport.call(target, request)
    }

    /// Returns a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns true if the transport believes it is connected to the target.
    pub fn is_connected(&self, target: &EngineId) -> bool {
        self.transport.is_connected(target)
    }

    /// Returns the list of engine instances this client is connected to.
    pub fn connected_targets(&self) -> Vec<EngineId> {
        self.transport.connected_targets()
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
    use crate::transport::InMemoryTransport;

    fn make_request(target: &EngineId, message_id: &str) -> UniversalRequest {
        let desc = ContractDescriptor::new(
            ContractId::new("test.contract").unwrap(),
            CapabilityId::new("test-cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let payload_desc = desc.payload.clone();
        let metadata = ContractMetadata::new(
            desc,
            Participants::new(EngineId::new("sender").unwrap(), target.clone()),
        );
        let op_ctx = OperationContext::new(Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        ));
        let envelope = MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            op_ctx,
            metadata,
            EncodedPayload::new(payload_desc, b"test payload"),
        );
        UniversalRequest::new(envelope)
    }

    #[test]
    fn client_can_send_request_and_receive_response() {
        let transport = InMemoryTransport::new();
        let client = UniversalClient::new(transport.clone());
        let target = EngineId::new("echo").unwrap();

        transport.register(target.clone(), |request| {
            UniversalResponse::new(request.envelope, Status::Success)
        });

        let request = make_request(&target, "msg-1");
        let response = futures::executor::block_on(client.send(&target, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.envelope.message_id.as_str(), "msg-1");
    }

    #[test]
    fn client_reports_connected_targets() {
        let transport = InMemoryTransport::new();
        let client = UniversalClient::new(transport.clone());
        let target = EngineId::new("echo").unwrap();

        transport.register(target.clone(), |request| {
            UniversalResponse::new(request.envelope, Status::Success)
        });

        assert!(client.is_connected(&target));
        let targets = client.connected_targets();
        assert!(targets.contains(&target));
    }
}
