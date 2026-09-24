//! Phase 16 application-level streaming conformance tests.
//!
//! These tests exercise the real `Stream<T>` implementation and keep logical
//! stream semantics separate from transport framing.

use std::sync::mpsc;
use std::time::Duration;

use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::operation::{Deadline, Operation, OperationContext};
use nizaam_core::retry::{
    Attempt, BackoffPolicy, FailureCategory, RetryAdmission, RetryAdmissionError,
    RetryAdmissionRequest, RetryPolicy, RetrySafetyGates,
};
use nizaam_core::runtime::EngineContext;
use nizaam_core::status::Retryability;
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamError, StreamItem, StreamItemKind,
    StreamLifecycleState,
};
use nizaam_core::transport::framing::{MAX_PAYLOAD_LENGTH, encode_message};

fn context(name: &str) -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new(format!("phase16-stream-operation-{name}")).unwrap(),
        CorrelationId::new(format!("phase16-stream-correlation-{name}")).unwrap(),
    )))
}

fn stream<T>(name: &str, capacity: usize, policy: BackpressurePolicy) -> Stream<T> {
    let context = context(name);
    Stream::new(&context, BackpressureConfig::new(capacity, policy).unwrap()).unwrap()
}

#[test]
fn stream_is_created_with_explicit_operation_ownership() {
    let context = context("ownership");
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();

    assert_eq!(
        stream.owner().operation_id(),
        &context.operation().operation.id
    );
    assert_eq!(
        stream.context().operation().operation.id,
        context.operation().operation.id
    );
}

#[test]
fn stream_allows_only_one_logical_consumer() {
    let stream: Stream<u8> = stream("single-consumer", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();

    let first = stream.consumer().unwrap();
    assert!(matches!(
        stream.consumer(),
        Err(StreamError::ConsumerAlreadyAttached)
    ));

    drop(first);
}

#[test]
fn stream_preserves_logical_item_order() {
    let stream: Stream<u8> = stream("order", 4, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 10)).unwrap();
    stream.publish(StreamItem::partial(1, 20)).unwrap();
    stream.publish(StreamItem::final_item(2, 30)).unwrap();

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &30);
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn partial_item_keeps_stream_open() {
    let stream: Stream<u8> = stream("partial", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 1)).unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Open);
    assert_eq!(
        consumer.next_item().unwrap().unwrap().kind(),
        StreamItemKind::Partial
    );
}

#[test]
fn final_item_completes_stream_only_after_the_item_is_observable() {
    let stream: Stream<u8> = stream("final", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::final_item(0, 9)).unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    assert!(!stream.has_observable_output());
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &9);
    assert!(stream.has_observable_output());
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn empty_stream_can_complete_without_producing_items() {
    let stream: Stream<u8> = stream("empty", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.complete().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    assert_eq!(consumer.next_item().unwrap(), None);
    assert!(!stream.has_observable_output());
}

#[test]
fn publication_after_terminal_state_is_rejected() {
    let stream: Stream<u8> = stream("terminal-publication", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    stream.complete().unwrap();

    assert!(matches!(
        stream.publish(StreamItem::partial(0, 1)),
        Err(StreamError::Lifecycle(_))
    ));
}

#[test]
fn parent_cancellation_reaches_the_stream_context() {
    let context = context("parent-cancellation");
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    context.cancellation().cancel();

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    assert_eq!(consumer.next_item(), Err(StreamError::Cancelled));
}

#[test]
fn stream_cancellation_does_not_cancel_parent_context() {
    let context = context("stream-cancellation");
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();

    stream.cancel().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    assert!(!context.cancellation().is_cancelled());
}

#[test]
fn dropping_the_only_consumer_cancels_an_open_stream() {
    let stream: Stream<u8> = stream("consumer-drop", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    drop(consumer);

    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
}

#[test]
fn deadline_reaches_a_waiting_producer() {
    let context =
        context("deadline-wait").with_deadline(Deadline::from_now(Duration::from_secs(1)).unwrap());
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(1, BackpressurePolicy::Wait).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let _consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 1)).unwrap();

    let producer = stream.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        producer.publish(StreamItem::partial(1, 2))
    });

    started_rx.recv().unwrap();
    assert_eq!(handle.join().unwrap(), Err(StreamError::DeadlineExpired));
    assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
}

#[test]
fn reject_backpressure_is_bounded_and_does_not_drop_the_accepted_item() {
    let stream: Stream<u8> = stream("reject", 1, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 1)).unwrap();

    assert!(matches!(
        stream.publish(StreamItem::partial(1, 2)),
        Err(StreamError::Backpressure(_))
    ));
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &1);
}

