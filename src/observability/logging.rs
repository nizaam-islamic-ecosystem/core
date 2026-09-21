//! Observability integration with the existing Core Logging System.

//!
//! Phase 11 does not define a second logging implementation. This module
//! provides a small observability-facing adapter over the established
//! [`crate::logging`] contracts and dispatcher.

use crate::logging::{
    DispatchOutcome, InstanceError, LogEvent, LogScope, LogSource, LoggingInstance,
};

/// Observability-facing handle over an existing structured logging instance.
///
/// The adapter deliberately does not own a dispatcher, queue, sink, or second
/// event model. It delegates event publication to the Phase 4 logging system.
#[derive(Clone, Copy)]
pub struct ObservabilityLogger<'a> {
    instance: &'a LoggingInstance,
}

impl<'a> ObservabilityLogger<'a> {
    /// Creates an observability logger over an existing logging instance.
    pub fn new(instance: &'a LoggingInstance) -> Self {
        Self { instance }
    }

    /// Emits a validated structured event through the existing logging system.
    ///
    /// Observability is intentionally best-effort at the call site: callers
    /// may handle or ignore the returned dispatch failure without coupling the
    /// business operation to observability availability.
    pub fn emit(&self, event: LogEvent) -> Result<DispatchOutcome, InstanceError> {
        self.instance.publish(event)
    }

    /// Returns the scope enforced by the underlying logging instance.
    pub fn scope(&self) -> LogScope {
        self.instance.scope()
    }

    /// Returns the source enforced by the underlying logging instance.
    pub fn source(&self) -> &LogSource {
        self.instance.source()
    }
}

impl<'a> From<&'a LoggingInstance> for ObservabilityLogger<'a> {
    fn from(instance: &'a LoggingInstance) -> Self {
        Self::new(instance)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use super::*;
    use crate::events::EventName;
    use crate::identity::{CorrelationId, EventId, OperationId};
    use crate::logging::{LogContext, LogEventType, LogLevel, LogSink, LoggingSystem};
    use crate::operation::{Operation, OperationContext};

    struct ChannelSink(mpsc::Sender<LogEvent>);

    impl LogSink for ChannelSink {
        fn publish(&self, event: &LogEvent) {
            self.0.send(event.clone()).unwrap();
        }
    }

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ))
    }

    fn global_event(event_id: &str) -> LogEvent {
        LogEvent::new(
            EventId::new(event_id).unwrap(),
            EventName::new("observability.event").unwrap(),
            LogLevel::Info,
            LogSource::Core,
            LogScope::Global,
            "observability",
            LogContext::new(operation_context()),
            "runtime event",
            LogEventType::Diagnostic,
        )
        .unwrap()
    }

    #[test]
    fn emits_through_the_existing_logging_system() {
        let system = LoggingSystem::new(4).unwrap();
        let (sender, receiver) = mpsc::channel();
        system.subscribe(Arc::new(ChannelSink(sender)));
        let instance = system.instance(LogScope::Global, LogSource::Core);
        let logger = ObservabilityLogger::new(&instance);
        let event = global_event("event-1");
        let event_id = event.event_id().clone();

        assert_eq!(logger.emit(event).unwrap(), DispatchOutcome::Queued);
        assert_eq!(receiver.recv().unwrap().event_id(), &event_id);

        system.shutdown().unwrap();
    }

    #[test]
    fn preserves_the_underlying_logging_scope_and_source() {
        let system = LoggingSystem::new(1).unwrap();
        let instance = system.instance(LogScope::Global, LogSource::Core);
        let logger = ObservabilityLogger::new(&instance);

        assert_eq!(logger.scope(), LogScope::Global);
        assert_eq!(logger.source(), &LogSource::Core);

        system.shutdown().unwrap();
    }

    #[test]
    fn delegates_event_validation_to_the_existing_logging_contract() {
        let system = LoggingSystem::new(1).unwrap();
        let instance = system.instance(LogScope::Global, LogSource::Core);
        let logger = ObservabilityLogger::new(&instance);

        let event = LogEvent::new(
            EventId::new("event-2").unwrap(),
            EventName::new("observability.event").unwrap(),
            LogLevel::Info,
            LogSource::Core,
            LogScope::Global,
            "observability",
            LogContext::new(operation_context()),
            "valid event",
            LogEventType::Diagnostic,
        )
        .unwrap()
        .with_metadata(" ", "value");

        assert_eq!(
            logger.emit(event),
            Err(InstanceError::InvalidEvent(
                crate::logging::LogValidationError::EmptyMetadataField
            ))
        );

        system.shutdown().unwrap();
    }

    #[test]
    fn logging_failure_is_returned_without_creating_a_second_error_model() {
        let system = LoggingSystem::new(1).unwrap();
        let instance = system.instance(LogScope::Global, LogSource::Core);
        let logger = ObservabilityLogger::new(&instance);

        system.shutdown().unwrap();

        assert_eq!(
            logger.emit(global_event("event-3")),
            Err(InstanceError::Dispatch(
                crate::logging::DispatchError::Closed
            ))
        );
    }

    #[test]
    fn conversion_from_logging_instance_preserves_the_same_instance() {
        let system = LoggingSystem::new(1).unwrap();
        let instance = system.instance(LogScope::Global, LogSource::Core);
        let logger = ObservabilityLogger::from(&instance);

        assert_eq!(logger.scope(), instance.scope());
        assert_eq!(logger.source(), instance.source());

        system.shutdown().unwrap();
    }
}
