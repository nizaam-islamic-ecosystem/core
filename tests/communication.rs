//! Integration tests for Phase 7: transport and universal client/server.
//!
//! These tests exercise the Phase 7 public surface the way a downstream engine
//! would: a universal client sends a universal request through an abstract
//! transport to an engine server, the server dispatches it to a capability
//! handler, and a universal response travels back. They also cover the byte
//! level surface a concrete transport implementor must satisfy, namely the
//! `ByteSink`/`ByteSource` traits, the framed `MessageStream`, the
//! `Connection` and `ClientConnection` abstractions, and the binary
//! `MessageHeader` framing metadata.
//!
//! Nothing here interprets engine payload meaning. Payloads are opaque bytes
//! and are only ever compared for byte equality.

use std::sync::{Arc, Condvar, Mutex};

use nizaam_core::client::UniversalClient;
use nizaam_core::client::connection::{
    ClientConnection, ClientConnectionFactory, ClientConnectionState,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId,
    NodeId, OperationId, PlanId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::server::{EngineServer, RequestHandler, ServerState, handle_request};
use nizaam_core::status::Status;
use nizaam_core::transport::stream::{ByteSourceState, MAX_FRAME_LENGTH};
use nizaam_core::transport::{
    BoxedFuture, ByteSink, ByteSource, Connection, ConnectionState, InMemoryTransport,
    MessageHeader, MessageStream, StreamError, Transport, TransportError,
};

/// The opaque media type used by every test payload. Core never interprets it.
const MEDIA_TYPE: &str = "application/octet-stream";

/// The capability every engine in these tests exposes.
const CAPABILITY: &str = "lookup";

/// The framing protocol version used by these tests.
const FRAMING_VERSION: u8 = 1;

/// The final fragment indicator inside the framing flags byte.
const FINAL_FRAGMENT_FLAG: u8 = 0b0000_0001;

/// The fixed transport header length defined by the framing protocol.
const HEADER_LENGTH: usize = 20;

/// The offset of the flags byte inside the transport header.
const FLAGS_OFFSET: usize = 1;

// ---------------------------------------------------------------------------
// Contract construction helpers
// ---------------------------------------------------------------------------

/// Builds a contract descriptor for the given capability and interaction.
fn descriptor_for(capability: &str, interaction: Interaction) -> ContractDescriptor {
    ContractDescriptor::new(
        ContractId::new(format!("{capability}.contract")).unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 4, 2),
        interaction,
        PayloadDescriptor::new(MEDIA_TYPE, Version::new(2, 1, 0)).unwrap(),
    )
}

/// Builds a universal request addressed to `target` from a fixed caller engine.
fn request_for(
    target: &EngineId,
    message_id: &str,
    capability: &str,
    payload: &[u8],
) -> UniversalRequest {
    let participants = Participants::new(EngineId::new("caller-engine").unwrap(), target.clone());
    let operation_context = OperationContext::new(Operation::new(
        OperationId::new("op-1").unwrap(),
        CorrelationId::new("corr-1").unwrap(),
    ));

    request_with(
        message_id,
        capability,
        payload,
        participants,
        operation_context,
    )
}

/// Builds a universal request with caller supplied participants and context.
fn request_with(
    message_id: &str,
    capability: &str,
    payload: &[u8],
    participants: Participants,
    operation_context: OperationContext,
) -> UniversalRequest {
    let descriptor = descriptor_for(capability, Interaction::Request);
    let payload_descriptor = descriptor.payload.clone();
    let metadata = ContractMetadata::new(descriptor, participants);

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message_id).unwrap(),
        operation_context,
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

/// Builds the response an engine would return for `request`.
///
/// The response reuses the request's message identity and operation context,
/// declares the response interaction, and reverses the participants so the
/// answering engine becomes the sender.
fn response_for(request: UniversalRequest, status: Status, payload: Vec<u8>) -> UniversalResponse {
    let envelope = request.envelope;
    let request_descriptor = envelope.metadata.descriptor;
    let payload_descriptor = request_descriptor.payload.clone();

    let descriptor = ContractDescriptor::new(
        request_descriptor.contract_id,
        request_descriptor.capability_id,
        request_descriptor.version,
        Interaction::Response,
        payload_descriptor.clone(),
    );

    let requesting_participants = envelope.metadata.participants;
    let participants = Participants::new(
        requesting_participants.target,
        requesting_participants.sender,
    );

    UniversalResponse::new(
        MessageEnvelope::new(
            envelope.message_id,
            envelope.operation_context,
            ContractMetadata::new(descriptor, participants),
            EncodedPayload::new(payload_descriptor, payload),
        ),
        status,
    )
}

// ---------------------------------------------------------------------------
// Engine server helpers
// ---------------------------------------------------------------------------

/// A handler that echoes the request payload back with the given status.
fn echo_handler(status: Status) -> RequestHandler {
    Arc::new(move |request: UniversalRequest| {
        let payload = request.envelope.payload.bytes().to_vec();
        response_for(request, status, payload)
    })
}

