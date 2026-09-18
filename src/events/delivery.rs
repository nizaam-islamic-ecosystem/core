//! Bounded, isolated delivery infrastructure for internal events.
//!
//! This module deliberately owns delivery mechanics only. Publisher matching,
//! event semantics, security, subscription lifecycle, retry, persistence, and
//! transport remain owned by their respective Core subsystems.

use std::collections::{HashSet, VecDeque};
use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::{
    events::{Event, EventSubscription, SubscriptionLifecycleState},
    operation::CancellationToken,
    runtime::{BackgroundTasks, BoundedSpawnError, ConcurrencyConfig},
};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Configuration for bounded internal event delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryConfig {
    queue_capacity: usize,
    worker_count: usize,
    max_subscriptions: usize,
}

/// Errors returned while constructing or operating the delivery dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryError {
    /// A delivery configuration value was zero.
    ZeroLimit,
    /// A delivery worker could not be admitted through the existing bounded
    /// background-task mechanism.
    WorkerSpawn(BoundedSpawnError),
    /// The dispatcher has already shut down.
    DispatcherClosed,
    /// The maximum number of active delivery subscriptions has been reached.
    SubscriptionLimitReached,
    /// The subscription is already registered with this dispatcher.
    SubscriptionAlreadyRegistered,
    /// The subscription has not been activated.
    SubscriptionNotActive,
    /// The existing Core authorization mechanism failed while admitting delivery.
    AuthorizationFailed(crate::security::AuthorizationError),
    /// The bounded worker scheduler could not accept a scheduling token.
    SchedulerSaturated,
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroLimit => {
                formatter.write_str("event delivery limits must be greater than zero")
            }
            Self::WorkerSpawn(error) => {
                write!(formatter, "failed to spawn event delivery worker: {error}")
            }
            Self::DispatcherClosed => formatter.write_str("event delivery dispatcher is closed"),
            Self::SubscriptionLimitReached => {
                formatter.write_str("event delivery subscription limit is full")
            }
            Self::SubscriptionAlreadyRegistered => {
                formatter.write_str("event subscription is already registered")
            }
            Self::SubscriptionNotActive => formatter.write_str("event subscription is not active"),
            Self::AuthorizationFailed(error) => {
                write!(formatter, "event delivery authorization failed: {error}")
            }
            Self::SchedulerSaturated => {
                formatter.write_str("event delivery scheduler capacity is exhausted")
            }
        }
    }
}

impl std::error::Error for DeliveryError {}

/// Result of attempting to enqueue an event for one subscription.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryOutcome {
    /// The event was accepted into the bounded subscription queue.
    Accepted,
    /// The subscription queue was full, so the newest event was dropped.
    Dropped,
    /// The subscription was cancelled and the event was not delivered.
    Cancelled,
    /// The subscription or dispatcher is closed.
    Closed,
    /// The subscription is not authorized to receive the event.
    Unauthorized,
    /// The bounded scheduler could not accept the subscription's ready token.
    Rejected,
}

impl DeliveryConfig {
    /// Creates bounded delivery configuration.
    pub const fn new(
        queue_capacity: usize,
        worker_count: usize,
        max_subscriptions: usize,
    ) -> Result<Self, DeliveryError> {
        if queue_capacity == 0 || worker_count == 0 || max_subscriptions == 0 {
            return Err(DeliveryError::ZeroLimit);
        }

        Ok(Self {
            queue_capacity,
            worker_count,
            max_subscriptions,
        })
    }

    /// Returns the configured per-subscription queue capacity.
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    /// Returns the number of delivery workers.
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    /// Returns the maximum number of simultaneously registered subscriptions.
    pub const fn max_subscriptions(self) -> usize {
        self.max_subscriptions
    }
}

/// Shared dispatcher state referenced by delivery paths and workers.
struct DeliveryShared {
    ready: ReadyQueue,
    cancellation: CancellationToken,
    config: DeliveryConfig,
    registered_subscriptions: Mutex<HashSet<usize>>,
    closed: AtomicBool,
}

