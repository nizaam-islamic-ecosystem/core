//! Phase 11 integration tests for observability across Core module boundaries.
//!
//! These tests intentionally exercise observability through the public Core
//! surfaces it integrates with: operation identity, runtime context, logging,
//! metrics, tracing, diagnostics, security, and provenance.
//!
//! The individual observability mechanisms are unit-tested inside
//! `src/observability/*.rs`; `src/observability/mod.rs` tests composition
//! within the observability module. This file verifies cross-module behavior.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use nizaam_core::error::{ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, Severity};
use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::logging::{
    LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSink, LogSource, LoggingSystem,
};
use nizaam_core::observability::{
    CorrelationContext, Diagnostic, DiagnosticCondition, DiagnosticKind, MetricDescriptor,
    MetricDimensions, MetricKind, MetricName, MetricRecorder, ObservabilityLogger, Span, SpanEvent,
    SpanId, TraceId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::provenance::ProvenanceContext;
use nizaam_core::runtime::EngineContext;
use nizaam_core::security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext};
use nizaam_core::status::Retryability;

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new("observability-operation").unwrap(),
        CorrelationId::new("observability-correlation").unwrap(),
    ))
}

fn engine_context() -> EngineContext {
    EngineContext::new(operation_context())
}

struct ChannelSink(mpsc::Sender<LogEvent>);

impl LogSink for ChannelSink {
    fn publish(&self, event: &LogEvent) {
        self.0.send(event.clone()).unwrap();
    }
}

fn global_event(context: OperationContext, event_id: &str) -> LogEvent {
    LogEvent::new(
        nizaam_core::identity::MessageId::new(event_id).unwrap(),
        LogLevel::Info,
        LogSource::Core,
        LogScope::Global,
        "phase11-observability",
        LogContext::new(context),
        "observability integration event",
        LogEventType::Diagnostic,
    )
    .unwrap()
}

#[test]
fn correlation_context_uses_existing_core_operation_identity() {
    let operation = operation_context();
    let correlation = CorrelationContext::from_operation_context(&operation);

    assert_eq!(
        correlation.correlation_id().as_str(),
        "observability-correlation"
    );
    assert_eq!(
        correlation.operation_id().unwrap().as_str(),
        "observability-operation"
    );
    assert_eq!(
        operation.operation.correlation_id.as_str(),
        correlation.correlation_id().as_str()
    );
}

#[test]
fn engine_context_and_correlation_context_keep_identity_consistent() {
    let context = engine_context();
    let correlation = CorrelationContext::from_operation_context(context.operation());

    assert_eq!(
        context.operation().operation.id.as_str(),
        correlation.operation_id().unwrap().as_str()
    );
    assert_eq!(
        context.operation().operation.correlation_id.as_str(),
        correlation.correlation_id().as_str()
    );
}

#[test]
fn correlation_can_be_carried_into_the_existing_logging_context() {
    let operation = operation_context();
    let correlation = CorrelationContext::from_operation_context(&operation);

    let event = global_event(operation, "observability-message");

    assert_eq!(
        event.context.operation.operation.correlation_id,
        correlation.correlation_id().clone()
    );
    assert_eq!(
        event.context.operation.operation.id,
        correlation.operation_id().unwrap().clone()
    );
}

#[test]
fn observability_logger_publishes_through_the_existing_logging_system() {
    let system = LoggingSystem::new(4).unwrap();
    let (sender, receiver) = mpsc::channel();

    system.subscribe(Arc::new(ChannelSink(sender)));

    let instance = system.instance(LogScope::Global, LogSource::Core);
    let logger = ObservabilityLogger::new(&instance);

    assert_eq!(
        logger
            .emit(global_event(operation_context(), "observability-event"))
            .unwrap(),
        nizaam_core::logging::DispatchOutcome::Queued
    );

    let event = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("logging event should be delivered within five seconds");
    assert_eq!(event.event_id.as_str(), "observability-event");
    assert_eq!(event.event_type, LogEventType::Diagnostic);

    system.shutdown().unwrap();
}

#[test]
fn tracing_preserves_a_distinct_trace_identity_for_core_operation_identity() {
    let operation = operation_context();
    let correlation = CorrelationContext::from_operation_context(&operation);

    let trace_id = TraceId::new("trace-observability").unwrap();
    let root_span_id = SpanId::new("span-root").unwrap();
    let child_span_id = SpanId::new("span-child").unwrap();

    let mut root = Span::root(trace_id.clone(), root_span_id.clone(), "request").unwrap();
    root.set_attribute("operation.id", correlation.operation_id().unwrap().as_str())
        .unwrap();

    let child = Span::child(&root, child_span_id.clone(), "downstream").unwrap();
    let completed = root.finish();

    assert_eq!(completed.trace_id(), &trace_id);
    assert_eq!(completed.span_id(), &root_span_id);
    assert_eq!(child.trace_id(), &trace_id);
    assert_eq!(child.span_id(), &child_span_id);
    assert_eq!(child.parent_span_id(), Some(&root_span_id));
    assert_ne!(correlation.correlation_id().as_str(), trace_id.as_str());
}

