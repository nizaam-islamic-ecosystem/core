//! Application-level streaming primitives for Phase 12.
//!
//! The streaming module composes lifecycle, logical items, logical messages,
//! execution context, bounded backpressure, and stream coordination. Transport
//! framing remains owned by [`crate::transport`].

pub mod backpressure;
pub mod context;
pub mod item;
pub mod lifecycle;
pub mod messages;
pub mod stream;

pub use backpressure::{
    BackpressureConfig, BackpressureError, BackpressurePolicy, BackpressureState,
};
pub use context::StreamContext;
pub use item::{StreamItem, StreamItemKind};
pub use lifecycle::{StreamLifecycle, StreamLifecycleError, StreamLifecycleState};
pub use messages::StreamMessage;
pub use stream::{Stream, StreamConsumer, StreamError, StreamId, StreamOwner};

#[cfg(test)]
mod tests {
    use crate::{
        identity::{CorrelationId, OperationId},
        operation::{Operation, OperationContext},
        runtime::EngineContext,
    };

    use super::{
        BackpressureConfig, BackpressureError, BackpressurePolicy, BackpressureState, Stream,
        StreamItem, StreamItemKind, StreamLifecycle, StreamLifecycleState, StreamMessage,
    };

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("stream-module-operation").unwrap(),
            CorrelationId::new("stream-module-correlation").unwrap(),
        )))
    }

    #[test]
    fn module_exports_all_streaming_primitives() {
        let _ = BackpressureConfig::new(2, BackpressurePolicy::Wait).unwrap();
        let _ = BackpressureState::new(2).unwrap();

        let item = StreamItem::partial(0, 42_u32);
        let message = StreamMessage::new(item.clone());

        assert_eq!(message.item(), &item);

        let mut lifecycle = StreamLifecycle::new();
        lifecycle.transition(StreamLifecycleState::Open).unwrap();
        assert_eq!(lifecycle.state(), StreamLifecycleState::Open);

        let stream: Stream<u32> = Stream::new(
            &context(),
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        assert_eq!(stream.state(), StreamLifecycleState::Created);
        assert_eq!(
            stream.owner().operation_id(),
            &context().operation().operation.id
        );
    }

    #[test]
    fn stream_context_and_stream_share_the_same_operation_identity() {
        let engine_context = context();
        let stream: Stream<u32> = Stream::new(
            &engine_context,
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        assert_eq!(
            stream.context().operation().operation.id,
            engine_context.operation().operation.id
        );
        assert_eq!(
            stream.context().operation().operation.correlation_id,
            engine_context.operation().operation.correlation_id
        );
        assert_eq!(
            stream.owner().operation_id(),
            &engine_context.operation().operation.id
        );
    }

    #[test]
    fn logical_item_passes_through_message_and_stream_without_metadata_loss() {
        let item = StreamItem::final_item(0, String::from("result"));
        let message = StreamMessage::from(item.clone());

        assert_eq!(message.sequence(), item.sequence());
        assert_eq!(message.kind(), StreamItemKind::Final);
        assert_eq!(message.item(), &item);

        let stream: Stream<String> = Stream::new(
            &context(),
            BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();
        stream.publish(message.into_item()).unwrap();

        let received = consumer.next_item().unwrap().unwrap();

        assert_eq!(received, item);
        assert_eq!(consumer.next_item().unwrap(), None);
        assert_eq!(stream.state(), StreamLifecycleState::Completed);
    }

    #[test]
    fn bounded_stream_uses_backpressure_state_without_dropping_items() {
        let config = BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap();
        let mut accounting = BackpressureState::new(config.capacity()).unwrap();

        assert_eq!(accounting.push(), Ok(()));
        assert_eq!(accounting.len(), 1);
        assert_eq!(accounting.available(), 0);
        assert_eq!(accounting.push(), Err(BackpressureError::Full));

        accounting.pop().unwrap();
        assert_eq!(accounting.available(), 1);

        let stream: Stream<u32> = Stream::new(&context(), config).unwrap();
        stream.open().unwrap();

        let consumer = stream.consumer().unwrap();
        stream.publish(StreamItem::partial(0, 7_u32)).unwrap();

        assert_eq!(
            stream.publish(StreamItem::partial(1, 8_u32)),
            Err(super::StreamError::Backpressure(BackpressureError::Full))
        );

        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7_u32);
    }

    #[test]
    fn final_item_coordinates_item_semantics_with_lifecycle_completion() {
        let stream: Stream<u32> = Stream::new(
            &context(),
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::partial(0, 10_u32)).unwrap();
        assert_eq!(stream.state(), StreamLifecycleState::Open);

        stream.publish(StreamItem::final_item(1, 20_u32)).unwrap();
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
    fn stream_context_cancellation_terminates_stream_without_affecting_parent() {
        let engine_context = context();
        let stream: Stream<u32> = Stream::new(
            &engine_context,
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.context().cancellation().cancel();

        assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
        assert_eq!(consumer.next_item(), Err(super::StreamError::Cancelled));
        assert!(!engine_context.cancellation().is_cancelled());
    }

    #[test]
    fn stream_can_complete_empty_successfully() {
        let stream: Stream<u32> = Stream::new(
            &context(),
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.complete().unwrap();

        assert_eq!(stream.state(), StreamLifecycleState::Completed);
        assert_eq!(consumer.next_item().unwrap(), None);
    }

    #[test]
    fn lifecycle_module_remains_independently_usable() {
        let mut lifecycle = StreamLifecycle::new();

        assert_eq!(lifecycle.state(), StreamLifecycleState::Created);
        lifecycle.transition(StreamLifecycleState::Open).unwrap();
        lifecycle
            .transition(StreamLifecycleState::Completed)
            .unwrap();

        assert!(lifecycle.is_terminal());
        assert!(
            lifecycle
                .transition(StreamLifecycleState::Completed)
                .is_ok()
        );
        assert!(lifecycle.transition(StreamLifecycleState::Open).is_err());
    }
}
