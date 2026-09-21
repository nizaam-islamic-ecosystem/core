//! Phase 7 boundary for the universal engine client mechanism.
//!
//! The universal client provides a high-level interface for sending universal
//! requests through an abstract transport and receiving universal responses.
//! Typed capability clients build on the universal client rather than
//! creating separate transport stacks.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineInstanceId;
use crate::transport::{BoxedFuture, Transport, TransportError};

/// A universal engine client.
///
/// Provides a simple interface for sending universal requests to concrete
/// engine instances via an abstract transport. Typed capability clients build
/// on top of the universal client instead of creating their own transport
/// stacks.
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

    /// Sends a universal request to the specified concrete engine instance
    /// and awaits the response.
    ///
    /// The request must explicitly identify the same concrete target instance
    /// in its contract metadata. This prevents a caller from routing through
    /// one instance while the message metadata identifies another instance.
    ///
    /// # Returns
    ///
    /// A boxed future yielding the universal response or a transport error.
    pub fn send(
        &self,
        target: &EngineInstanceId,
        request: UniversalRequest,
    ) -> BoxedFuture<UniversalResponse, TransportError> {
        let request_target_instance = request
            .event
            .envelope
            .metadata
            .participants
            .target_instance
            .as_ref();

        match request_target_instance {
            Some(instance) if instance == target => {}
            Some(_) => {
                return Box::pin(async {
                    Err(TransportError::Peer(
                        "request target instance does not match the client target".into(),
                    ))
                });
            }
            None => {
                return Box::pin(async {
                    Err(TransportError::Peer(
                        "request does not specify a concrete target instance".into(),
                    ))
                });
            }
        }

        self.transport.call(target, request)
    }

    /// Returns a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns true if the transport believes it is connected to the
    /// specified concrete engine instance.
    pub fn is_connected(&self, target: &EngineInstanceId) -> bool {
        self.transport.is_connected(target)
    }

    /// Returns the concrete engine instances this client is connected to.
    pub fn connected_targets(&self) -> Vec<EngineInstanceId> {
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
        CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::status::Status;
    use crate::transport::{InMemoryTransport, TransportError};

    fn make_request(
        target: &EngineId,
        target_instance: Option<&EngineInstanceId>,
        message_id: &str,
    ) -> UniversalRequest {
        let desc = ContractDescriptor::new(
            ContractId::new("test.contract").unwrap(),
            CapabilityId::new("test-cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let payload_desc = desc.payload.clone();

        let mut participants = Participants::new(EngineId::new("sender").unwrap(), target.clone());

        if let Some(instance) = target_instance {
            participants = participants.with_target_instance(instance.clone());
        }

        let metadata = ContractMetadata::new(desc, participants);

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
    fn client_can_send_request_to_a_concrete_instance() {
        let transport = InMemoryTransport::new();
        let client = UniversalClient::new(transport.clone());

        let target = EngineId::new("echo").unwrap();
        let instance = EngineInstanceId::new("echo-1").unwrap();

        transport.register(target.clone(), instance.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        let request = make_request(&target, Some(&instance), "msg-1");

        let response = futures::executor::block_on(client.send(&instance, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.event.envelope.message_id.as_str(), "msg-1");
    }

    #[test]
    fn client_rejects_a_request_with_a_mismatched_target_instance() {
        let transport = InMemoryTransport::new();

        let engine = EngineId::new("echo").unwrap();
        let dispatch_instance = EngineInstanceId::new("echo-1").unwrap();
        let request_instance = EngineInstanceId::new("echo-2").unwrap();

        transport.register(engine.clone(), dispatch_instance.clone(), |_request| {
            panic!("mismatched request target instance must not reach transport");
        });

        let client = UniversalClient::new(transport);

        let request = make_request(&engine, Some(&request_instance), "msg-mismatch");

        let result = futures::executor::block_on(client.send(&dispatch_instance, request));

        assert_eq!(
            result.expect_err("mismatched request target instance must be rejected"),
            TransportError::Peer("request target instance does not match the client target".into())
        );
    }

    #[test]
    fn client_rejects_a_request_without_a_concrete_target_instance() {
        let transport = InMemoryTransport::new();

        let engine = EngineId::new("echo").unwrap();
        let instance = EngineInstanceId::new("echo-1").unwrap();

        transport.register(engine, instance.clone(), |_request| {
            panic!("request without a concrete target instance must not reach transport");
        });

        let client = UniversalClient::new(transport);

        let request = make_request(
            &EngineId::new("echo").unwrap(),
            None,
            "msg-missing-instance",
        );

        let result = futures::executor::block_on(client.send(&instance, request));

        assert_eq!(
            result.expect_err("missing request target instance must be rejected"),
            TransportError::Peer("request does not specify a concrete target instance".into())
        );
    }

    #[test]
    fn client_reports_connected_concrete_instances() {
        let transport = InMemoryTransport::new();
        let client = UniversalClient::new(transport.clone());

        let engine = EngineId::new("echo").unwrap();
        let instance_1 = EngineInstanceId::new("echo-1").unwrap();
        let instance_2 = EngineInstanceId::new("echo-2").unwrap();

        transport.register(engine.clone(), instance_1.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        transport.register(engine, instance_2.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        assert!(client.is_connected(&instance_1));
        assert!(client.is_connected(&instance_2));

        let targets = client.connected_targets();

        assert!(targets.contains(&instance_1));
        assert!(targets.contains(&instance_2));
        assert_eq!(targets.len(), 2);
    }
}
