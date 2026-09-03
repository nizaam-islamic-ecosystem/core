use std::sync::{Arc, mpsc};

use nizaam_core::prelude::*;

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

#[test]
fn global_events_preserve_shared_context() {
    let event = LogEvent::new(
        MessageId::new("event-1").unwrap(),
        LogLevel::Info,
        LogSource::ControlPlane,
        LogScope::Global,
        "router",
        LogContext::new(operation_context()).for_message(MessageId::new("message-1").unwrap()),
        "request dispatched",
        LogEventType::RequestSent,
    )
    .unwrap();

    assert_eq!(event.scope, LogScope::Global);
    assert_eq!(event.context.operation.operation.id.as_str(), "operation-1");
    assert_eq!(event.context.message_id.unwrap().as_str(), "message-1");
}

#[test]
fn local_engine_events_require_matching_engine_context() {
    let engine_id = EngineId::new("arabic").unwrap();
    let event = LogEvent::new(
        MessageId::new("event-2").unwrap(),
        LogLevel::Debug,
        LogSource::Engine(engine_id.clone()),
        LogScope::Local,
        "morphology",
        LogContext::new(operation_context()).from_engine(engine_id),
        "analysis started",
        LogEventType::CapabilityStarted,
    )
    .unwrap();

    assert_eq!(event.scope, LogScope::Local);
    assert!(matches!(event.source, LogSource::Engine(_)));
}

#[test]
fn invalid_scope_and_source_combinations_are_rejected() {
    let engine_id = EngineId::new("arabic").unwrap();
    let global_engine_event = LogEvent::new(
        MessageId::new("event-3").unwrap(),
        LogLevel::Info,
        LogSource::Engine(engine_id),
        LogScope::Global,
        "morphology",
        LogContext::new(operation_context()),
        "invalid global event",
        LogEventType::Diagnostic,
    );

    assert_eq!(
        global_engine_event,
        Err(LogValidationError::GlobalEventHasEngineSource)
    );

    let local_core_event = LogEvent::new(
        MessageId::new("event-4").unwrap(),
        LogLevel::Info,
        LogSource::Core,
        LogScope::Local,
        "core",
        LogContext::new(operation_context()),
        "missing engine",
        LogEventType::Diagnostic,
    );

    assert_eq!(
        local_core_event,
        Err(LogValidationError::LocalEventNeedsEngineContext)
    );
}

#[test]
fn empty_structured_fields_are_rejected() {
    let event = LogEvent::new(
        MessageId::new("event-5").unwrap(),
        LogLevel::Info,
        LogSource::Core,
        LogScope::Global,
        "core",
        LogContext::new(operation_context()),
        "   ",
        LogEventType::Diagnostic,
    );

    assert_eq!(event, Err(LogValidationError::EmptyMessage));
}

#[test]
fn scoped_instances_fan_out_events_to_subscribers() {
    let system = LoggingSystem::new(4).unwrap();
    let (first_sender, first_receiver) = mpsc::channel();
    let (second_sender, second_receiver) = mpsc::channel();
    system.subscribe(Arc::new(ChannelSink(first_sender)));
    system.subscribe(Arc::new(ChannelSink(second_sender)));
    let source = LogSource::ControlPlane;
    let instance = system.instance(LogScope::Global, source.clone());
    let event = LogEvent::new(
        MessageId::new("event-6").unwrap(),
        LogLevel::Info,
        source,
        LogScope::Global,
        "router",
        LogContext::new(operation_context()),
        "request received",
        LogEventType::RequestReceived,
    )
    .unwrap();

    assert_eq!(instance.publish(event).unwrap(), DispatchOutcome::Queued);
    assert_eq!(first_receiver.recv().unwrap().event_id.as_str(), "event-6");
    assert_eq!(second_receiver.recv().unwrap().event_id.as_str(), "event-6");
    system.shutdown().unwrap();
}

#[test]
fn instance_rejects_events_from_another_scope() {
    let system = LoggingSystem::new(1).unwrap();
    let instance = system.instance(LogScope::Global, LogSource::ControlPlane);
    let event = LogEvent::new(
        MessageId::new("event-7").unwrap(),
        LogLevel::Info,
        LogSource::ControlPlane,
        LogScope::Local,
        "router",
        LogContext::new(operation_context()).from_engine(EngineId::new("arabic").unwrap()),
        "wrong scope",
        LogEventType::Diagnostic,
    )
    .unwrap();

    assert_eq!(instance.publish(event), Err(InstanceError::ScopeMismatch));
    system.shutdown().unwrap();
}