#[test]
fn tracing_can_attach_a_request_event_from_core_execution() {
    let context = engine_context();
    let trace_id = TraceId::new("trace-request").unwrap();
    let span_id = SpanId::new("span-request").unwrap();

    let mut span = Span::root(trace_id, span_id, "core.request").unwrap();
    span.set_attribute("operation.id", context.operation().operation.id.as_str())
        .unwrap();

    let event = SpanEvent::new("request.received", std::time::SystemTime::now()).unwrap();
    span.add_event(event).unwrap();

    let completed = span.finish();

    assert_eq!(completed.events().len(), 1);
    assert_eq!(completed.events()[0].name(), "request.received");
    assert_eq!(
        completed.attributes().get("operation.id"),
        Some("observability-operation")
    );
}

#[test]
fn request_metrics_are_recorded_independently_from_tracing_and_logging() {
    let recorder = MetricRecorder::new();
    let descriptor = MetricDescriptor::new(
        MetricName::new("core.requests.total").unwrap(),
        MetricKind::Counter,
    );

    let mut dimensions = MetricDimensions::new();
    dimensions.insert("operation", "execute").unwrap();

    recorder
        .increment_counter(&descriptor, dimensions.clone(), 1)
        .unwrap();

    let snapshots = recorder.snapshot().unwrap();

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].descriptor(), &descriptor);
    assert_eq!(snapshots[0].dimensions(), &dimensions);
}

#[test]
fn diagnostics_preserve_correlation_without_becoming_runtime_provenance() {
    let operation = operation_context();
    let correlation = CorrelationContext::from_operation_context(&operation);

    let diagnostic = Diagnostic::new(
        DiagnosticKind::Runtime,
        DiagnosticCondition::Degraded,
        "runtime observation",
    )
    .unwrap()
    .with_correlation(correlation.clone())
    .with_detail("component", "runtime")
    .unwrap();

    let provenance = ProvenanceContext::new().with_attribute("source", "phase11-test");

    assert_eq!(diagnostic.correlation(), Some(&correlation));
    assert_ne!(format!("{diagnostic:?}"), format!("{provenance:?}"));
    assert_eq!(provenance.attribute("source"), Some("phase11-test"));
}

#[test]
fn diagnostics_can_reference_an_existing_core_error_without_replacing_it() {
    let definition = ErrorDefinition::new(
        ErrorCode::new("CORE.OBSERVABILITY.001").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        nizaam_core::contracts::Version::new(1, 0, 0),
        ErrorClass::Execution,
        Severity::Error,
        "observability integration failure",
        Retryability::NonRetryable,
    )
    .unwrap();

    let error_reference =
        nizaam_core::error::ErrorReference::new(definition.code.as_str()).unwrap();

    let diagnostic = Diagnostic::new(
        DiagnosticKind::Runtime,
        DiagnosticCondition::Failed,
        "runtime failure observed",
    )
    .unwrap()
    .with_error_reference(error_reference.clone());

    assert_eq!(diagnostic.error_reference(), Some(&error_reference));
}

#[test]
fn authenticated_engine_context_keeps_security_and_observability_concerns_distinct() {
    let principal =
        PrincipalIdentity::new(PrincipalType::User, PrincipalId::new("obs-user").unwrap());
    let security = SecurityContext::new(principal, None);

    let context = engine_context().with_security(security.clone());
    let correlation = CorrelationContext::from_operation_context(context.operation());

    assert_eq!(context.security(), Some(&security));
    assert_eq!(
        correlation.correlation_id(),
        &context.operation().operation.correlation_id
    );
}

#[test]
fn child_context_preserves_observable_operation_identity_and_provenance() {
    let provenance = ProvenanceContext::new().with_attribute("source", "observability-test");

    let parent = engine_context().with_provenance(provenance.clone());
    let child = parent.child();

    assert_eq!(
        child.operation().operation.id,
        parent.operation().operation.id
    );
    assert_eq!(
        child.operation().operation.correlation_id,
        parent.operation().operation.correlation_id
    );
    assert_eq!(child.provenance(), &provenance);
}

#[test]
fn complete_request_observability_flow_emits_distinct_observability_signals() {
    let operation = operation_context();
    let context = EngineContext::new(operation.clone());
    let correlation = CorrelationContext::from_operation_context(context.operation());

    let trace_id = TraceId::new("trace-complete").unwrap();
    let span_id = SpanId::new("span-complete").unwrap();
    let mut span = Span::root(trace_id.clone(), span_id, "core.request").unwrap();
    span.set_attribute("operation.id", operation.operation.id.as_str())
        .unwrap();
    span.add_event(SpanEvent::new("request.received", std::time::SystemTime::now()).unwrap())
        .unwrap();
    let completed_span = span.finish();

    let recorder = MetricRecorder::new();
    let descriptor = MetricDescriptor::new(
        MetricName::new("core.requests.total").unwrap(),
        MetricKind::Counter,
    );
    recorder
        .increment_counter(&descriptor, MetricDimensions::new(), 1)
        .unwrap();

    let diagnostic = Diagnostic::new(
        DiagnosticKind::Runtime,
        DiagnosticCondition::Operational,
        "request observed",
    )
    .unwrap()
    .with_correlation(correlation.clone())
    .with_detail("operation", "core.request")
    .unwrap();

    assert_eq!(
        correlation.operation_id().unwrap().as_str(),
        operation.operation.id.as_str()
    );
    assert_eq!(completed_span.trace_id(), &trace_id);
    assert_eq!(completed_span.events().len(), 1);
    assert_eq!(recorder.snapshot().unwrap().len(), 1);
    assert_eq!(diagnostic.correlation(), Some(&correlation));
}