/// A handler that answers with the responding engine's own identity.
fn identity_handler(engine: &EngineId) -> RequestHandler {
    let engine = engine.clone();
    Arc::new(move |request: UniversalRequest| {
        let payload = engine.as_str().as_bytes().to_vec();
        response_for(request, Status::Success, payload)
    })
}

/// Creates an engine server that is already serving the given capability.
fn serving_engine(
    engine: &EngineId,
    capability: &str,
    handler: RequestHandler,
) -> Arc<Mutex<EngineServer>> {
    let mut server = EngineServer::new(engine.clone());
    server.register_handler(CapabilityId::new(capability).unwrap(), handler);
    server.start();
    Arc::new(Mutex::new(server))
}

/// Publishes an engine server behind `engine`'s address on the transport.
fn expose(transport: &InMemoryTransport, engine: &EngineId, server: &Arc<Mutex<EngineServer>>) {
    let server = Arc::clone(server);
    transport.register(engine.clone(), move |request| {
        let server = server.lock().expect("engine server lock");
        handle_request(&server, request).expect("transport level dispatch must succeed")
    });
}

/// Sends a request through the client and unwraps the transport result.
fn call<T: Transport>(
    client: &UniversalClient<T>,
    target: &EngineId,
    request: UniversalRequest,
) -> UniversalResponse {
    futures::executor::block_on(client.send(target, request)).expect("transport call must succeed")
}

// ---------------------------------------------------------------------------
// Byte level test doubles for the transport stream traits
// ---------------------------------------------------------------------------

/// Shared buffer backing a single direction byte pipe.
struct ChannelState {
    buffer: Vec<u8>,
    closed: bool,
}

/// A byte pipe a downstream transport implementor could build a connection on.
struct ByteChannel {
    state: Arc<Mutex<ChannelState>>,
    signal: Arc<Condvar>,
}

impl ByteChannel {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ChannelState {
                buffer: Vec::new(),
                closed: false,
            })),
            signal: Arc::new(Condvar::new()),
        }
    }

    /// Produces a write end and a read end over the same buffer.
    fn split(&self) -> (ChannelWriter, ChannelReader) {
        (
            ChannelWriter {
                state: Arc::clone(&self.state),
                signal: Arc::clone(&self.signal),
            },
            ChannelReader {
                state: Arc::clone(&self.state),
                signal: Arc::clone(&self.signal),
                stream_state: ByteSourceState::new(),
            },
        )
    }
}

struct ChannelWriter {
    state: Arc<Mutex<ChannelState>>,
    signal: Arc<Condvar>,
}

struct ChannelReader {
    state: Arc<Mutex<ChannelState>>,
    signal: Arc<Condvar>,
    stream_state: ByteSourceState,
}

impl ByteSink for ChannelWriter {
    fn write(&self, buf: &[u8]) -> Result<(), StreamError> {
        let mut state = self.state.lock().expect("channel lock");
        if state.closed {
            return Err(StreamError::Closed);
        }
        state.buffer.extend_from_slice(buf);
        drop(state);
        self.signal.notify_all();
        Ok(())
    }

    fn flush(&self) -> Result<(), StreamError> {
        Ok(())
    }

    fn close(&self) -> Result<(), StreamError> {
        let mut state = self.state.lock().expect("channel lock");
        state.closed = true;
        drop(state);
        self.signal.notify_all();
        Ok(())
    }
}

impl ByteSource for ChannelReader {
    fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError> {
        let mut state = self.state.lock().expect("channel lock");
        while state.buffer.is_empty() && !state.closed {
            state = self
                .signal
                .wait(state)
                .map_err(|error| StreamError::Io(error.to_string()))?;
        }

        if state.buffer.is_empty() {
            return Ok(None);
        }

        // Deliberately hand back a short read when the caller's buffer is
        // smaller than the pending bytes, which a real socket may also do.
        let count = state.buffer.len().min(buf.len());
        buf[..count].copy_from_slice(&state.buffer[..count]);
        state.buffer.drain(..count);
        Ok(Some(count))
    }

    fn stream_state(&self) -> &ByteSourceState {
        &self.stream_state
    }
}

/// A loopback `Connection` whose sink feeds its own source.
struct LoopbackConnection {
    peer: EngineId,
    state: ConnectionState,
    writer: ChannelWriter,
    reader: ChannelReader,
}

impl LoopbackConnection {
    fn open(peer: EngineId, channel: &ByteChannel) -> Self {
        let (writer, reader) = channel.split();
        Self {
            peer,
            state: ConnectionState::Open,
            writer,
            reader,
        }
    }
}

