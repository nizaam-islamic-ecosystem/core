//! Publisher coordination for internal Nizaam Core events.
//!
//! The publisher owns the publication boundary for one explicitly owned
//! publisher:
//!
//! ```text
//! Owner
//!   ↓
//! EventPublisher
//!   ├── subscription registration
//!   ├── subscription removal
//!   ├── matching
//!   ├── publication admission
//!   └── publisher lifecycle
//! ```
//!
//! Delivery execution remains the responsibility of `delivery.rs`.
//! This module does not create queues, workers, retry mechanisms, ACK/NACK
//! handling, persistence, replay, transport, or authorization frameworks.
//!
//! The Event subsystem lifecycle from `events::lifecycle` and the lifecycle of
//! an individual publisher are deliberately separate:
//!
//! ```text
//! EventLifecycle
//!     → subsystem-wide Event admission
//!
//! PublisherLifecycle
//!     → one publisher's lifetime
//! ```
//!
//! A publication is represented internally as an immutable event plus the
//! subscriptions that matched it. `delivery.rs` can consume that handoff
//! without acquiring the publisher's registry lock.

use std::fmt;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU8, Ordering},
};

use crate::{
    events::{Event, EventLifecycle, EventSubscription, SubscriptionLifecycleState},
    operation::CancellationToken,
};

const CREATED: u8 = 0;
const ACTIVE: u8 = 1;
const CLOSED: u8 = 2;

/// Lifecycle state of one event publisher.
///
/// This is deliberately separate from the Event subsystem lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublisherLifecycleState {
    /// The publisher exists but has not been activated.
    Created,

    /// The publisher may accept subscriptions and event publication.
    Active,

    /// The publisher is permanently closed.
    Closed,
}

impl PublisherLifecycleState {
    /// Returns whether the publisher is currently active.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns whether the publisher has reached its terminal state.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Closed)
    }

    /// Returns whether the requested lifecycle transition is valid.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Created)
                | (Self::Active, Self::Active)
                | (Self::Closed, Self::Closed)
                | (Self::Created, Self::Active)
                | (Self::Created, Self::Closed)
                | (Self::Active, Self::Closed)
        )
    }
}

/// Error returned when an invalid publisher lifecycle transition is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublisherLifecycleError {
    from: PublisherLifecycleState,
    to: PublisherLifecycleState,
}

impl PublisherLifecycleError {
    /// Creates a lifecycle transition error.
    pub const fn new(from: PublisherLifecycleState, to: PublisherLifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the state from which the transition was requested.
    pub const fn from(self) -> PublisherLifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub const fn to(self) -> PublisherLifecycleState {
        self.to
    }
}

impl fmt::Display for PublisherLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid event publisher lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for PublisherLifecycleError {}

/// Error returned by publisher operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublisherError {
    /// The publisher has not yet been activated.
    NotActive,

    /// The publisher has already been closed.
    Closed,

    /// The Event subsystem is no longer admitting publications.
    EventSubsystemUnavailable,

    /// The owning cancellation scope has already been cancelled.
    OwnerCancelled,

    /// The supplied subscription is already terminal.
    SubscriptionAlreadyTerminated,

    /// The subscription is already registered with this publisher.
    SubscriptionAlreadyRegistered,
}

impl fmt::Display for PublisherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotActive => formatter.write_str("event publisher is not active"),
            Self::Closed => formatter.write_str("event publisher is closed"),
            Self::EventSubsystemUnavailable => {
                formatter.write_str("event subsystem is not accepting publications")
            }
            Self::OwnerCancelled => formatter.write_str("event publisher owner has been cancelled"),
            Self::SubscriptionAlreadyTerminated => {
                formatter.write_str("event subscription is already terminated")
            }
            Self::SubscriptionAlreadyRegistered => {
                formatter.write_str("event subscription is already registered")
            }
        }
    }
}

