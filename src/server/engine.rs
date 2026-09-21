//! Phase 7 boundary for the engine server.
//!
//! The engine server handles incoming requests from clients and
//! dispatches them to capability handlers. It manages connection
//! state and request routing.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::events::EventName;
use crate::identity::{CapabilityId, EngineId, EngineInstanceId};
use crate::logging::{
    LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSource, LoggingSystem,
};
use crate::middleware::chain::MiddlewareChainError;
use crate::runtime::{
    EngineContext, ExecutionPipeline,
    pipeline::{PipelineConfigurationError, PipelineError, RequestPipelineError},
};
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
///
/// The server has both a logical [`EngineId`] and a concrete
/// [`EngineInstanceId`]. The logical ID identifies the engine type or
/// identity, while the instance ID identifies this particular running
/// server instance.
///
/// Keeping both identities here allows the server boundary to enforce
/// the same concrete-instance routing invariant used by the transport
/// layer.
pub struct EngineServer {
    engine_id: EngineId,
    instance_id: EngineInstanceId,
    state: ServerState,
    handlers: Arc<Mutex<HashMap<CapabilityId, RequestHandler>>>,
    pipeline: ExecutionPipeline,
    logging: Arc<LoggingSystem>,
}

impl EngineServer {
    /// Creates a new engine server for the specified engine instance.
    pub fn new(engine_id: EngineId, instance_id: EngineInstanceId) -> Self {
        Self {
            engine_id,
            instance_id,
            state: ServerState::Starting,
            handlers: Arc::new(Mutex::new(HashMap::new())),
            pipeline: ExecutionPipeline::new(),
            logging: Arc::new(LoggingSystem::new(64).expect("valid logging capacity")),
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
        if self.state == ServerState::Starting {
            self.state = ServerState::Serving;
        }
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

    /// Replaces the request execution pipeline used by the server.
    ///
    /// Every admitted request is routed through this pipeline before a
    /// capability handler can execute. The pipeline itself is responsible for
    /// enforcing the mandatory middleware boundary.
    pub fn with_pipeline(mut self, pipeline: ExecutionPipeline) -> Self {
        self.pipeline = pipeline;
        self
    }

    /// Returns a reference to the request execution pipeline.
    pub fn pipeline(&self) -> &ExecutionPipeline {
        &self.pipeline
    }

    /// Returns the Core logging system used by this server.
    pub fn logging(&self) -> &LoggingSystem {
        &self.logging
    }

    /// Returns the logical engine ID of this server.
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine instance ID of this server.
    pub fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }
}

/// Handles an incoming request by dispatching it to the appropriate handler.
///
/// Returns an error response if the server is not serving or no handler is found.
///
/// Before dispatching, the server validates both the logical engine target and,
/// when present, the concrete target instance. A request addressed to another
/// concrete instance is rejected before middleware or capability execution.
pub fn handle_request(
    server: &EngineServer,
    mut request: UniversalRequest,
) -> Result<UniversalResponse, TransportError> {
    if !request.has_request_interaction() {
        return Ok(failure_response(request));
    }

    if request.event.envelope.metadata.participants.target != server.engine_id {
        return Err(TransportError::Peer(
            "request target does not match the server identity".into(),
        ));
    }

    match request
        .event
        .envelope
        .metadata
        .participants
        .target_instance
        .as_ref()
    {
        Some(target_instance) if *target_instance != server.instance_id => {
            return Err(TransportError::Peer(
                "request target instance does not match the server instance".into(),
            ));
        }
        None => {}
        Some(_) => {}
    }

    if !server.state().is_serving() {
        return Ok(failure_response(request));
    }

    let admitted_capability = request
        .event
        .envelope
        .metadata
        .descriptor
        .capability_id
        .clone();

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone());

