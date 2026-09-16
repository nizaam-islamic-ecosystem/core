//! Runtime-managed task semantics for Phase 12.
//!
//! This module defines task identity, ownership, cancellation scope,
//! criticality, lifecycle, and terminal failure state. Execution, scheduling,
//! queueing, and worker ownership remain responsibilities of the runtime
//! concurrency and background-task layers.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::{identity::OperationId, streaming::StreamId};

use super::concurrency::TaskScope;

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Uniquely identifies a runtime-managed task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(u64);

impl TaskId {
    const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric task identifier.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies the explicit owner responsible for a task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskOwner {
    /// The engine itself owns the task.
    Engine,
    /// An operation owns the task.
    Operation(OperationId),
    /// A stream owns the task.
    Stream(StreamId),
    /// A named runtime subsystem owns the task.
    RuntimeSubsystem(String),
}

impl TaskOwner {
    /// Creates an engine-owned task owner.
    pub const fn engine() -> Self {
        Self::Engine
    }

    /// Creates an operation-owned task owner.
    pub fn operation(operation_id: OperationId) -> Self {
        Self::Operation(operation_id)
    }

    /// Creates a stream-owned task owner.
    pub const fn stream(stream_id: StreamId) -> Self {
        Self::Stream(stream_id)
    }

    /// Creates a runtime-subsystem-owned task owner.
    pub fn runtime_subsystem(name: impl Into<String>) -> Self {
        Self::RuntimeSubsystem(name.into())
    }
}

/// Classifies whether a task is required for the owning runtime function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskCriticality {
    /// Failure of this task represents failure of its required work contract.
    Required,
    /// This task is useful but is not required for the owning work contract.
    Optional,
}

/// Lifecycle state of a runtime-managed task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskLifecycleState {
    /// The task has been created but has not started execution.
    Created,
    /// The task is actively executing.
    Running,
    /// The task completed normally.
    Completed,
    /// Cancellation or expiration ended the task.
    Cancelled,
    /// The task terminated because execution failed.
    Failed,
}

impl TaskLifecycleState {
    /// Returns whether this state is terminal.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Returns whether a direct transition to `next` is valid.
    ///
    /// Repeating the same transition is a valid no-op.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Created)
                | (Self::Running, Self::Running)
                | (Self::Completed, Self::Completed)
                | (Self::Cancelled, Self::Cancelled)
                | (Self::Failed, Self::Failed)
                | (Self::Created, Self::Running)
                | (Self::Running, Self::Completed)
                | (Self::Running, Self::Cancelled)
                | (Self::Running, Self::Failed)
        )
    }
}

/// Error returned for an invalid task lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskLifecycleError {
    from: TaskLifecycleState,
    to: TaskLifecycleState,
}

impl TaskLifecycleError {
    /// Creates an invalid task lifecycle transition error.
    pub const fn new(from: TaskLifecycleState, to: TaskLifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the source state.
    pub const fn from(self) -> TaskLifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub const fn to(self) -> TaskLifecycleState {
        self.to
    }
}

impl std::fmt::Display for TaskLifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid task lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for TaskLifecycleError {}

/// Mutable task lifecycle state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskLifecycle {
    state: TaskLifecycleState,
}

impl Default for TaskLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskLifecycle {
    /// Creates a new lifecycle in the `Created` state.
    pub const fn new() -> Self {
        Self {
            state: TaskLifecycleState::Created,
        }
    }

    /// Returns the current state.
    pub const fn state(&self) -> TaskLifecycleState {
        self.state
    }

    /// Returns whether the lifecycle is terminal.
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Validates and applies a lifecycle transition.
    pub fn transition(&mut self, next: TaskLifecycleState) -> Result<(), TaskLifecycleError> {
        if !self.state.can_transition_to(next) {
            return Err(TaskLifecycleError::new(self.state, next));
        }

        self.state = next;
        Ok(())
    }
}

/// Validates a task lifecycle transition without modifying any state.
pub const fn can_transition(
    from: TaskLifecycleState,
    to: TaskLifecycleState,
) -> Result<(), TaskLifecycleError> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(TaskLifecycleError::new(from, to))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TaskState {
    lifecycle: TaskLifecycle,
    failure: Option<String>,
}

