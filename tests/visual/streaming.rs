use crate::support::{section, show_arrow, step, success};
use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamError, StreamItem, StreamLifecycleState,
};
use nizaam_core::transport::framing::{MAX_PAYLOAD_LENGTH, decode_frame, encode_message};

fn context(name: &str) -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new(format!("visual-stream-{name}")).unwrap(),
        CorrelationId::new(format!("visual-correlation-{name}")).unwrap(),
    )))
}

#[test]
fn visual_logical_streaming_and_transport_fragmentation_are_separate() {
    section("NIZAAM CORE — STREAMING");
    let main_context = context("main");
    let stream: Stream<u32> = Stream::new(
        &main_context,
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    step(1, "stream ownership");
    println!("  operation : {}", main_context.operation().operation.id);
    println!("  capacity  : {}", stream.capacity());
    assert_eq!(stream.capacity(), 2);
    assert_eq!(
        &main_context.operation().operation.id,
        &main_context.operation().operation.id
    );
    success("stream is attached to one operation context");

    step(2, "ordered logical StreamItems");
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 10)).unwrap();
    stream.publish(StreamItem::final_item(1, 20)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 0);
    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 1);
    assert_eq!(consumer.next_item().unwrap(), None);
    assert_eq!(stream.state(), StreamLifecycleState::Completed);
    show_arrow("Producer", "logical StreamItem #1 → #2 → Consumer");
    success("logical ordering is preserved");

    step(3, "bounded backpressure");
    let pressured: Stream<u8> = Stream::new(
        &context("pressure"),
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    pressured.open().unwrap();
    let consumer = pressured.consumer().unwrap();
    pressured.publish(StreamItem::partial(0, 7)).unwrap();
    assert!(matches!(
        pressured.publish(StreamItem::partial(1, 8)),
        Err(StreamError::Backpressure(_))
    ));
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7);
    assert_eq!(pressured.publish(StreamItem::final_item(1, 8)), Ok(()));
    println!("  capacity: 1 / 1 → consumer consumes → producer progresses");
    success("bounded pressure rejects excess work and recovers after consumption");

    step(4, "terminal lifecycle");
    pressured.complete().unwrap();
    assert!(pressured.state().is_terminal());
    assert!(pressured.publish(StreamItem::partial(2, 9)).is_err());
    success("terminal stream rejects later publication");

    step(5, "transport framing is separate");
    let logical = vec![0x44; MAX_PAYLOAD_LENGTH + 9];
    let frames = encode_message(900, &logical).unwrap();
    assert_eq!(frames.len(), 2);
    let first = decode_frame(&frames[0]).unwrap();
    let second = decode_frame(&frames[1]).unwrap();
    assert_eq!(
        first.0.transport_stream_id(),
        second.0.transport_stream_id()
    );
    let mut rebuilt = first.1;
    rebuilt.extend_from_slice(&second.1);
    assert_eq!(rebuilt, logical);
    println!("  StreamItem          ≠ transport frame");
    println!("  logical bytes       : {}", logical.len());
    println!("  transport fragments : {}", frames.len());
    success("transport fragmentation does not become application-level stream items");
}
