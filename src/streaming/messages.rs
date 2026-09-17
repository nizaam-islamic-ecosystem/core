//! Logical messages used by the application-level streaming layer.
//!
//! This module marks the boundary between a logical [`StreamItem`] and the
//! transport layer. It intentionally does not perform serialization,
//! fragmentation, reassembly, buffering, or transport I/O. Those concerns
//! remain owned by their respective Core modules.

use super::item::{StreamItem, StreamItemKind};

/// A logical application-level message carried by a stream.
///
/// The message owns exactly one [`StreamItem`] so stream-item metadata is not
/// duplicated at the message boundary. The payload type remains generic and
/// is defined by the operation or capability contract.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamMessage<T> {
    item: StreamItem<T>,
}

impl<T> StreamMessage<T> {
    /// Creates a logical stream message from an existing stream item.
    pub const fn new(item: StreamItem<T>) -> Self {
        Self { item }
    }

    /// Returns the logical sequence number carried by the stream item.
    pub const fn sequence(&self) -> u64 {
        self.item.sequence()
    }

    /// Returns the classification of the contained stream item.
    pub const fn kind(&self) -> StreamItemKind {
        self.item.kind()
    }

    /// Returns a reference to the contained stream item.
    pub const fn item(&self) -> &StreamItem<T> {
        &self.item
    }

    /// Consumes the message and returns its contained stream item.
    pub fn into_item(self) -> StreamItem<T> {
        self.item
    }
}

impl<T> From<StreamItem<T>> for StreamMessage<T> {
    fn from(item: StreamItem<T>) -> Self {
        Self::new(item)
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamItem, StreamItemKind, StreamMessage};

    #[test]
    fn creates_message_from_partial_item() {
        let item = StreamItem::partial(3, "partial");
        let message = StreamMessage::new(item.clone());

        assert_eq!(message.sequence(), 3);
        assert_eq!(message.kind(), StreamItemKind::Partial);
        assert_eq!(message.item(), &item);
    }

    #[test]
    fn creates_message_from_final_item() {
        let item = StreamItem::final_item(4, "final");
        let message = StreamMessage::new(item.clone());

        assert_eq!(message.sequence(), 4);
        assert_eq!(message.kind(), StreamItemKind::Final);
        assert_eq!(message.item(), &item);
    }

    #[test]
    fn from_conversion_preserves_item() {
        let item = StreamItem::partial(7, String::from("payload"));
        let message: StreamMessage<String> = item.clone().into();

        assert_eq!(message.item(), &item);
    }

    #[test]
    fn into_item_returns_original_item() {
        let item = StreamItem::final_item(8, String::from("payload"));
        let message = StreamMessage::new(item.clone());

        assert_eq!(message.into_item(), item);
    }

    #[test]
    fn sequence_and_kind_are_not_duplicated() {
        let item = StreamItem::partial(11, 42_u32);
        let message = StreamMessage::new(item);

        assert_eq!(message.sequence(), 11);
        assert_eq!(message.kind(), StreamItemKind::Partial);
        assert_eq!(message.item().payload(), &42_u32);
    }

    #[test]
    fn preserves_generic_payload_types() {
        #[derive(Clone, Debug, Eq, PartialEq)]
        struct TestPayload {
            value: u32,
        }

        let item = StreamItem::final_item(12, TestPayload { value: 99 });
        let message = StreamMessage::new(item);

        assert_eq!(message.item().payload().value, 99);
    }

    #[test]
    fn partial_and_final_messages_remain_distinguishable() {
        let partial = StreamMessage::new(StreamItem::partial(13, ()));
        let final_message = StreamMessage::new(StreamItem::final_item(14, ()));

        assert_ne!(partial.kind(), final_message.kind());
    }

    #[test]
    fn message_does_not_change_logical_item_sequence() {
        let item = StreamItem::partial(u64::MAX, "payload");
        let message = StreamMessage::new(item);

        assert_eq!(message.sequence(), u64::MAX);
    }
}