/// Runtime-managed task metadata and lifecycle.
///
/// The task does not execute or schedule work itself. Runtime execution layers
/// use its identity, ownership, cancellation scope, criticality, and state.
#[derive(Clone)]
pub struct Task {
    id: TaskId,
    owner: TaskOwner,
    scope: TaskScope,
    criticality: TaskCriticality,
    state: Arc<Mutex<TaskState>>,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Task")
            .field("id", &self.id)
            .field("owner", &self.owner)
            .field("criticality", &self.criticality)
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Task {
    /// Creates a task in the `Created` state with an explicit owner, scope,
    /// and criticality.
    pub fn new(owner: TaskOwner, scope: TaskScope, criticality: TaskCriticality) -> Self {
        Self {
            id: next_task_id(),
            owner,
            scope,
            criticality,
            state: Arc::new(Mutex::new(TaskState {
                lifecycle: TaskLifecycle::new(),
                failure: None,
            })),
        }
    }

    /// Returns the task's unique identifier.
    pub const fn id(&self) -> TaskId {
        self.id
    }

    /// Returns the task owner.
    pub const fn owner(&self) -> &TaskOwner {
        &self.owner
    }

    /// Returns the task cancellation scope.
    pub const fn scope(&self) -> &TaskScope {
        &self.scope
    }

    /// Returns the task criticality.
    pub const fn criticality(&self) -> TaskCriticality {
        self.criticality
    }

    /// Returns the current lifecycle state.
    pub fn state(&self) -> TaskLifecycleState {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.lifecycle.state()
    }

    /// Returns whether the task has terminated.
    pub fn is_terminal(&self) -> bool {
        self.state().is_terminal()
    }

    /// Returns the failure detail, if the task failed.
    pub fn failure(&self) -> Option<String> {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.failure.clone()
    }

    /// Starts a created task.
    pub fn start(&self) -> Result<(), TaskLifecycleError> {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.lifecycle.transition(TaskLifecycleState::Running)
    }

    /// Marks a running task as completed.
    pub fn complete(&self) -> Result<(), TaskLifecycleError> {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.lifecycle.transition(TaskLifecycleState::Completed)
    }

    /// Cancels a running task and propagates cancellation to its scope.
    pub fn cancel(&self) -> Result<(), TaskLifecycleError> {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.lifecycle.transition(TaskLifecycleState::Cancelled)?;
        self.scope.cancel();
        Ok(())
    }

    /// Marks a running task as failed and records its failure detail.
    pub fn fail(&self, failure: impl Into<String>) -> Result<(), TaskLifecycleError> {
        let mut state = self.state.lock().expect("task state lock poisoned");
        self.synchronize_cancellation(&mut state);
        state.lifecycle.transition(TaskLifecycleState::Failed)?;
        state.failure = Some(failure.into());
        Ok(())
    }

    fn synchronize_cancellation(&self, state: &mut TaskState) {
        if !state.lifecycle.state().is_terminal() && self.scope.is_cancelled() {
            let _ = state.lifecycle.transition(TaskLifecycleState::Cancelled);
        }
    }
}

fn next_task_id() -> TaskId {
    TaskId::new(
        NEXT_TASK_ID
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .unwrap_or(u64::MAX),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Task, TaskCriticality, TaskLifecycle, TaskLifecycleError, TaskLifecycleState, TaskOwner,
        TaskScope,
    };
    use crate::{identity::OperationId, runtime::CancellationToken, streaming::StreamItem};

    fn scope() -> TaskScope {
        TaskScope::new(&CancellationToken::new())
    }

    #[test]
    fn new_task_starts_created_with_explicit_metadata() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Required);