impl Connection for LoopbackConnection {
    fn peer(&self) -> &EngineId {
        &self.peer
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    fn sink(&self) -> &dyn ByteSink {
        &self.writer
    }

    fn source(&self) -> &dyn ByteSource {
        &self.reader
    }

    fn close(&mut self) {
        self.state = ConnectionState::Closing;
        self.writer.close().expect("closing the sink must succeed");
        self.state = ConnectionState::Closed;
    }
}

// ---------------------------------------------------------------------------
// Client connection test doubles
// ---------------------------------------------------------------------------

/// A `ClientConnection` that forwards calls to an in-memory transport.
struct TransportClientConnection {
    peer: EngineId,
    state: ClientConnectionState,
    transport: InMemoryTransport,
}

impl ClientConnection for TransportClientConnection {
    fn peer(&self) -> &EngineId {
        &self.peer
    }

    fn state(&self) -> ClientConnectionState {
        self.state
    }

    fn call(&self, request: UniversalRequest) -> BoxedFuture<UniversalResponse, TransportError> {
        if !self.state.is_open() {
            return Box::pin(async {
                Err::<UniversalResponse, TransportError>(TransportError::Closed)
            });
        }
        self.transport.call(&self.peer, request)
    }

    fn close(&mut self) {
        self.state = ClientConnectionState::Closed;
    }
}

/// A `ClientConnectionFactory` over an in-memory transport.
struct InMemoryConnectionFactory {
    transport: InMemoryTransport,
}

impl ClientConnectionFactory for InMemoryConnectionFactory {
    fn connect(&self, target: &EngineId) -> BoxedFuture<Box<dyn ClientConnection>, TransportError> {
        let transport = self.transport.clone();
        let target = target.clone();

        Box::pin(async move {
            if !transport.is_connected(&target) {
                return Err(TransportError::Disconnected);
            }

            let connection: Box<dyn ClientConnection> = Box::new(TransportClientConnection {
                peer: target,
                state: ClientConnectionState::Open,
                transport,
            });
            Ok(connection)
        })
    }
}

// ---------------------------------------------------------------------------
// Client, transport, and engine server end to end
// ---------------------------------------------------------------------------

#[test]
fn client_request_reaches_engine_server_and_returns_universal_response() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("lookup-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);
    let request = request_for(&engine, "msg-1", CAPABILITY, b"opaque request");
    assert!(request.has_request_interaction());

    let response = call(&client, &engine, request);

    assert_eq!(response.status, Status::Success);
    assert_eq!(response.envelope.message_id.as_str(), "msg-1");
    assert_eq!(response.envelope.payload.bytes(), b"opaque request");
}

#[test]
fn response_declares_the_response_interaction_and_reversed_participants() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("lookup-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-interaction", CAPABILITY, b"payload"),
    );

    assert!(response.has_response_interaction());
    assert_eq!(
        response.envelope.metadata.descriptor.interaction,
        Interaction::Response
    );
    assert_eq!(response.envelope.metadata.participants.sender, engine);
    assert_eq!(
        response.envelope.metadata.participants.target.as_str(),
        "caller-engine"
    );
    assert_eq!(
        response.envelope.metadata.descriptor.capability_id.as_str(),
        CAPABILITY
    );
}

#[test]
fn operation_context_survives_the_transport_round_trip() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("context-engine").unwrap();

    let observed: Arc<Mutex<Option<OperationContext>>> = Arc::new(Mutex::new(None));
    let observed_in_handler = Arc::clone(&observed);
    let handler: RequestHandler = Arc::new(move |request: UniversalRequest| {
        *observed_in_handler.lock().expect("observation lock") =
            Some(request.envelope.operation_context.clone());
        response_for(request, Status::Success, b"ok".to_vec())
    });

    let server = serving_engine(&engine, CAPABILITY, handler);
    expose(&transport, &engine, &server);

    let operation = Operation::new(
        OperationId::new("op-context").unwrap(),
        CorrelationId::new("corr-context").unwrap(),
    )
    .with_plan(PlanId::new("plan-context").unwrap())
    .with_parent(OperationId::new("parent-context").unwrap());
    let operation_context = OperationContext::new(operation).for_attempt(
        NodeId::new("node-3").unwrap(),
        AttemptId::new("attempt-2").unwrap(),
    );

    let request = request_with(
        "msg-context",
        CAPABILITY,
        b"payload",
        Participants::new(EngineId::new("caller-engine").unwrap(), engine.clone()),
        operation_context.clone(),
    );

    let client = UniversalClient::new(transport);
    let response = call(&client, &engine, request);

    let seen = observed
        .lock()
        .expect("observation lock")
        .take()
        .expect("the engine handler must have been invoked");

    // The transport encodes and decodes the request, so the context the engine
    // observes proves that identity, plan, parent, node, and attempt survive.
    assert_eq!(seen, operation_context);
    assert_eq!(response.envelope.operation_context, operation_context);
}

#[test]
fn payload_descriptor_metadata_survives_the_transport_round_trip() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("descriptor-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-descriptor", CAPABILITY, b"payload"),
    );

    let descriptor = response.envelope.payload.descriptor();
    assert_eq!(descriptor.media_type(), MEDIA_TYPE);
    assert_eq!(descriptor.schema_version(), &Version::new(2, 1, 0));
    assert_eq!(
        response.envelope.metadata.descriptor.version,
        Version::new(1, 4, 2)
    );
}

