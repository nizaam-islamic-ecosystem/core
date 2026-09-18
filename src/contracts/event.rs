//! Universal contract representation for internal Event messages.
//!
//! `UniversalEvent` is the contract-layer representation of an Event when the
//! event participates in the common logical-message boundary. It reuses the
//! existing [`MessageEnvelope`] rather than introducing a second envelope or
//! transport representation.
//!
//! The Event subsystem remains responsible for local event publication,
//! subscription, delivery, lifecycle, and matching. This type preserves the
//! Event-specific semantic metadata that must survive the communication
//! boundary:
//!
//! ```text
//! UniversalEvent
//! ├── EventId
//! ├── EventType
//! ├── Scope
//! └── MessageEnvelope
//! ```
//!
//! `EventId` remains distinct from the envelope's `MessageId`. Likewise,
//! `EventType` remains distinct from `CapabilityId`. The common
//! `ContractDescriptor` continues to provide the existing communication
//! contract metadata without being overloaded to represent EventType.
//!
//! Event payload meaning remains opaque to Core.

use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::identity::{EventId, MessageId};

/// Error returned when a message cannot be represented as a universal Event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UniversalEventError {
    /// The supplied envelope does not represent an Event interaction.
    WrongInteraction(Interaction),

    /// An Event type must contain at least one non-whitespace character.
    EmptyEventType,

    /// An Event scope must contain at least one non-whitespace character.
    EmptyEventScope,
}

impl std::fmt::Display for UniversalEventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongInteraction(interaction) => {
                write!(
                    formatter,
                    "universal event requires Interaction::Event, found {interaction:?}"
                )
            }
            Self::EmptyEventType => formatter.write_str("universal event type must not be empty"),
            Self::EmptyEventScope => formatter.write_str("universal event scope must not be empty"),
        }
    }
}

impl std::error::Error for UniversalEventError {}

/// A universal Event message carried by the common [`MessageEnvelope`].
///
/// `EventId` identifies the Event occurrence itself. The envelope independently
/// carries its `MessageId`, so the two identities remain semantically distinct.
///
/// `event_type` and `scope` are preserved explicitly because they are Event
/// semantics rather than Request/Response capability metadata.
///
/// The event payload remains opaque and is owned/interpreted by the producer or
/// receiving engine. This type does not inspect domain-specific payload content.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct UniversalEvent {
    /// The common logical-message envelope used for communication.
    pub envelope: MessageEnvelope,

    /// Identity of the Event occurrence represented by this message.
    pub event_id: EventId,

    /// Semantic Event type preserved across the contract boundary.
    pub event_type: Box<str>,

    /// Generic applicability scope preserved across the contract boundary.
    pub scope: Box<str>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct UniversalEventDeserialization {
    envelope: MessageEnvelope,
    event_id: EventId,
    event_type: Box<str>,
    scope: Box<str>,
}

impl<'de> serde::Deserialize<'de> for UniversalEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let intermediate = UniversalEventDeserialization::deserialize(deserializer)?;

        Self::new(
            intermediate.envelope,
            intermediate.event_id,
            intermediate.event_type,
            intermediate.scope,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl UniversalEvent {
    /// Creates a universal Event from an existing message envelope and its
    /// Event-specific semantic metadata.
    ///
    /// Construction succeeds only when the envelope's contract descriptor
    /// declares `Interaction::Event`.
    pub fn new(
        envelope: MessageEnvelope,
        event_id: EventId,
        event_type: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Self, UniversalEventError> {
        let interaction = envelope.metadata.descriptor.interaction;

        if interaction != Interaction::Event {
            return Err(UniversalEventError::WrongInteraction(interaction));
        }

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
            event_type: event_type.into_boxed_str(),
            scope: scope.into_boxed_str(),
        })
    }

    /// Returns whether this universal message has Event interaction semantics.
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
            EventId::new("event-1").unwrap(),
            "operation.completed",
            "engine:test",
        )
        .unwrap()
    }

    #[test]
    fn creates_universal_event_from_event_interaction() {
        let universal_event = event();

        assert!(universal_event.has_event_interaction());
        assert_eq!(universal_event.event_id().as_str(), "event-1");
        assert_eq!(universal_event.message_id().as_str(), "msg-1");
        assert_eq!(universal_event.event_type(), "operation.completed");
        assert_eq!(universal_event.scope(), "engine:test");
    }

    #[test]
    fn rejects_request_interaction() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Request),
                EventId::new("event-1").unwrap(),
                "operation.completed",
                "engine:test",
            ),
            Err(UniversalEventError::WrongInteraction(Interaction::Request))
        );
    }

    #[test]
    fn rejects_response_interaction() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Response),
                EventId::new("event-1").unwrap(),
                "operation.completed",
                "engine:test",
            ),
            Err(UniversalEventError::WrongInteraction(Interaction::Response))
        );
    }

    #[test]
    fn rejects_empty_event_type() {
        assert_eq!(
            UniversalEvent::new(
                make_envelope(Interaction::Event),
                EventId::new("event-1").unwrap(),
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
                EventId::new("event-1").unwrap(),
                "operation.completed",
                "\t\n",
            ),
            Err(UniversalEventError::EmptyEventScope)
        );
    }

    #[test]
    fn preserves_event_and_message_identity_separately() {
        let universal_event = event();

        assert_eq!(universal_event.event_id().as_str(), "event-1");
        assert_eq!(universal_event.message_id().as_str(), "msg-1");
        assert_ne!(
            universal_event.event_id().as_str(),
            universal_event.message_id().as_str()
        );
    }

    #[test]
    fn preserves_event_semantics_and_complete_envelope() {
        let universal_event = event();

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
    fn deserialization_rejects_non_event_interaction() {
        let request = UniversalEventDeserialization {
            envelope: make_envelope(Interaction::Request),
            event_id: EventId::new("event-1").unwrap(),
            event_type: "operation.completed".into(),
            scope: "engine:test".into(),
        };

        let encoded = serde_json::to_string(&request).unwrap();
        let result: Result<UniversalEvent, _> = serde_json::from_str(&encoded);

        assert!(result.is_err());
    }
}
