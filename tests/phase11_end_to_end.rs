//! Phase 11 end-to-end integration tests.
//!
//! This file intentionally crosses subsystem boundaries. Unit tests remain in
//! each implementation file, and each subsystem's `mod.rs` covers composition
//! inside that subsystem. These tests cover the completed Phase 11 path across
//! configuration, runtime context, middleware, observability, health,
//! capability dispatch, logging, security, and provenance.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityInvocation, CapabilityOutcome, CapabilityRegistry, arc_handler,
    dispatch,
};
use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
};
use nizaam_core::health::{
    CapabilityHealthReport, DependencyId, DependencyReport, DependencyRequirement, HealthReport,
    HealthStatus, LivenessReport, ReadinessReport,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::logging::{
    LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSink, LogSource, LoggingSystem,
};
use nizaam_core::middleware::chain::{MiddlewareChain, MiddlewareChainError};
use nizaam_core::middleware::stages::{Middleware, MiddlewareError, MiddlewareResult};
use nizaam_core::observability::{
    CorrelationContext, Diagnostic, DiagnosticCondition, DiagnosticKind, MetricDescriptor,
    MetricDimensions, MetricKind, MetricName, MetricRecorder, ObservabilityLogger, Span, SpanEvent,
    SpanId, TraceId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::provenance::ProvenanceContext;
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState};
use nizaam_core::security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext};

const TEST_ENGINE: &str = "phase11.test.engine";
const TEST_CAPABILITY: &str = "phase11.test.capability";
const TEST_CONTRACT: &str = "phase11.test.contract";

fn operation_context(id: &str, correlation: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(id).unwrap(),
        CorrelationId::new(correlation).unwrap(),
    ))
}

fn resolved_configuration(key: &str, value: ConfigurationValue) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::empty();
    parsed.insert(key, value);

    let validated = ConfigurationValidator::new()
        .validate(parsed)
        .expect("test configuration must validate");

    let resolved = ConfigurationResolver::new()
        .resolve(&validated)
        .expect("test configuration must resolve");

    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(1),
        resolved,
    ))
}

fn request(operation_context: OperationContext) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new(TEST_CONTRACT).unwrap(),
        CapabilityId::new(TEST_CAPABILITY).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("phase11.caller").unwrap(),
            EngineId::new(TEST_ENGINE).unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("phase11.request").unwrap(),
        operation_context,
        metadata,
        EncodedPayload::new(payload_descriptor, b"phase11 payload"),
    ))
}

fn response() -> UniversalResponse {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new(TEST_CONTRACT).unwrap(),
        CapabilityId::new(TEST_CAPABILITY).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Response,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("phase11.caller").unwrap(),
            EngineId::new(TEST_ENGINE).unwrap(),
        ),
    );

    UniversalResponse::new(
        MessageEnvelope::new(
            MessageId::new("phase11.response").unwrap(),
            operation_context("phase11.response.operation", "phase11.response.correlation"),
            metadata,
            EncodedPayload::new(payload_descriptor, b"phase11 response"),
        ),
        nizaam_core::status::Status::Success,
    )
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new();

    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime
            .transition(state)
            .expect("runtime should follow the established lifecycle");
    }

    runtime
}

struct RecordingMiddleware {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl Middleware for RecordingMiddleware {
    fn on_request(
        &self,
        context: &mut EngineContext,
        _request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        assert!(context.configuration().is_some());
        self.calls.lock().unwrap().push("request");
        MiddlewareResult::Continue
    }

