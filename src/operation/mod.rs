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
