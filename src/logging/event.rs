use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::contracts::UniversalEvent;
use crate::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use crate::contracts::envelope::MessageEnvelope;
use crate::contracts::metadata::{ContractMetadata, Participants};
use crate::events::EventName;
use crate::identity::{CapabilityId, ContractId, EngineId, EventId, MessageId};
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

impl LogEventType {
    fn as_str(self) -> &'static str {
        match self {
            Self::RequestReceived => "request.received",
            Self::RequestSent => "request.sent",
            Self::CapabilityStarted => "capability.started",
            Self::CapabilityCompleted => "capability.completed",
            Self::DependencyWaiting => "dependency.waiting",
            Self::ResponseReceived => "response.received",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::LifecycleChange => "lifecycle.change",
            Self::Diagnostic => "diagnostic",
        }
    }
}

pub type LogMetadata = BTreeMap<String, String>;

/// Logging-specific data that owns a universal Event contract.
///
/// Callers provide only logging-domain data. The common `UniversalEvent`
/// boundary and its `MessageEnvelope` are constructed internally.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEvent {
    /// The universal Event contract owned by this logging occurrence.
    pub event: UniversalEvent,
    /// Time at which this logging record was created.
    pub timestamp: SystemTime,
    /// Logging severity used for filtering and delivery policy.
    pub level: LogLevel,
    /// Component that produced the log record.
    pub source: LogSource,
    /// Logging-specific global/local routing scope.
    pub scope: LogScope,
    /// Component name associated with the log record.
    pub component: String,
    /// Logging-specific execution context.
    pub context: LogContext,
    /// Human-readable logging message.
    pub message: String,
    pub status: Option<Status>,
    pub error_reference: Option<ErrorReference>,
    pub metadata: LogMetadata,
    pub artifact_reference: Option<ArtifactReference>,
}

impl LogEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: EventId,
        event_name: EventName,
        level: LogLevel,
        source: LogSource,
        scope: LogScope,
        component: impl Into<String>,
        context: LogContext,
        message: impl Into<String>,
        event_type: LogEventType,
    ) -> Result<Self, LogValidationError> {
        let component = component.into();
        let message = message.into();

        validate_log_inputs(&source, scope, &context, &component, &message)?;

        let event = Self {
            event: build_universal_event(
                event_id,
                event_name,
                source.clone(),
                scope,
                context.clone(),
                message.clone(),
                event_type,
            ),
            timestamp: SystemTime::now(),
            level,
            source,
            scope,
            component,
            context,
            message,
            status: None,
            error_reference: None,
            metadata: BTreeMap::new(),
            artifact_reference: None,
        };

        event.validate()?;
        Ok(event)
    }

    /// Returns the universal Event occurrence identity.
    pub fn event_id(&self) -> &EventId {
        self.event.event_id()
    }

    /// Returns the universal logical message identity.
    pub fn message_id(&self) -> &MessageId {
        self.event.message_id()
    }

    /// Returns the semantic Event name from the universal Event contract.
    pub fn event_name(&self) -> &EventName {
        self.event.event_name()
    }

    /// Returns the semantic Event type from the universal Event contract.
    pub fn event_type(&self) -> &str {
        self.event.event_type()
    }

    /// Returns the generic Event scope from the universal Event contract.
    pub fn event_scope(&self) -> &str {
        self.event.scope()
    }

    /// Returns the complete universal Event contract.
    pub fn universal_event(&self) -> &UniversalEvent {
        &self.event
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
        if let LogSource::Engine(source_engine_id) = &self.source
            && self.context.engine_id.as_ref() != Some(source_engine_id)
        {
            return Err(LogValidationError::SourceContextMismatch);
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

fn validate_log_inputs(
    source: &LogSource,
    scope: LogScope,
    context: &LogContext,
    component: &str,
    message: &str,
) -> Result<(), LogValidationError> {
    if component.trim().is_empty() {
        return Err(LogValidationError::EmptyComponent);
    }
    if message.trim().is_empty() {
        return Err(LogValidationError::EmptyMessage);
    }
    if scope == LogScope::Global && context.engine_id.is_some() {
        return Err(LogValidationError::GlobalEventHasEngineContext);
    }
    if scope == LogScope::Local && context.engine_id.is_none() {
        return Err(LogValidationError::LocalEventNeedsEngineContext);
    }
    if scope == LogScope::Global && matches!(source, LogSource::Engine(_)) {
        return Err(LogValidationError::GlobalEventHasEngineSource);
    }
    if let LogSource::Engine(source_engine_id) = source
        && context.engine_id.as_ref() != Some(source_engine_id)
    {
        return Err(LogValidationError::SourceContextMismatch);
    }
    Ok(())
}

fn build_universal_event(
    event_id: EventId,
    event_name: EventName,
    source: LogSource,
    scope: LogScope,
    context: LogContext,
    message: String,
    event_type: LogEventType,
) -> UniversalEvent {
    let message_id = context.message_id.clone().unwrap_or_else(|| {
        MessageId::new(format!("log-message-{event_id}"))
            .expect("derived logging message ids are non-empty")
    });

    let sender = match &source {
        LogSource::Core => EngineId::new("core").expect("static core engine id is valid"),
        LogSource::ControlPlane => {
            EngineId::new("control-plane").expect("static control-plane engine id is valid")
        }
        LogSource::Runtime => EngineId::new("runtime").expect("static runtime engine id is valid"),
        LogSource::Engine(engine_id) => engine_id.clone(),
    };

    let descriptor = ContractDescriptor::new(
        ContractId::new("logging.event").expect("static logging contract id is valid"),
        CapabilityId::new("logging.emit").expect("static logging capability id is valid"),
        Version::new(1, 0, 0),
        Interaction::Event,
        PayloadDescriptor::new("text/plain", Version::new(1, 0, 0))
            .expect("static logging payload descriptor is valid"),
    );

    let payload_descriptor = descriptor.payload.clone();
    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            sender,
            EngineId::new("logging-system").expect("static logging-system engine id is valid"),
        ),
    );

    let envelope = MessageEnvelope::new(
        message_id,
        context.operation.clone(),
        metadata,
        EncodedPayload::new(payload_descriptor, message.as_bytes().to_vec()),
    );

    let event_scope = match scope {
        LogScope::Global => "global".to_owned(),
        LogScope::Local => format!(
            "engine:{}",
            context
                .engine_id
                .as_ref()
                .expect("validated local logging events have an engine context")
                .as_str()
        ),
    };

    UniversalEvent::from_parts(
        envelope,
        event_id,
        event_name.to_string(),
        event_type.as_str(),
        event_scope,
    )
    .expect("validated logging metadata must produce a valid universal Event")
}
