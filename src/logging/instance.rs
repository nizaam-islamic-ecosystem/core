use std::sync::Arc;

use super::dispatch::LogDispatcher;
use super::{DispatchError, DispatchOutcome, LogEvent, LogScope, LogSource};

/// Failure while publishing through a scoped logging instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceError {
    ScopeMismatch,
    SourceMismatch,
    InvalidEvent(super::LogValidationError),
    Dispatch(DispatchError),
}

/// A configured global or engine local view of the shared logging system.
pub struct LoggingInstance {
    dispatcher: Arc<LogDispatcher>,
    scope: LogScope,
    source: LogSource,
}

impl LoggingInstance {
    pub(crate) fn new(dispatcher: Arc<LogDispatcher>, scope: LogScope, source: LogSource) -> Self {
        Self {
            dispatcher,
            scope,
            source,
        }
    }

    pub fn publish(&self, event: LogEvent) -> Result<DispatchOutcome, InstanceError> {
        if event.scope != self.scope {
            return Err(InstanceError::ScopeMismatch);
        }
        if event.source != self.source {
            return Err(InstanceError::SourceMismatch);
        }
        event.validate().map_err(InstanceError::InvalidEvent)?;
        self.dispatcher
            .submit(event)
            .map_err(InstanceError::Dispatch)
    }

    pub fn scope(&self) -> LogScope {
        self.scope
    }

    pub fn source(&self) -> &LogSource {
        &self.source
    }
}
