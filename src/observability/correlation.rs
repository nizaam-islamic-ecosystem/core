//! Request- and operation-scoped correlation context for observability.
//!
//! This module provides a small observability-facing view over the existing
//! Core identities. It does not create replacement identifiers, maintain
//! global request state, or define a transport-specific propagation format.

use crate::identity::{CapabilityId, CorrelationId, EngineId, MessageId, OperationId};
use crate::operation::{Operation, OperationContext};

/// Correlation information that can be propagated with related activity.
///
/// `CorrelationId` is the primary correlation identity. The remaining fields
/// provide optional context about the message, logical operation, engine, or
/// capability involved in the activity.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CorrelationContext {
    correlation_id: CorrelationId,
    operation_id: Option<OperationId>,
    message_id: Option<MessageId>,
    engine_id: Option<EngineId>,
    capability_id: Option<CapabilityId>,
}

impl CorrelationContext {
    /// Creates a correlation context from an existing Core correlation ID.
    pub fn new(correlation_id: CorrelationId) -> Self {
        Self {
            correlation_id,
            operation_id: None,
            message_id: None,
            engine_id: None,
            capability_id: None,
        }
    }

    /// Creates a correlation context from an existing operation.
    pub fn from_operation(operation: &Operation) -> Self {
        Self::new(operation.correlation_id.clone()).with_operation_id(operation.id.clone())
    }

    /// Creates a correlation context from trusted operation execution context.
    pub fn from_operation_context(context: &OperationContext) -> Self {
        Self::from_operation(&context.operation)
    }

    /// Returns the existing correlation identifier.
    pub fn correlation_id(&self) -> &CorrelationId {
        &self.correlation_id
    }

    /// Returns the associated operation identifier, when available.
    pub fn operation_id(&self) -> Option<&OperationId> {
        self.operation_id.as_ref()
    }

    /// Returns the associated message identifier, when available.
    pub fn message_id(&self) -> Option<&MessageId> {
        self.message_id.as_ref()
    }

    /// Returns the associated engine identifier, when available.
    pub fn engine_id(&self) -> Option<&EngineId> {
        self.engine_id.as_ref()
    }

    /// Returns the associated capability identifier, when available.
    pub fn capability_id(&self) -> Option<&CapabilityId> {
        self.capability_id.as_ref()
    }

    /// Derives a context with an existing operation identifier attached.
    pub fn with_operation_id(mut self, operation_id: OperationId) -> Self {
        self.operation_id = Some(operation_id);
        self
    }

    /// Derives a context with an existing message identifier attached.
    pub fn with_message_id(mut self, message_id: MessageId) -> Self {
        self.message_id = Some(message_id);
        self
    }

    /// Derives a context with an existing engine identifier attached.
    pub fn with_engine_id(mut self, engine_id: EngineId) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    /// Derives a context with an existing capability identifier attached.
    pub fn with_capability_id(mut self, capability_id: CapabilityId) -> Self {
        self.capability_id = Some(capability_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correlation_id(value: &str) -> CorrelationId {
        CorrelationId::new(value).unwrap()
    }

    #[test]
    fn new_context_preserves_correlation_id() {
        let context = CorrelationContext::new(correlation_id("corr-1"));

        assert_eq!(context.correlation_id().as_str(), "corr-1");
        assert!(context.operation_id().is_none());
        assert!(context.message_id().is_none());
        assert!(context.engine_id().is_none());
        assert!(context.capability_id().is_none());
    }

    #[test]
    fn enrichment_preserves_correlation_id() {
        let context = CorrelationContext::new(correlation_id("corr-2"))
            .with_operation_id(OperationId::new("op-2").unwrap())
            .with_message_id(MessageId::new("msg-2").unwrap())
            .with_engine_id(EngineId::new("engine-2").unwrap())
            .with_capability_id(CapabilityId::new("cap-2").unwrap());

        assert_eq!(context.correlation_id().as_str(), "corr-2");
        assert_eq!(context.operation_id().unwrap().as_str(), "op-2");
        assert_eq!(context.message_id().unwrap().as_str(), "msg-2");
        assert_eq!(context.engine_id().unwrap().as_str(), "engine-2");
        assert_eq!(context.capability_id().unwrap().as_str(), "cap-2");
    }

    #[test]
    fn derivation_does_not_mutate_original_context() {
        let original = CorrelationContext::new(correlation_id("corr-3"));
        let derived = original
            .clone()
            .with_operation_id(OperationId::new("op-3").unwrap());

        assert!(original.operation_id().is_none());
        assert_eq!(derived.operation_id().unwrap().as_str(), "op-3");
        assert_eq!(original.correlation_id().as_str(), "corr-3");
    }

    #[test]
    fn from_operation_preserves_existing_core_identities() {
        let operation = Operation::new(
            OperationId::new("op-4").unwrap(),
            CorrelationId::new("corr-4").unwrap(),
        );

        let context = CorrelationContext::from_operation(&operation);

        assert_eq!(context.correlation_id().as_str(), "corr-4");
        assert_eq!(context.operation_id().unwrap().as_str(), "op-4");
    }

    #[test]
    fn from_operation_context_uses_trusted_operation_context() {
        let operation = Operation::new(
            OperationId::new("op-5").unwrap(),
            CorrelationId::new("corr-5").unwrap(),
        );
        let operation_context = OperationContext::new(operation);

        let correlation = CorrelationContext::from_operation_context(&operation_context);

        assert_eq!(correlation.correlation_id().as_str(), "corr-5");
        assert_eq!(correlation.operation_id().unwrap().as_str(), "op-5");
    }

    #[test]
    fn serialization_round_trip_preserves_context() {
        let original = CorrelationContext::new(correlation_id("corr-6"))
            .with_operation_id(OperationId::new("op-6").unwrap())
            .with_message_id(MessageId::new("msg-6").unwrap())
            .with_engine_id(EngineId::new("engine-6").unwrap())
            .with_capability_id(CapabilityId::new("cap-6").unwrap());

        let encoded = serde_json::to_string(&original).unwrap();
        let decoded: CorrelationContext = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, original);
    }

    #[test]
    fn independent_contexts_remain_isolated() {
        let first = CorrelationContext::new(correlation_id("corr-a"))
            .with_operation_id(OperationId::new("op-a").unwrap());
        let second = CorrelationContext::new(correlation_id("corr-b"))
            .with_operation_id(OperationId::new("op-b").unwrap());

        assert_eq!(first.correlation_id().as_str(), "corr-a");
        assert_eq!(first.operation_id().unwrap().as_str(), "op-a");
        assert_eq!(second.correlation_id().as_str(), "corr-b");
        assert_eq!(second.operation_id().unwrap().as_str(), "op-b");
    }
}
