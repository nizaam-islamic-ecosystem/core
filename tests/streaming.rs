use nizaam_core::{
    identity::{CorrelationId, OperationId},
    operation::{Operation, OperationContext},
    runtime::{Deadline, EngineContext},
    streaming::{
        BackpressureConfig, BackpressureError, BackpressurePolicy, Stream, StreamError, StreamItem,
        StreamItemKind, StreamLifecycleState, StreamMessage,
    },
};
use std::{sync::mpsc, thread, time::Duration};

fn context() -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new("streaming-integration-operation").unwrap(),
        CorrelationId::new("streaming-integration-correlation").unwrap(),
    )))
}

fn stream<T>(capacity: usize, policy: BackpressurePolicy) -> Stream<T> {
    Stream::new(
        &context(),
        BackpressureConfig::new(capacity, policy).unwrap(),
    )
    .unwrap()
}

#[test]
fn stream_can_be_created_through_public_api() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);

    assert_eq!(stream.state(), StreamLifecycleState::Created);
    assert_eq!(stream.capacity(), 2);
    assert_eq!(stream.backpressure_policy(), BackpressurePolicy::Reject);
}

#[test]
fn stream_has_explicit_operation_owner() {
    let engine_context = context();
    let stream: Stream<u32> = Stream::new(
        &engine_context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(
        stream.owner().operation_id(),
        &engine_context.operation().operation.id
    );
}

#[test]
fn stream_ids_are_unique_across_instances() {
    let first: Stream<u32> = stream(1, BackpressurePolicy::Reject);
    let second: Stream<u32> = stream(1, BackpressurePolicy::Reject);

    assert_ne!(first.id(), second.id());
}

#[test]
fn logical_item_can_cross_message_boundary_into_stream_without_metadata_loss() {
    let item = StreamItem::final_item(0, String::from("result"));
    let message = StreamMessage::from(item.clone());
    let recovered = message.into_item();

    assert_eq!(recovered, item);

    let stream: Stream<String> = stream(1, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(recovered).unwrap();

    assert_eq!(consumer.next_item().unwrap(), Some(item));
    assert_eq!(consumer.next_item().unwrap(), None);
    assert_eq!(stream.state(), StreamLifecycleState::Completed);
}

#[test]
fn stream_opens_from_created() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);

    stream.open().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Open);
}

#[test]
fn partial_item_keeps_stream_open() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Open);
}

#[test]
fn final_item_completes_stream_and_is_consumed_before_successful_end() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();
    stream.publish(StreamItem::final_item(1, 20)).unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    assert_eq!(
        consumer.next_item().unwrap().unwrap().kind(),
        StreamItemKind::Partial
    );
    assert_eq!(
        consumer.next_item().unwrap().unwrap().kind(),
        StreamItemKind::Final
    );
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn empty_stream_can_complete_successfully() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.complete().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn terminal_stream_rejects_new_publication() {
    let completed: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    completed.open().unwrap();
    completed.complete().unwrap();

    assert!(matches!(
        completed.publish(StreamItem::partial(0, 1)),
        Err(StreamError::Lifecycle(_))
    ));

    let cancelled: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    cancelled.open().unwrap();
    cancelled.cancel().unwrap();

    assert_eq!(
        cancelled.publish(StreamItem::partial(0, 1)),
        Err(StreamError::Cancelled)
    );

    let failed: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    failed.open().unwrap();
    failed.fail().unwrap();

    assert_eq!(
        failed.publish(StreamItem::partial(0, 1)),
        Err(StreamError::Failed)
    );
}

