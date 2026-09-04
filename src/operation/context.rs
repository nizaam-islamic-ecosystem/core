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
}
