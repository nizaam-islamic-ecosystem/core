//! Universal logical-message occurrence representation.
//!
//! An Event is one interaction form of the common universal message boundary.
//! Requests, Responses, and Events all use `MessageEnvelope`, while their
//! interaction semantics remain explicit through `Interaction`.
//!
//! The occurrence boundary can wrap Request, Response, or Event interactions.
//!
//! Conceptually:
//!
//! ```text
//! Universal logical message
//!          │
//!          ├── Request
//!          ├── Response
//!          └── Event
//!                ├── EventId
//!                ├── EventName
//!                ├── EventType
//!                ├── Scope
//!                └── MessageEnvelope
//! ```
//!
//! `EventId` remains distinct from the envelope's `MessageId`.
//! `EventName` identifies the semantic event being communicated.
//! `EventType` remains distinct from `CapabilityId`.
//!
//! The event payload remains opaque to Core.
//!
//! The Phase 14 Event subsystem remains responsible for local event
//! publication, subscription, delivery, lifecycle, matching, ownership, and
//! cancellation. This contract representation does not turn the local Event
//! subsystem into a distributed broker or Control Plane transport mechanism.
//!
//! In particular:
//!
//! ```text
//! Common logical-message boundary
//!     ├── Request
//!     ├── Response
//!     └── Event
//!
//! Phase 14 Event subsystem
//!     └── local Event publication/subscription infrastructure
//!
//! Control Plane
//!     └── formal engine/platform communication
//! ```
//!
//! Therefore "event" at the common message boundary does not erase the
//! semantic distinction between Request, Response, and Event interactions.

use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::events::EventName;
use crate::identity::{EventId, MessageId};

/// Error returned when a message cannot be represented as a universal Event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UniversalEventError {
    /// An Event name must contain at least one non-whitespace character.
    EmptyEventName,

    /// An Event type must contain at least one non-whitespace character.
    EmptyEventType,

    /// An Event scope must contain at least one non-whitespace character.
    EmptyEventScope,
}

impl std::fmt::Display for UniversalEventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyEventName => formatter.write_str("universal event name must not be empty"),
            Self::EmptyEventType => formatter.write_str("universal event type must not be empty"),
            Self::EmptyEventScope => formatter.write_str("universal event scope must not be empty"),
        }
    }
}

impl std::error::Error for UniversalEventError {}

/// A universal logical message carrying Event interaction semantics.
///
/// `EventId` identifies the Event occurrence itself. The enclosing universal
/// message independently carries its `MessageId`.
///
/// `event_name`, `event_type`, and `scope` are explicit Event semantics and are
/// not inferred from the payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniversalEvent {
    /// The common logical-message envelope used for communication.
    pub envelope: MessageEnvelope,

    /// Identity of the Event occurrence represented by this message.
    pub event_id: EventId,

    /// Semantic Event name preserved across the contract boundary.
    pub event_name: EventName,

    /// Semantic Event type preserved across the contract boundary.
    pub event_type: Box<str>,

    /// Generic applicability scope preserved across the contract boundary.
    pub scope: Box<str>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct UniversalEventDeserialization {
    envelope: MessageEnvelope,
    event_id: EventId,
    event_name: String,
    event_type: Box<str>,
    scope: Box<str>,
}

