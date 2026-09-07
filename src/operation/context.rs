use super::Operation;
use crate::identity::{AttemptId, NodeId};

/// Per execution context derived from a trusted operation, never reconstructed
/// from raw transport metadata by an engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationContext {
    pub operation: Operation,
    pub node_id: Option<NodeId>,
    pub attempt_id: Option<AttemptId>,
}

impl OperationContext {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            node_id: None,
            attempt_id: None,
        }
    }

    pub fn for_attempt(mut self, node_id: NodeId, attempt_id: AttemptId) -> Self {
        self.node_id = Some(node_id);
        self.attempt_id = Some(attempt_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CorrelationId, OperationId};

    #[test]
    fn operation_context_preserves_operation_and_attempt_identity() {
        let operation = Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        );
        let context = OperationContext::new(operation).for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-1").unwrap(),
        );

        assert_eq!(context.operation.id.as_str(), "operation-1");
        assert_eq!(context.attempt_id.unwrap().as_str(), "attempt-1");
    }

    #[test]
    fn new_context_has_no_node_or_attempt() {
        let operation = Operation::new(
            OperationId::new("operation-2").unwrap(),
            CorrelationId::new("correlation-2").unwrap(),
        );
        let context = OperationContext::new(operation);

        assert!(context.node_id.is_none());
        assert!(context.attempt_id.is_none());
    }

    #[test]
    fn operation_context_supports_clone_and_eq() {
        let operation = Operation::new(
            OperationId::new("operation-3").unwrap(),
            CorrelationId::new("correlation-3").unwrap(),
        );
        let original = OperationContext::new(operation);
        let clone = original.clone();

        assert_eq!(original, clone);
    }

    #[test]
    fn for_attempt_builder_chains_and_sets_both_fields() {
        let operation = Operation::new(
            OperationId::new("operation-4").unwrap(),
            CorrelationId::new("correlation-4").unwrap(),
        );
        let context = OperationContext::new(operation).for_attempt(
            NodeId::new("node-4").unwrap(),
            AttemptId::new("attempt-4").unwrap(),
        );

        assert_eq!(context.node_id.as_ref().unwrap().as_str(), "node-4");
        assert_eq!(context.attempt_id.as_ref().unwrap().as_str(), "attempt-4");
    }
}
