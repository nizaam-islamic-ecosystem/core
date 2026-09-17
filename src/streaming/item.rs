//! Logical application-level stream items.
//!
//! This module defines the immutable item that flows through a Phase 12
//! application stream. It intentionally knows nothing about stream lifecycle,
//! buffering, cancellation, runtime scheduling, or transport framing.

/// Identifies whether a logical stream item is an intermediate or final result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamItemKind {
    /// More logical stream items may follow.
    Partial,
    /// No further logical stream items should be successfully produced.
    Final,
}

/// An immutable logical application-level item published by a stream.
///
/// `T` is defined by the operation or capability contract. Core does not
/// interpret the payload's domain-specific meaning.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamItem<T> {
    sequence: u64,
    kind: StreamItemKind,
    payload: T,
}

impl<T> StreamItem<T> {
    /// Creates a stream item with the supplied sequence number, kind, and payload.
    pub const fn new(sequence: u64, kind: StreamItemKind, payload: T) -> Self {
        Self {
            sequence,
            kind,
            payload,
        }
    }

    /// Creates a partial stream item.
    pub const fn partial(sequence: u64, payload: T) -> Self {
        Self::new(sequence, StreamItemKind::Partial, payload)
    }

    /// Creates a final stream item.
    pub const fn final_item(sequence: u64, payload: T) -> Self {
        Self::new(sequence, StreamItemKind::Final, payload)
    }

    /// Returns the item's logical sequence number.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the item's classification.
    pub const fn kind(&self) -> StreamItemKind {
        self.kind
    }

    /// Returns a reference to the item's payload.
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the item and returns its payload.
    pub fn into_payload(self) -> T {
        self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamItem, StreamItemKind};

    #[test]
    fn creates_partial_item() {
        let item = StreamItem::partial(0, "first");

        assert_eq!(item.sequence(), 0);
        assert_eq!(item.kind(), StreamItemKind::Partial);
        assert_eq!(item.payload(), &"first");
    }

    #[test]
    fn creates_final_item() {
        let item = StreamItem::final_item(7, "last");

        assert_eq!(item.sequence(), 7);
        assert_eq!(item.kind(), StreamItemKind::Final);
        assert_eq!(item.payload(), &"last");
    }

    #[test]
    fn generic_constructor_preserves_all_fields() {
        let item = StreamItem::new(42, StreamItemKind::Partial, 123_u32);

        assert_eq!(item.sequence(), 42);
        assert_eq!(item.kind(), StreamItemKind::Partial);
        assert_eq!(item.payload(), &123);
    }

    #[test]
    fn sequence_supports_full_u64_range() {
        let first = StreamItem::partial(0, ());
        let last = StreamItem::final_item(u64::MAX, ());

        assert_eq!(first.sequence(), 0);
        assert_eq!(last.sequence(), u64::MAX);
    }

    #[test]
    fn partial_and_final_are_distinguishable() {
        let partial = StreamItem::partial(1, ());
        let final_item = StreamItem::final_item(2, ());

        assert_ne!(partial.kind(), final_item.kind());
    }

    #[test]
    fn payload_can_be_borrowed_without_consuming_item() {
        let item = StreamItem::partial(3, String::from("payload"));

        assert_eq!(item.payload(), "payload");
        assert_eq!(item.sequence(), 3);
    }

    #[test]
    fn into_payload_consumes_item() {
        let item = StreamItem::final_item(4, String::from("payload"));

        assert_eq!(item.into_payload(), "payload");
    }

    #[test]
    fn supports_payload_types_without_domain_specific_semantics() {
        #[derive(Debug, Eq, PartialEq)]
        struct TestPayload {
            value: u32,
        }

        let item = StreamItem::partial(5, TestPayload { value: 99 });

        assert_eq!(item.payload().value, 99);
    }

    #[test]
    fn item_metadata_is_not_mutable_through_public_api() {
        let item = StreamItem::partial(6, 10_u32);

        assert_eq!(item.sequence(), 6);
        assert_eq!(item.kind(), StreamItemKind::Partial);
        assert_eq!(item.payload(), &10);
    }
}