impl DeliveryShared {
    fn new(config: DeliveryConfig, cancellation: CancellationToken) -> Self {
        Self {
            ready: ReadyQueue::new(config.max_subscriptions()),
            cancellation,
            config,
            registered_subscriptions: Mutex::new(HashSet::new()),
            closed: AtomicBool::new(false),
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn try_register(&self, subscription: &Arc<EventSubscription>) -> Result<(), DeliveryError> {
        if self.is_closed() || self.is_cancelled() {
            return Err(DeliveryError::DispatcherClosed);
        }

        let key = Arc::as_ptr(subscription) as usize;
        let mut registered = self
            .registered_subscriptions
            .lock()
            .expect("event registration lock poisoned");

        if self.is_closed() || self.is_cancelled() {
            return Err(DeliveryError::DispatcherClosed);
        }

        if registered.contains(&key) {
            return Err(DeliveryError::SubscriptionAlreadyRegistered);
        }

        if registered.len() >= self.config.max_subscriptions() {
            return Err(DeliveryError::SubscriptionLimitReached);
        }

        registered.insert(key);

        Ok(())
    }

    fn unregister(&self, subscription: &Arc<EventSubscription>) {
        let key = Arc::as_ptr(subscription) as usize;
        self.registered_subscriptions
            .lock()
            .expect("event registration lock poisoned")
            .remove(&key);
    }

    fn close(&self) {
        let should_close = {
            let mut registered = self
                .registered_subscriptions
                .lock()
                .expect("event registration lock poisoned");

            if self.closed.swap(true, Ordering::AcqRel) {
                false
            } else {
                registered.clear();
                true
            }
        };

        if should_close {
            self.cancellation.cancel();
            self.ready.close();
        }
    }
}

/// Bounded scheduler shared by all delivery workers.
struct ReadyQueue {
    state: Mutex<ReadyQueueState>,
    available: Condvar,
    capacity: usize,
}

struct ReadyQueueState {
    closed: bool,
    entries: VecDeque<Arc<DeliveryPath>>,
}

impl ReadyQueue {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(ReadyQueueState {
                closed: false,
                entries: VecDeque::with_capacity(capacity),
            }),
            available: Condvar::new(),
            capacity,
        }
    }

    fn push(&self, path: Arc<DeliveryPath>) -> Result<(), DeliveryError> {
        let mut state = self.state.lock().expect("event ready queue lock poisoned");

        if state.closed || path.is_closed() || !path.is_registered() {
            return Err(DeliveryError::DispatcherClosed);
        }

        if state.entries.len() >= self.capacity {
            return Err(DeliveryError::SchedulerSaturated);
        }

        state.entries.push_back(path);
        self.available.notify_one();
        Ok(())
    }

    fn pop(&self, cancellation: &CancellationToken) -> Option<Arc<DeliveryPath>> {
        let mut state = self.state.lock().expect("event ready queue lock poisoned");

        loop {
            if let Some(path) = state.entries.pop_front() {
                return Some(path);
            }

            if state.closed || cancellation.is_cancelled() {
                return None;
            }

            let (guard, _) = self
                .available
                .wait_timeout(state, CANCELLATION_POLL_INTERVAL)
                .expect("event ready queue lock poisoned");
            state = guard;
        }
    }

    fn remove(&self, target: &Arc<DeliveryPath>) {
        let mut state = self.state.lock().expect("event ready queue lock poisoned");
        state.entries.retain(|path| !Arc::ptr_eq(path, target));
    }

    fn close(&self) {
        let mut state = self.state.lock().expect("event ready queue lock poisoned");
        state.closed = true;
        state.entries.clear();
        self.available.notify_all();
    }
}

/// Owns one subscription's bounded queue and its delivery scheduling state.
struct DeliveryPath {
    subscription: Arc<EventSubscription>,
    shared: Arc<DeliveryShared>,
    queue: Mutex<DeliveryQueueState>,
    scheduled: AtomicBool,
    closed: AtomicBool,
    registered: AtomicBool,
}

struct DeliveryQueueState {
    events: VecDeque<Arc<Event>>,
}

impl DeliveryPath {
    fn new(subscription: Arc<EventSubscription>, shared: Arc<DeliveryShared>) -> Arc<Self> {
        Arc::new(Self {
            subscription,
            shared: Arc::clone(&shared),
            queue: Mutex::new(DeliveryQueueState {
                events: VecDeque::with_capacity(shared.config.queue_capacity()),
            }),
            scheduled: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            registered: AtomicBool::new(true),
        })
    }

