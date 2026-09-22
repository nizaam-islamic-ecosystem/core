use crate::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor,
};
use crate::contracts::metadata::{ContractMetadata, Participants};
use crate::contracts::{MessageEnvelope, UniversalEvent, Version};
use crate::identity::{CapabilityId, ContractId, EngineId, EventId, MessageId};
use crate::operation::OperationContext;
use crate::status::Retryability;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{ErrorClass, ErrorCode, ErrorOwner, ErrorReference, Severity};

/// One machine-readable diagnostic field attached to an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticDetail {
    pub key: String,
    pub value: String,
}

impl DiagnosticDetail {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Option<Self> {
        let key = key.into();
        let value = value.into();
        (!key.trim().is_empty() && !value.trim().is_empty()).then_some(Self { key, value })
    }
}

/// Trusted execution context associated with an error occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorContext {
    pub operation: OperationContext,
    pub engine_id: Option<EngineId>,
    pub capability_id: Option<CapabilityId>,
}

impl ErrorContext {
    pub fn new(operation: OperationContext) -> Self {
        Self {
            operation,
            engine_id: None,
            capability_id: None,
        }
    }

    pub fn from_engine(mut self, engine_id: EngineId) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    pub fn for_capability(mut self, capability_id: CapabilityId) -> Self {
        self.capability_id = Some(capability_id);
        self
    }
}

/// The strict common error contract emitted by Core systems.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalError {
    pub code: ErrorCode,
    pub owner: ErrorOwner,
    pub version: Version,
    pub class: ErrorClass,
    pub severity: Severity,
    pub retryability: Retryability,
    pub message: String,
    pub details: Vec<DiagnosticDetail>,
    pub solution_reference: Option<String>,
    pub context: ErrorContext,
    pub cause: Option<ErrorReference>,
}

/// A runtime error occurrence that owns its universal Event contract.
///
/// The error system supplies only the error-domain occurrence. The universal
/// Event boundary is assembled internally so callers never have to construct
/// or pass a `UniversalEvent`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorEvent {
    /// The universal Event contract carrying the common event boundary.
    pub event: UniversalEvent,
    /// Error-specific occurrence data.
    pub error: GlobalError,
}

static NEXT_ERROR_EVENT_ID: AtomicU64 = AtomicU64::new(1);

impl ErrorEvent {
    /// Creates an ErrorEvent and constructs its universal Event internally.
    pub fn new(error: GlobalError) -> Self {
        let sequence = NEXT_ERROR_EVENT_ID.fetch_add(1, Ordering::Relaxed);
        let event_id = EventId::new(format!("error-{sequence}-{}", error.code.as_str()))
            .expect("validated error codes produce valid event ids");

        let message_id = MessageId::new(format!("error-message-{event_id}"))
            .expect("derived error message ids are non-empty");

        let sender = error
            .context
            .engine_id
            .clone()
            .unwrap_or_else(|| EngineId::new("core").expect("static core engine id is valid"));

        let descriptor = ContractDescriptor::new(
            ContractId::new("error.event").expect("static error contract id is valid"),
            CapabilityId::new("error.report").expect("static error capability id is valid"),
            Version::new(1, 0, 0),
            Interaction::Event,
            PayloadDescriptor::new("text/plain", Version::new(1, 0, 0))
                .expect("static error payload descriptor is valid"),
        );

        let payload_descriptor = descriptor.payload.clone();
        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                sender,
                EngineId::new("error-system").expect("static error-system engine id is valid"),
            ),
        );

        let envelope = MessageEnvelope::new(
            message_id,
            error.context.operation.clone(),
            metadata,
            EncodedPayload::new(payload_descriptor, error.message.as_bytes().to_vec()),
        );

        let scope = error
            .context
            .engine_id
            .as_ref()
            .map(|engine| format!("engine:{}", engine.as_str()))
            .unwrap_or_else(|| "global".to_owned());

        let event =
            UniversalEvent::from_parts(envelope, event_id, "error.occurred", "error", scope)
                .expect("internal ErrorEvent construction must produce a valid universal Event");

        Self { event, error }
    }

    /// Returns the error reference derived from the validated error code.
    pub fn reference(&self) -> ErrorReference {
        ErrorReference::new(self.error.code.as_str().to_owned())
            .expect("validated error codes produce valid references")
    }

    /// Returns the universal Event occurrence identity.
    pub fn event_id(&self) -> &EventId {
        self.event.event_id()
    }

    /// Returns the universal logical message identity.
    pub fn message_id(&self) -> &MessageId {
        self.event.message_id()
    }

    /// Returns the semantic Event name.
    pub fn event_name(&self) -> &crate::events::EventName {
        self.event.event_name()
    }

    /// Returns the semantic Event type.
    pub fn event_type(&self) -> &str {
        self.event.event_type()
    }

    /// Returns the generic Event scope.
    pub fn event_scope(&self) -> &str {
        self.event.scope()
    }

    /// Returns the complete universal Event contract.
    pub fn universal_event(&self) -> &UniversalEvent {
        &self.event
    }

    /// Returns the error-specific payload.
    pub fn error(&self) -> &GlobalError {
        &self.error
    }
}

