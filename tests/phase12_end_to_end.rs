//! Phase 12 end-to-end integration tests.
//!
//! These tests cross the public Phase 12 boundaries instead of duplicating the
//! implementation-level unit tests. They verify that established Core context,
//! managed tasks, bounded admission, application streams, and runtime shutdown
//! compose without introducing a second context, cancellation, or transport
//! abstraction.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use nizaam_core::{
    config::{
        resolution::ConfigurationResolver,
        snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
        validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
    },
    identity::{CorrelationId, OperationId},
    operation::{Operation, OperationContext},
    provenance::ProvenanceContext,
    runtime::{
        BackgroundTasks, CancellationToken, ConcurrencyConfig, EngineContext, EngineRuntime,
        LifecycleState, Task, TaskCriticality, TaskLifecycleState, TaskOwner, TaskScope,
    },
    security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext},
    streaming::{
        BackpressureConfig, BackpressurePolicy, Stream, StreamItem, StreamItemKind, StreamMessage,
    },
    transport::MessageHeader,
};

const TEST_CONFIGURATION_KEY: &str = "phase12.mode";

fn operation_context(id: &str, correlation: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(id).unwrap(),
        CorrelationId::new(correlation).unwrap(),
    ))
}

fn configuration(value: &str) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::empty();
    parsed.insert(
        TEST_CONFIGURATION_KEY,
        ConfigurationValue::String(value.to_owned()),
    );

    let validated = ConfigurationValidator::new()
        .validate(parsed)
        .expect("phase 12 test configuration should validate");

    let resolved = ConfigurationResolver::new()
        .resolve(&validated)
        .expect("phase 12 test configuration should resolve");

    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(42),
        resolved,
    ))
}

fn security_context() -> SecurityContext {
    SecurityContext::new(
        PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("phase12-user").unwrap(),
        ),
        None,
    )
}

fn trusted_context() -> EngineContext {
    EngineContext::new(operation_context(
        "phase12.operation",
        "phase12.correlation",
    ))
    .with_configuration(configuration("stable"))
    .with_security(security_context())
    .with_provenance(ProvenanceContext::new().with_attribute("phase", "12"))
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::with_concurrency(
        nizaam_core::identity::EngineId::new("phase12-runtime-engine").unwrap(),
        nizaam_core::identity::EngineInstanceId::new("phase12-runtime-instance").unwrap(),
        ConcurrencyConfig::new(1, 1).unwrap(),
    );

    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime
            .transition(state)
            .expect("runtime should follow the established lifecycle");
    }

    runtime
}

#[test]
fn established_core_context_flows_into_stream_context_without_duplication() {
    let context = trusted_context();
    let stream = Stream::<String>::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(
        stream.context().operation().operation.id,
        context.operation().operation.id
    );
    assert_eq!(
        stream.context().operation().operation.correlation_id,
        context.operation().operation.correlation_id
    );
    assert_eq!(stream.context().security(), context.security());
    assert_eq!(stream.context().provenance(), context.provenance());
    assert_eq!(stream.context().configuration(), context.configuration());
    assert_eq!(stream.context().deadline(), context.deadline());
}

