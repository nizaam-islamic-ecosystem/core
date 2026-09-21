//! Observability Event backed by the common Universal Event contract.
//!
//! Observability owns its domain-specific event construction while reusing
//! `UniversalEvent` as the canonical Event representation. Callers provide
//! observability inputs; the universal envelope and Event metadata are built
//! internally.

use crate::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use crate::contracts::envelope::MessageEnvelope;
use crate::contracts::metadata::{ContractMetadata, Participants};
use crate::contracts::{UniversalEvent, UniversalEventError};
use crate::events::EventName;
use crate::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EventId, MessageId, OperationId,
};
use crate::operation::OperationContext;

/// An observability event that owns the canonical universal Event contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservabilityEvent {
    event: UniversalEvent,
}

impl ObservabilityEvent {
    /// Creates an observability event from observability-domain inputs.
    ///
    /// The common `UniversalEvent` and `MessageEnvelope` are constructed
    /// internally so callers do not need to assemble the universal contract.
    pub fn new(
        event_id: EventId,
        event_name: EventName,
        event_type: impl Into<String>,
        scope: impl Into<String>,
        operation_context: OperationContext,
    ) -> Result<Self, UniversalEventError> {
        let event_type = event_type.into();
        let scope = scope.into();

        let message_id = MessageId::new(format!("observability-message-{event_id}"))
            .expect("derived observability message ids are non-empty");

        let descriptor = ContractDescriptor::new(
            ContractId::new("observability.event")
                .expect("static observability contract id is valid"),
            CapabilityId::new("observability.emit")
                .expect("static observability capability id is valid"),
            Version::new(1, 0, 0),
            Interaction::Event,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
                .expect("static observability payload descriptor is valid"),
        );

        let payload_descriptor = descriptor.payload.clone();
        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("observability").expect("static observability engine id is valid"),
                EngineId::new("observability-system")
                    .expect("static observability-system engine id is valid"),
            ),
        );

        let envelope = MessageEnvelope::new(
            message_id,
            operation_context,
            metadata,
            EncodedPayload::new(payload_descriptor, vec![0]),
        );

        let event = UniversalEvent::from_parts(
            envelope,
            event_id,
            event_name.to_string(),
            event_type,
            scope,
        )?;

        Ok(Self { event })
    }

    /// Returns the complete universal Event contract.
    pub fn universal_event(&self) -> &UniversalEvent {
        &self.event
    }

    /// Returns the Event occurrence identity.
    pub fn event_id(&self) -> &EventId {
        self.event.event_id()
    }

    /// Returns the logical message identity.
    pub fn message_id(&self) -> &MessageId {
        self.event.message_id()
    }

    /// Returns the semantic Event name.
    pub fn event_name(&self) -> &EventName {
        self.event.event_name()
    }

    /// Returns the semantic Event type.
    pub fn event_type(&self) -> &str {
        self.event.event_type()
    }

    /// Returns the Event scope.
    pub fn event_scope(&self) -> &str {
        self.event.scope()
    }

    /// Returns the operation context carried by the universal message.
    pub fn operation_context(&self) -> &OperationContext {
        &self.event.envelope.operation_context
    }

    /// Returns the operation identity carried by the universal message.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_context().operation.id
    }

    /// Returns the correlation identity carried by the universal message.
    pub fn correlation_id(&self) -> &CorrelationId {
        &self.operation_context().operation.correlation_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::{Operation, OperationContext};

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ))
    }

    fn event(event_id: &str) -> ObservabilityEvent {
        ObservabilityEvent::new(
            EventId::new(event_id).unwrap(),
            EventName::new("observability.event").unwrap(),
            "observability",
            "global",
            operation_context(),
        )
        .unwrap()
    }

    #[test]
    fn constructs_the_universal_event_internally() {
        let event = event("event-1");

        assert!(!event.event_id().as_str().is_empty());
        assert_eq!(event.event_id().as_str(), "event-1");
        assert_eq!(event.message_id().as_str(), "observability-message-event-1");
        assert_eq!(event.event_name().as_str(), "observability.event");
        assert_eq!(event.event_type(), "observability");
        assert_eq!(event.event_scope(), "global");
    }

    #[test]
    fn exposes_operation_and_correlation_identity() {
        let event = event("event-2");

        assert_eq!(event.operation_id().as_str(), "operation-1");
        assert_eq!(event.correlation_id().as_str(), "correlation-1");
    }

    #[test]
    fn exposes_the_complete_universal_event() {
        let event = event("event-3");

        assert_eq!(event.universal_event().event_id(), event.event_id());
        assert_eq!(event.universal_event().event_name(), event.event_name());
        assert_eq!(event.universal_event().event_type(), event.event_type());
        assert_eq!(event.universal_event().scope(), event.event_scope());
    }

    #[test]
    fn rejects_empty_event_type_without_panicking() {
        let result = ObservabilityEvent::new(
            EventId::new("event-empty-type").unwrap(),
            EventName::new("diagnostic.created").unwrap(),
            "   ",
            "global",
            operation_context(),
        );

        assert_eq!(result, Err(UniversalEventError::EmptyEventType));
    }

    #[test]
    fn rejects_empty_event_scope_without_panicking() {
        let result = ObservabilityEvent::new(
            EventId::new("event-empty-scope").unwrap(),
            EventName::new("diagnostic.created").unwrap(),
            "diagnostic",
            " \t",
            operation_context(),
        );

        assert_eq!(result, Err(UniversalEventError::EmptyEventScope));
    }

    #[test]
    fn preserves_domain_inputs_in_the_universal_event() {
        let event = ObservabilityEvent::new(
            EventId::new("event-4").unwrap(),
            EventName::new("diagnostic.created").unwrap(),
            "diagnostic",
            "global",
            operation_context(),
        )
        .unwrap();

        assert_eq!(event.event_name().as_str(), "diagnostic.created");
        assert_eq!(event.event_type(), "diagnostic");
        assert_eq!(event.event_scope(), "global");
    }
}