#[test]
fn engine_instance_identities_survive_the_transport_round_trip() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("instance-engine").unwrap();

    let observed: Arc<Mutex<Option<Participants>>> = Arc::new(Mutex::new(None));
    let observed_in_handler = Arc::clone(&observed);
    let handler: RequestHandler = Arc::new(move |request: UniversalRequest| {
        *observed_in_handler.lock().expect("observation lock") =
            Some(request.envelope.metadata.participants.clone());
        response_for(request, Status::Success, b"ok".to_vec())
    });

    let server = serving_engine(&engine, CAPABILITY, handler);
    expose(&transport, &engine, &server);

    let participants = Participants::new(EngineId::new("caller-engine").unwrap(), engine.clone())
        .with_sender_instance(EngineInstanceId::new("caller-engine-7").unwrap())
        .with_target_instance(EngineInstanceId::new("instance-engine-2").unwrap());

    let request = request_with(
        "msg-instance",
        CAPABILITY,
        b"payload",
        participants.clone(),
        OperationContext::new(Operation::new(
            OperationId::new("op-instance").unwrap(),
            CorrelationId::new("corr-instance").unwrap(),
        )),
    );

    let client = UniversalClient::new(transport);
    let response = call(&client, &engine, request);

    let seen = observed
        .lock()
        .expect("observation lock")
        .take()
        .expect("the engine handler must have been invoked");
    assert_eq!(seen, participants);
    assert_eq!(response.status, Status::Success);
}

#[test]
fn opaque_binary_payload_is_not_interpreted_or_altered() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("binary-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    // Deliberately not valid UTF-8, and containing NUL and 0xFF bytes.
    let payload: Vec<u8> = vec![
        0x00, 0xFF, 0x80, 0xC3, 0x28, 0x0A, 0x0D, 0x7F, 0xFE, 0x01, 0x00, 0x00,
    ];
    assert!(String::from_utf8(payload.clone()).is_err());

    let client = UniversalClient::new(transport);
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-binary", CAPABILITY, &payload),
    );

    assert_eq!(response.envelope.payload.bytes(), payload.as_slice());
}

#[test]
fn large_logical_payload_round_trips_through_client_and_server() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("bulk-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    // A deterministic payload far larger than any header or small message.
    let payload: Vec<u8> = (0..256 * 1024u32)
        .map(|index| (index % 251) as u8)
        .collect();

    let client = UniversalClient::new(transport);
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-bulk", CAPABILITY, &payload),
    );

    assert_eq!(response.status, Status::Success);
    assert_eq!(response.envelope.payload.bytes().len(), payload.len());
    assert_eq!(response.envelope.payload.bytes(), payload.as_slice());
}

#[test]
fn client_routes_each_request_to_the_addressed_engine() {
    let transport = InMemoryTransport::new();
    let alpha = EngineId::new("alpha-engine").unwrap();
    let beta = EngineId::new("beta-engine").unwrap();

    let alpha_server = serving_engine(&alpha, CAPABILITY, identity_handler(&alpha));
    let beta_server = serving_engine(&beta, CAPABILITY, identity_handler(&beta));
    expose(&transport, &alpha, &alpha_server);
    expose(&transport, &beta, &beta_server);

    let client = UniversalClient::new(transport);

    let alpha_response = call(
        &client,
        &alpha,
        request_for(&alpha, "msg-alpha", CAPABILITY, b"ping"),
    );
    let beta_response = call(
        &client,
        &beta,
        request_for(&beta, "msg-beta", CAPABILITY, b"ping"),
    );

    assert_eq!(alpha_response.envelope.payload.bytes(), b"alpha-engine");
    assert_eq!(beta_response.envelope.payload.bytes(), b"beta-engine");
    assert_eq!(alpha_response.envelope.message_id.as_str(), "msg-alpha");
    assert_eq!(beta_response.envelope.message_id.as_str(), "msg-beta");
}

#[test]
fn request_to_unregistered_engine_reports_a_retryable_disconnect() {
    let transport = InMemoryTransport::new();
    let known = EngineId::new("known-engine").unwrap();
    let unknown = EngineId::new("unknown-engine").unwrap();

    let server = serving_engine(&known, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &known, &server);

    let client = UniversalClient::new(transport);
    let result = futures::executor::block_on(client.send(
        &unknown,
        request_for(&unknown, "msg-1", CAPABILITY, b"ping"),
    ));

    let error = result.expect_err("an unregistered engine must not answer");
    assert_eq!(error, TransportError::Disconnected);
    assert!(error.is_retryable());
    assert_eq!(error.to_string(), "transport is not connected");
}

