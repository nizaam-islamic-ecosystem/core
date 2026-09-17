use nizaam_core::{
    identity::{CorrelationId, OperationId},
    operation::{Operation, OperationContext},
    runtime::{
        BackgroundTasks, CancellationToken, ConcurrencyConfig, EngineRuntime, Task,
        TaskCriticality, TaskLifecycleError, TaskLifecycleState, TaskOwner, TaskScope,
    },
    streaming::{BackpressureConfig, BackpressurePolicy, Stream},
};
use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
    time::Duration,
};

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        operation_id("tasks-test-operation"),
        CorrelationId::new("tasks-test-correlation").unwrap(),
    ))
}

fn task_scope() -> TaskScope {
    let parent = CancellationToken::new();
    TaskScope::new(&parent)
}

fn engine_context() -> nizaam_core::runtime::EngineContext {
    nizaam_core::runtime::EngineContext::new(operation_context())
}

#[test]
fn task_can_be_created_with_explicit_owner_and_criticality() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Required);

    assert_eq!(task.state(), TaskLifecycleState::Created);
    assert_eq!(task.owner(), &TaskOwner::Engine);
    assert_eq!(task.criticality(), TaskCriticality::Required);
    assert!(!task.is_terminal());
    assert!(task.failure().is_none());
}

#[test]
fn task_ids_are_unique_and_stable() {
    let first = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Optional);
    let second = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Optional);

    assert_ne!(first.id(), second.id());

    let first_id = first.id();
    assert_eq!(first.id(), first_id);
    assert_eq!(first.id().get(), first_id.get());
}

#[test]
fn operation_owned_task_preserves_operation_identity() {
    let id = operation_id("operation-owner");
    let task = Task::new(
        TaskOwner::operation(id.clone()),
        task_scope(),
        TaskCriticality::Required,
    );

    assert_eq!(task.owner(), &TaskOwner::Operation(id));
}

#[test]
fn stream_owned_task_preserves_stream_identity() {
    let stream = Stream::<u32>::new(
        &engine_context(),
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        task_scope(),
        TaskCriticality::Optional,
    );

    assert_eq!(task.owner(), &TaskOwner::Stream(stream.id()));
}

#[test]
fn runtime_subsystem_owned_task_preserves_owner_name() {
    let task = Task::new(
        TaskOwner::runtime_subsystem("stream-producer"),
        task_scope(),
        TaskCriticality::Optional,
    );

    assert_eq!(
        task.owner(),
        &TaskOwner::RuntimeSubsystem(String::from("stream-producer"))
    );
}

#[test]
fn required_and_optional_criticality_remain_explicit() {
    let required = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Required);
    let optional = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Optional);

    assert_eq!(required.criticality(), TaskCriticality::Required);
    assert_eq!(optional.criticality(), TaskCriticality::Optional);
    assert_ne!(required.criticality(), optional.criticality());
}

#[test]
fn task_transitions_from_created_to_running_and_completed() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Required);

    task.start().unwrap();
    assert_eq!(task.state(), TaskLifecycleState::Running);

    task.complete().unwrap();
    assert_eq!(task.state(), TaskLifecycleState::Completed);
    assert!(task.is_terminal());
}

#[test]
fn task_can_cancel_and_propagates_to_its_scope() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Optional);

    task.start().unwrap();
    task.cancel().unwrap();

    assert_eq!(task.state(), TaskLifecycleState::Cancelled);
    assert!(task.scope().is_cancelled());
}

#[test]
fn task_can_fail_and_preserve_failure_detail() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Required);

    task.start().unwrap();
    task.fail("worker failed").unwrap();

    assert_eq!(task.state(), TaskLifecycleState::Failed);
    assert_eq!(task.failure().as_deref(), Some("worker failed"));
    assert!(task.is_terminal());
}

#[test]
fn terminal_task_cannot_restart_or_change_terminal_state() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Required);

    task.start().unwrap();
    task.complete().unwrap();

    assert_eq!(
        task.start(),
        Err(TaskLifecycleError::new(
            TaskLifecycleState::Completed,
            TaskLifecycleState::Running,
        ))
    );
    assert_eq!(
        task.cancel(),
        Err(TaskLifecycleError::new(
            TaskLifecycleState::Completed,
            TaskLifecycleState::Cancelled,
        ))
    );
    assert_eq!(
        task.fail("too late"),
        Err(TaskLifecycleError::new(
            TaskLifecycleState::Completed,
            TaskLifecycleState::Failed,
        ))
    );
}

#[test]
fn same_terminal_transition_is_idempotent_at_lifecycle_level() {
    let mut lifecycle = nizaam_core::runtime::TaskLifecycle::new();

    lifecycle.transition(TaskLifecycleState::Running).unwrap();
    lifecycle.transition(TaskLifecycleState::Cancelled).unwrap();

    assert!(lifecycle.transition(TaskLifecycleState::Cancelled).is_ok());
    assert_eq!(lifecycle.state(), TaskLifecycleState::Cancelled);
}