#[test]
fn stream_preserves_logical_item_order() {
    let stream: Stream<u32> = stream(4, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    for sequence in 0..3 {
        stream
            .publish(StreamItem::partial(sequence, sequence as u32 * 10))
            .unwrap();
    }
    stream.publish(StreamItem::final_item(3, 30)).unwrap();

    let received: Vec<_> = (0..4)
        .map(|_| consumer.next_item().unwrap().unwrap())
        .collect();

    assert_eq!(
        received
            .iter()
            .map(StreamItem::sequence)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(
        received
            .iter()
            .map(|item| *item.payload())
            .collect::<Vec<_>>(),
        vec![0, 10, 20, 30]
    );
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn invalid_sequence_does_not_mutate_stream_ordering_state() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    assert_eq!(
        stream.publish(StreamItem::partial(1, 10)),
        Err(StreamError::InvalidSequence {
            expected: 0,
            actual: 1,
        })
    );

    stream.publish(StreamItem::partial(0, 20)).unwrap();

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
}

#[test]
fn stream_allows_only_one_consumer() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let _first = stream.consumer().unwrap();

    assert!(matches!(
        stream.consumer(),
        Err(StreamError::ConsumerAlreadyAttached)
    ));
}

#[test]
fn reject_policy_applies_bounded_capacity_without_dropping_accepted_items() {
    let stream: Stream<u32> = stream(1, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();

    assert_eq!(
        stream.publish(StreamItem::partial(1, 20)),
        Err(StreamError::Backpressure(BackpressureError::Full))
    );

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
}

#[test]
fn wait_policy_blocks_until_consumer_releases_capacity() {
    let stream: Stream<u32> = stream(1, BackpressurePolicy::Wait);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();

    let producer = stream.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = producer.publish(StreamItem::partial(1, 20));
        finished_tx.send(result).unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(finished_rx.recv_timeout(Duration::from_millis(25)).is_err());

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
    assert_eq!(
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        Ok(())
    );

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
    handle.join().unwrap();
}

#[test]
fn parent_operation_cancellation_reaches_stream() {
    let engine_context = context();
    let stream: Stream<u32> = Stream::new(
        &engine_context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();

    engine_context.cancellation().cancel();

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
}

#[test]
fn stream_cancellation_does_not_cancel_parent_context() {
    let engine_context = context();
    let stream: Stream<u32> = Stream::new(
        &engine_context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();

    stream.cancel().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    assert!(!engine_context.cancellation().is_cancelled());
}

#[test]
fn cancellation_race_prevents_post_terminal_publication() {
    let stream: Stream<u32> = stream(1, BackpressurePolicy::Wait);
    stream.open().unwrap();
    let _consumer = stream.consumer().unwrap();

    let producer = stream.clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        ready_tx.send(()).unwrap();
        result_tx
            .send(producer.publish(StreamItem::partial(0, 10)))
            .unwrap();
    });

    ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    stream.cancel().unwrap();

    assert_eq!(
        result_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        Err(StreamError::Cancelled)
    );
    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    handle.join().unwrap();
}

#[test]
fn stream_inherits_parent_deadline() {
    let deadline = Deadline::from_now(Duration::from_secs(5)).unwrap();
    let context = context().with_deadline(deadline);
    let stream: Stream<u32> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(stream.context().deadline(), Some(deadline));
}

#[test]
fn expired_stream_cancels_waiting_producer() {
    let deadline = Deadline::from_now(Duration::from_millis(250)).unwrap();
    let context = context().with_deadline(deadline);
    let stream: Stream<u32> = Stream::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Wait).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let _consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 10)).unwrap();

    let producer = stream.clone();
    let handle = thread::spawn(move || producer.publish(StreamItem::partial(1, 20)));

    assert_eq!(handle.join().unwrap(), Err(StreamError::DeadlineExpired));
    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
}

#[test]
fn consumer_drop_cancels_open_stream() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();

    let consumer = stream.consumer().unwrap();
    drop(consumer);

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
}

#[test]
fn buffered_items_remain_observable_before_cancellation_error() {
    let stream: Stream<u32> = stream(2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();
    stream.cancel().unwrap();

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
    assert_eq!(consumer.next_item(), Err(StreamError::Cancelled));
}