    fn on_response(
        &self,
        context: &EngineContext,
        _request: &UniversalRequest,
        _response: &mut UniversalResponse,
    ) -> Result<(), MiddlewareError> {
        assert!(context.configuration().is_some());
        self.calls.lock().unwrap().push("response");
        Ok(())
    }
}

struct ChannelSink(mpsc::Sender<LogEvent>);

impl LogSink for ChannelSink {
    fn publish(&self, event: &LogEvent) {
        self.0.send(event.clone()).unwrap();
    }
}

fn global_log_event(context: OperationContext) -> LogEvent {
    LogEvent::new(
        MessageId::new("phase11.log").unwrap(),
        LogLevel::Info,
        LogSource::Core,
        LogScope::Global,
        "phase11-end-to-end",
        LogContext::new(context),
        "phase 11 request observed",
        LogEventType::Diagnostic,
    )
    .unwrap()
}

#[test]
fn phase11_request_uses_a_stable_configuration_snapshot_through_execution() {
    let configuration =
        resolved_configuration("phase11.mode", ConfigurationValue::String("stable".into()));

    let operation = operation_context("phase11.request.operation", "phase11.request.correlation");
    let mut context = EngineContext::new(operation.clone()).with_configuration(configuration);

    let chain = MiddlewareChain::with_stage(RecordingMiddleware {
        calls: Arc::new(Mutex::new(Vec::new())),
    });

    let mut request = request(operation);

    let observed = chain
        .execute(
            &mut context,
            &mut request,
            |downstream_context, _request| {
                let value = downstream_context
                    .configuration()
                    .and_then(|snapshot| snapshot.get("phase11.mode"))
                    .and_then(ConfigurationValue::as_string);

                assert_eq!(value, Some("stable"));
                Ok::<_, ()>(response())
            },
        )
        .expect("phase 11 request should execute");

    assert_eq!(observed.status, nizaam_core::status::Status::Success);
}

#[test]
fn separate_execution_contexts_retain_their_own_configuration_snapshots() {
    let first = resolved_configuration("phase11.mode", ConfigurationValue::String("v1".into()));
    let second = resolved_configuration("phase11.mode", ConfigurationValue::String("v2".into()));

    let first_context = EngineContext::new(operation_context("phase11.op.a", "phase11.corr.a"))
        .with_configuration(first.clone());
    let second_context = EngineContext::new(operation_context("phase11.op.b", "phase11.corr.b"))
        .with_configuration(second.clone());

    assert_eq!(
        first_context
            .configuration()
            .unwrap()
            .get("phase11.mode")
            .and_then(ConfigurationValue::as_string),
        Some("v1")
    );
    assert_eq!(
        second_context
            .configuration()
            .unwrap()
            .get("phase11.mode")
            .and_then(ConfigurationValue::as_string),
        Some("v2")
    );
    assert_eq!(first_context.configuration().unwrap().id().value(), 1);
    assert_eq!(second_context.configuration().unwrap().id().value(), 1);
}

#[test]
fn middleware_observability_uses_the_same_operation_context_as_downstream_execution() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let middleware = RecordingMiddleware {
        calls: Arc::clone(&calls),
    };

    let configuration = resolved_configuration(
        "phase11.mode",
        ConfigurationValue::String("observed".into()),
    );

    let operation = operation_context(
        "phase11.middleware.operation",
        "phase11.middleware.correlation",
    );
    let expected_operation_id = operation.operation.id.clone();
    let mut context = EngineContext::new(operation.clone()).with_configuration(configuration);
    let mut request = request(operation);

    let chain = MiddlewareChain::with_stage(middleware);

    chain
        .execute(
            &mut context,
            &mut request,
            |downstream_context, _request| {
                assert_eq!(
                    downstream_context.operation().operation.id,
                    expected_operation_id
                );
                Ok::<_, ()>(response())
            },
        )
        .expect("middleware and downstream execution should succeed");

    assert_eq!(*calls.lock().unwrap(), vec!["request", "response"]);
}

#[test]
fn authenticated_request_preserves_security_configuration_and_provenance_together() {
    let configuration =
        resolved_configuration("phase11.mode", ConfigurationValue::String("secure".into()));

    let security = SecurityContext::new(
        PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("phase11.user").unwrap(),
        ),
        None,
    );

    let provenance = ProvenanceContext::new().with_attribute("phase", "11");

    let context = EngineContext::new(operation_context(
        "phase11.secure.operation",
        "phase11.secure.correlation",
    ))
    .with_configuration(configuration)
    .with_security(security.clone())
    .with_provenance(provenance.clone());

    let child = context.child();

    assert_eq!(child.configuration(), context.configuration());
    assert_eq!(child.security(), Some(&security));
    assert_eq!(child.provenance(), &provenance);
}

