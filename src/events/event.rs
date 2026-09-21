//! Immutable internal event occurrence representation for Nizaam Core.
//!
//! An `Event` identifies one internal event occurrence together with the
//! generic metadata required for publisher matching. Event semantics remain
//! owned by the producer; Core stores the identity, name, category, scope,
//! trusted context, and eventually the logical message envelope associated
//! with the occurrence.

use super::scope::Scope;
use crate::{identity::EventId, operation::OperationContext, security::SecurityContext};

/// The semantic name of an Event occurrence.
///
/// `EventId` identifies one concrete occurrence, while `EventName` describes
/// what happened. Multiple distinct Event occurrences may therefore have the
/// same `EventName`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventName(Box<str>);

/// Error returned when an Event name is empty or consists only of whitespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidEventName;

impl std::fmt::Display for InvalidEventName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an event name must not be empty")
    }
}

impl std::error::Error for InvalidEventName {}

impl EventName {
    /// Creates an Event name from a non-empty value.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidEventName> {
        let value = value.into();

        if value.trim().is_empty() {
            return Err(InvalidEventName);
        }

        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the underlying Event name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for EventName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for EventName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for EventName {
    type Err = InvalidEventName;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for EventName {
    type Error = InvalidEventName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

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
    /// An event name must contain at least one non-whitespace character.
    EmptyEventName,

    /// An event type must contain at least one non-whitespace character.
    EmptyEventType,
}

impl std::fmt::Display for EventCreationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyEventName => formatter.write_str("event name must not be empty"),
            Self::EmptyEventType => formatter.write_str("event type must not be empty"),
        }
    }
}

impl std::error::Error for EventCreationError {}

/// One immutable internal Event occurrence.
///
/// `EventId` identifies the occurrence itself. `event_name` identifies what
/// happened, `event_type` identifies its semantic category, while `scope`
/// identifies the explicit generic applicability boundary used for
/// subscription matching.
///
/// The event carries only generic Core semantics. Domain-specific payload
/// meaning belongs to the logical message envelope associated with the Event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    event_id: EventId,
    event_name: EventName,
    event_type: Box<str>,
    scope: Scope,
    context: EventContext,
}

impl Event {
    /// Creates a fully described immutable Event occurrence.
    pub fn new(
        event_id: EventId,
        event_name: EventName,
        event_type: impl Into<String>,
        scope: Scope,
    ) -> Result<Self, EventCreationError> {
        Self::new_with_context(
            event_id,
            event_name,
            event_type,
            scope,
            EventContext::empty(),
        )
    }

    /// Creates an immutable Event occurrence with trusted propagated context.
    pub fn new_with_context(
        event_id: EventId,
        event_name: EventName,
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
            event_name,
            event_type: event_type.into_boxed_str(),
            scope,
            context,
        })
    }

    /// Returns the identity of this Event occurrence.
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    /// Returns the semantic name of this Event occurrence.
    pub fn event_name(&self) -> &EventName {
        &self.event_name
    }

    /// Returns the semantic Event type used for generic subscription matching.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the explicit generic Event scope.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Returns the minimal trusted context propagated with this Event.
    pub fn context(&self) -> &EventContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::EventId;

    fn event_name() -> EventName {
        EventName::new("engine.started").unwrap()
    }

    fn scope() -> Scope {
        Scope::new("engine:test").unwrap()
    }

    #[test]
    fn event_name_constructs_and_rejects_empty_values() {
        let name = EventName::new("engine.started").unwrap();

        assert_eq!(name.as_str(), "engine.started");
        assert_eq!(name.to_string(), "engine.started");
        assert!(EventName::new("").is_err());
        assert!(EventName::new("   ").is_err());
    }

    #[test]
    fn event_name_supports_standard_string_conversions() {
        use std::str::FromStr;

        let name = EventName::from_str("engine.started").unwrap();

        assert_eq!(name.as_ref(), "engine.started");
        assert_eq!(
            EventName::try_from(String::from("engine.started"))
                .unwrap()
                .as_str(),
            "engine.started"
        );
    }

    #[test]
    fn event_name_error_has_stable_message() {
        assert_eq!(
            InvalidEventName.to_string(),
            "an event name must not be empty"
        );
    }

    #[test]
    fn event_constructs_with_identity_name_type_and_scope() {
        let event = Event::new(
            EventId::new("event-1").unwrap(),
            event_name(),
            "lifecycle",
            scope(),
        )
        .unwrap();

        assert_eq!(event.event_id().as_str(), "event-1");
        assert_eq!(event.event_name().as_str(), "engine.started");
        assert_eq!(event.event_type(), "lifecycle");
        assert_eq!(event.scope().as_str(), "engine:test");
    }

    #[test]
    fn event_rejects_empty_event_type() {
        assert_eq!(
            Event::new(EventId::new("event-1").unwrap(), event_name(), "", scope(),).unwrap_err(),
            EventCreationError::EmptyEventType
        );

        assert_eq!(
            Event::new(
                EventId::new("event-2").unwrap(),
                event_name(),
                "   ",
                scope(),
            )
            .unwrap_err(),
            EventCreationError::EmptyEventType
        );
    }

    #[test]
    fn event_name_and_event_id_remain_distinct() {
        let first = Event::new(
            EventId::new("event-1").unwrap(),
            EventName::new("engine.started").unwrap(),
            "lifecycle",
            scope(),
        )
        .unwrap();

        let second = Event::new(
            EventId::new("event-2").unwrap(),
            EventName::new("engine.started").unwrap(),
            "lifecycle",
            scope(),
        )
        .unwrap();

        assert_ne!(first.event_id(), second.event_id());
        assert_eq!(first.event_name(), second.event_name());
    }

    #[test]
    fn event_is_immutable_and_clone_preserves_metadata() {
        let event = Event::new(
            EventId::new("event-1").unwrap(),
            event_name(),
            "lifecycle",
            scope(),
        )
        .unwrap();

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
            event_name(),
            "lifecycle",
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
