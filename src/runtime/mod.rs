//! Shared execution context mechanisms for Nizaam engines.

pub mod background;
pub mod concurrency;
pub mod engine;
pub mod lifecycle;
pub mod pipeline;
pub mod task;

use std::sync::Arc;

use crate::{
    config::snapshot::ConfigurationSnapshot,
    error::{ErrorContext, ErrorDefinition, GlobalError},
    identity::{AttemptId, NodeId},
    operation::OperationContext,
    provenance::ProvenanceContext,
    security::SecurityContext,
};

pub use crate::operation::{CancellationToken, Deadline};

pub use background::{BackgroundTasks, BoundedSpawnError, SpawnError};
pub use concurrency::{ConcurrencyConfig, ConcurrencyError, ConcurrencyState, TaskScope};
pub use engine::EngineRuntime;
pub use lifecycle::{Lifecycle, LifecycleState};
pub use pipeline::{ExecutionPipeline, PipelineError, PipelineStage};
pub use task::{
    Task, TaskCriticality, TaskId, TaskLifecycle, TaskLifecycleError, TaskLifecycleState, TaskOwner,
};

/// Shared context passed to capability and downstream execution.
///
/// The security context is absent until authentication succeeds at the
/// mandatory middleware boundary.
#[derive(Clone, Debug)]
pub struct EngineContext {
    operation: OperationContext,
    cancellation: CancellationToken,
    deadline: Option<Deadline>,
    security: Option<SecurityContext>,
    provenance: ProvenanceContext,
    configuration: Option<Arc<ConfigurationSnapshot>>,
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
    ///
    /// Authentication has not happened yet, so no security identity is
    /// attached to the context at construction time.
    pub fn new(operation: OperationContext) -> Self {
        Self {
            operation,
            cancellation: CancellationToken::new(),
            deadline: None,
            security: None,
            provenance: ProvenanceContext::new(),
            configuration: None,
        }
    }

    /// Creates an execution context for a concrete retry attempt of the same
    /// logical operation.
    ///
    /// The operation context receives the supplied node and attempt identity,
    /// while the operation-level execution context remains unchanged:
    ///
    /// - the same logical `OperationId` is preserved;
    /// - the same cancellation authority is preserved;
    /// - the same operation deadline is preserved;
    /// - the same authenticated security context is preserved;
    /// - the same provenance context is preserved;
    /// - the same immutable configuration snapshot is preserved.
    ///
    /// A retry is therefore represented as a new attempt of the existing
    /// operation rather than as a new logical operation or an independently
    /// cancellable child context.
    pub fn for_attempt(&self, node_id: NodeId, attempt_id: AttemptId) -> Self {
        Self {
            operation: self.operation.clone().for_attempt(node_id, attempt_id),
            cancellation: self.cancellation.clone(),
            deadline: self.deadline,
            security: self.security.clone(),
            provenance: self.provenance.clone(),
            configuration: self.configuration.clone(),
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

    /// Returns the trusted security context when authentication has succeeded.
    pub fn security(&self) -> Option<&SecurityContext> {
        self.security.as_ref()
    }

    pub fn provenance(&self) -> &ProvenanceContext {
        &self.provenance
    }

    /// Returns the immutable configuration snapshot attached to this context.
    pub fn configuration(&self) -> Option<&ConfigurationSnapshot> {
        self.configuration.as_deref()
    }

    /// Attaches a shared immutable configuration snapshot to this context.
    pub fn with_configuration(mut self, configuration: Arc<ConfigurationSnapshot>) -> Self {
        self.configuration = Some(configuration);
        self
    }

    pub fn with_deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = Some(
            self.deadline
                .map_or(deadline, |current| current.min_with(deadline)),
        );
        self
    }

    /// Attaches the trusted security context established by authentication.
    pub fn with_security(mut self, security: SecurityContext) -> Self {
        self.security = Some(security);
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
            configuration: self.configuration.clone(),
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
        config::{
            resolution::ConfigurationResolver,
            snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
            validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
        },
        contracts::Version,
        error::{ErrorClass, ErrorCode, ErrorOwner, Severity},
        identity::{AttemptId, CorrelationId, NodeId, OperationId},
        operation::{Operation, OperationContext},
        security::{PrincipalId, PrincipalIdentity, PrincipalType},
        status::Retryability,
    };
    use std::time::Duration;

    fn sample_operation() -> Operation {
        Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        )
    }

    fn sample_configuration_snapshot() -> Arc<ConfigurationSnapshot> {
        let mut parsed = ParsedConfiguration::empty();
        parsed.insert("runtime.test", ConfigurationValue::Boolean(true));

        let validated = ConfigurationValidator::new()
            .validate(parsed)
            .expect("test configuration should be valid");
        let resolved = ConfigurationResolver::new()
            .resolve(&validated)
            .expect("test configuration should resolve");

        Arc::new(ConfigurationSnapshot::new(
            ConfigurationSnapshotId::new(1),
            resolved,
        ))
    }