    match server
        .pipeline
        .run_request(&mut context, &mut request, |context, request| {
            if context.security().is_none()
                || request.event.envelope.metadata.descriptor.capability_id != admitted_capability
            {
                return Ok::<UniversalResponse, ()>(failure_response(request.clone()));
            }

            let capability_id = admitted_capability.clone();
            let handler = {
                let handlers_guard = server.handlers.lock().unwrap();
                handlers_guard.get(&capability_id).cloned()
            };

            match handler {
                Some(handler) => Ok::<UniversalResponse, ()>(handler(request.clone())),
                None => Ok::<UniversalResponse, ()>(failure_response(request.clone())),
            }
        }) {
        Ok(response) => Ok(response),
        Err(error) => {
            log_pipeline_error(server, &request, &error);
            Ok(failure_response(request))
        }
    }
}

fn log_pipeline_error(
    server: &EngineServer,
    request: &UniversalRequest,
    error: &RequestPipelineError<()>,
) {
    let error_key: &str = match error {
        RequestPipelineError::Context(PipelineError::Cancelled) => {
            "request_pipeline.context.cancelled"
        }
        RequestPipelineError::Context(PipelineError::DeadlineExpired) => {
            "request_pipeline.context.deadline_expired"
        }
        RequestPipelineError::Configuration(
            PipelineConfigurationError::MandatoryMiddlewareNotConfigured,
        ) => "request_pipeline.configuration.mandatory_middleware_not_configured",
        RequestPipelineError::Validation(_) => "request_pipeline.validation.failed",
        RequestPipelineError::Middleware(MiddlewareChainError::Context(
            PipelineError::Cancelled,
        )) => "request_pipeline.middleware.context.cancelled",
        RequestPipelineError::Middleware(MiddlewareChainError::Context(
            PipelineError::DeadlineExpired,
        )) => "request_pipeline.middleware.context.deadline_expired",
        RequestPipelineError::Middleware(MiddlewareChainError::Rejected(_)) => {
            "request_pipeline.middleware.rejected"
        }
        RequestPipelineError::Middleware(MiddlewareChainError::Middleware(_)) => {
            "request_pipeline.middleware.failed"
        }
        RequestPipelineError::Middleware(MiddlewareChainError::Downstream(_)) => {
            "request_pipeline.downstream.failed"
        }
    };

    let context = LogContext::new(request.event.envelope.operation_context.clone())
        .for_message(request.event.envelope.message_id.clone());

    let event = match LogEvent::new(
        request.event_id().clone(),
        EventName::new("engine.server.pipeline.error").expect("static log event name is valid"),
        LogLevel::Error,
        LogSource::Core,
        LogScope::Global,
        "engine-server",
        context,
        error_key,
        LogEventType::Diagnostic,
    ) {
        Ok(event) => event,
        Err(_) => return,
    };

    let instance = server.logging().instance(LogScope::Global, LogSource::Core);
    let _ = instance.publish(event);
}