impl GlobalError {
    pub(crate) fn from_definition(
        definition: &super::ErrorDefinition,
        context: ErrorContext,
        message: Option<String>,
    ) -> Self {
        Self {
            code: definition.code.clone(),
            owner: definition.owner.clone(),
            version: definition.version.clone(),
            class: definition.class,
            severity: definition.severity,
            retryability: definition.retryability,
            message: message.unwrap_or_else(|| definition.default_message().to_owned()),
            details: Vec::new(),
            solution_reference: definition.solution_reference.clone(),
            context,
            cause: None,
        }
    }

    pub fn with_detail(mut self, detail: DiagnosticDetail) -> Self {
        self.details.push(detail);
        self
    }

    pub fn details(&self) -> &[DiagnosticDetail] {
        &self.details
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn caused_by(mut self, cause: ErrorReference) -> Self {
        self.cause = Some(cause);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::contracts::descriptor::Interaction;
    use crate::error::{ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, Severity};
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::Operation;
    use crate::operation::OperationContext;
    use crate::status::Retryability;

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("test-op").unwrap(),
            CorrelationId::new("test-corr").unwrap(),
        ))
    }

    fn global_error() -> GlobalError {
        GlobalError {
            code: ErrorCode::new("CORE.TEST.001").unwrap(),
            owner: ErrorOwner::new("CORE").unwrap(),
            version: Version::new(1, 0, 0),
            class: ErrorClass::Contract,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: "Test error".to_string(),
            details: Vec::new(),
            solution_reference: None,
            context: ErrorContext::new(operation_context()),
            cause: None,
        }
    }

    #[test]
    fn diagnostic_detail_rejects_empty_key() {
        let detail = DiagnosticDetail::new("", "value");
        assert!(detail.is_none());
    }

    #[test]
    fn diagnostic_detail_rejects_empty_value() {
        let detail = DiagnosticDetail::new("key", "");
        assert!(detail.is_none());
    }

    #[test]
    fn diagnostic_detail_accepts_valid_fields() {
        let detail = DiagnosticDetail::new("key", "value").unwrap();
        assert_eq!(detail.key, "key");
        assert_eq!(detail.value, "value");
    }

    #[test]
    fn error_context_starts_without_engine_or_capability() {
        let context = ErrorContext::new(operation_context());
        assert!(context.engine_id.is_none());
        assert!(context.capability_id.is_none());
    }

    #[test]
    fn error_context_builder_methods_set_fields() {
        let context = ErrorContext::new(operation_context())
            .from_engine(EngineId::new("engine-1").unwrap())
            .for_capability(CapabilityId::new("cap-1").unwrap());
        assert!(context.engine_id.is_some());
        assert!(context.capability_id.is_some());
    }

    #[test]
    fn global_error_from_definition_preserves_fields() {
        let definition = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.002").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Warning,
            "Warning message",
            Retryability::Retryable,
        )
        .unwrap();
        let error =
            GlobalError::from_definition(&definition, ErrorContext::new(operation_context()), None);
        assert_eq!(error.code.as_str(), "CORE.TEST.002");
        assert_eq!(error.severity, Severity::Warning);
        assert_eq!(error.retryability, Retryability::Retryable);
        assert!(error.cause.is_none());
        assert!(error.details.is_empty());
    }

    #[test]
    fn global_error_with_message_overrides_default() {
        let mut error = global_error();
        error = error.with_message("Custom message".to_string());
        assert_eq!(error.message, "Custom message");
    }

    #[test]
    fn global_error_with_detail_adds_to_details() {
        let mut error = global_error();
        error = error.with_detail(DiagnosticDetail::new("key", "value").unwrap());
        assert_eq!(error.details.len(), 1);
        assert_eq!(error.details[0].key, "key");
    }

    #[test]
    fn global_error_caused_by_sets_cause() {
        let mut error = global_error();
        let cause = ErrorReference::new("CORE.CAUSE.001").unwrap();
        error = error.caused_by(cause.clone());
        assert_eq!(error.cause, Some(cause));
    }

    #[test]
    fn error_event_constructs_universal_event_internally() {
        let event = ErrorEvent::new(global_error());

        assert!(!event.event_id().as_str().is_empty());
        assert!(!event.message_id().as_str().is_empty());
        assert_eq!(event.event_name().as_str(), "error.occurred");
        assert_eq!(event.event_type(), "error");
        assert_eq!(event.event_scope(), "global");
        assert_eq!(event.reference().as_str(), "CORE.TEST.001");
        assert_eq!(event.error().message, "Test error");
    }

    #[test]
    fn error_event_preserves_operation_context_in_universal_envelope() {
        let error = global_error();
        let operation = error.context.operation.clone();

        let event = ErrorEvent::new(error);

        assert_eq!(
            event.universal_event().envelope.operation_context,
            operation
        );
        assert_eq!(
            event
                .universal_event()
                .envelope
                .metadata
                .descriptor
                .interaction,
            Interaction::Event
        );
    }
}