#[test]
fn managed_task_and_stream_share_the_same_operation_ownership_boundary() {
    let context = trusted_context();
    let stream = Stream::<String>::new(
        &context,
        BackpressureConfig::new(4, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        TaskScope::new(context.cancellation()),
        TaskCriticality::Required,
    );

    assert_eq!(
        stream.owner().operation_id(),
        &context.operation().operation.id
    );
    assert_eq!(task.owner(), &TaskOwner::Stream(stream.id()));
    assert_eq!(task.state(), TaskLifecycleState::Created);
}

#[test]
fn managed_task_can_produce_ordered_logical_items_into_a_stream() {
    let context = trusted_context();
    let stream = Stream::<String>::new(
        &context,
        BackpressureConfig::new(3, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        TaskScope::new(context.cancellation()),
        TaskCriticality::Required,
    );
    let producer_task = task.clone();
    let producer_stream = stream.clone();

    let handle = thread::spawn(move || {
        producer_task.start().unwrap();

        producer_stream
            .publish(StreamItem::partial(0, "first".to_owned()))
            .unwrap();
        producer_stream
            .publish(StreamItem::final_item(1, "final".to_owned()))
            .unwrap();

        producer_task.complete().unwrap();
    });

    let first = consumer.next_item().unwrap().unwrap();
    let second = consumer.next_item().unwrap().unwrap();

    handle.join().unwrap();

    assert_eq!(first.sequence(), 0);
    assert_eq!(first.kind(), StreamItemKind::Partial);
    assert_eq!(first.payload(), "first");

    assert_eq!(second.sequence(), 1);
    assert_eq!(second.kind(), StreamItemKind::Final);
    assert_eq!(second.payload(), "final");

    assert_eq!(consumer.next_item().unwrap(), None);
    assert_eq!(
        stream.state(),
        nizaam_core::streaming::StreamLifecycleState::Completed
    );
    assert_eq!(task.state(), TaskLifecycleState::Completed);
}

#[test]
fn logical_stream_message_remains_independent_from_transport_fragmentation() {
    let item = StreamItem::final_item(7, b"logical-result".to_vec());
    let message = StreamMessage::from(item.clone());

    assert_eq!(message.sequence(), 7);
    assert_eq!(message.kind(), StreamItemKind::Final);
    assert_eq!(message.item(), &item);

    let message_id = [1, 2, 3, 4, 5, 6, 7, 8];
    let stream_id = u64::from_be_bytes(message_id);
    let first_frame = MessageHeader::new(1, 0, 7, stream_id, 0, u32::MAX, 0)
        .unwrap()
        .serialize();
    let second_frame = MessageHeader::new(1, 1, 6, stream_id, 1, u32::MAX, 0)
        .unwrap()
        .serialize();

    assert_eq!(
        first_frame.len(),
        nizaam_core::transport::framing::HEADER_LENGTH
    );
    assert_eq!(
        second_frame.len(),
        nizaam_core::transport::framing::HEADER_LENGTH
    );

    let first = MessageHeader::deserialize(&first_frame).unwrap();
    let second = MessageHeader::deserialize(&second_frame).unwrap();

    assert_ne!(first.serialize(), second.serialize());
    assert_eq!(first.serialize()[8..16], second.serialize()[8..16]);
    assert_eq!(first.serialize()[16..20], 0_u32.to_be_bytes());
    assert_eq!(second.serialize()[16..20], 1_u32.to_be_bytes());

    // Fragmentation identifies pieces of one logical transport message; it
    // does not change the single logical stream-item identity above.
    assert_eq!(message.item(), &item);
}

#[test]
fn bounded_task_admission_can_control_stream_producer_execution() {
    let tasks = BackgroundTasks::with_concurrency(
        CancellationToken::new(),
        ConcurrencyConfig::new(1, 1).unwrap(),
    );

    let (release_tx, release_rx) = mpsc::channel();
    let stream = Stream::<u32>::new(
        &trusted_context(),
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    let producer_stream = stream.clone();

    tasks
        .spawn_bounded(move |cancellation| {
            assert!(!cancellation.is_cancelled());
            producer_stream.publish(StreamItem::partial(0, 11)).unwrap();
            release_rx.recv().unwrap();
        })
        .unwrap();

    assert_eq!(
        tasks.spawn_bounded(|_| {}),
        Err(nizaam_core::runtime::BoundedSpawnError::ActiveLimitReached)
    );

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &11);

    release_tx.send(()).unwrap();
    tasks.shutdown();
}

#[test]
fn parent_cancellation_propagates_through_task_scope_and_stream_context() {
    let context = trusted_context();
    let stream = Stream::<u32>::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        TaskScope::new(context.cancellation()),
        TaskCriticality::Required,
    );
    task.start().unwrap();

    context.cancellation().cancel();

    assert!(task.scope().is_cancelled());
    assert_eq!(task.state(), TaskLifecycleState::Cancelled);
    assert_eq!(
        stream.state(),
        nizaam_core::streaming::StreamLifecycleState::Cancelled
    );
}

#[test]
fn sibling_task_and_stream_contexts_remain_isolated() {
    let parent = CancellationToken::new();

    let first_context = EngineContext::new(operation_context(
        "phase12.first",
        "phase12.first.correlation",
    ));
    let second_context = EngineContext::new(operation_context(
        "phase12.second",
        "phase12.second.correlation",
    ));

    let first_stream = Stream::<u32>::new(
        &first_context,
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    let second_stream = Stream::<u32>::new(
        &second_context,
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    let first_task = Task::new(
        TaskOwner::stream(first_stream.id()),
        TaskScope::new(&parent),
        TaskCriticality::Optional,
    );
    let second_task = Task::new(
        TaskOwner::stream(second_stream.id()),
        TaskScope::new(&parent),
        TaskCriticality::Optional,
    );

    first_task.start().unwrap();
    second_task.start().unwrap();

    first_task.cancel().unwrap();

    assert_eq!(first_task.state(), TaskLifecycleState::Cancelled);
    assert_eq!(second_task.state(), TaskLifecycleState::Running);
    assert!(!parent.is_cancelled());
    assert!(!first_context.cancellation().is_cancelled());
}

#[test]
fn parent_deadline_is_visible_to_the_stream_and_task_context() {
    let context = trusted_context().with_deadline(
        nizaam_core::runtime::Deadline::from_now(Duration::from_millis(50)).unwrap(),
    );

    let stream = Stream::<u32>::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(stream.context().deadline(), context.deadline());
    assert!(!stream.context().is_expired());

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        TaskScope::new(context.cancellation()),
        TaskCriticality::Optional,
    );

    assert!(!task.scope().is_cancelled());
}

#[test]
fn stream_backpressure_and_task_execution_compose_without_dropping_accepted_items() {
    let context = trusted_context();
    let stream = Stream::<u32>::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    let task = Task::new(
        TaskOwner::stream(stream.id()),
        TaskScope::new(context.cancellation()),
        TaskCriticality::Required,
    );
    task.start().unwrap();

    stream.publish(StreamItem::partial(0, 100)).unwrap();

    assert_eq!(
        stream.publish(StreamItem::partial(1, 200)),
        Err(nizaam_core::streaming::StreamError::Backpressure(
            nizaam_core::streaming::BackpressureError::Full
        ))
    );

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &100);

    stream.publish(StreamItem::final_item(1, 200)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &200);
    assert_eq!(consumer.next_item().unwrap(), None);

    task.complete().unwrap();

    assert_eq!(task.state(), TaskLifecycleState::Completed);
}

#[test]
fn consumer_disappearance_cancels_an_open_stream_and_notifies_producer_scope() {
    let context = trusted_context();
    let stream = Stream::<u32>::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Wait).unwrap(),
    )
    .unwrap();

    stream.open().unwrap();

    let consumer = stream.consumer().unwrap();

    let producer_scope = TaskScope::new(stream.context().cancellation());
    let producer_task = Task::new(
        TaskOwner::stream(stream.id()),
        producer_scope,
        TaskCriticality::Required,
    );
    producer_task.start().unwrap();

    let producer = stream.clone();
    let task = producer_task.clone();
    let (published_tx, published_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        producer.publish(StreamItem::partial(0, 1)).unwrap();
        published_tx.send(()).unwrap();
        producer.publish(StreamItem::partial(1, 2))
    });

    published_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first stream item was not published");
    drop(consumer);

    assert_eq!(
        handle.join().unwrap(),
        Err(nizaam_core::streaming::StreamError::Cancelled)
    );
    assert!(
        producer_task.scope().is_cancelled(),
        "consumer disappearance must cancel the producer task scope"
    );
    assert_eq!(producer_task.state(), TaskLifecycleState::Cancelled);
    drop(task);
}

#[test]
fn runtime_shutdown_rejects_new_bounded_tasks_after_existing_work_is_coordinated() {
    let runtime = serving_runtime();

    let (ready_tx, ready_rx) = mpsc::channel();

    runtime
        .background_tasks()
        .spawn(move |cancellation| {
            ready_tx.send(cancellation.clone()).unwrap();

            while !cancellation.is_cancelled() {
                thread::yield_now();
            }
        })
        .unwrap();

    let cancellation = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("existing runtime task did not start");

    assert!(!cancellation.is_cancelled());

    assert!(runtime.shutdown().is_ok());
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.shutdown_token().is_cancelled());

    assert_eq!(
        runtime.background_tasks().spawn_bounded(|_| {}),
        Err(nizaam_core::runtime::BoundedSpawnError::Closed)
    );
}

#[test]
fn phase12_runtime_shutdown_cleans_up_existing_background_work_before_stopped() {
    let runtime = serving_runtime();

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
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.shutdown_token().is_cancelled());
    assert!(*finished.lock().unwrap());
}