#[test]
fn parent_cancellation_reaches_owned_task() {
    let parent = CancellationToken::new();
    let scope = TaskScope::new(&parent);
    let task = Task::new(
        TaskOwner::operation(operation_id("parent-owner")),
        scope,
        TaskCriticality::Required,
    );

    task.start().unwrap();
    parent.cancel();

    assert_eq!(task.state(), TaskLifecycleState::Cancelled);
}

#[test]
fn task_cancellation_isolated_from_sibling_task_and_parent() {
    let parent = CancellationToken::new();
    let scope_a = TaskScope::new(&parent);
    let scope_b = TaskScope::new(&parent);

    let task_a = Task::new(TaskOwner::engine(), scope_a, TaskCriticality::Optional);
    let task_b = Task::new(TaskOwner::engine(), scope_b, TaskCriticality::Optional);

    task_a.start().unwrap();
    task_b.start().unwrap();

    task_a.cancel().unwrap();

    assert_eq!(task_a.state(), TaskLifecycleState::Cancelled);
    assert_eq!(task_b.state(), TaskLifecycleState::Running);
    assert!(!parent.is_cancelled());
}

#[test]
fn cloned_task_shares_lifecycle_state() {
    let task = Task::new(TaskOwner::engine(), task_scope(), TaskCriticality::Optional);
    let clone = task.clone();

    task.start().unwrap();
    clone.complete().unwrap();

    assert_eq!(task.state(), TaskLifecycleState::Completed);
    assert_eq!(clone.state(), TaskLifecycleState::Completed);
}

#[test]
fn independent_tasks_can_run_in_parallel_using_local_counters() {
    let config = ConcurrencyConfig::new(2, 2).unwrap();
    let parent = CancellationToken::new();
    let barrier = Arc::new(Barrier::new(3));
    let active = Arc::new(Mutex::new(0_usize));
    let peak = Arc::new(Mutex::new(0_usize));

    let task_a = Task::new(
        TaskOwner::engine(),
        TaskScope::new(&parent),
        TaskCriticality::Required,
    );
    let task_b = Task::new(
        TaskOwner::engine(),
        TaskScope::new(&parent),
        TaskCriticality::Required,
    );

    let mut handles = Vec::new();

    for task in [task_a, task_b] {
        let barrier = Arc::clone(&barrier);
        let active = Arc::clone(&active);
        let peak = Arc::clone(&peak);

        assert_eq!(task.start(), Ok(()));

        handles.push(thread::spawn(move || {
            {
                let mut active = active.lock().unwrap();
                *active += 1;

                let mut peak_value = peak.lock().unwrap();
                *peak_value = (*peak_value).max(*active);
            }

            barrier.wait();
            thread::sleep(Duration::from_millis(5));

            {
                let mut active = active.lock().unwrap();
                *active -= 1;
            }
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(*peak.lock().unwrap(), config.max_active());
    assert_eq!(*active.lock().unwrap(), 0);
}

#[test]
fn runtime_shutdown_cancels_and_waits_for_runtime_owned_background_work() {
    let runtime = EngineRuntime::new();

    for state in [
        nizaam_core::runtime::LifecycleState::Starting,
        nizaam_core::runtime::LifecycleState::Configuring,
        nizaam_core::runtime::LifecycleState::Dependencies,
        nizaam_core::runtime::LifecycleState::Capabilities,
        nizaam_core::runtime::LifecycleState::Registering,
        nizaam_core::runtime::LifecycleState::Ready,
        nizaam_core::runtime::LifecycleState::Serving,
    ] {
        runtime.transition(state).unwrap();
    }

    let finished = Arc::new(Mutex::new(false));
    let finished_by_task = Arc::clone(&finished);

    runtime
        .background_tasks()
        .spawn(move |cancellation| {
            while !cancellation.is_cancelled() {
                thread::yield_now();
            }

            *finished_by_task.lock().unwrap() = true;
        })
        .unwrap();

    assert!(runtime.shutdown().unwrap());
    assert_eq!(
        runtime.state(),
        nizaam_core::runtime::LifecycleState::Stopped
    );
    assert!(*finished.lock().unwrap());
}

#[test]
fn bounded_background_task_admission_rejects_work_after_active_limit() {
    let tasks = BackgroundTasks::with_concurrency(
        CancellationToken::new(),
        ConcurrencyConfig::new(1, 1).unwrap(),
    );

    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    tasks
        .spawn_bounded(move |cancellation| {
            assert!(!cancellation.is_cancelled());
            release_rx.recv().unwrap();
            finished_tx.send(()).unwrap();
        })
        .unwrap();

    assert_eq!(
        tasks.spawn_bounded(|_| {}),
        Err(nizaam_core::runtime::BoundedSpawnError::ActiveLimitReached)
    );

    release_tx.send(()).unwrap();
    finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("bounded task did not finish");

    tasks.shutdown();
}