    fn subscription(&self) -> &Arc<EventSubscription> {
        &self.subscription
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }

    fn enqueue(self: &Arc<Self>, event: Arc<Event>) -> Result<DeliveryOutcome, DeliveryError> {
        if self.shared.is_closed() {
            return Ok(DeliveryOutcome::Closed);
        }

        match self.subscription.authorization_decision() {
            Ok(crate::security::AuthorizationDecision::Allow) => {}
            Ok(crate::security::AuthorizationDecision::Deny) => {
                return Ok(DeliveryOutcome::Unauthorized);
            }
            Err(error) => {
                return Err(DeliveryError::AuthorizationFailed(error));
            }
        }

        match self.subscription.state() {
            SubscriptionLifecycleState::Cancelled => {
                self.close_and_unregister();
                return Ok(DeliveryOutcome::Cancelled);
            }
            SubscriptionLifecycleState::Closed => {
                self.close_and_unregister();
                return Ok(DeliveryOutcome::Closed);
            }
            SubscriptionLifecycleState::Created => {
                return Ok(DeliveryOutcome::Closed);
            }
            SubscriptionLifecycleState::Active => {}
        }

        let should_schedule = {
            let mut queue = self
                .queue
                .lock()
                .expect("event delivery queue lock poisoned");

            if self.is_closed() {
                return Ok(DeliveryOutcome::Closed);
            }

            if self.shared.is_cancelled() {
                queue.events.clear();
                return Ok(DeliveryOutcome::Cancelled);
            }

            match self.subscription.state() {
                SubscriptionLifecycleState::Cancelled => {
                    queue.events.clear();
                    return Ok(DeliveryOutcome::Cancelled);
                }
                SubscriptionLifecycleState::Closed => {
                    queue.events.clear();
                    return Ok(DeliveryOutcome::Closed);
                }
                SubscriptionLifecycleState::Created => return Ok(DeliveryOutcome::Closed),
                SubscriptionLifecycleState::Active => {}
            }

            if queue.events.len() >= self.shared.config.queue_capacity() {
                return Ok(DeliveryOutcome::Dropped);
            }

            queue.events.push_back(event);
            !self.scheduled.swap(true, Ordering::AcqRel)
        };

        if !should_schedule {
            return Ok(DeliveryOutcome::Accepted);
        }

        match self.shared.ready.push(Arc::clone(self)) {
            Ok(()) => Ok(DeliveryOutcome::Accepted),
            Err(DeliveryError::DispatcherClosed) => {
                self.clear_after_schedule_failure();
                Ok(DeliveryOutcome::Closed)
            }
            Err(DeliveryError::SchedulerSaturated) => {
                self.clear_after_schedule_failure();
                Ok(DeliveryOutcome::Rejected)
            }
            Err(error) => {
                self.clear_after_schedule_failure();
                Err(error)
            }
        }
    }

    fn take_next(&self) -> Option<Arc<Event>> {
        let mut queue = self
            .queue
            .lock()
            .expect("event delivery queue lock poisoned");

        match self.subscription.state() {
            SubscriptionLifecycleState::Active => queue.events.pop_front(),
            SubscriptionLifecycleState::Cancelled | SubscriptionLifecycleState::Closed => {
                queue.events.clear();
                None
            }
            SubscriptionLifecycleState::Created => {
                queue.events.clear();
                None
            }
        }
    }

    fn complete_delivery(self: &Arc<Self>) {
        let (reschedule, terminate) = {
            let mut queue = self
                .queue
                .lock()
                .expect("event delivery queue lock poisoned");

            if self.is_closed()
                || !matches!(
                    self.subscription.state(),
                    SubscriptionLifecycleState::Active
                )
            {
                queue.events.clear();
                self.scheduled.store(false, Ordering::Release);
                (false, true)
            } else if queue.events.is_empty() {
                self.scheduled.store(false, Ordering::Release);
                (false, false)
            } else {
                (true, false)
            }
        };

        if terminate {
            self.close_and_unregister();
        } else if reschedule && self.shared.ready.push(Arc::clone(self)).is_err() {
            self.clear_after_schedule_failure();
        }
    }

    fn close_and_unregister(self: &Arc<Self>) {
        self.close_local();
    }

