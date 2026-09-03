use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::identity::MessageId;
use crate::status::{ArtifactReference, ErrorReference, Status};

use super::{LogContext, LogScope, LogSource, LogValidationError};

/// Severity used for filtering and delivery policy.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Audit,
}

/// Machine readable category independent from severity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogEventType {
    RequestReceived,
    RequestSent,
    CapabilityStarted,
    CapabilityCompleted,
    DependencyWaiting,
    ResponseReceived,
    Error,
    Warning,
    LifecycleChange,
    Diagnostic,
}

pub type LogMetadata = BTreeMap<String, String>;

/// The common structured event emitted by Core and engines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEvent {
    pub event_id: MessageId,
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub source: LogSource,
    pub scope: LogScope,
    pub component: String,
    pub context: LogContext,
    pub message: String,
    pub event_type: LogEventType,
    pub status: Option<Status>,
    pub error_reference: Option<ErrorReference>,
    pub metadata: LogMetadata,
    pub artifact_reference: Option<ArtifactReference>,
}

impl LogEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: MessageId,
        level: LogLevel,
        source: LogSource,
        scope: LogScope,
        component: impl Into<String>,
        context: LogContext,
        message: impl Into<String>,
        event_type: LogEventType,
    ) -> Result<Self, LogValidationError> {
        let event = Self {
            event_id,
            timestamp: SystemTime::now(),
            level,
            source,
            scope,
            component: component.into(),
            context,
            message: message.into(),
            event_type,
            status: None,
            error_reference: None,
            metadata: BTreeMap::new(),
            artifact_reference: None,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_error(mut self, error_reference: ErrorReference) -> Self {
        self.error_reference = Some(error_reference);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn with_artifact(mut self, artifact_reference: ArtifactReference) -> Self {
        self.artifact_reference = Some(artifact_reference);
        self
    }

    pub fn validate(&self) -> Result<(), LogValidationError> {
        if self.component.trim().is_empty() {
            return Err(LogValidationError::EmptyComponent);
        }
        if self.message.trim().is_empty() {
            return Err(LogValidationError::EmptyMessage);
        }
        if self.scope == LogScope::Global && self.context.engine_id.is_some() {
            return Err(LogValidationError::GlobalEventHasEngineContext);
        }
        if self.scope == LogScope::Local && self.context.engine_id.is_none() {
            return Err(LogValidationError::LocalEventNeedsEngineContext);
        }
        if self.scope == LogScope::Global && matches!(self.source, LogSource::Engine(_)) {
            return Err(LogValidationError::GlobalEventHasEngineSource);
        }
        if let LogSource::Engine(source_engine_id) = &self.source {
            if self.context.engine_id.as_ref() != Some(source_engine_id) {
                return Err(LogValidationError::SourceContextMismatch);
            }
        }
        if self
            .metadata
            .iter()
            .any(|(key, value)| key.trim().is_empty() || value.trim().is_empty())
        {
            return Err(LogValidationError::EmptyMetadataField);
        }
        Ok(())
    }
}
