//! Shared Core observability mechanisms.
//!
//! Phase 11 keeps Logging, Metrics, Tracing, Correlation, and Diagnostics as
//! distinct mechanisms. This module wires those components together at the
//! module boundary without selecting external observability providers or
//! making observability a correctness dependency.

pub mod correlation;
pub mod diagnostics;
pub mod event;
pub mod logging;
pub mod metrics;
pub mod tracing;
pub use correlation::CorrelationContext;
pub use diagnostics::{
    Diagnostic, DiagnosticCondition, DiagnosticDetails, DiagnosticError, DiagnosticKind,
    DiagnosticSubject,
};
pub use event::ObservabilityEvent;
pub use logging::ObservabilityLogger;
pub use metrics::{
    MetricDescriptor, MetricDimensions, MetricError, MetricKind, MetricName, MetricRecorder,
    MetricSnapshot, MetricValue,
};
pub use tracing::{
    CompletedSpan, Span, SpanAttributes, SpanEvent, SpanId, TraceContext, TraceError, TraceId,
};

#[cfg(test)]
mod test {
    use super::*;
    use crate::events::EventName;
    use crate::identity::{CorrelationId, OperationId};
    use crate::logging::{LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSource};
    use crate::operation::{Operation, OperationContext};

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ))
    }

    #[test]
    fn observability_modules_compose_without_collapsing_concerns() {
        let operation_context = operation_context();
        let correlation = CorrelationContext::from_operation_context(&operation_context);

        let diagnostic = Diagnostic::new(
            DiagnosticKind::Runtime,
            DiagnosticCondition::Degraded,
            "runtime condition requires attention",
        )
        .unwrap()
        .with_correlation(correlation.clone())
        .with_detail("component", "runtime")
        .unwrap();

        let trace_id = TraceId::new("trace-1").unwrap();
        let span_id = SpanId::new("span-1").unwrap();
        let mut span = Span::root(trace_id.clone(), span_id.clone(), "runtime.operation").unwrap();
        span.set_attribute("operation", "operation-1").unwrap();
        span.add_event(SpanEvent::new("diagnostic.created", diagnostic.observed_at()).unwrap())
            .unwrap();
        let completed = span.finish();

        let descriptor = MetricDescriptor::new(
            MetricName::new("runtime.operations.total").unwrap(),
            MetricKind::Counter,
        );
        let recorder = MetricRecorder::new();
        recorder
            .increment_counter(&descriptor, MetricDimensions::new(), 1)
            .unwrap();

        assert_eq!(correlation.correlation_id().as_str(), "correlation-1");
        assert_eq!(diagnostic.correlation(), Some(&correlation));
        assert_eq!(completed.trace_id(), &trace_id);
        assert_eq!(completed.span_id(), &span_id);
        assert_eq!(completed.events().len(), 1);
        assert_eq!(recorder.snapshot().unwrap().len(), 1);
    }

    #[test]
    fn correlation_can_feed_existing_logging_context_without_replacing_logging() {
        let operation_context = operation_context();
        let correlation = CorrelationContext::from_operation_context(&operation_context);

        let log_context = LogContext::new(operation_context);
        let event = LogEvent::new(
            EventName::new("logging.event").unwrap(),
            LogLevel::Info,
            LogSource::Core,
            LogScope::Global,
            "observability",
            log_context,
            "correlated runtime observation",
            LogEventType::Diagnostic,
        )
        .unwrap();

        assert_eq!(correlation.correlation_id().as_str(), "correlation-1");
        assert_eq!(
            event.context.operation.operation.correlation_id.as_str(),
            "correlation-1"
        );
        assert_eq!(event.event_type(), "diagnostic");
    }
}