    fn clear_after_schedule_failure(self: &Arc<Self>) {
        let mut queue = self
            .queue
            .lock()
            .expect("event delivery queue lock poisoned");
        queue.events.clear();
        self.scheduled.store(false, Ordering::Release);
        drop(queue);
        self.close_and_unregister();
    }
}

/// Releases this path's registration if it has not already been closed.
impl Drop for DeliveryPath {
    fn drop(&mut self) {
        if self.registered.swap(false, Ordering::AcqRel) {
            self.shared.unregister(&self.subscription);
        }
    }
}

/// Handle used by the publisher to hand an immutable event to one subscription.
#[derive(Clone)]
pub struct DeliveryHandle {
    path: Arc<DeliveryPath>,
}

impl DeliveryHandle {
    fn new(path: Arc<DeliveryPath>) -> Self {
        Self { path }
    }

    /// Enqueues one immutable event occurrence for this subscription.
    ///
    /// Delivery remains bounded and best-effort; the returned outcome reports
    /// whether the event was accepted, dropped, cancelled, closed, or rejected.
    pub fn enqueue(&self, event: Arc<Event>) -> Result<DeliveryOutcome, DeliveryError> {
        self.path.enqueue(event)
    }

    /// Closes this delivery path and releases its dispatcher registration.
    pub fn close(&self) {
        self.path.close_from_handle();
    }
}

/// Bounded delivery dispatcher shared by internal event publishers.
pub struct DeliveryDispatcher {
    shared: Arc<DeliveryShared>,
    background: BackgroundTasks,
}

impl Drop for DeliveryDispatcher {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl DeliveryDispatcher {
    /// Creates and starts the configured bounded worker pool.
    pub fn new(
        config: DeliveryConfig,
        cancellation: CancellationToken,
    ) -> Result<Self, DeliveryError> {
        let concurrency = ConcurrencyConfig::new(config.worker_count(), config.max_subscriptions())
            .map_err(|_| DeliveryError::ZeroLimit)?;
        let delivery_cancellation = cancellation.child_token();
        let background =
            BackgroundTasks::with_concurrency(delivery_cancellation.clone(), concurrency);
        let shared = Arc::new(DeliveryShared::new(config, delivery_cancellation));

        let dispatcher = Self {
            shared: Arc::clone(&shared),
            background,
        };

        for _ in 0..config.worker_count() {
            let worker_shared = Arc::clone(&shared);
            if let Err(error) = dispatcher.background.spawn_bounded(move |worker_token| {
                worker_loop(worker_shared, worker_token);
            }) {
                dispatcher.shutdown();
                return Err(DeliveryError::WorkerSpawn(error));
            }
        }

        Ok(dispatcher)
    }

    /// Registers one active subscription with the bounded dispatcher.
    pub fn register(
        &self,
        subscription: Arc<EventSubscription>,
    ) -> Result<DeliveryHandle, DeliveryError> {
        self.shared.try_register(&subscription)?;

        if !matches!(subscription.state(), SubscriptionLifecycleState::Active) {
            self.shared.unregister(&subscription);
            return Err(DeliveryError::SubscriptionNotActive);
        }

        let path = DeliveryPath::new(subscription, Arc::clone(&self.shared));
        Ok(DeliveryHandle::new(path))
    }

    /// Stops the dispatcher and its worker pool.
    pub fn shutdown(&self) {
        self.shared.close();
        self.background.shutdown();
    }

    /// Returns whether the dispatcher has shut down.
    pub fn is_closed(&self) -> bool {
        self.shared.is_closed()
    }
}

fn worker_loop(shared: Arc<DeliveryShared>, worker_token: CancellationToken) {
    while let Some(path) = shared.ready.pop(&worker_token) {
        let subscription_cancellation = path.subscription().cancellation_token();

        if shared.is_closed()
            || shared.is_cancelled()
            || subscription_cancellation.is_cancelled()
            || path.is_closed()
        {
            path.close_from_worker();
            continue;
        }

        let Some(event) = path.take_next() else {
            path.complete_delivery();
            continue;
        };

        if shared.is_closed()
            || shared.is_cancelled()
            || subscription_cancellation.is_cancelled()
            || !matches!(
                path.subscription().state(),
                SubscriptionLifecycleState::Active
            )
        {
            path.complete_delivery();
            continue;
        }

        let subscriber = path.subscription().subscriber();

        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            subscriber.handle(&event);
        }));

        path.complete_delivery();
    }
}