        assert_eq!(task.state(), TaskLifecycleState::Created);
        assert_eq!(task.owner(), &TaskOwner::Engine);
        assert_eq!(task.criticality(), TaskCriticality::Required);
        assert!(!task.is_terminal());
        assert!(task.failure().is_none());
    }

    #[test]
    fn task_ids_are_distinct() {
        let first = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Optional);
        let second = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Optional);

        assert_ne!(first.id(), second.id());
        assert_ne!(first.id().get(), second.id().get());
    }

    #[test]
    fn task_owner_variants_preserve_identity() {
        let operation_id = OperationId::new("task-operation").unwrap();
        let operation = TaskOwner::operation(operation_id.clone());
        let subsystem = TaskOwner::runtime_subsystem("background");

        assert_eq!(operation, TaskOwner::Operation(operation_id));
        assert_eq!(
            subsystem,
            TaskOwner::RuntimeSubsystem(String::from("background"))
        );
    }

    #[test]
    fn lifecycle_follows_created_running_terminal_model() {
        let lifecycle = TaskLifecycle::new();

        assert!(
            lifecycle
                .state()
                .can_transition_to(TaskLifecycleState::Running)
        );
        assert!(TaskLifecycleState::Running.can_transition_to(TaskLifecycleState::Completed));
        assert!(TaskLifecycleState::Running.can_transition_to(TaskLifecycleState::Cancelled));
        assert!(TaskLifecycleState::Running.can_transition_to(TaskLifecycleState::Failed));
    }

    #[test]
    fn lifecycle_rejects_reopening_terminal_state() {
        let mut lifecycle = TaskLifecycle::new();
        lifecycle.transition(TaskLifecycleState::Running).unwrap();
        lifecycle.transition(TaskLifecycleState::Completed).unwrap();

        assert_eq!(
            lifecycle.transition(TaskLifecycleState::Running),
            Err(TaskLifecycleError::new(
                TaskLifecycleState::Completed,
                TaskLifecycleState::Running,
            ))
        );
    }

    #[test]
    fn lifecycle_allows_same_state_no_ops() {
        let mut lifecycle = TaskLifecycle::new();

        assert!(lifecycle.transition(TaskLifecycleState::Created).is_ok());
        lifecycle.transition(TaskLifecycleState::Running).unwrap();
        assert!(lifecycle.transition(TaskLifecycleState::Running).is_ok());
        lifecycle.transition(TaskLifecycleState::Failed).unwrap();
        assert!(lifecycle.transition(TaskLifecycleState::Failed).is_ok());
    }

    #[test]
    fn task_starts_and_completes() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Required);

        task.start().unwrap();
        assert_eq!(task.state(), TaskLifecycleState::Running);

        task.complete().unwrap();
        assert_eq!(task.state(), TaskLifecycleState::Completed);
        assert!(task.is_terminal());
    }

    #[test]
    fn task_cancellation_updates_state_and_scope() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Optional);

        task.start().unwrap();
        task.cancel().unwrap();

        assert_eq!(task.state(), TaskLifecycleState::Cancelled);
        assert!(task.scope().is_cancelled());
    }

    #[test]
    fn parent_cancellation_reaches_task_state() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);
        let task = Task::new(TaskOwner::engine(), scope, TaskCriticality::Required);

        task.start().unwrap();
        parent.cancel();

        assert_eq!(task.state(), TaskLifecycleState::Cancelled);
    }

    #[test]
    fn task_failure_records_failure_detail() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Required);

        task.start().unwrap();
        task.fail("worker failed").unwrap();

        assert_eq!(task.state(), TaskLifecycleState::Failed);
        assert_eq!(task.failure().as_deref(), Some("worker failed"));
    }

    #[test]
    fn terminal_task_does_not_change_after_scope_cancellation() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Required);

        task.start().unwrap();
        task.complete().unwrap();
        task.scope().cancel();

        assert_eq!(task.state(), TaskLifecycleState::Completed);
    }

    #[test]
    fn task_clone_shares_lifecycle_state() {
        let task = Task::new(TaskOwner::engine(), scope(), TaskCriticality::Optional);
        let clone = task.clone();

        task.start().unwrap();
        clone.complete().unwrap();

        assert_eq!(task.state(), TaskLifecycleState::Completed);
        assert_eq!(clone.state(), TaskLifecycleState::Completed);
    }

    #[test]
    fn task_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<Task>();
    }

    #[test]
    fn task_item_payload_types_remain_uninterpreted() {
        let _item = StreamItem::partial(0, String::from("payload"));
        let task = Task::new(
            TaskOwner::runtime_subsystem("stream-producer"),
            scope(),
            TaskCriticality::Optional,
        );

        assert_eq!(task.criticality(), TaskCriticality::Optional);
    }
}
