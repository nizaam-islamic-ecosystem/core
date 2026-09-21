//! Phase 7 boundary for the universal engine client mechanism.
//!
//! The universal client provides a high-level interface for sending universal
//! requests through an abstract transport and receiving universal responses.
//! Typed capability clients build on the universal client rather than
//! creating separate transport stacks.

pub mod connection;
pub mod universal;

pub use universal::UniversalClient;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::connection::{
        ClientConnection, ClientConnectionFactory, ClientConnectionState,
    };
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::envelope::MessageEnvelope;
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::contracts::{UniversalRequest, UniversalResponse};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::status::Status;
    use crate::transport::{BoxedFuture, Transport, TransportError};
    use std::sync::Arc;

    fn make_request(
        target: &EngineId,
        target_instance: &EngineInstanceId,
        message_id: &str,
    ) -> UniversalRequest {
        let descriptor = ContractDescriptor::new(
            ContractId::new("client.level2.contract").unwrap(),
            CapabilityId::new("client.level2.capability").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let payload_descriptor = descriptor.payload.clone();

        let participants = Participants::new(EngineId::new("client").unwrap(), target.clone())
            .with_target_instance(target_instance.clone());

        let metadata = ContractMetadata::new(descriptor, participants);

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("client-level2-operation").unwrap(),
            CorrelationId::new("client-level2-correlation").unwrap(),
        ));

        let envelope = MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(payload_descriptor, b"level-2 payload"),
        );

        UniversalRequest::new(envelope)
    }

    struct TestClientConnection {
        engine_id: EngineId,
        instance_id: EngineInstanceId,
        state: ClientConnectionState,
        responder: Arc<dyn Fn(UniversalRequest) -> UniversalResponse + Send + Sync>,
    }

    impl TestClientConnection {
        fn new<F>(engine_id: EngineId, instance_id: EngineInstanceId, responder: F) -> Self
        where
            F: Fn(UniversalRequest) -> UniversalResponse + Send + Sync + 'static,
        {
            Self {
                engine_id,
                instance_id,
                state: ClientConnectionState::Open,
                responder: Arc::new(responder),
            }
        }
    }

    impl ClientConnection for TestClientConnection {
        fn peer_engine(&self) -> &EngineId {
            &self.engine_id
        }

        fn peer_instance(&self) -> &EngineInstanceId {
            &self.instance_id
        }

        fn state(&self) -> ClientConnectionState {
            self.state
        }

        fn call(
            &self,
            request: UniversalRequest,
        ) -> BoxedFuture<UniversalResponse, TransportError> {
            let responder = Arc::clone(&self.responder);

            Box::pin(async move { Ok(responder(request)) })
        }

        fn close(&mut self) {
            self.state = ClientConnectionState::Closed;
        }
    }

    struct ConnectionBackedTransport {
        connection: Arc<TestClientConnection>,
    }

    impl ConnectionBackedTransport {
        fn new(connection: TestClientConnection) -> Self {
            Self {
                connection: Arc::new(connection),
            }
        }
    }

    impl Transport for ConnectionBackedTransport {
        fn call(
            &self,
            target: &EngineInstanceId,
            request: UniversalRequest,
        ) -> BoxedFuture<UniversalResponse, TransportError> {
            if target != self.connection.peer_instance() {
                return Box::pin(async { Err(TransportError::Disconnected) });
            }

            self.connection.call(request)
        }

        fn is_connected(&self, target: &EngineInstanceId) -> bool {
            target == self.connection.peer_instance() && self.connection.state().is_open()
        }

        fn connected_targets(&self) -> Vec<EngineInstanceId> {
            if self.connection.state().is_open() {
                vec![self.connection.peer_instance().clone()]
            } else {
                Vec::new()
            }
        }
    }

    struct TestConnectionFactory {
        engine_id: EngineId,
        instance_id: EngineInstanceId,
    }

    impl ClientConnectionFactory for TestConnectionFactory {
        fn connect(
            &self,
            target: &EngineInstanceId,
        ) -> BoxedFuture<Box<dyn ClientConnection>, TransportError> {
            let engine_id = self.engine_id.clone();
            let instance_id = self.instance_id.clone();

            if target != &instance_id {
                return Box::pin(async { Err(TransportError::Disconnected) });
            }

            Box::pin(async move {
                Ok(Box::new(TestClientConnection::new(
                    engine_id,
                    instance_id,
                    |request| UniversalResponse::new(request.event.envelope, Status::Success),
                )) as Box<dyn ClientConnection>)
            })
        }
    }

    #[test]
    fn level_2_client_connection_exposes_both_engine_identities() {
        let engine_id = EngineId::new("echo").unwrap();
        let instance_id = EngineInstanceId::new("echo-instance-1").unwrap();

        let connection =
            TestClientConnection::new(engine_id.clone(), instance_id.clone(), |request| {
                UniversalResponse::new(request.event.envelope, Status::Success)
            });

        assert_eq!(connection.peer_engine(), &engine_id);
        assert_eq!(connection.peer_instance(), &instance_id);
        assert_eq!(connection.state(), ClientConnectionState::Open);
        assert!(connection.state().is_open());
    }

    #[test]
    fn level_2_connection_factory_connects_to_concrete_instance() {
        let engine_id = EngineId::new("echo").unwrap();
        let instance_id = EngineInstanceId::new("echo-instance-1").unwrap();

        let factory = TestConnectionFactory {
            engine_id: engine_id.clone(),
            instance_id: instance_id.clone(),
        };

        let connection = futures::executor::block_on(factory.connect(&instance_id)).unwrap();

        assert_eq!(connection.peer_engine(), &engine_id);
        assert_eq!(connection.peer_instance(), &instance_id);
        assert_eq!(connection.state(), ClientConnectionState::Open);
    }

    #[test]
    fn level_2_connection_factory_rejects_wrong_instance() {
        let engine_id = EngineId::new("echo").unwrap();
        let registered_instance = EngineInstanceId::new("echo-instance-1").unwrap();
        let wrong_instance = EngineInstanceId::new("echo-instance-2").unwrap();

        let factory = TestConnectionFactory {
            engine_id,
            instance_id: registered_instance,
        };

        let result = futures::executor::block_on(factory.connect(&wrong_instance));

        assert!(
            matches!(result, Err(TransportError::Disconnected)),
            "wrong concrete instance must be rejected"
        );
    }

    #[test]
    fn level_2_universal_client_integrates_with_client_connection() {
        let engine_id = EngineId::new("echo").unwrap();
        let instance_id = EngineInstanceId::new("echo-instance-1").unwrap();

        let connection =
            TestClientConnection::new(engine_id.clone(), instance_id.clone(), |request| {
                assert_eq!(
                    request
                        .event
                        .envelope
                        .metadata
                        .participants
                        .target_instance
                        .as_ref(),
                    Some(&EngineInstanceId::new("echo-instance-1").unwrap())
                );

                UniversalResponse::new(request.event.envelope, Status::Success)
            });

        let transport = ConnectionBackedTransport::new(connection);
        let client = UniversalClient::new(transport);

        let request = make_request(&engine_id, &instance_id, "integration-message");

        let response = futures::executor::block_on(client.send(&instance_id, request))
            .expect("universal client should receive connection response");

        assert_eq!(response.status, Status::Success);
        assert_eq!(
            response.event.envelope.message_id.as_str(),
            "integration-message"
        );
    }

    #[test]
    fn level_2_universal_client_preserves_concrete_instance_boundary() {
        let engine_id = EngineId::new("echo").unwrap();
        let instance_1 = EngineInstanceId::new("echo-instance-1").unwrap();
        let instance_2 = EngineInstanceId::new("echo-instance-2").unwrap();

        let connection =
            TestClientConnection::new(engine_id.clone(), instance_1.clone(), |request| {
                UniversalResponse::new(request.event.envelope, Status::Success)
            });

        let transport = ConnectionBackedTransport::new(connection);
        let client = UniversalClient::new(transport);

        let request = make_request(&engine_id, &instance_2, "wrong-instance");

        let result = futures::executor::block_on(client.send(&instance_1, request));

        assert_eq!(
            result.expect_err("universal client must reject mismatched instance"),
            TransportError::Peer("request target instance does not match the client target".into())
        );
    }
}
