//! Operation identity and context foundations.

use crate::identity::{AttemptId, CorrelationId, NodeId, OperationId, PlanId};

/// The stable platform identity of work that may span many messages and attempts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Operation {
    pub id: OperationId,
    pub correlation_id: CorrelationId,
    pub plan_id: Option<PlanId>,
    pub parent_operation_id: Option<OperationId>,
}

impl Operation {
    pub fn new(id: OperationId, correlation_id: CorrelationId) -> Self {
        Self {
            id,
            correlation_id,
            plan_id: None,
            parent_operation_id: None,
        }
    }

    pub fn with_plan(mut self, plan_id: PlanId) -> Self {
        self.plan_id = Some(plan_id);
        self
    }

    pub fn with_parent(mut self, parent_operation_id: OperationId) -> Self {
        self.parent_operation_id = Some(parent_operation_id);
        self
    }
}

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