fn failure_response(request: UniversalRequest) -> UniversalResponse {
    // `ValidationError::EmptyPayload` is a structural contract invariant, so
    // failure responses must carry a non-empty payload as well. The failure
    // outcome itself remains in `Status`; this byte is only an opaque,
    // non-empty representation and does not define application payload
    // semantics.
    const FAILURE_PAYLOAD: &[u8] = &[0];

    let envelope = request.event.envelope;
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
            crate::contracts::EncodedPayload::new(payload_descriptor, FAILURE_PAYLOAD),
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
        CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
    };
    use crate::middleware::stages::{Middleware, MiddlewareResult};
    use crate::operation::{Operation, OperationContext};
    use crate::runtime::EngineContext;
    use crate::security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext};
    use crate::status::Status;

    #[derive(Debug)]
    struct AdmissionMiddleware;

    impl Middleware for AdmissionMiddleware {
        fn on_request(
            &self,
            context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            let principal = PrincipalIdentity::new(
                PrincipalType::Service,
                PrincipalId::new("test-admission-service").unwrap(),
            );

            *context = context
                .clone()
                .with_security(SecurityContext::new(principal, None));

            MiddlewareResult::Continue
        }
    }

    fn pipeline() -> ExecutionPipeline {
        ExecutionPipeline::new().with_middleware(AdmissionMiddleware)
    }

    struct TestLogSink(std::sync::mpsc::Sender<crate::logging::LogEvent>);

    impl crate::logging::LogSink for TestLogSink {
        fn publish(&self, event: &crate::logging::LogEvent) {
            self.0.send(event.clone()).unwrap();
        }
    }

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

    fn server_ids() -> (EngineId, EngineInstanceId) {
        (
            EngineId::new("server").unwrap(),
            EngineInstanceId::new("server-instance-1").unwrap(),
        )
    }

    #[test]
    fn server_starts_in_starting_state() {
        let (engine_id, instance_id) = server_ids();

        let server = EngineServer::new(engine_id, instance_id).with_pipeline(pipeline());

        assert_eq!(server.state(), ServerState::Starting);
    }

    #[test]
    fn server_exposes_logical_and_concrete_identity() {
        let (engine_id, instance_id) = server_ids();

        let server = EngineServer::new(engine_id.clone(), instance_id.clone());

        assert_eq!(server.engine_id(), &engine_id);
        assert_eq!(server.instance_id(), &instance_id);
    }

    #[test]
    fn server_transitions_starting_to_serving() {
        let (engine_id, instance_id) = server_ids();

        let mut server = EngineServer::new(engine_id, instance_id).with_pipeline(pipeline());

        server.start();

        assert_eq!(server.state(), ServerState::Serving);
    }

    #[test]
    fn server_transitions_serving_to_draining() {
        let (engine_id, instance_id) = server_ids();

        let mut server = EngineServer::new(engine_id, instance_id).with_pipeline(pipeline());

        server.start();
        server.drain();

        assert_eq!(server.state(), ServerState::Draining);
    }

    #[test]
    fn server_transitions_to_stopped() {
        let (engine_id, instance_id) = server_ids();

        let mut server = EngineServer::new(engine_id, instance_id).with_pipeline(pipeline());

        server.start();
        server.stop();

        assert_eq!(server.state(), ServerState::Stopped);
    }

    #[test]
    fn server_can_register_and_handle_requests() {
        let (server_id, server_instance) = server_ids();

        let mut server =
            EngineServer::new(server_id.clone(), server_instance.clone()).with_pipeline(pipeline());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|request| UniversalResponse::new(request.event.envelope, Status::Success)),
        );

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-1");
        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Success);
    }

    #[test]
    fn server_rejects_request_when_middleware_does_not_establish_security_context() {
        #[derive(Debug)]
        struct PassThroughMiddleware;

        impl Middleware for PassThroughMiddleware {
            fn on_request(
                &self,
                _context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                MiddlewareResult::Continue
            }
        }

        let (server_id, server_instance) = server_ids();

        let mut server = EngineServer::new(server_id.clone(), server_instance.clone())
            .with_pipeline(ExecutionPipeline::new().with_middleware(PassThroughMiddleware));

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_| panic!("unauthenticated request reached handler")),
        );

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-no-security");
        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn server_rejects_capability_mutation_after_middleware() {
        #[derive(Debug)]
        struct MutatingMiddleware;

        impl Middleware for MutatingMiddleware {
            fn on_request(
                &self,
                context: &mut EngineContext,
                request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                let principal = PrincipalIdentity::new(
                    PrincipalType::Service,
                    PrincipalId::new("test-mutating-service").unwrap(),
                );

                *context = context
                    .clone()
                    .with_security(SecurityContext::new(principal, None));

                request.event.envelope.metadata.descriptor.capability_id =
                    CapabilityId::new("mutated-capability").unwrap();

                MiddlewareResult::Continue
            }
        }

        let (server_id, server_instance) = server_ids();

        let mut server = EngineServer::new(server_id.clone(), server_instance.clone())
            .with_pipeline(ExecutionPipeline::new().with_middleware(MutatingMiddleware));

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_| panic!("capability mutation bypassed authorization")),
        );

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-mutated-capability");

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn server_records_pipeline_failures_without_exposing_credentials() {
        let (server_id, server_instance) = server_ids();

        let mut server = EngineServer::new(server_id.clone(), server_instance.clone());

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-logging-failure");

        let (sender, receiver) = std::sync::mpsc::channel();

        server
            .logging()
            .subscribe(std::sync::Arc::new(TestLogSink(sender)));

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);

        let event = receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("pipeline failure must produce a log event");

        let rendered = format!("{event:?}");

        assert!(
            rendered.contains("request_pipeline.configuration.mandatory_middleware_not_configured")
        );

        assert_eq!(event.event_type(), "diagnostic");
    }

    #[test]
    fn server_requires_mandatory_middleware_before_handler_dispatch() {
        let (server_id, server_instance) = server_ids();

        let mut server = EngineServer::new(server_id.clone(), server_instance.clone());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_| panic!("request bypassed mandatory middleware")),
        );

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-no-middleware");

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn server_rejects_request_when_not_serving() {
        let (server_id, server_instance) = server_ids();

        let server =
            EngineServer::new(server_id.clone(), server_instance.clone()).with_pipeline(pipeline());

        let request = make_request(&server_id, Some(&server_instance), "msg-1");

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
    }

    #[test]
    fn server_rejects_non_request_interaction_before_handler_dispatch() {
        let (server_id, server_instance) = server_ids();

        let mut server =
            EngineServer::new(server_id.clone(), server_instance.clone()).with_pipeline(pipeline());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_request| panic!("non-request interaction reached handler")),
        );

        server.start();

        let mut request = make_request(&server_id, Some(&server_instance), "msg-invalid");

        request.event.envelope.metadata.descriptor.interaction = Interaction::Response;

        let response = handle_request(&server, request).unwrap();

        assert_eq!(response.status, Status::Failure);
        assert_eq!(
            response.event.envelope.metadata.descriptor.interaction,
            Interaction::Response
        );
    }

    #[test]
    fn server_rejects_request_with_mismatched_target() {
        let (server_id, server_instance) = server_ids();

        let mut server =
            EngineServer::new(server_id, server_instance.clone()).with_pipeline(pipeline());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_request| panic!("mismatched target reached handler")),
        );

        server.start();

        let target = EngineId::new("other-server").unwrap();

        let request = make_request(&target, Some(&server_instance), "msg-mismatched-target");

        let result = handle_request(&server, request);

        assert!(matches!(
            result,
            Err(TransportError::Peer(message))
                if message == "request target does not match the server identity"
        ));
    }

    #[test]
    fn server_rejects_request_with_mismatched_target_instance() {
        let (server_id, server_instance) = server_ids();

        let mut server =
            EngineServer::new(server_id.clone(), server_instance).with_pipeline(pipeline());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|_request| panic!("mismatched target instance reached handler")),
        );

        server.start();

        let wrong_instance = EngineInstanceId::new("server-instance-2").unwrap();

        let request = make_request(&server_id, Some(&wrong_instance), "msg-mismatched-instance");

        let result = handle_request(&server, request);

        assert!(matches!(
            result,
            Err(TransportError::Peer(message))
                if message == "request target instance does not match the server instance"
        ));
    }

    #[test]
    fn server_returns_failure_when_handler_missing() {
        let (server_id, server_instance) = server_ids();

        let mut server =
            EngineServer::new(server_id.clone(), server_instance.clone()).with_pipeline(pipeline());

        server.start();

        let request = make_request(&server_id, Some(&server_instance), "msg-1");

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
        let server_instance = EngineInstanceId::new("server-instance-1").unwrap();

        let mut server =
            EngineServer::new(server_id.clone(), server_instance.clone()).with_pipeline(pipeline());

        server.register_handler(
            CapabilityId::new("test-cap").unwrap(),
            Arc::new(|request| UniversalResponse::new(request.event.envelope, Status::Success)),
        );

        server.start();

        let registered_server = server;

        transport.register(server_id.clone(), server_instance.clone(), move |request| {
            handle_request(&registered_server, request).unwrap()
        });

        let client: UniversalClient<InMemoryTransport> = UniversalClient::new(transport);

        let request: UniversalRequest = make_request(&server_id, Some(&server_instance), "msg-1");

        let response = futures::executor::block_on(client.send(&server_instance, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.event.envelope.message_id.as_str(), "msg-1");
    }
}