#[test]
fn capability_dispatch_receives_the_request_configuration_snapshot() {
    let configuration = resolved_configuration(
        "phase11.capability.mode",
        ConfigurationValue::String("enabled".into()),
    );

    let registry = CapabilityRegistry::new();
    let capability_id = CapabilityId::new(TEST_CAPABILITY).unwrap();

    let observed_mode = Arc::new(Mutex::new(None::<String>));
    let observed_mode_by_handler = Arc::clone(&observed_mode);

    registry
        .register(
            CapabilityDefinition::new(
                capability_id.clone(),
                EngineId::new(TEST_ENGINE).unwrap(),
                "Phase 11 Test Capability",
            )
            .unwrap(),
            arc_handler(
                move |context: &EngineContext, _invocation: &CapabilityInvocation| {
                    let mode = context
                        .configuration()
                        .and_then(|snapshot| snapshot.get("phase11.capability.mode"))
                        .and_then(ConfigurationValue::as_string)
                        .map(str::to_owned);

                    *observed_mode_by_handler.lock().unwrap() = mode;

                    Ok(CapabilityOutcome::new(b"ok".to_vec()))
                },
            ),
        )
        .unwrap();

    let context = EngineContext::new(operation_context(
        "phase11.capability.operation",
        "phase11.capability.correlation",
    ))
    .with_configuration(configuration);

    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new(TEST_CONTRACT).unwrap(),
        b"payload".to_vec(),
    );

    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_ok());
    assert_eq!(*observed_mode.lock().unwrap(), Some("enabled".to_owned()));
}

#[test]
fn request_emits_trace_metrics_diagnostics_and_existing_logging_for_one_operation() {
    let operation = operation_context("phase11.observe.operation", "phase11.observe.correlation");

    let correlation = CorrelationContext::from_operation_context(&operation);

    let trace_id = TraceId::new("phase11.trace").unwrap();
    let span_id = SpanId::new("phase11.span").unwrap();
    let mut span = Span::root(trace_id.clone(), span_id, "phase11.request").unwrap();

    span.set_attribute("operation.id", operation.operation.id.as_str())
        .unwrap();
    span.add_event(
        SpanEvent::new("phase11.request.started", std::time::SystemTime::now()).unwrap(),
    )
    .unwrap();

    let completed = span.finish();

    let recorder = MetricRecorder::new();
    let descriptor = MetricDescriptor::new(
        MetricName::new("phase11.requests.total").unwrap(),
        MetricKind::Counter,
    );

    recorder
        .increment_counter(&descriptor, MetricDimensions::new(), 1)
        .unwrap();

    let diagnostic = Diagnostic::new(
        DiagnosticKind::Operational,
        DiagnosticCondition::Operational,
        "phase 11 request observed",
    )
    .unwrap()
    .with_correlation(correlation.clone());

    let (sender, receiver) = mpsc::channel();
    let logging_system = LoggingSystem::new(8).unwrap();
    logging_system.subscribe(Arc::new(ChannelSink(sender)));
    let logging_instance = logging_system.instance(LogScope::Global, LogSource::Core);
    let logger = ObservabilityLogger::new(&logging_instance);

    logger
        .emit(global_log_event(operation.clone()))
        .expect("existing logging system should accept the event");

    let log_event = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("logging event should be delivered within five seconds");

    assert_eq!(
        correlation.operation_id().unwrap().as_str(),
        operation.operation.id.as_str()
    );
    assert_eq!(completed.trace_id(), &trace_id);
    assert_eq!(completed.events().len(), 1);
    assert_eq!(recorder.snapshot().unwrap().len(), 1);
    assert_eq!(diagnostic.correlation(), Some(&correlation));
    assert_eq!(
        log_event.context.operation.operation.id,
        operation.operation.id
    );

    logging_system.shutdown().unwrap();
}