#[test]
fn client_reports_connection_state_per_engine() {
    let transport = InMemoryTransport::new();
    let alpha = EngineId::new("alpha-engine").unwrap();
    let beta = EngineId::new("beta-engine").unwrap();
    let absent = EngineId::new("absent-engine").unwrap();

    let alpha_server = serving_engine(&alpha, CAPABILITY, echo_handler(Status::Success));
    let beta_server = serving_engine(&beta, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &alpha, &alpha_server);
    expose(&transport, &beta, &beta_server);

    let client = UniversalClient::new(transport);

    assert!(client.is_connected(&alpha));
    assert!(client.is_connected(&beta));
    assert!(!client.is_connected(&absent));

    let targets = client.connected_targets();
    assert_eq!(targets.len(), 2);
    assert!(targets.contains(&alpha));
    assert!(targets.contains(&beta));
    assert!(!targets.contains(&absent));
}

#[test]
fn client_exposes_its_underlying_transport() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("lookup-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);

    // The client must not hide the transport it was built on, so typed
    // capability clients can reuse it instead of opening their own stack.
    let registered = client.transport().registered_targets();
    assert_eq!(registered, vec![engine.clone()]);
    assert_eq!(registered, client.connected_targets());
}

#[test]
fn transport_is_usable_through_a_trait_object() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("dyn-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let abstract_transport: &dyn Transport = &transport;

    assert!(abstract_transport.is_connected(&engine));
    assert!(abstract_transport.connected_targets().contains(&engine));

    let response = futures::executor::block_on(abstract_transport.call(
        &engine,
        request_for(&engine, "msg-dyn", CAPABILITY, b"through dyn"),
    ))
    .expect("the abstract transport must deliver the request");

    assert_eq!(response.status, Status::Success);
    assert_eq!(response.envelope.payload.bytes(), b"through dyn");
}

#[test]
fn engine_server_rejects_requests_before_it_starts() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("unstarted-engine").unwrap();

    let server = EngineServer::new(engine.clone());
    server.register_handler(
        CapabilityId::new(CAPABILITY).unwrap(),
        echo_handler(Status::Success),
    );
    assert_eq!(server.state(), ServerState::Starting);
    assert_eq!(server.engine_id(), &engine);

    let server = Arc::new(Mutex::new(server));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-early", CAPABILITY, b"ping"),
    );

    assert_eq!(response.status, Status::Failure);
    // The rejection preserves the caller's message identity for correlation.
    assert_eq!(response.envelope.message_id.as_str(), "msg-early");
}

#[test]
fn engine_server_stops_accepting_requests_once_draining_and_stopped() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("lifecycle-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);

    let serving = call(
        &client,
        &engine,
        request_for(&engine, "msg-serving", CAPABILITY, b"ping"),
    );
    assert_eq!(serving.status, Status::Success);

    server.lock().expect("engine server lock").drain();
    assert!(
        server
            .lock()
            .expect("engine server lock")
            .state()
            .is_draining()
    );

    let draining = call(
        &client,
        &engine,
        request_for(&engine, "msg-draining", CAPABILITY, b"ping"),
    );
    assert_eq!(draining.status, Status::Failure);

    server.lock().expect("engine server lock").stop();
    assert_eq!(
        server.lock().expect("engine server lock").state(),
        ServerState::Stopped
    );

    let stopped = call(
        &client,
        &engine,
        request_for(&engine, "msg-stopped", CAPABILITY, b"ping"),
    );
    assert_eq!(stopped.status, Status::Failure);
}

#[test]
fn engine_server_reports_failure_for_an_unregistered_capability() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("partial-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);

    // The engine is reachable and serving, but this capability is not bound.
    let response = call(
        &client,
        &engine,
        request_for(&engine, "msg-unbound", "unbound-capability", b"ping"),
    );

    assert_eq!(response.status, Status::Failure);
    assert_eq!(
        response.envelope.metadata.descriptor.capability_id.as_str(),
        "unbound-capability"
    );
}

#[test]
fn unregistering_a_capability_handler_stops_serving_it() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("mutable-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);

    let before = call(
        &client,
        &engine,
        request_for(&engine, "msg-before", CAPABILITY, b"ping"),
    );
    assert_eq!(before.status, Status::Success);

    server
        .lock()
        .expect("engine server lock")
        .unregister_handler(&CapabilityId::new(CAPABILITY).unwrap());

    let after = call(
        &client,
        &engine,
        request_for(&engine, "msg-after", CAPABILITY, b"ping"),
    );
    assert_eq!(after.status, Status::Failure);
}

#[test]
fn engine_level_failure_is_not_reported_as_a_transport_error() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("failing-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Failure));
    expose(&transport, &engine, &server);

    let client = UniversalClient::new(transport);
    let result = futures::executor::block_on(client.send(
        &engine,
        request_for(&engine, "msg-fail", CAPABILITY, b"ping"),
    ));

    // A capability that answers with a failing status is still a delivered
    // message, so the transport must report success and carry the status.
    let response = result.expect("delivery must succeed even when the engine fails");
    assert_eq!(response.status, Status::Failure);
    assert_eq!(response.envelope.payload.bytes(), b"ping");
}

