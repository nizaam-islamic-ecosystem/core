//! Application-level logical stream coordination.
//!
//! A [`Stream`] coordinates ordered logical items, stream lifecycle, bounded
//! buffering, context propagation, and a single logical consumer. It is
//! deliberately independent from transport framing and does not interpret the
//! meaning of an item's payload.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::identity::OperationId;

use super::{
    backpressure::{BackpressureConfig, BackpressureError, BackpressurePolicy, BackpressureState},
    context::StreamContext,
    item::StreamItem,
    lifecycle::{StreamLifecycle, StreamLifecycleError, StreamLifecycleState},
};

static NEXT_STREAM_ID: AtomicU64 = AtomicU64::new(1);
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Uniquely identifies an application-level stream.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StreamId(u64);

impl StreamId {
    const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric stream identifier.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Identifies the owner of a stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamOwner {
    operation_id: OperationId,
}

impl StreamOwner {
    fn operation(operation_id: OperationId) -> Self {
        Self { operation_id }
    }

    /// Returns the operation that owns this stream.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
}

/// Errors produced while coordinating an application-level stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamError {
    /// The requested lifecycle transition is invalid.
    Lifecycle(StreamLifecycleError),
    /// The submitted item does not have the next required logical sequence.
    InvalidSequence { expected: u64, actual: u64 },
    /// The logical sequence space has been exhausted by a non-final item.
    SequenceExhausted,
    /// The stream already has its single logical consumer.
    ConsumerAlreadyAttached,
    /// The stream is not active and cannot perform the requested operation.
    NotOpen,
    /// The stream was cancelled.
    Cancelled,
    /// The stream reached its inherited deadline and was cancelled.
    DeadlineExpired,
    /// The stream terminated because of an execution failure.
    Failed,
    /// A bounded publication operation was rejected because the buffer is full.
    Backpressure(BackpressureError),
    /// No further stream identifiers are available.
    IdExhausted,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lifecycle(error) => error.fmt(formatter),
            Self::InvalidSequence { expected, actual } => write!(
                formatter,
                "invalid stream item sequence: expected {expected}, received {actual}"
            ),
            Self::SequenceExhausted => formatter.write_str("stream sequence space exhausted"),
            Self::ConsumerAlreadyAttached => {
                formatter.write_str("stream already has a logical consumer")
            }
            Self::NotOpen => formatter.write_str("stream is not open"),
            Self::Cancelled => formatter.write_str("stream is cancelled"),
            Self::DeadlineExpired => formatter.write_str("stream deadline expired"),
            Self::Failed => formatter.write_str("stream failed"),
            Self::Backpressure(error) => error.fmt(formatter),
            Self::IdExhausted => formatter.write_str("stream identifier space exhausted"),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<StreamLifecycleError> for StreamError {
    fn from(error: StreamLifecycleError) -> Self {
        Self::Lifecycle(error)
    }
}

impl From<BackpressureError> for StreamError {
    fn from(error: BackpressureError) -> Self {
        Self::Backpressure(error)
    }
}

struct StreamState<T> {
    lifecycle: StreamLifecycle,
    queue: VecDeque<StreamItem<T>>,
    backpressure: BackpressureState,
    next_sequence: u64,
    sequence_exhausted: bool,
    consumer_attached: bool,
}

struct StreamInner<T> {
    id: StreamId,
    owner: StreamOwner,
    context: StreamContext,
    config: BackpressureConfig,
    state: Mutex<StreamState<T>>,
    changed: Condvar,
}

impl<T> StreamInner<T> {
    fn new(
        id: StreamId,
        owner: StreamOwner,
        context: StreamContext,
        config: BackpressureConfig,
    ) -> Result<Self, StreamError> {
        let backpressure = BackpressureState::new(config.capacity())?;

        Ok(Self {
            id,
            owner,
            context,
            config,
            state: Mutex::new(StreamState {
                lifecycle: StreamLifecycle::new(),
                queue: VecDeque::new(),
                backpressure,
                next_sequence: 0,
                sequence_exhausted: false,
                consumer_attached: false,
            }),
            changed: Condvar::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StreamState<T>> {
        self.state.lock().expect("stream state lock poisoned")
    }

    fn synchronize_context_locked(&self, state: &mut StreamState<T>) -> Result<(), StreamError> {
        if state.lifecycle.state().is_terminal() {
            return Ok(());
        }

        if self.context.is_cancelled() {
            self.transition_terminal_locked(state, StreamLifecycleState::Cancelled)?;
            return Err(StreamError::Cancelled);
        }

        if self.context.is_expired() {
            self.context.cancellation().cancel();
            self.transition_terminal_locked(state, StreamLifecycleState::Cancelled)?;
            return Err(StreamError::DeadlineExpired);
        }

        Ok(())
    }

    fn transition_terminal_locked(
        &self,
        state: &mut StreamState<T>,
        terminal: StreamLifecycleState,
    ) -> Result<(), StreamError> {
        state.lifecycle.transition(terminal)?;
        self.changed.notify_all();
        Ok(())
    }

    fn wait_timeout<'a>(
        &self,
        guard: std::sync::MutexGuard<'a, StreamState<T>>,
        duration: Duration,
    ) -> std::sync::MutexGuard<'a, StreamState<T>> {
        self.changed
            .wait_timeout(guard, duration)
            .expect("stream condition variable wait failed")
            .0
    }
}

/// A bounded application-level stream of logical items.
#[derive(Clone)]
pub struct Stream<T> {
    inner: Arc<StreamInner<T>>,
}

impl<T> std::fmt::Debug for Stream<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stream")
            .field("id", &self.id())
            .field("owner", self.owner())
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl<T> Stream<T> {
    /// Creates an operation-owned stream in the `Created` state.
    pub fn new(
        context: &crate::runtime::EngineContext,
        config: BackpressureConfig,
    ) -> Result<Self, StreamError> {
        let operation_id = context.operation().operation.id.clone();
        let owner = StreamOwner::operation(operation_id);
        let stream_context = StreamContext::from_engine_context(context);
        let id = next_stream_id()?;

        Ok(Self {
            inner: Arc::new(StreamInner::new(id, owner, stream_context, config)?),
        })
    }

    /// Returns this stream's unique identifier.
    pub fn id(&self) -> StreamId {
        self.inner.id
    }

    /// Returns this stream's explicit owner.
    pub fn owner(&self) -> &StreamOwner {
        &self.inner.owner
    }

    /// Returns this stream's execution context.
    pub fn context(&self) -> &StreamContext {
        &self.inner.context
    }

    /// Returns the configured backpressure policy.
    pub fn backpressure_policy(&self) -> BackpressurePolicy {
        self.inner.config.policy()
    }

    /// Returns the configured buffer capacity.
    pub fn capacity(&self) -> usize {
        self.inner.config.capacity()
    }

    /// Returns the current stream lifecycle state.
    pub fn state(&self) -> StreamLifecycleState {
        let mut state = self.inner.lock();
        let _ = self.inner.synchronize_context_locked(&mut state);
        state.lifecycle.state()
    }

    /// Opens the stream for logical item publication and consumption.
    pub fn open(&self) -> Result<(), StreamError> {
        let mut state = self.inner.lock();
        self.inner.synchronize_context_locked(&mut state)?;
        state.lifecycle.transition(StreamLifecycleState::Open)?;
        self.inner.changed.notify_all();
        Ok(())
    }

    /// Publishes one accepted logical stream item.
    pub fn publish(&self, item: StreamItem<T>) -> Result<(), StreamError> {
        let mut state = self.inner.lock();

        loop {
            self.inner.synchronize_context_locked(&mut state)?;

            match state.lifecycle.state() {
                StreamLifecycleState::Created => return Err(StreamError::NotOpen),
                StreamLifecycleState::Completed => {
                    return Err(StreamError::Lifecycle(StreamLifecycleError::new(
                        StreamLifecycleState::Completed,
                        StreamLifecycleState::Open,
                    )));
                }
                StreamLifecycleState::Cancelled => return Err(StreamError::Cancelled),
                StreamLifecycleState::Failed => return Err(StreamError::Failed),
                StreamLifecycleState::Open => {}
            }

            if state.sequence_exhausted {
                return Err(StreamError::SequenceExhausted);
            }

            let expected = state.next_sequence;
            if item.sequence() != expected {
                return Err(StreamError::InvalidSequence {
                    expected,
                    actual: item.sequence(),
                });
            }

            if !state.backpressure.is_full() {
                state.backpressure.push()?;
                let is_final = item.kind() == super::item::StreamItemKind::Final;
                state.queue.push_back(item);

                if expected == u64::MAX {
                    state.sequence_exhausted = true;
                } else {
                    state.next_sequence += 1;
                }

                if is_final {
                    self.inner
                        .transition_terminal_locked(&mut state, StreamLifecycleState::Completed)?;
                } else {
                    self.inner.changed.notify_all();
                }

                return Ok(());
            }

            match self.inner.config.policy() {
                BackpressurePolicy::Reject => return Err(BackpressureError::Full.into()),
                BackpressurePolicy::Wait => {
                    if let Some(deadline) = self.inner.context.deadline() {
                        let remaining = deadline.remaining();
                        if remaining.is_zero() {
                            self.inner.context.cancellation().cancel();
                            self.inner.transition_terminal_locked(
                                &mut state,
                                StreamLifecycleState::Cancelled,
                            )?;
                            return Err(StreamError::DeadlineExpired);
                        }

                        state = self
                            .inner
                            .wait_timeout(state, remaining.min(CANCELLATION_POLL_INTERVAL));
                    } else {
                        state = self.inner.wait_timeout(state, CANCELLATION_POLL_INTERVAL);
                    }
                }
            }
        }
    }

    /// Completes the stream successfully without requiring a final item.
    pub fn complete(&self) -> Result<(), StreamError> {
        let mut state = self.inner.lock();
        self.inner.synchronize_context_locked(&mut state)?;

        match state.lifecycle.state() {
            StreamLifecycleState::Completed => Ok(()),
            StreamLifecycleState::Open => {
                state
                    .lifecycle
                    .transition(StreamLifecycleState::Completed)?;
                self.inner.changed.notify_all();
                Ok(())
            }
            StreamLifecycleState::Cancelled => Err(StreamError::Cancelled),
            StreamLifecycleState::Failed => Err(StreamError::Failed),
            StreamLifecycleState::Created => Err(StreamError::NotOpen),
        }
    }

    /// Cancels the stream and wakes blocked producers and consumers.
    pub fn cancel(&self) -> Result<(), StreamError> {
        let mut state = self.inner.lock();

        match state.lifecycle.state() {
            StreamLifecycleState::Cancelled => Ok(()),
            StreamLifecycleState::Completed => {
                Err(StreamError::Lifecycle(StreamLifecycleError::new(
                    StreamLifecycleState::Completed,
                    StreamLifecycleState::Cancelled,
                )))
            }
            StreamLifecycleState::Failed => Err(StreamError::Lifecycle(StreamLifecycleError::new(
                StreamLifecycleState::Failed,
                StreamLifecycleState::Cancelled,
            ))),
            StreamLifecycleState::Created => Err(StreamError::NotOpen),
            StreamLifecycleState::Open => {
                self.inner.context.cancellation().cancel();
                state
                    .lifecycle
                    .transition(StreamLifecycleState::Cancelled)?;
                self.inner.changed.notify_all();
                Ok(())
            }
        }
    }

    /// Marks the stream as failed and wakes blocked producers and consumers.
    pub fn fail(&self) -> Result<(), StreamError> {
        let mut state = self.inner.lock();

        match state.lifecycle.state() {
            StreamLifecycleState::Failed => Ok(()),
            StreamLifecycleState::Completed => {
                Err(StreamError::Lifecycle(StreamLifecycleError::new(
                    StreamLifecycleState::Completed,
                    StreamLifecycleState::Failed,
                )))
            }
            StreamLifecycleState::Cancelled => {
                Err(StreamError::Lifecycle(StreamLifecycleError::new(
                    StreamLifecycleState::Cancelled,
                    StreamLifecycleState::Failed,
                )))
            }
            StreamLifecycleState::Created => Err(StreamError::NotOpen),
            StreamLifecycleState::Open => {
                state.lifecycle.transition(StreamLifecycleState::Failed)?;
                self.inner.changed.notify_all();
                Ok(())
            }
        }
    }

    /// Claims the stream's single logical consumer.
    pub fn consumer(&self) -> Result<StreamConsumer<T>, StreamError> {
        let mut state = self.inner.lock();
        self.inner.synchronize_context_locked(&mut state)?;

        if state.lifecycle.state() != StreamLifecycleState::Open {
            return match state.lifecycle.state() {
                StreamLifecycleState::Cancelled => Err(StreamError::Cancelled),
                StreamLifecycleState::Failed => Err(StreamError::Failed),
                StreamLifecycleState::Completed => {
                    Err(StreamError::Lifecycle(StreamLifecycleError::new(
                        StreamLifecycleState::Completed,
                        StreamLifecycleState::Open,
                    )))
                }
                StreamLifecycleState::Created => Err(StreamError::NotOpen),
                StreamLifecycleState::Open => unreachable!(),
            };
        }

        if state.consumer_attached {
            return Err(StreamError::ConsumerAlreadyAttached);
        }

        state.consumer_attached = true;

        Ok(StreamConsumer {
            inner: Arc::clone(&self.inner),
        })
    }
}

/// The single logical consumer handle for a [`Stream`].
///
/// The handle is intentionally not `Clone`, which prevents implicit stream
/// fan-out in the default abstraction.
pub struct StreamConsumer<T> {
    inner: Arc<StreamInner<T>>,
}

impl<T> std::fmt::Debug for StreamConsumer<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamConsumer")
            .field("stream_id", &self.inner.id)
            .finish_non_exhaustive()
    }
}