impl std::error::Error for PublisherError {}

/// The publication result handed from Publisher to Delivery.
///
/// The publisher owns selection. Delivery owns execution.
///
/// This representation remains inside the crate-private Event subsystem so
/// unrelated transport and SDK layers cannot depend on the handoff details.
#[derive(Debug)]
pub struct Publication {
    event: Arc<Event>,
    subscriptions: Vec<Arc<EventSubscription>>,
}

impl Publication {
    /// Creates a publication handoff.
    fn new(event: Arc<Event>, subscriptions: Vec<Arc<EventSubscription>>) -> Self {
        Self {
            event,
            subscriptions,
        }
    }

    /// Returns the immutable event occurrence.
    pub fn event(&self) -> &Arc<Event> {
        &self.event
    }

    /// Returns the matching subscriptions selected at publication time.
    pub fn subscriptions(&self) -> &[Arc<EventSubscription>] {
        &self.subscriptions
    }

    /// Returns the number of matching subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Returns whether no subscription matched this publication.
    pub fn is_unobserved(&self) -> bool {
        self.subscriptions.is_empty()
    }
}

/// One explicitly owned Event publisher.
///
/// A publisher owns the registration relationship with its subscriptions.
/// Delivery workers are not stored here and are created by the Delivery layer.
pub struct EventPublisher {
    lifecycle: AtomicU8,
    event_lifecycle: Arc<EventLifecycle>,
    owner_cancellation: CancellationToken,
    subscriptions: RwLock<Vec<Arc<EventSubscription>>>,
}

impl fmt::Debug for EventPublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventPublisher")
            .field("lifecycle", &self.state())
            .field("owner_cancelled", &self.owner_cancellation.is_cancelled())
            .field("subscription_count", &self.subscription_count())
            .finish_non_exhaustive()
    }
}

impl EventPublisher {
    /// Creates a publisher in the `Created` state.
    ///
    /// The publisher observes the Event subsystem lifecycle and derives its
    /// owned lifetime from the supplied cancellation scope.
    pub fn new(
        event_lifecycle: Arc<EventLifecycle>,
        owner_cancellation: &CancellationToken,
    ) -> Self {
        Self {
            lifecycle: AtomicU8::new(CREATED),
            event_lifecycle,
            owner_cancellation: owner_cancellation.clone(),
            subscriptions: RwLock::new(Vec::new()),
        }
    }

    /// Returns the current publisher lifecycle state.
    pub fn state(&self) -> PublisherLifecycleState {
        match self.lifecycle.load(Ordering::Acquire) {
            CREATED => PublisherLifecycleState::Created,
            ACTIVE => PublisherLifecycleState::Active,
            CLOSED => PublisherLifecycleState::Closed,
            _ => unreachable!("event publisher contains an invalid lifecycle state"),
        }
    }

    /// Activates a newly created publisher.
    pub fn activate(&self) -> Result<(), PublisherLifecycleError> {
        if self.owner_cancellation.is_cancelled() {
            return Err(PublisherLifecycleError::new(
                PublisherLifecycleState::Created,
                PublisherLifecycleState::Closed,
            ));
        }

        self.transition_to(PublisherLifecycleState::Active)
    }

    /// Returns whether the publisher is currently active.
    pub fn is_active(&self) -> bool {
        self.state().is_active()
    }

    /// Returns whether the publisher is permanently closed.
    pub fn is_closed(&self) -> bool {
        self.state().is_terminal()
    }

    /// Returns the number of currently registered subscriptions.
    pub fn subscription_count(&self) -> usize {
        self.subscriptions
            .read()
            .expect("event publisher subscription lock poisoned")
            .len()
    }