#[test]
fn concurrent_client_calls_stay_paired_with_their_engines() {
    let transport = InMemoryTransport::new();
    let alpha = EngineId::new("alpha-engine").unwrap();
    let beta = EngineId::new("beta-engine").unwrap();

    let alpha_server = serving_engine(&alpha, CAPABILITY, identity_handler(&alpha));
    let beta_server = serving_engine(&beta, CAPABILITY, identity_handler(&beta));
    expose(&transport, &alpha, &alpha_server);
    expose(&transport, &beta, &beta_server);

    let client = UniversalClient::new(transport);

    std::thread::scope(|scope| {
        let mut handles = Vec::new();

        for index in 0..12u32 {
            let engine = if index % 2 == 0 {
                alpha.clone()
            } else {
                beta.clone()
            };
            let client = &client;

            handles.push(scope.spawn(move || {
                let message_id = format!("msg-{index}");
                let request = request_for(&engine, &message_id, CAPABILITY, b"ping");
                let response = call(client, &engine, request);
                (engine, message_id, response)
            }));
        }

        for handle in handles {
            let (engine, message_id, response) = handle.join().expect("worker thread");
            assert_eq!(response.status, Status::Success);
            assert_eq!(response.envelope.message_id.as_str(), message_id);
            assert_eq!(
                response.envelope.payload.bytes(),
                engine.as_str().as_bytes(),
                "response for {message_id} came from the wrong engine"
            );
            assert_eq!(response.envelope.metadata.participants.sender, engine);
        }
    });
}

// ---------------------------------------------------------------------------
// Framed byte streams
// ---------------------------------------------------------------------------

#[test]
fn framed_stream_preserves_logical_message_boundaries_and_order() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();
    let stream = MessageStream::new(&writer, &reader);

    let messages: Vec<Vec<u8>> = vec![
        b"first".to_vec(),
        Vec::new(),
        b"a considerably longer third logical message".to_vec(),
        vec![0x00, 0xFF, 0x00],
    ];

    for message in &messages {
        stream.send(message).expect("framed send must succeed");
    }

    // Every logical message must come back whole and in send order, even
    // though they were all written into one shared byte buffer.
    for expected in &messages {
        let received = stream
            .recv()
            .expect("framed receive must succeed")
            .expect("a framed message must be available");
        assert_eq!(&received, expected);
    }
}

#[test]
fn framed_stream_round_trips_a_large_logical_message() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();
    let stream = MessageStream::new(&writer, &reader);

    // Larger than any single short read the source hands back, which proves
    // the stream reassembles a payload across multiple reads.
    let payload: Vec<u8> = (0..1024 * 1024u32)
        .map(|index| (index % 253) as u8)
        .collect();

    stream.send(&payload).expect("framed send must succeed");
    let received = stream
        .recv()
        .expect("framed receive must succeed")
        .expect("a framed message must be available");

    assert_eq!(received.len(), payload.len());
    assert_eq!(received, payload);
}

#[test]
fn framed_stream_rejects_a_logical_message_larger_than_the_frame_bound() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();
    let stream = MessageStream::new(&writer, &reader);

    let oversized = vec![0u8; MAX_FRAME_LENGTH + 1];
    let error = stream
        .send(&oversized)
        .expect_err("a frame above the bound must be refused");

    assert_eq!(
        error,
        StreamError::Encode("framed message exceeds the maximum length".into())
    );

    // Refusing the write must not leave a partial frame behind.
    let within_bound = b"still usable".to_vec();
    stream
        .send(&within_bound)
        .expect("framed send must succeed");
    assert_eq!(stream.recv().unwrap(), Some(within_bound));
}

#[test]
fn framed_stream_marks_the_source_unusable_after_an_oversized_frame() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();

    // A hostile or broken peer announces a frame larger than the bound.
    let announced = u32::try_from(MAX_FRAME_LENGTH + 1).unwrap();
    writer
        .write(&announced.to_be_bytes())
        .expect("raw write must succeed");

    let first = MessageStream::new(&writer, &reader);
    assert_eq!(
        first
            .recv()
            .expect_err("an oversized frame must be rejected"),
        StreamError::Decode("framed message exceeds the maximum length".into())
    );

    // The rejection is terminal for that source, so a freshly built stream
    // over the same source must not try to parse the rejected bytes.
    let second = MessageStream::new(&writer, &reader);
    assert_eq!(
        second.recv().expect_err("the source must stay unusable"),
        StreamError::Decode("stream is unusable after an oversized frame".into())
    );
}

#[test]
fn framed_stream_reports_end_of_stream_when_the_peer_closes() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();
    let stream = MessageStream::new(&writer, &reader);

    stream.send(b"last message").expect("framed send");
    writer.close().expect("closing the sink");

    // Buffered messages drain first, then the stream reports end of stream
    // rather than an error.
    assert_eq!(stream.recv().unwrap(), Some(b"last message".to_vec()));
    assert_eq!(stream.recv().unwrap(), None);
    assert_eq!(stream.recv().unwrap(), None);
}