    fn sample_context() -> EngineContext {
        EngineContext::new(OperationContext::new(sample_operation()))
    }

    fn sample_security_context() -> SecurityContext {
        let principal =
            PrincipalIdentity::new(PrincipalType::User, PrincipalId::new("user-1").unwrap());

        SecurityContext::new(principal, None)
    }

    #[test]
    fn new_context_has_no_deadline_and_active_cancellation() {
        let context = sample_context();

        assert!(context.deadline().is_none());
        assert!(!context.cancellation().is_cancelled());
        assert!(!context.is_expired());
        assert!(context.security().is_none());
        assert!(context.configuration().is_none());
    }

    #[test]
    fn accessors_return_consistent_views() {
        let context = sample_context();
        let operation = sample_operation();

        assert_eq!(context.operation().operation.id, operation.id);
        assert_eq!(
            context.operation().operation.correlation_id,
            operation.correlation_id
        );
    }

    #[test]
    fn with_deadline_keeps_earlier_deadline() {
        // Set an earlier deadline first, then a later one.
        // with_deadline must keep the earlier of the two.
        let earlier = Deadline::from_now(Duration::from_secs(10)).unwrap();
        let later = Deadline::from_now(Duration::from_secs(60)).unwrap();

        let context = sample_context().with_deadline(earlier).with_deadline(later);

        let deadline = context.deadline().unwrap();

        // The result must be the earlier deadline (10s), not the later one.
        assert_eq!(deadline, earlier);
        assert!(deadline.remaining() <= Duration::from_secs(10));
    }

    #[test]
    fn with_security_attaches_security_context() {
        let security = sample_security_context();
        let context = sample_context().with_security(security.clone());

        assert_eq!(context.security(), Some(&security));
    }

    #[test]
    fn with_provenance_replaces_provenance_context() {
        let provenance = ProvenanceContext::new().with_attribute("k", "v");
        let context = sample_context().with_provenance(provenance.clone());

        assert_eq!(context.provenance(), &provenance);
        assert_eq!(context.provenance().attribute("k"), Some("v"));
    }

    #[test]
    fn with_configuration_attaches_immutable_snapshot() {
        let configuration = sample_configuration_snapshot();
        let context = sample_context().with_configuration(configuration.clone());

        assert_eq!(context.configuration(), Some(configuration.as_ref()));
        assert_eq!(context.configuration().unwrap().id().value(), 1);
        assert_eq!(
            context.configuration().unwrap().get("runtime.test"),
            Some(&ConfigurationValue::Boolean(true))
        );
    }

    #[test]
    fn child_context_preserves_trusted_context_and_parent_cancellation() {
        let operation = Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        );

        let security = sample_security_context();
        let configuration = sample_configuration_snapshot();

        let parent = EngineContext::new(OperationContext::new(operation))
            .with_deadline(Deadline::from_now(Duration::from_secs(1)).unwrap())
            .with_security(security.clone())
            .with_provenance(ProvenanceContext::new())
            .with_configuration(configuration);

        let child = parent.child();

        assert_eq!(
            child.operation().operation.id,
            parent.operation().operation.id
        );
        assert_eq!(child.security(), Some(&security));
        assert_eq!(child.provenance(), parent.provenance());
        assert_eq!(child.deadline(), parent.deadline());
        assert_eq!(child.configuration(), parent.configuration());

        parent.cancellation().cancel();