    /// Registers one explicit subscription with this publisher.
    ///
    /// Registration activates a newly created subscription. Terminal
    /// subscriptions are rejected.
    pub fn subscribe(
        &self,
        subscription: EventSubscription,
    ) -> Result<Arc<EventSubscription>, PublisherError> {
        self.ensure_registration_allowed()?;

        if subscription.is_terminal() || subscription.state() != SubscriptionLifecycleState::Created
        {
            return Err(PublisherError::SubscriptionAlreadyTerminated);
        }

        let subscription = Arc::new(subscription);

        let mut subscriptions = self
            .subscriptions
            .write()
            .expect("event publisher subscription lock poisoned");

        match self.state() {
            PublisherLifecycleState::Closed => return Err(PublisherError::Closed),
            PublisherLifecycleState::Created => return Err(PublisherError::NotActive),
            PublisherLifecycleState::Active => {}
        }

        subscriptions.push(Arc::clone(&subscription));

        drop(subscriptions);

        if let Err(error) = subscription.activate() {
            let mut subscriptions = self
                .subscriptions
                .write()
                .expect("event publisher subscription lock poisoned");

            subscriptions.retain(|registered| !Arc::ptr_eq(registered, &subscription));

            return Err(match error.from() {
                SubscriptionLifecycleState::Cancelled | SubscriptionLifecycleState::Closed => {
                    PublisherError::SubscriptionAlreadyTerminated
                }
                _ => PublisherError::SubscriptionAlreadyTerminated,
            });
        }

        Ok(subscription)
    }

    /// Removes a previously registered subscription.
    ///
    /// Removal prevents future publications from selecting the subscription.
    /// Delivery cleanup remains the responsibility of the Delivery layer.
    pub fn unsubscribe(&self, subscription: &Arc<EventSubscription>) -> bool {
        let removed = {
            let mut subscriptions = self
                .subscriptions
                .write()
                .expect("event publisher subscription lock poisoned");

            let original_len = subscriptions.len();

            subscriptions.retain(|registered| !Arc::ptr_eq(registered, subscription));

            subscriptions.len() != original_len
        };

        if removed {
            let _ = subscription.close();
        }

        removed
    }

    /// Publishes one immutable Event occurrence and returns the matching
    /// subscription handoff for Delivery.
    ///
    /// This method never invokes a subscriber handler. It snapshots matching
    /// subscriptions and releases the registry lock before returning.
    pub fn publish(&self, event: Event) -> Result<Publication, PublisherError> {
        self.ensure_publication_allowed()?;

        let event = Arc::new(event);

        let subscriptions = {
            let subscriptions = self
                .subscriptions
                .read()
                .expect("event publisher subscription lock poisoned");

            match self.state() {
                PublisherLifecycleState::Closed => return Err(PublisherError::Closed),
                PublisherLifecycleState::Created => return Err(PublisherError::NotActive),
                PublisherLifecycleState::Active => {}
            }

            subscriptions
                .iter()
                .filter(|subscription| subscription.matches(event.event_type(), event.scope()))
                .cloned()
                .collect()
        };

        Ok(Publication::new(event, subscriptions))
    }

    /// Closes this publisher permanently.
    ///
    /// Closing stops new publications and terminates publisher-owned
    /// subscriptions. Delivery resources are cleaned up by Delivery.
    pub fn close(&self) {
        let subscriptions = {
            let mut registered = self
                .subscriptions
                .write()
                .expect("event publisher subscription lock poisoned");

            if self.lifecycle.swap(CLOSED, Ordering::AcqRel) == CLOSED {
                return;
            }

            std::mem::take(&mut *registered)
        };

        for subscription in subscriptions {
            let _ = subscription.cancel();
        }
    }

