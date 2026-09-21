mod context;
mod dispatch;
mod event;
mod instance;
mod sink;
mod system;
mod validation;

pub use context::{LogContext, LogScope, LogSource};
pub use dispatch::{DispatchError, DispatchOutcome};
pub use event::{LogEvent, LogEventType, LogLevel, LogMetadata};
pub use instance::{InstanceError, LoggingInstance};
pub use sink::LogSink;
pub use system::LoggingSystem;
pub use validation::LogValidationError;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use super::*;
    use crate::events::EventName;
    use crate::identity::{CorrelationId, EngineId, EventId, MessageId, OperationId};
    use crate::operation::{Operation, OperationContext};

    struct RecordingSink {
        sender: Mutex<mpsc::Sender<LogEvent>>,
    }

    impl RecordingSink {
        fn new(sender: mpsc::Sender<LogEvent>) -> Self {
            Self {
                sender: Mutex::new(sender),
            }
        }
    }

    impl LogSink for RecordingSink {
        fn publish(&self, event: &LogEvent) {
            let _ = self
                .sender
                .lock()
                .expect("recording sink mutex must not be poisoned")
                .send(event.clone());
        }
    }

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("logging-operation").unwrap(),
            CorrelationId::new("logging-correlation").unwrap(),
        ))
    }

    fn global_event() -> LogEvent {
        LogEvent::new(
            EventId::new("log-event-global-1").unwrap(),
            EventName::new("logging.event").unwrap(),
            LogLevel::Info,
            LogSource::Core,
            LogScope::Global,
            "logging-test",
            LogContext::new(operation_context()),
            "global log event",
            LogEventType::Diagnostic,
        )
        .unwrap()
    }

    fn local_engine_event(engine_id: EngineId) -> LogEvent {
        LogEvent::new(
            EventId::new("log-event-local-1").unwrap(),
            EventName::new("engine.event").unwrap(),
            LogLevel::Info,
            LogSource::Engine(engine_id.clone()),
            LogScope::Local,
            "engine-component",
            LogContext::new(operation_context()).from_engine(engine_id),
            "local engine event",
            LogEventType::RequestReceived,
        )
        .unwrap()
    }

    #[test]
    fn log_event_constructs_universal_event_internally() {
        let event_id = EventId::new("log-event-1").unwrap();
        let message_id = MessageId::new("message-1").unwrap();

        let event = LogEvent::new(
            event_id.clone(),
            EventName::new("logging.event").unwrap(),
            LogLevel::Info,
            LogSource::Core,
            LogScope::Global,
            "logging-test",
            LogContext::new(operation_context()).for_message(message_id.clone()),
            "log event",
            LogEventType::Diagnostic,
        )
        .unwrap();

        assert!(!event.event_id().as_str().is_empty());
        assert_eq!(event.event_id(), &event_id);
        assert_eq!(event.message_id(), &message_id);
        assert_eq!(event.event_name().as_str(), "logging.event");
        assert_eq!(event.event_type(), "diagnostic");
        assert_eq!(event.event_scope(), "global");
        assert_eq!(event.context.message_id.as_ref(), Some(&message_id));
        assert_ne!(event.event_id().as_str(), message_id.as_str());
    }

    #[test]
    fn level_2_logging_system_composes_event_instance_dispatch_and_sink() {
        let system = LoggingSystem::new(8).unwrap();
        let (sender, receiver) = mpsc::channel();
        system.subscribe(Arc::new(RecordingSink::new(sender)));

        let instance = system.instance(LogScope::Global, LogSource::Core);
        let event = global_event();

        assert_eq!(
            instance.publish(event.clone()).unwrap(),
            DispatchOutcome::Queued
        );

        let received = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("queued event should reach the subscribed sink");

        assert_eq!(received, event);
        system.shutdown().unwrap();
    }

    #[test]
    fn level_2_engine_instance_isolation_is_enforced_by_logging_instance() {
        let system = LoggingSystem::new(8).unwrap();
        let engine = EngineId::new("quran-engine").unwrap();

        let global_instance = system.instance(LogScope::Global, LogSource::Core);
        let local_instance = system.instance(LogScope::Local, LogSource::Engine(engine.clone()));

        assert_eq!(
            global_instance.publish(local_engine_event(engine.clone())),
            Err(InstanceError::ScopeMismatch)
        );

        assert_eq!(
            local_instance.publish(global_event()),
            Err(InstanceError::ScopeMismatch)
        );

        system.shutdown().unwrap();
    }

    #[test]
    fn level_2_validation_rejects_source_and_context_mismatch() {
        let system = LoggingSystem::new(8).unwrap();
        let quran = EngineId::new("quran-engine").unwrap();
        let hadith = EngineId::new("hadith-engine").unwrap();

        let event = LogEvent::new(
            EventId::new("log-event-mismatch-1").unwrap(),
            EventName::new("engine.event").unwrap(),
            LogLevel::Info,
            LogSource::Engine(quran),
            LogScope::Local,
            "engine-component",
            LogContext::new(operation_context()).from_engine(hadith),
            "mismatched engine context",
            LogEventType::RequestReceived,
        );

        assert_eq!(event, Err(LogValidationError::SourceContextMismatch));

        let local_instance = system.instance(
            LogScope::Local,
            LogSource::Engine(EngineId::new("quran-engine").unwrap()),
        );

        assert_eq!(
            local_instance.publish(global_event()),
            Err(InstanceError::ScopeMismatch)
        );

        system.shutdown().unwrap();
    }

    #[test]
    fn level_2_metadata_and_context_survive_end_to_end_dispatch() {
        let system = LoggingSystem::new(8).unwrap();
        let (sender, receiver) = mpsc::channel();
        system.subscribe(Arc::new(RecordingSink::new(sender)));

        let engine = EngineId::new("quran-engine").unwrap();
        let instance = system.instance(LogScope::Local, LogSource::Engine(engine.clone()));

        let event = LogEvent::new(
            EventId::new("log-event-context-1").unwrap(),
            EventName::new("quran.request").unwrap(),
            LogLevel::Warning,
            LogSource::Engine(engine.clone()),
            LogScope::Local,
            "quran-api",
            LogContext::new(operation_context())
                .from_engine(engine.clone())
                .for_message(MessageId::new("message-1").unwrap())
                .with_trace_context("trace-context-1"),
            "request is degraded",
            LogEventType::Warning,
        )
        .unwrap()
        .with_metadata("component", "quran-api")
        .with_metadata("state", "degraded");

        instance.publish(event.clone()).unwrap();

        let received = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("event should be delivered");

        assert_eq!(received, event);
        assert_eq!(
            received.context.message_id.as_ref().unwrap().as_str(),
            "message-1"
        );
        assert_eq!(
            received.context.trace_context.as_deref(),
            Some("trace-context-1")
        );
        assert_eq!(
            received.metadata.get("component").map(String::as_str),
            Some("quran-api")
        );
        assert_eq!(
            received.metadata.get("state").map(String::as_str),
            Some("degraded")
        );
        assert_eq!(received.event_name().as_str(), "quran.request");
        assert_eq!(received.event_type(), "warning");
        assert_eq!(received.event_scope(), "engine:quran-engine");

        system.shutdown().unwrap();
    }

    #[test]
    fn level_2_invalid_capacity_is_rejected_at_system_boundary() {
        assert!(matches!(
            LoggingSystem::new(0),
            Err(DispatchError::InvalidCapacity)
        ));
    }
}