impl<T> StreamConsumer<T> {
    /// Returns the next accepted logical item.
    ///
    /// `None` means the stream completed successfully and all accepted items
    /// have been consumed. Buffered items accepted before cancellation or
    /// failure remain observable before their terminal error is returned.
    pub fn next_item(&self) -> Result<Option<StreamItem<T>>, StreamError> {
        let mut state = self.inner.lock();

        loop {
            if let Some(item) = state.queue.pop_front() {
                state.backpressure.pop()?;
                self.inner.changed.notify_all();
                return Ok(Some(item));
            }

            match self.inner.synchronize_context_locked(&mut state) {
                Ok(()) => {}
                Err(error) => return Err(error),
            }

            match state.lifecycle.state() {
                StreamLifecycleState::Created => return Err(StreamError::NotOpen),
                StreamLifecycleState::Completed => return Ok(None),
                StreamLifecycleState::Cancelled => return Err(StreamError::Cancelled),
                StreamLifecycleState::Failed => return Err(StreamError::Failed),
                StreamLifecycleState::Open => {
                    if let Some(deadline) = self.inner.context.deadline() {
                        let remaining = deadline.remaining();
                        if remaining.is_zero() {
                            self.inner.context.cancellation().cancel();
                            self.inner.transition_terminal_locked(
                                &mut state,
                                StreamLifecycleState::Cancelled,
                            )?;
                            return Err(StreamError::DeadlineExpired);
                        }

                        state = self
                            .inner
                            .wait_timeout(state, remaining.min(CANCELLATION_POLL_INTERVAL));
                    } else {
                        state = self.inner.wait_timeout(state, CANCELLATION_POLL_INTERVAL);
                    }
                }
            }
        }
    }
}