impl serde::Serialize for UniversalEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("UniversalEvent", 5)?;
        state.serialize_field("envelope", &self.envelope)?;
        state.serialize_field("event_id", &self.event_id)?;
        state.serialize_field("event_name", self.event_name.as_str())?;
        state.serialize_field("event_type", &self.event_type)?;
        state.serialize_field("scope", &self.scope)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for UniversalEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let intermediate = UniversalEventDeserialization::deserialize(deserializer)?;

        Self::from_parts(
            intermediate.envelope,
            intermediate.event_id,
            intermediate.event_name,
            intermediate.event_type,
            intermediate.scope,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl UniversalEvent {
    /// Creates a universal occurrence wrapper from an existing message
    /// envelope and its semantic metadata.
    pub fn new(
        envelope: MessageEnvelope,
        event_name: impl Into<String>,
        event_type: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Self, UniversalEventError> {
        Self::from_parts(envelope, EventId::generate(), event_name, event_type, scope)
    }

    fn from_parts(
        envelope: MessageEnvelope,
        event_id: EventId,
        event_name: impl Into<String>,
        event_type: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Self, UniversalEventError> {
        let event_name =
            EventName::new(event_name).map_err(|_| UniversalEventError::EmptyEventName)?;

        let event_type = event_type.into();

        if event_type.trim().is_empty() {
            return Err(UniversalEventError::EmptyEventType);
        }

        let scope = scope.into();

        if scope.trim().is_empty() {
            return Err(UniversalEventError::EmptyEventScope);
        }

        Ok(Self {
            envelope,
            event_id,
            event_name,
            event_type: event_type.into_boxed_str(),
            scope: scope.into_boxed_str(),
        })
    }

    /// Returns whether this logical message has Event interaction semantics.
    pub fn has_event_interaction(&self) -> bool {
        self.envelope.metadata.descriptor.interaction == Interaction::Event
    }

    /// Returns the Event occurrence identity.
    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    /// Returns the logical message identity carried by the envelope.
    pub fn message_id(&self) -> &MessageId {
        &self.envelope.message_id
    }

    /// Returns the semantic Event name.
    pub fn event_name(&self) -> &EventName {
        &self.event_name
    }

    /// Returns the semantic Event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the generic Event scope.
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};

    fn make_envelope(interaction: Interaction) -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("test.event.contract").unwrap(),
            CapabilityId::new("events.read").unwrap(),
            Version::new(1, 0, 0),
            interaction,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let payload_descriptor = descriptor.payload.clone();

        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("sender").unwrap(),
                EngineId::new("receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new("msg-1").unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(payload_descriptor, b"event payload"),
        )
    }

    fn event() -> UniversalEvent {
        UniversalEvent::new(
            make_envelope(Interaction::Event),
            "operation.completed",
            "operation.completed",
            "engine:test",
        )
        .unwrap()
    }

    #[test]
    fn creates_universal_event_from_event_interaction() {
        let universal_event = event();

        assert!(universal_event.has_event_interaction());
        assert!(!universal_event.event_id().as_str().is_empty());
        assert_eq!(universal_event.message_id().as_str(), "msg-1");
        assert_eq!(universal_event.event_name().as_str(), "operation.completed");
        assert_eq!(universal_event.event_type(), "operation.completed");
        assert_eq!(universal_event.scope(), "engine:test");
    }

    #[test]
    fn preserves_request_interaction() {
        let universal_event = UniversalEvent::new(
            make_envelope(Interaction::Request),
            "universal.request",
            "request",
            "global",
        )
        .unwrap();

        assert!(!universal_event.has_event_interaction());
        assert_eq!(
            universal_event.envelope.metadata.descriptor.interaction,
            Interaction::Request
        );
    }

    #[test]
    fn preserves_response_interaction() {
        let universal_event = UniversalEvent::new(
            make_envelope(Interaction::Response),
            "universal.response",
            "response",
            "global",
        )
        .unwrap();

        assert!(!universal_event.has_event_interaction());
        assert_eq!(
            universal_event.envelope.metadata.descriptor.interaction,
            Interaction::Response
        );
    }

    #[test]
    fn rejects_empty_event_name() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                "   ",
                "operation.completed",
                "engine:test",
            ),
            Err(UniversalEventError::EmptyEventName)
        );
    }

    #[test]
    fn rejects_empty_event_type() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                "operation.completed",
                "   ",
                "engine:test",
            ),
            Err(UniversalEventError::EmptyEventType)
        );
    }

    #[test]
    fn rejects_empty_event_scope() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                "operation.completed",
                "operation.completed",
                "\t\n",
            ),
            Err(UniversalEventError::EmptyEventScope)
        );
    }

    #[test]
    fn generated_event_ids_are_automatic_and_distinct() {
        let first = event();
        let second = event();

        assert!(!first.event_id().as_str().is_empty());
        assert!(!second.event_id().as_str().is_empty());
        assert_ne!(first.event_id(), second.event_id());
    }

    #[test]
    fn preserves_event_and_message_identity_separately() {
        let universal_event = event();

        assert!(!universal_event.event_id().as_str().is_empty());
        assert_eq!(universal_event.message_id().as_str(), "msg-1");
        assert_ne!(
            universal_event.event_id().as_str(),
            universal_event.message_id().as_str()
        );
    }

    #[test]
    fn preserves_event_semantics_and_complete_envelope() {
        let universal_event = event();

        assert_eq!(universal_event.event_name().as_str(), "operation.completed");
        assert_eq!(universal_event.event_type(), "operation.completed");
        assert_eq!(universal_event.scope(), "engine:test");
        assert_eq!(
            universal_event
                .envelope
                .operation_context
                .operation
                .id
                .as_str(),
            "op-1"
        );
        assert_eq!(
            universal_event
                .envelope
                .operation_context
                .operation
                .correlation_id
                .as_str(),
            "corr-1"
        );
        assert_eq!(universal_event.envelope.payload.bytes(), b"event payload");
    }

    #[test]
    fn clone_preserves_event_and_envelope() {
        let universal_event = event();

        let cloned = universal_event.clone();

        assert_eq!(cloned, universal_event);
    }

    #[test]
    fn serializes_and_deserializes_valid_event() {
        let universal_event = event();

        let encoded = serde_json::to_string(&universal_event).unwrap();
        let decoded: UniversalEvent = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, universal_event);
    }

    #[test]
    fn deserialization_preserves_request_interaction_and_generated_identity() {
        let request = UniversalEventDeserialization {
            envelope: make_envelope(Interaction::Request),
            event_id: EventId::generate(),
            event_name: "universal.request".into(),
            event_type: "request".into(),
            scope: "global".into(),
        };

        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: UniversalEvent = serde_json::from_str(&encoded).unwrap();

        assert!(!decoded.has_event_interaction());
        assert_eq!(
            decoded.envelope.metadata.descriptor.interaction,
            Interaction::Request
        );
        assert_eq!(decoded.event_id(), &request.event_id);
    }
}