        assert!(child.cancellation().is_cancelled());
    }

    #[test]
    fn child_without_security_context_preserves_absence() {
        let parent = sample_context();
        let child = parent.child();

        assert!(parent.security().is_none());
        assert!(child.security().is_none());
    }

    #[test]
    fn child_with_deadline_keeps_the_earlier_parent_deadline() {
        let operation = Operation::new(
            OperationId::new("operation-cd-1").unwrap(),
            CorrelationId::new("correlation-cd-1").unwrap(),
        );

        let parent = EngineContext::new(OperationContext::new(operation))
            .with_deadline(Deadline::from_now(Duration::from_millis(50)).unwrap());

        let child =
            parent.child_with_deadline(Deadline::from_now(Duration::from_secs(10)).unwrap());

        assert!(child.deadline().unwrap().remaining() <= Duration::from_secs(1));
    }

    #[test]
    fn expiration_error_returns_none_when_not_expired() {
        let context =
            sample_context().with_deadline(Deadline::from_now(Duration::from_secs(60)).unwrap());

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

        assert!(context.expiration_error(&definition).is_none());
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

    #[test]
    fn for_attempt_preserves_operation_identity_and_attaches_attempt_identity() {
        let parent = sample_context();
        let node_id = NodeId::new("node-1").unwrap();
        let attempt_id = AttemptId::new("attempt-1").unwrap();

        let attempt = parent.for_attempt(node_id.clone(), attempt_id.clone());

        assert_eq!(
            attempt.operation().operation.id,
            parent.operation().operation.id
        );
        assert_eq!(attempt.operation().node_id, Some(node_id));
        assert_eq!(attempt.operation().attempt_id, Some(attempt_id));
    }

    #[test]
    fn multiple_attempt_contexts_share_the_same_operation_identity() {
        let parent = sample_context();

        let attempt_one = parent.for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-1").unwrap(),
        );
        let attempt_two = parent.for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-2").unwrap(),
        );

        assert_eq!(
            attempt_one.operation().operation.id,
            attempt_two.operation().operation.id
        );
        assert_ne!(
            attempt_one.operation().attempt_id,
            attempt_two.operation().attempt_id
        );
    }

    #[test]
    fn for_attempt_preserves_execution_context() {
        let security = sample_security_context();
        let configuration = sample_configuration_snapshot();
        let provenance = ProvenanceContext::new().with_attribute("k", "v");
        let deadline = Deadline::from_now(Duration::from_secs(60)).unwrap();

        let parent = sample_context()
            .with_deadline(deadline)
            .with_security(security.clone())
            .with_provenance(provenance.clone())
            .with_configuration(configuration.clone());

        let attempt = parent.for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-1").unwrap(),
        );

        assert_eq!(attempt.deadline(), parent.deadline());
        assert_eq!(attempt.security(), Some(&security));
        assert_eq!(attempt.provenance(), &provenance);
        assert_eq!(attempt.configuration(), Some(configuration.as_ref()));
    }

    #[test]
    fn for_attempt_preserves_the_same_cancellation_authority() {
        let parent = sample_context();
        let attempt = parent.for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-1").unwrap(),
        );

        assert!(!attempt.cancellation().is_cancelled());

        parent.cancellation().cancel();

        assert!(parent.cancellation().is_cancelled());
        assert!(attempt.cancellation().is_cancelled());
    }

    #[test]
    fn for_attempt_preserves_operation_deadline() {
        let deadline = Deadline::from_now(Duration::from_secs(60)).unwrap();

        let parent = sample_context().with_deadline(deadline);
        let attempt = parent.for_attempt(
            NodeId::new("node-1").unwrap(),
            AttemptId::new("attempt-1").unwrap(),
        );

        assert_eq!(attempt.deadline(), Some(deadline));
    }

    #[test]
    fn expired_attempt_context_preserves_attempt_lineage_in_expiration_error() {
        let node_id = NodeId::new("node-expired").unwrap();
        let attempt_id = AttemptId::new("attempt-expired").unwrap();

        let parent = sample_context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

        let attempt = parent.for_attempt(node_id.clone(), attempt_id.clone());

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

        let error = attempt.expiration_error(&definition).unwrap();

        assert_eq!(
            error.context.operation.operation.id,
            attempt.operation().operation.id
        );
        assert_eq!(error.context.operation.node_id, Some(node_id));
        assert_eq!(error.context.operation.attempt_id, Some(attempt_id));
    }

    #[test]
    fn phase12_runtime_exports_are_available() {
        let config = ConcurrencyConfig::new(2, 4).unwrap();
        let mut concurrency = ConcurrencyState::new();
        concurrency.try_acquire_active(&config).unwrap();
        assert_eq!(concurrency.active(), 1);

        let scope = TaskScope::new(&CancellationToken::new());
        let task = Task::new(TaskOwner::engine(), scope, TaskCriticality::Required);

        assert_eq!(task.state(), TaskLifecycleState::Created);
        assert_eq!(task.criticality(), TaskCriticality::Required);
        assert_eq!(task.owner(), &TaskOwner::engine());

        let runtime = EngineRuntime::with_concurrency(config);
        assert_eq!(runtime.state(), LifecycleState::Created);

        let _ = TaskLifecycle::new();
        let _ = SpawnError;
        let _ = BoundedSpawnError::NotConfigured;
    }

    #[test]
    fn engine_context_supports_clone() {
        let security = sample_security_context();

        let configuration = sample_configuration_snapshot();
        let context = sample_context()
            .with_deadline(Deadline::from_now(Duration::from_secs(60)).unwrap())
            .with_security(security.clone())
            .with_provenance(ProvenanceContext::new().with_attribute("k", "v"))
            .with_configuration(configuration);

        let clone = context.clone();

        assert_eq!(context.deadline(), clone.deadline());
        assert_eq!(context.security(), clone.security());
        assert_eq!(context.provenance(), clone.provenance());
        assert_eq!(context.configuration(), clone.configuration());
    }
}
