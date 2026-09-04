//! Shared execution context mechanisms for Nizaam engines.

pub mod background;
pub mod concurrency;
pub mod engine;
pub mod lifecycle;
pub mod pipeline;

use crate::{
    error::{ErrorContext, ErrorDefinition, GlobalError},
    operation::OperationContext,
    provenance::ProvenanceContext,
    security::SecurityContext,
};

pub use crate::operation::{CancellationToken, Deadline};

/// Shared context passed to capability and downstream execution.
#[derive(Clone, Debug)]
pub struct EngineContext {
    operation: OperationContext,
    cancellation: CancellationToken,
    deadline: Option<Deadline>,
    security: SecurityContext,
    provenance: ProvenanceContext,
}

pub(crate) fn check_context(context: &EngineContext) -> Result<(), pipeline::PipelineError> {
    if context.cancellation().is_cancelled() {
        return Err(pipeline::PipelineError::Cancelled);
    }
    if context.is_expired() {
        return Err(pipeline::PipelineError::DeadlineExpired);
    }
    Ok(())
}

impl EngineContext {
    /// Creates an execution context from trusted operation and platform context.
    pub fn new(operation: OperationContext) -> Self {
        Self {
            operation,
            cancellation: CancellationToken::new(),
            deadline: None,
            security: SecurityContext::new(),
            provenance: ProvenanceContext::new(),
        }
    }

    pub fn operation(&self) -> &OperationContext {
        &self.operation
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn deadline(&self) -> Option<Deadline> {
        self.deadline
    }

    pub fn security(&self) -> &SecurityContext {
        &self.security
    }

    pub fn provenance(&self) -> &ProvenanceContext {
        &self.provenance
    }

    pub fn with_deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = Some(
            self.deadline
                .map_or(deadline, |current| current.min_with(deadline)),
        );
        self
    }

    pub fn with_security(mut self, security: SecurityContext) -> Self {
        self.security = security;
        self
    }

    pub fn with_provenance(mut self, provenance: ProvenanceContext) -> Self {
        self.provenance = provenance;
        self
    }

    pub fn child(&self) -> Self {
        Self {
            operation: self.operation.clone(),
            cancellation: self.cancellation.child_token(),
            deadline: self.deadline,
            security: self.security.clone(),
            provenance: self.provenance.clone(),
        }
    }

    pub fn child_with_deadline(&self, deadline: Deadline) -> Self {
        let mut child = self.child();
        child.deadline = Some(
            self.deadline
                .map_or(deadline, |parent| parent.min_with(deadline)),
        );
        child
    }

    pub fn is_expired(&self) -> bool {
        self.deadline.is_some_and(Deadline::is_expired)
    }

    /// Translates an expired context into the shared technical error contract.
    pub fn expiration_error(&self, definition: &ErrorDefinition) -> Option<GlobalError> {
        self.is_expired().then(|| {
            GlobalError::from_definition(
                definition,
                ErrorContext::new(self.operation.clone()),
                None,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contracts::Version,
        error::{ErrorClass, ErrorCode, ErrorOwner, Severity},
        identity::{CorrelationId, OperationId},
        operation::{Operation, OperationContext},
        status::Retryability,
    };
    use std::time::Duration;

    #[test]
    fn child_context_preserves_trusted_context_and_parent_cancellation() {
        let operation = Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        );
        let parent = EngineContext::new(OperationContext::new(operation))
            .with_deadline(Deadline::from_now(Duration::from_secs(1)).unwrap())
            .with_security(SecurityContext::new())
            .with_provenance(ProvenanceContext::new());
        let child = parent.child();

        assert_eq!(
            child.operation().operation.id,
            parent.operation().operation.id
        );
        assert_eq!(child.security(), parent.security());
        assert_eq!(child.provenance(), parent.provenance());
        assert_eq!(child.deadline(), parent.deadline());

        parent.cancellation().cancel();

        assert!(child.cancellation().is_cancelled());
    }

    #[test]
    fn expired_context_translates_through_the_shared_error_contract() {
        let operation = Operation::new(
            OperationId::new("operation-2").unwrap(),
            CorrelationId::new("correlation-2").unwrap(),
        );
        let context = EngineContext::new(OperationContext::new(operation))
            .with_deadline(Deadline::from_now(Duration::ZERO).unwrap());
        let definition = ErrorDefinition::new(
            ErrorCode::new("CORE.EXECUTION.001").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Execution,
            Severity::Error,
            "Execution deadline expired",
            Retryability::NonRetryable,
        )
        .unwrap();

        let error = context.expiration_error(&definition).unwrap();

        assert_eq!(error.code.as_str(), "CORE.EXECUTION.001");
        assert_eq!(error.context.operation.operation.id.as_str(), "operation-2");
    }
}