#[test]
fn connection_exposes_framed_duplex_streams_to_consumers() {
    let channel = ByteChannel::new();
    let peer = EngineId::new("peer-engine").unwrap();
    let connection = LoopbackConnection::open(peer.clone(), &channel);

    assert_eq!(connection.peer(), &peer);
    assert_eq!(connection.state(), ConnectionState::Open);
    assert!(connection.state().is_open());

    // A transport implementor frames messages over the connection's byte pair.
    let stream = MessageStream::new(connection.sink(), connection.source());
    stream.send(b"connection payload").expect("framed send");

    assert_eq!(stream.recv().unwrap(), Some(b"connection payload".to_vec()));
}

#[test]
fn closing_a_connection_stops_further_writes() {
    let channel = ByteChannel::new();
    let mut connection = LoopbackConnection::open(EngineId::new("peer-engine").unwrap(), &channel);

    {
        let stream = MessageStream::new(connection.sink(), connection.source());
        stream.send(b"before close").expect("framed send");
        assert_eq!(stream.recv().unwrap(), Some(b"before close".to_vec()));
    }

    connection.close();

    assert_eq!(connection.state(), ConnectionState::Closed);
    assert!(!connection.state().is_open());

    let stream = MessageStream::new(connection.sink(), connection.source());
    assert_eq!(
        stream
            .send(b"after close")
            .expect_err("a closed connection must refuse writes"),
        StreamError::Closed
    );
    assert_eq!(stream.recv().unwrap(), None);
}

// ---------------------------------------------------------------------------
// Binary framing metadata
// ---------------------------------------------------------------------------

#[test]
fn message_header_round_trips_through_its_binary_layout() {
    let message_id = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33];
    let original = MessageHeader::new(2, FINAL_FRAGMENT_FLAG, 65_536, message_id, 7);

    let bytes = original.serialize();
    assert_eq!(bytes.len(), HEADER_LENGTH);

    // Big-endian, transport focused fields at their documented offsets.
    assert_eq!(bytes[0], 2);
    assert_eq!(bytes[FLAGS_OFFSET], FINAL_FRAGMENT_FLAG);
    assert_eq!(&bytes[4..8], 65_536u32.to_be_bytes());
    assert_eq!(&bytes[8..16], message_id);
    assert_eq!(&bytes[16..20], 7u32.to_be_bytes());

    let parsed = MessageHeader::deserialize(&bytes).expect("a 20 byte header must parse");
    assert_eq!(parsed.serialize(), bytes);
    assert_eq!(parsed.to_string(), original.to_string());
}

#[test]
fn message_header_deserialization_rejects_wrong_sized_input() {
    assert!(MessageHeader::deserialize(&[]).is_err());
    assert!(MessageHeader::deserialize(&[0u8; HEADER_LENGTH - 1]).is_err());
    assert!(MessageHeader::deserialize(&[0u8; HEADER_LENGTH + 1]).is_err());
    assert!(MessageHeader::deserialize(&[0u8; HEADER_LENGTH]).is_ok());
}

#[test]
fn message_header_encodes_fragment_metadata_independently() {
    let message_id = [1u8; 8];
    let first = MessageHeader::new(FRAMING_VERSION, 0, 512, message_id, 0).serialize();
    let second = MessageHeader::new(FRAMING_VERSION, 0, 512, message_id, 1).serialize();
    let last =
        MessageHeader::new(FRAMING_VERSION, FINAL_FRAGMENT_FLAG, 512, message_id, 1).serialize();

    // Fragment index is the only difference between two ordinary fragments.
    assert_ne!(first, second);
    assert_eq!(first[..16], second[..16]);

    // The final fragment indicator lives in the flags byte alone, so it does
    // not disturb the payload length, message id, or fragment index.
    assert_ne!(second, last);
    assert_eq!(second[2..], last[2..]);
    assert_eq!(
        last[FLAGS_OFFSET] & FINAL_FRAGMENT_FLAG,
        FINAL_FRAGMENT_FLAG
    );
    assert_eq!(second[FLAGS_OFFSET] & FINAL_FRAGMENT_FLAG, 0);
}

#[test]
fn message_header_renders_its_transport_metadata() {
    let header = MessageHeader::new(FRAMING_VERSION, FINAL_FRAGMENT_FLAG, 1024, [9u8; 8], 3);
    let rendered = header.to_string();

    assert!(rendered.contains("version=1"), "{rendered}");
    assert!(rendered.contains("flags=1"), "{rendered}");
    assert!(rendered.contains("payload_length=1024"), "{rendered}");
    assert!(rendered.contains("fragment_index=3"), "{rendered}");
}

