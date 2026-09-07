//! Operation identity and context foundations.

use crate::identity::{CorrelationId, OperationId, PlanId};

pub mod cancellation;
mod context;
pub mod deadline;

pub use cancellation::CancellationToken;
pub use deadline::Deadline;

pub use context::OperationContext;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::CorrelationId;

    fn sample_operation() -> Operation {
        Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        )
    }

    #[test]
    fn operation_starts_without_plan_or_parent() {
        let op = sample_operation();

        assert_eq!(op.id.as_str(), "op-1");
        assert_eq!(op.correlation_id.as_str(), "corr-1");
        assert!(op.plan_id.is_none());
        assert!(op.parent_operation_id.is_none());
    }

    #[test]
    fn with_plan_sets_plan_id() {
        let op = sample_operation().with_plan(PlanId::new("plan-7").unwrap());

        assert_eq!(op.plan_id.unwrap().as_str(), "plan-7");
        assert!(op.parent_operation_id.is_none());
    }

    #[test]
    fn with_parent_sets_parent_operation_id() {
        let op = sample_operation().with_parent(OperationId::new("parent-op").unwrap());

        assert_eq!(op.parent_operation_id.unwrap().as_str(), "parent-op");
        assert!(op.plan_id.is_none());
    }

    #[test]
    fn builder_methods_chain() {
        let op = sample_operation()
            .with_plan(PlanId::new("plan-x").unwrap())
            .with_parent(OperationId::new("parent-y").unwrap());

        assert_eq!(op.plan_id.unwrap().as_str(), "plan-x");
        assert_eq!(op.parent_operation_id.unwrap().as_str(), "parent-y");
    }

    #[test]
    fn operation_derives_clone_eq() {
        let op1 = sample_operation().with_plan(PlanId::new("p").unwrap());
        let op2 = op1.clone();

        assert_eq!(op1, op2);
    }
}
