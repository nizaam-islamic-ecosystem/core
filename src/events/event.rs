//! Immutable internal event occurrence representation for Nizaam Core.
//!
//! An `Event` identifies one internal event occurrence together with the
//! generic metadata required for publisher matching. Event semantics remain
//! owned by the producer; Core only stores and compares the metadata needed by
//! the internal notification mechanism.

use super::scope::Scope;
use crate::{identity::EventId, operation::OperationContext, security::SecurityContext};

/// Minimal trusted context propagated with an internal Event.
///
/// The context intentionally reuses existing Core context types rather than
/// copying the complete runtime `EngineContext`. Operation context is used for
/// correlation and attempt lineage; security context preserves the trusted
/// producer identity when one is available. The context does not contain
/// credentials, authorization decisions, deadlines, runtime configuration, or
/// other execution machinery.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventContext {
    operation_context: Option<OperationContext>,
    security_context: Option<SecurityContext>,
}

impl EventContext {
    /// Creates an empty event context.
    pub const fn empty() -> Self {
        Self {
            operation_context: None,
            security_context: None,
        }
    }

    /// Creates an event context from optional trusted Core contexts.
    pub const fn new(
        operation_context: Option<OperationContext>,
        security_context: Option<SecurityContext>,
    ) -> Self {
        Self {
            operation_context,
            security_context,
        }
    }

    /// Returns the propagated operation context, when present.
    pub fn operation_context(&self) -> Option<&OperationContext> {
        self.operation_context.as_ref()
    }

    /// Returns the propagated trusted security context, when present.
    pub fn security_context(&self) -> Option<&SecurityContext> {
        self.security_context.as_ref()
    }

    /// Returns a copy of this context with the supplied operation context.
    pub fn with_operation_context(mut self, operation_context: OperationContext) -> Self {
        self.operation_context = Some(operation_context);
        self
    }

    /// Returns a copy of this context with the supplied trusted security context.
    pub fn with_security_context(mut self, security_context: SecurityContext) -> Self {
        self.security_context = Some(security_context);
        self
    }
}

/// Error returned when an event cannot be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventCreationError {
    /// An event type must contain at least one non-whitespace character.
    EmptyEventType,
}

impl std::fmt::Display for EventCreationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyEventType => formatter.write_str("event type must not be empty"),
        }
    }
}

impl std::error::Error for EventCreationError {}

/// One immutable internal event occurrence.
///
/// `EventId` identifies the occurrence itself. `event_type` identifies its
/// semantic category, while `scope` identifies the explicit generic
/// applicability boundary used for subscription matching.
///
/// The event carries only generic Core semantics. Domain-specific payload
/// meaning remains outside this type and belongs to the surrounding contracts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    event_id: EventId,
    event_type: Box<str>,
    scope: Scope,
    context: EventContext,
}

impl Event {
    /// Creates a fully described immutable event occurrence.
    pub fn new(
        event_id: EventId,
        event_type: impl Into<String>,
        scope: Scope,
    ) -> Result<Self, EventCreationError> {
        Self::new_with_context(event_id, event_type, scope, EventContext::empty())
    }

    /// Creates an immutable event occurrence with trusted propagated context.
    pub fn new_with_context(
        event_id: EventId,
        event_type: impl Into<String>,
        scope: Scope,
        context: EventContext,
    ) -> Result<Self, EventCreationError> {
        let event_type = event_type.into();

        if event_type.trim().is_empty() {
            return Err(EventCreationError::EmptyEventType);
        }

        Ok(Self {
            event_id,
            event_type: event_type.into_boxed_str(),
            scope,
            context,
        })
    }

    /// Returns the identity of this event occurrence.
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    /// Returns the semantic event type used for generic subscription matching.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the explicit generic event scope.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Returns the minimal trusted context propagated with this event.
    pub fn context(&self) -> &EventContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::EventId;

    fn scope() -> Scope {
        Scope::new("engine:test").unwrap()
    }

    #[test]
    fn event_constructs_with_identity_type_and_scope() {
        let event = Event::new(EventId::new("event-1").unwrap(), "test.event", scope()).unwrap();

        assert_eq!(event.event_id().as_str(), "event-1");
        assert_eq!(event.event_type(), "test.event");
        assert_eq!(event.scope().as_str(), "engine:test");
    }

    #[test]
    fn event_rejects_empty_event_type() {
        assert_eq!(
            Event::new(EventId::new("event-1").unwrap(), "", scope()).unwrap_err(),
            EventCreationError::EmptyEventType
        );

        assert_eq!(
            Event::new(EventId::new("event-2").unwrap(), "   ", scope(),).unwrap_err(),
            EventCreationError::EmptyEventType
        );
    }

    #[test]
    fn event_is_immutable_and_clone_preserves_metadata() {
        let event = Event::new(EventId::new("event-1").unwrap(), "test.event", scope()).unwrap();

        let cloned = event.clone();

        assert_eq!(cloned, event);
    }

    #[test]
    fn event_context_preserves_operation_and_security_context() {
        use crate::{
            identity::{CorrelationId, OperationId},
            operation::Operation,
            security::identity::{PrincipalId, PrincipalIdentity, PrincipalType},
        };

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ));

        let principal = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("producer-engine").unwrap(),
        );
        let security_context = SecurityContext::new(principal, None);

        let context = EventContext::empty()
            .with_operation_context(operation_context.clone())
            .with_security_context(security_context.clone());

        assert_eq!(context.operation_context(), Some(&operation_context));
        assert_eq!(context.security_context(), Some(&security_context));

        let event = Event::new_with_context(
            EventId::new("event-ctx-1").unwrap(),
            "test.event",
            scope(),
            context,
        )
        .unwrap();

        assert_eq!(
            event.context().operation_context(),
            Some(&operation_context)
        );
        assert_eq!(event.context().security_context(), Some(&security_context));
    }
}
