use std::sync::Arc;

use super::dispatch::LogDispatcher;
use super::{DispatchError, LogScope, LogSink, LogSource, LoggingInstance};

/// Shared factory and asynchronous dispatcher for structured logging.
pub struct LoggingSystem {
    dispatcher: Arc<LogDispatcher>,
}

impl LoggingSystem {
    pub fn new(capacity: usize) -> Result<Self, DispatchError> {
        Ok(Self {
            dispatcher: LogDispatcher::new(capacity)?,
        })
    }

    pub fn instance(&self, scope: LogScope, source: LogSource) -> LoggingInstance {
        LoggingInstance::new(Arc::clone(&self.dispatcher), scope, source)
    }

    pub fn subscribe(&self, sink: Arc<dyn LogSink>) {
        self.dispatcher.subscribe(sink);
    }

    pub fn shutdown(&self) -> Result<(), DispatchError> {
        self.dispatcher.shutdown()
    }
}
