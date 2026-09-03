use crate::identity::{CapabilityId, EngineId, MessageId};
use crate::operation::OperationContext;

/// Identifies whether an event belongs to platform wide or engine local work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogScope {
    Global,
    Local,
}

/// Identifies the component that produced an event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LogSource {
    ControlPlane,
    Core,
    Runtime,
    Engine(EngineId),
}

/// Shared execution context attached to a log event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogContext {
    pub operation: OperationContext,
    pub message_id: Option<MessageId>,
    pub engine_id: Option<EngineId>,
    pub capability_id: Option<CapabilityId>,
    pub trace_context: Option<String>,
}

impl LogContext {
    pub fn new(operation: OperationContext) -> Self {
        Self {
            operation,
            message_id: None,
            engine_id: None,
            capability_id: None,
            trace_context: None,
        }
    }

    pub fn for_message(mut self, message_id: MessageId) -> Self {
        self.message_id = Some(message_id);
        self
    }

    pub fn from_engine(mut self, engine_id: EngineId) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    pub fn for_capability(mut self, capability_id: CapabilityId) -> Self {
        self.capability_id = Some(capability_id);
        self
    }

    pub fn with_trace_context(mut self, trace_context: impl Into<String>) -> Self {
        self.trace_context = Some(trace_context.into());
        self
    }
}