#[test]
fn fragmented_logical_message_travels_as_separate_transport_frames() {
    let channel = ByteChannel::new();
    let (writer, reader) = channel.split();
    let stream = MessageStream::new(&writer, &reader);

    let payload: Vec<u8> = (0..5_000u32).map(|index| (index % 251) as u8).collect();
    let fragment_length = 512usize;
    let message_id = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8];

    let fragments: Vec<&[u8]> = payload.chunks(fragment_length).collect();
    let expected_fragments = payload.len().div_ceil(fragment_length);
    assert_eq!(fragments.len(), expected_fragments);

    // Send side: one logical message becomes several bounded transport frames,
    // each carrying only transport metadata ahead of its payload slice.
    for (index, fragment) in fragments.iter().enumerate() {
        let is_final = index + 1 == fragments.len();
        let flags = if is_final { FINAL_FRAGMENT_FLAG } else { 0 };
        let header = MessageHeader::new(
            FRAMING_VERSION,
            flags,
            u32::try_from(fragment.len()).unwrap(),
            message_id,
            u32::try_from(index).unwrap(),
        );

        let mut frame = header.serialize();
        assert_eq!(frame.len(), HEADER_LENGTH);
        frame.extend_from_slice(fragment);
        stream.send(&frame).expect("framed send must succeed");
    }

    // Receive side: frames arrive whole, in contiguous fragment order, and the
    // reassembled logical payload contains no header bytes.
    let mut reassembled: Vec<u8> = Vec::with_capacity(payload.len());
    let mut received = 0u32;

    loop {
        let frame = stream
            .recv()
            .expect("framed receive must succeed")
            .expect("a transport frame must be available");

        assert!(frame.len() >= HEADER_LENGTH);
        let (header_bytes, body) = frame.split_at(HEADER_LENGTH);

        assert!(
            MessageHeader::deserialize(header_bytes).is_ok(),
            "each frame must start with a parsable transport header"
        );

        // The header has no public accessors, so a consumer reads the flags at
        // their documented offset. Rebuilding the header from the fields this
        // fragment should carry proves the encoding matched byte for byte.
        let flags = header_bytes[FLAGS_OFFSET];
        let expected_header = MessageHeader::new(
            FRAMING_VERSION,
            flags,
            u32::try_from(body.len()).unwrap(),
            message_id,
            received,
        )
        .serialize();
        assert_eq!(header_bytes, expected_header.as_slice());

        assert!(body.len() <= fragment_length);
        reassembled.extend_from_slice(body);
        received += 1;

        if flags & FINAL_FRAGMENT_FLAG != 0 {
            break;
        }
    }

    assert_eq!(received, u32::try_from(expected_fragments).unwrap());
    assert!(received > 1, "the payload must have needed fragmenting");
    assert_eq!(reassembled.len(), payload.len());
    assert_eq!(reassembled, payload);
}

// ---------------------------------------------------------------------------
// Client connection abstraction
// ---------------------------------------------------------------------------

#[test]
fn connection_factory_opens_client_connections_to_registered_engines() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("connected-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let factory = InMemoryConnectionFactory {
        transport: transport.clone(),
    };

    let connection = futures::executor::block_on(factory.connect(&engine))
        .expect("connecting to a registered engine must succeed");

    assert_eq!(connection.peer(), &engine);
    assert_eq!(connection.state(), ClientConnectionState::Open);
    assert!(connection.state().is_open());

    let response = futures::executor::block_on(connection.call(request_for(
        &engine,
        "msg-conn",
        CAPABILITY,
        b"through connection",
    )))
    .expect("an open connection must deliver the request");

    assert_eq!(response.status, Status::Success);
    assert_eq!(response.envelope.payload.bytes(), b"through connection");
}

#[test]
fn connection_factory_refuses_unregistered_engines() {
    let transport = InMemoryTransport::new();
    let factory = InMemoryConnectionFactory { transport };

    let result =
        futures::executor::block_on(factory.connect(&EngineId::new("absent-engine").unwrap()));

    match result {
        Err(error) => {
            assert_eq!(error, TransportError::Disconnected);
        }
        Ok(_) => {
            panic!("connecting to an unregistered engine must fail");
        }
    }
}

#[test]
fn closed_client_connection_refuses_further_calls() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("closing-engine").unwrap();
    let server = serving_engine(&engine, CAPABILITY, echo_handler(Status::Success));
    expose(&transport, &engine, &server);

    let factory = InMemoryConnectionFactory { transport };
    let mut connection = futures::executor::block_on(factory.connect(&engine))
        .expect("connecting to a registered engine must succeed");

    connection.close();

    assert_eq!(connection.state(), ClientConnectionState::Closed);
    assert!(!connection.state().is_open());

    let error = futures::executor::block_on(connection.call(request_for(
        &engine,
        "msg-closed",
        CAPABILITY,
        b"ping",
    )))
    .expect_err("a closed connection must refuse calls");

    assert_eq!(error, TransportError::Closed);
    assert_eq!(error.to_string(), "connection closed by peer");
}