    fn ensure_registration_allowed(&self) -> Result<(), PublisherError> {
        self.refresh_owner_state();

        match self.state() {
            PublisherLifecycleState::Created => Err(PublisherError::NotActive),
            PublisherLifecycleState::Closed => Err(PublisherError::Closed),
            PublisherLifecycleState::Active => {
                if !self.event_lifecycle.allows_publication() {
                    Err(PublisherError::EventSubsystemUnavailable)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn ensure_publication_allowed(&self) -> Result<(), PublisherError> {
        self.refresh_owner_state();

        match self.state() {
            PublisherLifecycleState::Created => Err(PublisherError::NotActive),
            PublisherLifecycleState::Closed => Err(PublisherError::Closed),
            PublisherLifecycleState::Active => {
                if !self.event_lifecycle.allows_publication() {
                    return Err(PublisherError::EventSubsystemUnavailable);
                }

                Ok(())
            }
        }
    }

    fn refresh_owner_state(&self) {
        if self.owner_cancellation.is_cancelled() {
            self.close();
        }
    }

    fn transition_to(&self, next: PublisherLifecycleState) -> Result<(), PublisherLifecycleError> {
        loop {
            let current = self.state();

            if current == next {
                return Ok(());
            }

            if !current.can_transition_to(next) {
                return Err(PublisherLifecycleError::new(current, next));
            }

            match self.lifecycle.compare_exchange(
                encode_state(current),
                encode_state(next),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
    }
}

fn encode_state(state: PublisherLifecycleState) -> u8 {
    match state {
        PublisherLifecycleState::Created => CREATED,
        PublisherLifecycleState::Active => ACTIVE,
        PublisherLifecycleState::Closed => CLOSED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        events::{Event, Scope},
        identity::EventId,
        operation::CancellationToken,
    };

    fn event_lifecycle() -> Arc<EventLifecycle> {
        Arc::new(EventLifecycle::new())
    }

    fn publisher() -> EventPublisher {
        EventPublisher::new(event_lifecycle(), &CancellationToken::new())
    }

    fn event() -> Event {
        Event::new(
            EventId::new("event-1").unwrap(),
            "test.event",
            Scope::new("engine:test").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn publisher_starts_created() {
        let publisher = publisher();

        assert_eq!(publisher.state(), PublisherLifecycleState::Created);
        assert!(!publisher.is_active());
        assert!(!publisher.is_closed());
    }

    #[test]
    fn publisher_activates() {
        let publisher = publisher();

        publisher.activate().unwrap();

        assert_eq!(publisher.state(), PublisherLifecycleState::Active);
        assert!(publisher.is_active());
    }

    #[test]
    fn publisher_activation_is_idempotent() {
        let publisher = publisher();

        publisher.activate().unwrap();
        publisher.activate().unwrap();

        assert_eq!(publisher.state(), PublisherLifecycleState::Active);
    }

    #[test]
    fn created_publisher_cannot_publish() {
        let publisher = publisher();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::NotActive)
        ));
    }

    #[test]
    fn active_publisher_can_publish() {
        let publisher = publisher();

        publisher.activate().unwrap();

        let publication = publisher.publish(event()).unwrap();

        assert_eq!(publication.event().event_id().as_str(), "event-1");
    }

    #[test]
    fn publisher_close_is_terminal() {
        let publisher = publisher();

        publisher.activate().unwrap();
        publisher.close();
        publisher.close();

        assert_eq!(publisher.state(), PublisherLifecycleState::Closed);
        assert!(publisher.is_closed());
        assert!(!publisher.is_active());
    }

    #[test]
    fn closed_publisher_rejects_publication() {
        let publisher = publisher();

        publisher.activate().unwrap();
        publisher.close();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::Closed)
        ));
    }

    #[test]
    fn closed_publisher_rejects_subscription() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();
        publisher.close();

        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        assert!(matches!(
            publisher.subscribe(subscription),
            Err(PublisherError::Closed)
        ));
    }

    #[test]
    fn closed_publisher_recheck_prevents_late_subscription_admission() {
        let publisher = publisher();
        publisher.activate().unwrap();
        publisher.close();

        let owner = CancellationToken::new();
        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        assert!(matches!(
            publisher.subscribe(subscription),
            Err(PublisherError::Closed)
        ));
        assert_eq!(publisher.subscription_count(), 0);
    }

    #[test]
    fn closed_publisher_recheck_prevents_late_publication_handoff() {
        let publisher = publisher();
        publisher.activate().unwrap();
        publisher.close();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::Closed)
        ));
    }

    #[test]
    fn publisher_owner_cancellation_closes_publisher() {
        let owner = CancellationToken::new();

        let publisher = EventPublisher::new(event_lifecycle(), &owner);

        publisher.activate().unwrap();
        owner.cancel();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::Closed)
        ));
        assert!(publisher.is_closed());
    }

    #[test]
    fn publisher_rejects_publication_when_event_subsystem_is_draining() {
        let lifecycle = event_lifecycle();
        let publisher = EventPublisher::new(Arc::clone(&lifecycle), &CancellationToken::new());

        publisher.activate().unwrap();
        lifecycle.begin_draining();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::EventSubsystemUnavailable)
        ));
    }

    #[test]
    fn publisher_rejects_publication_when_event_subsystem_is_stopped() {
        let lifecycle = event_lifecycle();
        let publisher = EventPublisher::new(Arc::clone(&lifecycle), &CancellationToken::new());

        publisher.activate().unwrap();
        lifecycle.stop();

        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::EventSubsystemUnavailable)
        ));
    }

    #[test]
    fn subscription_registration_activates_subscription() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();

        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let registered = publisher.subscribe(subscription).unwrap();

        assert_eq!(registered.state(), SubscriptionLifecycleState::Active);
        assert_eq!(publisher.subscription_count(), 1);
    }

    #[test]
    fn unsubscribe_removes_subscription() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();

        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let registered = publisher.subscribe(subscription).unwrap();

        assert!(publisher.unsubscribe(&registered));
        assert_eq!(publisher.subscription_count(), 0);
        assert_eq!(registered.state(), SubscriptionLifecycleState::Closed);
    }

    #[test]
    fn unsubscribe_unknown_subscription_returns_false() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();

        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let subscription = Arc::new(subscription);

        assert!(!publisher.unsubscribe(&subscription));
        assert_eq!(publisher.subscription_count(), 0);
    }

    #[test]
    fn closing_publisher_terminates_registered_subscriptions() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();

        let subscription = EventSubscription::new(
            "operation.completed",
            Scope::new("engine:test").unwrap(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let registered = publisher.subscribe(subscription).unwrap();

        publisher.close();

        assert_eq!(publisher.subscription_count(), 0);
        assert!(registered.is_cancelled());
    }

    #[test]
    fn publication_with_no_matching_subscriptions_is_valid() {
        let publisher = publisher();

        publisher.activate().unwrap();

        let publication = publisher.publish(event()).unwrap();

        assert!(publication.is_unobserved());
        assert_eq!(publication.subscription_count(), 0);
    }

    #[test]
    fn publication_preserves_one_event_identity() {
        let publisher = publisher();

        publisher.activate().unwrap();

        let publication = publisher.publish(event()).unwrap();

        assert_eq!(publication.event().event_id().as_str(), "event-1");
    }

    #[test]
    fn publisher_lifecycle_terminal_transition_cannot_reopen() {
        let publisher = publisher();

        publisher.activate().unwrap();
        publisher.close();

        assert!(publisher.activate().is_err());
        assert_eq!(publisher.state(), PublisherLifecycleState::Closed);
    }

    #[test]
    fn concurrent_close_is_safe() {
        use std::thread;

        let publisher = Arc::new(publisher());
        publisher.activate().unwrap();

        let mut handles = Vec::new();

        for _ in 0..16 {
            let publisher = Arc::clone(&publisher);

            handles.push(thread::spawn(move || {
                publisher.close();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(publisher.state(), PublisherLifecycleState::Closed);
        assert_eq!(publisher.subscription_count(), 0);
    }
}