impl DeliveryPath {
    fn close_from_handle(self: &Arc<Self>) {
        self.close_local();
    }

    fn close_from_worker(self: &Arc<Self>) {
        self.close_local();
    }

    fn close_local(self: &Arc<Self>) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let mut queue = self
                .queue
                .lock()
                .expect("event delivery queue lock poisoned");
            queue.events.clear();
            self.scheduled.store(false, Ordering::Release);
            drop(queue);
            self.shared.ready.remove(self);
        }

        if self.registered.swap(false, Ordering::AcqRel) {
            self.shared.unregister(&self.subscription);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        events::{Event, EventSubscriber, EventSubscription, Scope},
        identity::EventId,
        operation::CancellationToken,
    };
    use std::sync::{
        Arc, Barrier, Mutex,
        mpsc::{self, Receiver},
    };
    use std::thread;
    use std::time::Duration;

    fn config(
        queue_capacity: usize,
        worker_count: usize,
        max_subscriptions: usize,
    ) -> DeliveryConfig {
        DeliveryConfig::new(queue_capacity, worker_count, max_subscriptions).unwrap()
    }

    fn scope() -> Scope {
        Scope::new("engine:test").unwrap()
    }

    fn subscription_value(
        handler: impl EventSubscriber,
        owner: &CancellationToken,
    ) -> EventSubscription {
        EventSubscription::new("test.event", scope(), handler, owner).unwrap()
    }

    fn subscription(
        handler: impl EventSubscriber,
        owner: &CancellationToken,
    ) -> Arc<EventSubscription> {
        Arc::new(subscription_value(handler, owner))
    }

    fn activate(subscription: &EventSubscription) {
        subscription.activate().unwrap();
    }

    fn make_event(id: &str) -> Arc<Event> {
        Arc::new(Event::new(EventId::new(id).unwrap(), "test.event", scope()).unwrap())
    }

    fn receive<T: Send + 'static>(receiver: &Receiver<T>) -> T {
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("timed out waiting for delivery")
    }

    #[test]
    fn configuration_rejects_zero_limits() {
        assert_eq!(DeliveryConfig::new(0, 1, 1), Err(DeliveryError::ZeroLimit));
        assert_eq!(DeliveryConfig::new(1, 0, 1), Err(DeliveryError::ZeroLimit));
        assert_eq!(DeliveryConfig::new(1, 1, 0), Err(DeliveryError::ZeroLimit));
    }

    #[test]
    fn configuration_preserves_limits() {
        let config = DeliveryConfig::new(8, 3, 20).unwrap();

        assert_eq!(config.queue_capacity(), 8);
        assert_eq!(config.worker_count(), 3);
        assert_eq!(config.max_subscriptions(), 20);
    }

    #[test]
    fn accepted_event_is_delivered() {
        let dispatcher =
            DeliveryDispatcher::new(config(4, 1, 4), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();
        let subscription = subscription(
            move |event: &Event| {
                sender.send(event.event_id().as_str().to_owned()).unwrap();
            },
            &owner,
        );
        activate(&subscription);

        let handle = dispatcher.register(subscription).unwrap();
        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(receive(&receiver), "e1");

        dispatcher.shutdown();
    }

    #[test]
    fn one_subscription_preserves_fifo_order() {
        let dispatcher =
            DeliveryDispatcher::new(config(8, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();
        let subscription = subscription(
            move |event: &Event| {
                sender.send(event.event_id().as_str().to_owned()).unwrap();
            },
            &owner,
        );
        activate(&subscription);
        let handle = dispatcher.register(subscription).unwrap();

        for id in ["e1", "e2", "e3"] {
            assert_eq!(
                handle.enqueue(make_event(id)).unwrap(),
                DeliveryOutcome::Accepted
            );
        }

        assert_eq!(receive(&receiver), "e1");
        assert_eq!(receive(&receiver), "e2");
        assert_eq!(receive(&receiver), "e3");

        dispatcher.shutdown();
    }

    #[test]
    fn different_subscriptions_can_execute_concurrently() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 2, 4), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let barrier = Arc::new(Barrier::new(2));
        let (sender, receiver) = mpsc::channel();

        let barrier_a = Arc::clone(&barrier);
        let sender_a = sender.clone();
        let subscription_a = subscription(
            move |_event: &Event| {
                barrier_a.wait();
                thread::sleep(Duration::from_millis(50));
                sender_a.send('a').unwrap();
            },
            &owner,
        );
        activate(&subscription_a);

        let barrier_b = Arc::clone(&barrier);
        let sender_b = sender.clone();
        let subscription_b = subscription(
            move |_event: &Event| {
                barrier_b.wait();
                sender_b.send('b').unwrap();
            },
            &owner,
        );
        activate(&subscription_b);

        let handle_a = dispatcher.register(subscription_a).unwrap();
        let handle_b = dispatcher.register(subscription_b).unwrap();

        assert_eq!(
            handle_a.enqueue(make_event("a1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(
            handle_b.enqueue(make_event("b1")).unwrap(),
            DeliveryOutcome::Accepted
        );

        let first = receive(&receiver);
        let second = receive(&receiver);
        assert_ne!(first, second);
        assert!([first, second].contains(&'a'));
        assert!([first, second].contains(&'b'));

        dispatcher.shutdown();
    }

    #[test]
    fn full_subscription_queue_drops_newest_event() {
        let dispatcher =
            DeliveryDispatcher::new(config(1, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let release_receiver = Arc::new(Mutex::new(release_receiver));
        let (delivered_sender, delivered_receiver) = mpsc::channel();

        let release_receiver_for_handler = Arc::clone(&release_receiver);
        let subscription = subscription(
            move |event: &Event| {
                started_sender.send(()).unwrap();
                let _ = release_receiver_for_handler
                    .lock()
                    .expect("release receiver lock poisoned")
                    .recv();
                delivered_sender
                    .send(event.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        );
        activate(&subscription);
        let handle = dispatcher.register(subscription).unwrap();

        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        receive(&started_receiver);

        assert_eq!(
            handle.enqueue(make_event("e2")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(
            handle.enqueue(make_event("e3")).unwrap(),
            DeliveryOutcome::Dropped
        );

        release_sender.send(()).unwrap();
        assert_eq!(receive(&delivered_receiver), "e1");

        // The same handler instance processes e2 and waits for its own release.
        // Wait until that invocation has actually started before releasing it.
        receive(&started_receiver);
        release_sender.send(()).unwrap();

        assert_eq!(receive(&delivered_receiver), "e2");

        dispatcher.shutdown();
    }

    #[test]
    fn handler_panic_does_not_close_subscription_or_block_other_subscribers() {
        let dispatcher =
            DeliveryDispatcher::new(config(4, 2, 4), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        let panic_subscription =
            subscription(move |_event: &Event| panic!("subscriber failure"), &owner);
        activate(&panic_subscription);

        let sender_ok = sender.clone();
        let healthy_subscription = subscription(
            move |event: &Event| {
                sender_ok
                    .send(event.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        );
        activate(&healthy_subscription);

        let panic_handle = dispatcher.register(panic_subscription.clone()).unwrap();
        let healthy_handle = dispatcher.register(healthy_subscription).unwrap();

        assert_eq!(
            panic_handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(
            healthy_handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(receive(&receiver), "e1");

        assert_eq!(
            panic_subscription.state(),
            SubscriptionLifecycleState::Active
        );

        assert_eq!(
            panic_handle.enqueue(make_event("e2")).unwrap(),
            DeliveryOutcome::Accepted
        );

        dispatcher.shutdown();
    }

    #[test]
    fn cancellation_discards_queued_events_but_does_not_interrupt_running_handler() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let release_receiver = Arc::new(Mutex::new(release_receiver));
        let (delivered_sender, delivered_receiver) = mpsc::channel();

        let release_receiver_for_handler = Arc::clone(&release_receiver);
        let subscription = subscription(
            move |event: &Event| {
                started_sender.send(()).unwrap();
                let _ = release_receiver_for_handler
                    .lock()
                    .expect("release receiver lock poisoned")
                    .recv();
                delivered_sender
                    .send(event.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        );
        activate(&subscription);
        let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        receive(&started_receiver);
        assert_eq!(
            handle.enqueue(make_event("e2")).unwrap(),
            DeliveryOutcome::Accepted
        );

        subscription.cancel().unwrap();
        release_sender.send(()).unwrap();

        assert_eq!(receive(&delivered_receiver), "e1");
        assert!(
            delivered_receiver
                .recv_timeout(Duration::from_millis(150))
                .is_err()
        );
        assert_eq!(subscription.state(), SubscriptionLifecycleState::Cancelled);

        dispatcher.shutdown();
    }

    #[test]
    fn reentrant_publish_is_serialized_by_the_subscription_queue() {
        let dispatcher =
            Arc::new(DeliveryDispatcher::new(config(4, 1, 2), CancellationToken::new()).unwrap());
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        // DeliveryPath itself is deliberately unaware of Publisher. This test
        // verifies the queue property by directly enqueueing the second event
        // from the first handler through a cloned delivery handle.
        let handle_cell: Arc<Mutex<Option<DeliveryHandle>>> = Arc::new(Mutex::new(None));
        let handle_cell_for_handler = Arc::clone(&handle_cell);
        let sender_for_handler = sender.clone();

        let subscription = subscription(
            move |event: &Event| {
                sender_for_handler
                    .send(event.event_id().as_str().to_owned())
                    .unwrap();
                if event.event_id().as_str() == "e1" {
                    let handle = handle_cell_for_handler
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .clone();
                    let _ = handle.enqueue(make_event("e2"));
                }
            },
            &owner,
        );
        activate(&subscription);

        let handle = dispatcher.register(subscription).unwrap();
        *handle_cell.lock().unwrap() = Some(handle.clone());

        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(receive(&receiver), "e1");
        assert_eq!(receive(&receiver), "e2");

        dispatcher.shutdown();
    }

    #[test]
    fn unregistering_a_path_removes_its_ready_token_and_releases_capacity() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 1), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        let subscription_a = subscription(move |_event: &Event| sender.send(()).unwrap(), &owner);
        activate(&subscription_a);
        let handle_a = dispatcher.register(Arc::clone(&subscription_a)).unwrap();
        assert_eq!(
            handle_a.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Accepted
        );

        handle_a.close();
        subscription_a.close().unwrap();

        let subscription_b = subscription(|_event: &Event| {}, &owner);
        activate(&subscription_b);
        let handle_b = dispatcher.register(subscription_b).unwrap();
        assert_eq!(
            handle_b.enqueue(make_event("e2")).unwrap(),
            DeliveryOutcome::Accepted
        );

        let _ = receiver.recv_timeout(Duration::from_millis(100));
        dispatcher.shutdown();
    }

    #[test]
    fn dispatcher_rejects_duplicate_subscription_registration() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let subscription = Arc::new(subscription(|_event: &Event| {}, &owner));
        activate(&subscription);

        let first_handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

        assert!(matches!(
            dispatcher.register(Arc::clone(&subscription)),
            Err(DeliveryError::SubscriptionAlreadyRegistered)
        ));

        first_handle.close();

        let second_handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
        second_handle.close();

        dispatcher.shutdown();
    }

    #[test]
    fn dropping_a_delivery_handle_releases_registration_capacity() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 1), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();

        let subscription_a = subscription(|_event: &Event| {}, &owner);
        activate(&subscription_a);
        let handle_a = dispatcher.register(subscription_a).unwrap();
        drop(handle_a);

        let subscription_b = subscription(|_event: &Event| {}, &owner);
        activate(&subscription_b);
        let handle_b = dispatcher.register(subscription_b).unwrap();
        handle_b.close();

        dispatcher.shutdown();
    }

    #[test]
    fn duplicate_subscription_error_has_explicit_error_text() {
        assert_eq!(
            DeliveryError::SubscriptionAlreadyRegistered.to_string(),
            "event subscription is already registered"
        );
    }

    #[test]
    fn dispatcher_enforces_max_subscription_limit() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 1), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();

        let first_subscription = subscription(|_event: &Event| {}, &owner);
        activate(&first_subscription);
        let first_handle = dispatcher.register(first_subscription).unwrap();

        let second_subscription = subscription(|_event: &Event| {}, &owner);
        activate(&second_subscription);

        assert!(matches!(
            dispatcher.register(second_subscription),
            Err(DeliveryError::SubscriptionLimitReached)
        ));

        first_handle.close();
        dispatcher.shutdown();
    }

    #[test]
    fn owner_cancellation_stops_future_delivery() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        let subscription = subscription(
            move |_event: &Event| {
                sender.send(()).unwrap();
            },
            &owner,
        );
        activate(&subscription);
        let handle = dispatcher.register(subscription.clone()).unwrap();

        owner.cancel();

        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Cancelled
        );
        assert!(receiver.recv_timeout(Duration::from_millis(150)).is_err());
        assert_eq!(subscription.state(), SubscriptionLifecycleState::Cancelled);

        dispatcher.shutdown();
    }

    #[test]
    fn dispatcher_shutdown_does_not_cancel_parent_scope() {
        let parent = CancellationToken::new();
        let dispatcher = DeliveryDispatcher::new(config(2, 1, 2), parent.clone()).unwrap();

        dispatcher.shutdown();

        assert!(!parent.is_cancelled());
    }

    #[test]
    fn dispatcher_shutdown_rejects_future_delivery() {
        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let subscription = subscription(|_event: &Event| {}, &owner);
        activate(&subscription);
        let handle = dispatcher.register(subscription).unwrap();

        dispatcher.shutdown();

        assert!(dispatcher.is_closed());
        assert_eq!(
            handle.enqueue(make_event("e1")).unwrap(),
            DeliveryOutcome::Closed
        );
    }

    #[test]
    fn unauthorized_subscription_does_not_queue_event() {
        use crate::security::{
            AuthorizationDecision, AuthorizationRequest, Authorizer, SecurityContext,
            identity::{PrincipalId, PrincipalIdentity, PrincipalType},
        };

        struct DenyAuthorizer;

        impl Authorizer for DenyAuthorizer {
            fn authorize(
                &self,
                _request: &AuthorizationRequest<'_>,
            ) -> Result<AuthorizationDecision, crate::security::AuthorizationError> {
                Ok(AuthorizationDecision::Deny)
            }
        }

        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let principal = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("subscriber").unwrap(),
        );
        let security_context = SecurityContext::new(principal, None);

        let subscription = subscription_value(
            |_event: &Event| panic!("unauthorized event must not reach handler"),
            &owner,
        )
        .with_security_context(security_context)
        .with_authorizer(Arc::new(DenyAuthorizer))
        .requiring_capability(crate::identity::CapabilityId::new("events.read").unwrap());

        activate(&subscription);

        let handle = dispatcher.register(Arc::new(subscription)).unwrap();

        assert_eq!(
            handle.enqueue(make_event("secure-1")).unwrap(),
            DeliveryOutcome::Unauthorized
        );

        dispatcher.shutdown();
    }

    #[test]
    fn authorizer_failure_is_reported_without_invoking_handler() {
        use crate::security::{
            AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer,
            SecurityContext,
            identity::{PrincipalId, PrincipalIdentity, PrincipalType},
        };

        struct FailingAuthorizer;

        impl Authorizer for FailingAuthorizer {
            fn authorize(
                &self,
                _request: &AuthorizationRequest<'_>,
            ) -> Result<AuthorizationDecision, AuthorizationError> {
                Err(AuthorizationError::Failed)
            }
        }

        let dispatcher =
            DeliveryDispatcher::new(config(2, 1, 2), CancellationToken::new()).unwrap();
        let owner = CancellationToken::new();
        let principal = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("subscriber").unwrap(),
        );
        let security_context = SecurityContext::new(principal, None);

        let subscription = subscription_value(
            |_event: &Event| panic!("authorization failure must prevent handler"),
            &owner,
        )
        .with_security_context(security_context)
        .with_authorizer(Arc::new(FailingAuthorizer))
        .requiring_capability(crate::identity::CapabilityId::new("events.read").unwrap());

        activate(&subscription);

        let handle = dispatcher.register(Arc::new(subscription)).unwrap();

        assert!(matches!(
            handle.enqueue(make_event("secure-2")),
            Err(DeliveryError::AuthorizationFailed(
                AuthorizationError::Failed
            ))
        ));

        dispatcher.shutdown();
    }

    #[test]
    fn authorization_failure_has_explicit_error_text() {
        let error = DeliveryError::AuthorizationFailed(crate::security::AuthorizationError::Failed);

        assert_eq!(
            error.to_string(),
            "event delivery authorization failed: authorization evaluation failed"
        );
    }
}