#[test]
fn wait_backpressure_blocks_until_capacity_is_released() {
    let stream: Stream<u8> = stream("wait", 1, BackpressurePolicy::Wait);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 1)).unwrap();

    let producer = stream.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (completed_tx, completed_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let result = producer.publish(StreamItem::final_item(1, 2));
        completed_tx.send(result).unwrap();
    });

    started_rx.recv().unwrap();
    assert!(
        completed_rx
            .recv_timeout(Duration::from_millis(25))
            .is_err()
    );

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &1);
    assert_eq!(
        completed_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        Ok(())
    );
    handle.join().unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &2);
}

#[test]
fn invalid_sequence_does_not_corrupt_stream_state() {
    let stream: Stream<u8> = stream("sequence", 3, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    assert_eq!(
        stream.publish(StreamItem::partial(1, 1)),
        Err(StreamError::InvalidSequence {
            expected: 0,
            actual: 1
        })
    );

    stream.publish(StreamItem::partial(0, 7)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7);
}

#[test]
fn buffered_items_remain_observable_before_terminal_failure() {
    let stream: Stream<u8> = stream("buffered-failure", 3, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 1)).unwrap();
    stream.publish(StreamItem::partial(1, 2)).unwrap();
    stream.fail().unwrap();

    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &1);
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &2);
    assert_eq!(consumer.next_item(), Err(StreamError::Failed));
}

#[test]
fn one_logical_stream_item_can_be_larger_than_one_transport_frame() {
    let payload = vec![0xA5; MAX_PAYLOAD_LENGTH + 1];
    let frames = encode_message(321, &payload).unwrap();

    assert_eq!(frames.len(), 2);

    let stream: Stream<Vec<u8>> = stream("fragmented-logical-item", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream
        .publish(StreamItem::final_item(0, payload.clone()))
        .unwrap();

    let item = consumer.next_item().unwrap().unwrap();
    assert_eq!(item.sequence(), 0);
    assert_eq!(item.payload().len(), payload.len());
    assert_eq!(item.payload().as_slice(), payload.as_slice());
}

#[test]
fn multiple_logical_items_remain_distinct_from_transport_frame_boundaries() {
    let stream: Stream<Vec<u8>> = stream("logical-items", 3, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, vec![1, 2])).unwrap();
    stream
        .publish(StreamItem::final_item(1, vec![3, 4]))
        .unwrap();

    assert_eq!(
        consumer.next_item().unwrap().unwrap().payload(),
        &vec![1, 2]
    );
    assert_eq!(
        consumer.next_item().unwrap().unwrap().payload(),
        &vec![3, 4]
    );
    assert_eq!(consumer.next_item().unwrap(), None);
}

#[test]
fn stream_failure_is_reported_as_stream_failure_not_transport_failure() {
    let stream: Stream<u8> = stream("failure-kind", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.fail().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Failed);
    assert_eq!(consumer.next_item(), Err(StreamError::Failed));
}

#[test]
fn observable_stream_output_blocks_retry_through_the_real_retry_safety_gate() {
    let context = context("retry-output");
    let stream: Stream<u8> = Stream::new(
        &context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::final_item(0, 42)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &42);
    assert!(stream.has_observable_output());

    let policy = RetryPolicy::new(2, 3)
        .unwrap()
        .with_retryable_category(FailureCategory::Transient);
    let backoff = BackoffPolicy::no_backoff();
    let admission = RetryAdmission::new(
        &policy,
        &backoff,
        context.cancellation(),
        context.deadline(),
    );
    let current_attempt = Attempt::new(
        context.operation().operation.id.clone(),
        nizaam_core::identity::AttemptId::new("phase16-stream-attempt").unwrap(),
        1,
    )
    .unwrap();
    current_attempt.start().unwrap();
    current_attempt.fail().unwrap();

    let mut budget = nizaam_core::retry::RetryBudget::new(1);
    let result = admission.admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &current_attempt,
        category: FailureCategory::Transient,
        retryability: Retryability::Retryable,
        next_attempt_id: nizaam_core::identity::AttemptId::new("phase16-stream-next-attempt")
            .unwrap(),
        jitter_source: None,
        safety_gates: RetrySafetyGates::new(true, true, true, false),
    });

    assert!(matches!(
        result,
        Err(RetryAdmissionError::SafetyGate(
            nizaam_core::retry::RetrySafetyGate::ObservableOutput
        ))
    ));
}

#[test]
fn stream_completion_is_independent_of_transport_connection_state() {
    let stream: Stream<u8> = stream("completion-boundary", 2, BackpressurePolicy::Reject);
    stream.open().unwrap();
    stream.complete().unwrap();

    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    assert!(stream.state().is_terminal());
}
