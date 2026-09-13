//! Phase 7 boundary for the engine server.
//!
//! The engine server handles incoming requests from clients and
//! dispatches them to capability handlers. It manages connection
//! state and request routing.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::{CapabilityId, EngineId};
use crate::status::Status;
use crate::transport::TransportError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The server state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerState {
    /// The server is starting up.
    Starting,
    /// The server is running and accepting requests.
    Serving,
    /// The server is draining and not accepting new requests.
    Draining,
    /// The server has stopped.
    Stopped,
}

impl ServerState {
    /// Returns true if the server is currently serving.
    pub fn is_serving(&self) -> bool {
        matches!(self, ServerState::Serving)
    }

    /// Returns true if the server is currently draining.
    pub fn is_draining(&self) -> bool {
        matches!(self, ServerState::Draining)
    }
}

/// A handler for incoming universal requests.
pub type RequestHandler = Arc<dyn Fn(UniversalRequest) -> UniversalResponse + Send + Sync>;

/// An engine server that listens for and handles incoming requests.
pub struct EngineServer {
    engine_id: EngineId,
    state: ServerState,
    handlers: Arc<Mutex<HashMap<CapabilityId, RequestHandler>>>,
}

impl EngineServer {
    /// Creates a new engine server for the specified engine ID.
    pub fn new(engine_id: EngineId) -> Self {
        Self {
            engine_id,
            state: ServerState::Starting,
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the current server state.
    pub fn state(&self) -> ServerState {
        self.state
    }

    /// Registers a capability handler for the specified capability ID.
    pub fn register_handler(&self, capability_id: CapabilityId, handler: RequestHandler) {
        self.handlers.lock().unwrap().insert(capability_id, handler);
    }

    /// Removes a handler for the specified capability ID.
    pub fn unregister_handler(&self, capability_id: &CapabilityId) {
        self.handlers.lock().unwrap().remove(capability_id);
    }

    /// Starts the server, transitioning from Starting to Serving.
    pub fn start(&mut self) {
        self.state = ServerState::Serving;
    }

    /// Initiates graceful shutdown, transitioning from Serving to Draining.
    pub fn drain(&mut self) {
        if self.state == ServerState::Serving {
            self.state = ServerState::Draining;
        }
    }

    /// Stops the server, transitioning to Stopped.
    pub fn stop(&mut self) {
        self.state = ServerState::Stopped;
    }

    /// Returns a reference to the registered handlers.
    pub fn handlers(&self) -> Arc<Mutex<HashMap<CapabilityId, RequestHandler>>> {
        Arc::clone(&self.handlers)
    }

    /// Returns the engine ID of this server.
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }
}

/// Handles an incoming request by dispatching it to the appropriate handler.
///
/// Returns an error response if the server is not serving or no handler is found.
pub fn handle_request(
    server: &EngineServer,
    request: UniversalRequest,
) -> Result<UniversalResponse, TransportError> {
    if !request.has_request_interaction() {
        return Ok(failure_response(request));
    }

    if !server.state().is_serving() {
        return Ok(failure_response(request));
    }

    let handlers = server.handlers();
    let capability_id = request.envelope.metadata.descriptor.capability_id.clone();
    let handler = {
        let handlers_guard = handlers.lock().unwrap();
        handlers_guard.get(&capability_id).cloned()
    };
    match handler {
        Some(handler) => Ok(handler(request)),
        None => Ok(failure_response(request)),
    }
}

fn failure_response(request: UniversalRequest) -> UniversalResponse {
    let envelope = request.envelope;
    let metadata = envelope.metadata;
    let request_descriptor = metadata.descriptor;
    let payload_descriptor = request_descriptor.payload.clone();
    let requesting_participants = metadata.participants;

    let descriptor = crate::contracts::ContractDescriptor::new(
        request_descriptor.contract_id,
        request_descriptor.capability_id,
        request_descriptor.version,
        crate::contracts::Interaction::Response,
        payload_descriptor.clone(),
    );

    let sender_instance = requesting_participants.target_instance.clone();
    let target_instance = requesting_participants.sender_instance.clone();
    let mut participants = crate::contracts::Participants::new(
        requesting_participants.target,
        requesting_participants.sender,
    );
    if let Some(instance) = sender_instance {
        participants = participants.with_sender_instance(instance);
    }
    if let Some(instance) = target_instance {
        participants = participants.with_target_instance(instance);
    }

    let response_metadata = crate::contracts::ContractMetadata::new(descriptor, participants)
        .with_requirements(metadata.requirements)
        .with_execution(metadata.execution);

    UniversalResponse::new(
        crate::contracts::MessageEnvelope::new(
            envelope.message_id,
            envelope.operation_context,
            response_metadata,
            crate::contracts::EncodedPayload::new(payload_descriptor, Vec::new()),
        ),
        Status::Failure,
    )
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
    fn server_starts_in_starting_state() {
        let server = EngineServer::new(EngineId::new("server").unwrap());
        assert_eq!(server.state(), ServerState::Starting);
    }

    #[test]
    fn server_transitions_starting_to_serving() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        server.start();
        assert_eq!(server.state(), ServerState::Serving);
    }

    #[test]
    fn server_transitions_serving_to_draining() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        server.start();
        server.drain();
        assert_eq!(server.state(), ServerState::Draining);
    }

    #[test]
    fn server_transitions_to_stopped() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        server.start();
        server.stop();
        assert_eq!(server.state(), ServerState::Stopped);
    }

    #[test]
    fn server_can_register_and_handle_requests() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        let target = EngineId::new("server").unwrap();

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|request| UniversalResponse::new(request.envelope, Status::Success)),
        );

        server.start();

        let request = make_request(&target, "msg-1");
        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Success);
    }

    #[test]
    fn server_rejects_request_when_not_serving() {
        let server = EngineServer::new(EngineId::new("server").unwrap());
        let target = EngineId::new("server").unwrap();

        let request = make_request(&target, "msg-1");
        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn server_rejects_non_request_interaction_before_handler_dispatch() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        let target = EngineId::new("server").unwrap();

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_request| panic!("non-request interaction reached handler")),
        );
        server.start();

        let mut request = make_request(&target, "msg-invalid");
        request.envelope.metadata.descriptor.interaction = Interaction::Response;

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
        assert_eq!(
            response.envelope.metadata.descriptor.interaction,
            Interaction::Response
        );
    }

    #[test]
    fn server_returns_failure_when_handler_missing() {
        let mut server = EngineServer::new(EngineId::new("server").unwrap());
        let target = EngineId::new("server").unwrap();

        server.start();

        let request = make_request(&target, "msg-1");
        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn in_memory_transport_integration_with_server() {
        use crate::client::UniversalClient;
        use crate::identity::CapabilityId;
        use crate::transport::InMemoryTransport;

        let transport = InMemoryTransport::new();
        let server_id = EngineId::new("server").unwrap();

        let mut server = EngineServer::new(server_id.clone());
        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|request| UniversalResponse::new(request.envelope, Status::Success)),
        );

        server.start();

        transport.register(server_id.clone(), move |request| {
            handle_request(&server, request).unwrap()
        });

        let client = UniversalClient::new(transport);
        let request = make_request(&server_id, "msg-1");
        let response = futures::executor::block_on(client.send(&server_id, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.envelope.message_id.as_str(), "msg-1");
    }
}