impl<T> Drop for StreamConsumer<T> {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        state.consumer_attached = false;

        if state.lifecycle.state() == StreamLifecycleState::Open {
            self.inner.context.cancellation().cancel();
            let _ = state.lifecycle.transition(StreamLifecycleState::Cancelled);
        }

        self.inner.changed.notify_all();
    }
}

fn next_stream_id() -> Result<StreamId, StreamError> {
    NEXT_STREAM_ID
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .map(StreamId::new)
        .map_err(|_| StreamError::IdExhausted)
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use crate::{
        identity::{CorrelationId, OperationId},
        operation::{Operation, OperationContext},
        runtime::EngineContext,
    };

    use super::{
        BackpressureConfig, BackpressurePolicy, Stream, StreamError, StreamItem,
        StreamLifecycleState,
    };

    fn context() -> EngineContext {
        let operation = Operation::new(
            OperationId::new("stream-operation").unwrap(),
            CorrelationId::new("stream-correlation").unwrap(),
        );

        EngineContext::new(OperationContext::new(operation))
    }

    fn stream(capacity: usize, policy: BackpressurePolicy) -> Stream<u32> {
        Stream::new(
            &context(),
            BackpressureConfig::new(capacity, policy).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn new_stream_is_created_and_operation_owned() {
        let engine = context();
        let stream: Stream<u32> = Stream::new(
            &engine,
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();

        assert_eq!(stream.state(), StreamLifecycleState::Created);
        assert_eq!(
            stream.owner().operation_id(),
            &engine.operation().operation.id
        );
        assert_eq!(stream.capacity(), 2);
        assert_eq!(stream.backpressure_policy(), BackpressurePolicy::Reject);
    }

    #[test]
    fn stream_ids_are_distinct() {
        let first = stream(1, BackpressurePolicy::Reject);
        let second = stream(1, BackpressurePolicy::Reject);

        assert_ne!(first.id(), second.id());
        assert_ne!(first.id().get(), second.id().get());
    }

    #[test]
    fn open_transitions_created_stream_to_open() {
        let stream = stream(2, BackpressurePolicy::Reject);

        assert_eq!(stream.open(), Ok(()));
        assert_eq!(stream.state(), StreamLifecycleState::Open);
    }

    #[test]
    fn open_is_idempotent_while_already_open() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();

        assert_eq!(stream.open(), Ok(()));
        assert_eq!(stream.state(), StreamLifecycleState::Open);
    }

    #[test]
    fn publication_and_consumption_preserve_logical_order() {
        let stream = stream(3, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::partial(0, 10)).unwrap();
        stream.publish(StreamItem::partial(1, 20)).unwrap();
        stream.publish(StreamItem::final_item(2, 30)).unwrap();

        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &30);
        assert_eq!(consumer.next_item().unwrap(), None);
        assert_eq!(stream.state(), StreamLifecycleState::Completed);
    }

    #[test]
    fn partial_item_keeps_stream_open() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();

        stream.publish(StreamItem::partial(0, 10)).unwrap();

        assert_eq!(stream.state(), StreamLifecycleState::Open);
    }

    #[test]
    fn final_item_completes_stream_atomically_with_publication() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::final_item(0, 10)).unwrap();

        assert_eq!(stream.state(), StreamLifecycleState::Completed);
        assert!(consumer.next_item().unwrap().is_some());
        assert_eq!(consumer.next_item().unwrap(), None);
    }

    #[test]
    fn explicit_completion_supports_empty_streams() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        assert_eq!(stream.complete(), Ok(()));
        assert_eq!(consumer.next_item().unwrap(), None);
        assert_eq!(stream.state(), StreamLifecycleState::Completed);
    }

    #[test]
    fn invalid_sequence_is_rejected_without_mutating_stream() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        assert_eq!(
            stream.publish(StreamItem::partial(1, 10)),
            Err(StreamError::InvalidSequence {
                expected: 0,
                actual: 1
            })
        );
        stream.publish(StreamItem::partial(0, 20)).unwrap();

        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
    }

    #[test]
    fn publication_after_completion_is_rejected() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        stream.complete().unwrap();

        assert!(matches!(
            stream.publish(StreamItem::partial(0, 10)),
            Err(StreamError::Lifecycle(_))
        ));
    }

    #[test]
    fn publication_after_cancellation_is_rejected() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        stream.cancel().unwrap();

        assert_eq!(
            stream.publish(StreamItem::partial(0, 10)),
            Err(StreamError::Cancelled)
        );
    }

    #[test]
    fn publication_after_failure_is_rejected() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        stream.fail().unwrap();

        assert_eq!(
            stream.publish(StreamItem::partial(0, 10)),
            Err(StreamError::Failed)
        );
    }

    #[test]
    fn termination_is_idempotent_for_the_same_terminal_state() {
        let cancelled: Stream<u32> = stream(2, BackpressurePolicy::Reject);
        cancelled.open().unwrap();

        cancelled.cancel().unwrap();
        assert_eq!(cancelled.cancel(), Ok(()));

        let completed = stream(2, BackpressurePolicy::Reject);
        completed.open().unwrap();
        completed.complete().unwrap();
        assert_eq!(completed.complete(), Ok(()));

        let failed = stream(2, BackpressurePolicy::Reject);
        failed.open().unwrap();
        failed.fail().unwrap();
        assert_eq!(failed.fail(), Ok(()));
    }

    #[test]
    fn different_terminal_transitions_are_rejected() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        stream.complete().unwrap();

        assert!(matches!(stream.cancel(), Err(StreamError::Lifecycle(_))));
        assert!(matches!(stream.fail(), Err(StreamError::Lifecycle(_))));
    }

    #[test]
    fn only_one_consumer_can_be_attached() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let _first = stream.consumer().unwrap();

        assert!(matches!(
            stream.consumer(),
            Err(StreamError::ConsumerAlreadyAttached)
        ));
    }

    #[test]
    fn consumer_drop_cancels_open_stream() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        drop(consumer);

        assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    }

    #[test]
    fn consumer_drop_does_not_rewrite_completed_stream() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.complete().unwrap();
        drop(consumer);

        assert_eq!(stream.state(), StreamLifecycleState::Completed);
    }

    #[test]
    fn reject_policy_returns_full_without_dropping_items() {
        let stream = stream(1, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::partial(0, 10)).unwrap();

        assert_eq!(
            stream.publish(StreamItem::partial(1, 20)),
            Err(StreamError::Backpressure(super::BackpressureError::Full))
        );
        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
    }

    #[test]
    fn wait_policy_unblocks_when_consumer_releases_capacity() {
        let stream = stream(1, BackpressurePolicy::Wait);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();
        stream.publish(StreamItem::partial(0, 10)).unwrap();

        let producer = stream.clone();
        let handle = thread::spawn(move || producer.publish(StreamItem::partial(1, 20)));

        thread::sleep(Duration::from_millis(25));
        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);

        assert_eq!(handle.join().unwrap(), Ok(()));
        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &20);
    }

    #[test]
    fn waiting_producer_observes_stream_cancellation() {
        let stream = stream(1, BackpressurePolicy::Wait);
        stream.open().unwrap();
        let _consumer = stream.consumer().unwrap();
        stream.publish(StreamItem::partial(0, 10)).unwrap();

        let producer = stream.clone();
        let handle = thread::spawn(move || producer.publish(StreamItem::partial(1, 20)));

        thread::sleep(Duration::from_millis(25));
        stream.cancel().unwrap();

        assert_eq!(handle.join().unwrap(), Err(StreamError::Cancelled));
    }

    #[test]
    fn deadline_expiration_cancels_waiting_producer() {
        let deadline = crate::runtime::Deadline::from_now(Duration::from_millis(30)).unwrap();
        let engine = context().with_deadline(deadline);
        let stream = Stream::new(
            &engine,
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
    fn parent_context_cancellation_reaches_stream() {
        let engine = context();
        let stream: Stream<u32> = Stream::new(
            &engine,
            BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();
        stream.open().unwrap();

        engine.cancellation().cancel();

        assert_eq!(stream.state(), StreamLifecycleState::Cancelled);
    }

    #[test]
    fn cancellation_preserves_already_accepted_items() {
        let stream = stream(2, BackpressurePolicy::Reject);
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::partial(0, 10)).unwrap();
        stream.cancel().unwrap();

        assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &10);
        assert_eq!(consumer.next_item(), Err(StreamError::Cancelled));
    }
}