#[test]
fn middleware_failure_can_be_observed_without_replacing_the_core_error_boundary() {
    struct FailingMiddleware;

    impl Middleware for FailingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            MiddlewareResult::Fail(MiddlewareError::new("downstream observation failure"))
        }
    }

    let operation = operation_context("phase11.failure.operation", "phase11.failure.correlation");
    let correlation = CorrelationContext::from_operation_context(&operation);
    let mut context = EngineContext::new(operation).with_configuration(resolved_configuration(
        "phase11.failure.mode",
        ConfigurationValue::String("unchanged".into()),
    ));
    let mut request = request(context.operation().clone());

    let chain = MiddlewareChain::with_stage(FailingMiddleware);
    let result = chain.execute(&mut context, &mut request, |_, _| Ok::<_, ()>(response()));

    assert!(matches!(result, Err(MiddlewareChainError::Middleware(_))));

    let diagnostic = Diagnostic::new(
        DiagnosticKind::Runtime,
        DiagnosticCondition::Failed,
        "middleware failure observed",
    )
    .unwrap()
    .with_correlation(correlation.clone());

    assert_eq!(diagnostic.correlation(), Some(&correlation));
    assert_eq!(
        context
            .configuration()
            .unwrap()
            .get("phase11.failure.mode")
            .and_then(ConfigurationValue::as_string),
        Some("unchanged")
    );
}

#[test]
fn health_observes_the_runtime_lifecycle_without_controlling_it() {
    let runtime = serving_runtime();
    assert_eq!(runtime.state(), LifecycleState::Serving);

    let before = runtime.state();

    let readiness = ReadinessReport::from_lifecycle(before);
    let liveness = LivenessReport::healthy();

    let dependency = DependencyReport::healthy(
        DependencyId::new("phase11.database").unwrap(),
        DependencyRequirement::Required,
    );
    let capability = CapabilityHealthReport::healthy(CapabilityId::new(TEST_CAPABILITY).unwrap());

    let report = HealthReport::new(
        EngineId::new(TEST_ENGINE).unwrap(),
        before,
        liveness,
        readiness,
        vec![dependency],
        vec![capability],
    )
    .unwrap();

    assert_eq!(report.lifecycle(), before);
    assert_eq!(report.overall(), HealthStatus::Healthy);
    assert_eq!(runtime.state(), before);

    runtime.shutdown().unwrap();
    assert_eq!(runtime.state(), LifecycleState::Stopped);
}

#[test]
fn draining_runtime_is_visible_to_health_without_health_controlling_lifecycle() {
    let runtime = serving_runtime();
    runtime
        .transition(LifecycleState::Draining)
        .expect("serving runtime should enter draining through its lifecycle");

    let lifecycle = runtime.state();
    let report = HealthReport::new(
        EngineId::new(TEST_ENGINE).unwrap(),
        lifecycle,
        LivenessReport::healthy(),
        ReadinessReport::from_lifecycle(lifecycle),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    assert_eq!(report.lifecycle(), LifecycleState::Draining);
    assert_eq!(runtime.state(), LifecycleState::Draining);
}

#[test]
fn phase11_subsystems_extend_existing_core_boundaries_without_replacing_them() {
    let configuration = resolved_configuration(
        "phase11.boundary",
        ConfigurationValue::String("stable".into()),
    );

    let operation = operation_context("phase11.boundary.operation", "phase11.boundary.correlation");

    let provenance = ProvenanceContext::new().with_attribute("phase", "11");
    let security = SecurityContext::new(
        PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("phase11.boundary.user").unwrap(),
        ),
        None,
    );

    let context = EngineContext::new(operation.clone())
        .with_configuration(configuration)
        .with_security(security.clone())
        .with_provenance(provenance.clone());

    let correlation = CorrelationContext::from_operation_context(&operation);

    let child = context.child();

    assert_eq!(child.operation().operation.id, operation.operation.id);
    assert_eq!(child.security(), Some(&security));
    assert_eq!(child.provenance(), &provenance);
    assert_eq!(child.configuration(), context.configuration());
    assert_eq!(
        correlation.correlation_id(),
        &operation.operation.correlation_id
    );
}
