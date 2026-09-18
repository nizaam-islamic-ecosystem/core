//! Level 3 integration tests for the Phase 14 internal Event subsystem.
//!
//! These tests exercise the Event subsystem through the public Core API and
//! verify behavior that only emerges when Publisher, Subscription, Delivery,
//! Security, Operation context, and cancellation work together.
//!
//! Component-specific behavior remains covered by the unit tests inside
//! `src/events/*`, while module composition is covered by `events/mod.rs`.

use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryError, DeliveryOutcome, Event, EventContext,
    EventPublisher, EventSubscriber, EventSubscription, Scope,
};
use nizaam_core::identity::{CapabilityId, CorrelationId, EventId, OperationId};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::security::{
    AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext,
};

// ---------------------------------------------------------------------------
// Shared construction helpers
// ---------------------------------------------------------------------------

const EVENT_TYPE: &str = "test.event";
const EVENT_SCOPE: &str = "engine:test";

fn scope() -> Scope {
    Scope::new(EVENT_SCOPE).unwrap()
}

fn event(id: &str) -> Event {
    Event::new(EventId::new(id).unwrap(), EVENT_TYPE, scope()).unwrap()
}

fn dispatcher(queue_capacity: usize, worker_count: usize) -> DeliveryDispatcher {
    DeliveryDispatcher::new(
        DeliveryConfig::new(queue_capacity, worker_count, 8).unwrap(),
        CancellationToken::new(),
    )
    .unwrap()
}

fn subscription(handler: impl EventSubscriber, owner: &CancellationToken) -> EventSubscription {
    EventSubscription::new(EVENT_TYPE, scope(), handler, owner).unwrap()
}

fn operation_context(operation_id: &str, correlation_id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(operation_id).unwrap(),
        CorrelationId::new(correlation_id).unwrap(),
    ))
}

fn security_context(principal_id: &str) -> SecurityContext {
    SecurityContext::new(
        PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new(principal_id).unwrap(),
        ),
        None,
    )
}

fn receive<T: Send + 'static>(receiver: &mpsc::Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for event delivery")
}

// ---------------------------------------------------------------------------
// Publisher + Delivery integration
// ---------------------------------------------------------------------------

#[test]
fn published_event_reaches_matching_subscriber() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let subscription = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender
                    .send(received.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    publisher.activate().unwrap();

    let publication = publisher.publish(event("e1")).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert!(Arc::ptr_eq(&publication.subscriptions()[0], &subscription));

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(receive(&receiver), "e1");

    dispatcher.shutdown();
}

#[test]
fn published_event_only_reaches_matching_subscriptions() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let matching = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender
                    .send(received.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let different_type = publisher
        .subscribe(
            EventSubscription::new(
                "other.event",
                scope(),
                |_event: &Event| panic!("non-matching event type must not be delivered"),
                &owner,
            )
            .unwrap(),
        )
        .unwrap();

    let different_scope = publisher
        .subscribe(
            EventSubscription::new(
                EVENT_TYPE,
                Scope::new("engine:other").unwrap(),
                |_event: &Event| panic!("non-matching scope must not be delivered"),
                &owner,
            )
            .unwrap(),
        )
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let matching_handle = dispatcher.register(Arc::clone(&matching)).unwrap();
    let different_type_handle = dispatcher.register(Arc::clone(&different_type)).unwrap();
    let different_scope_handle = dispatcher.register(Arc::clone(&different_scope)).unwrap();

    let publication = publisher.publish(event("e1")).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert!(Arc::ptr_eq(&publication.subscriptions()[0], &matching));

    assert_eq!(
        matching_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(receive(&receiver), "e1");

    assert!(
        !publication
            .subscriptions()
            .iter()
            .any(|subscription| Arc::ptr_eq(subscription, &different_type))
    );
    assert!(
        !publication
            .subscriptions()
            .iter()
            .any(|subscription| Arc::ptr_eq(subscription, &different_scope))
    );

    different_type_handle.close();
    different_scope_handle.close();
    dispatcher.shutdown();
}

#[test]
fn one_event_occurrence_is_delivered_to_multiple_matching_subscribers() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let sender_a = sender.clone();
    let first = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender_a
                    .send(("a", received.event_id().as_str().to_owned()))
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let second = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender
                    .send(("b", received.event_id().as_str().to_owned()))
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 2);
    let first_handle = dispatcher.register(Arc::clone(&first)).unwrap();
    let second_handle = dispatcher.register(Arc::clone(&second)).unwrap();

    let publication = publisher.publish(event("shared-1")).unwrap();

    assert_eq!(publication.subscription_count(), 2);

    assert_eq!(
        first_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        second_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Accepted
    );

    let first_received = receive(&receiver);
    let second_received = receive(&receiver);

    assert!(
        first_received == ("a", "shared-1".to_owned())
            || second_received == ("a", "shared-1".to_owned())
    );
    assert!(
        first_received == ("b", "shared-1".to_owned())
            || second_received == ("b", "shared-1".to_owned())
    );

    dispatcher.shutdown();
}

#[test]
fn publication_is_only_a_delivery_handoff() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let subscription = publisher
        .subscribe(subscription(
            move |_event: &Event| {
                sender
                    .send(())
                    .expect("test thread must still be waiting for delivery");
            },
            &owner,
        ))
        .unwrap();

    let publication = publisher.publish(event("handoff-1")).unwrap();

    assert_eq!(publication.subscription_count(), 1);

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(subscription).unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    receive(&receiver);

    dispatcher.shutdown();
}

#[test]
fn one_subscription_preserves_fifo_order_through_publication() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let subscription = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender
                    .send(received.event_id().as_str().to_owned())
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(8, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    for id in ["e1", "e2", "e3"] {
        let publication = publisher.publish(event(id)).unwrap();

        assert_eq!(publication.subscription_count(), 1);
        assert_eq!(
            handle.enqueue(Arc::clone(publication.event())).unwrap(),
            DeliveryOutcome::Accepted
        );
    }

    assert_eq!(receive(&receiver), "e1");
    assert_eq!(receive(&receiver), "e2");
    assert_eq!(receive(&receiver), "e3");

    dispatcher.shutdown();
}

#[test]
fn matching_subscriptions_execute_independently() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let barrier = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let barrier_a = Arc::clone(&barrier);
    let sender_a = sender.clone();
    let first = publisher
        .subscribe(subscription(
            move |_event: &Event| {
                barrier_a.wait();
                sender_a.send('a').unwrap();
            },
            &owner,
        ))
        .unwrap();

    let barrier_b = Arc::clone(&barrier);
    let second = publisher
        .subscribe(subscription(
            move |_event: &Event| {
                barrier_b.wait();
                sender.send('b').unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 2);
    let first_handle = dispatcher.register(Arc::clone(&first)).unwrap();
    let second_handle = dispatcher.register(Arc::clone(&second)).unwrap();

    let publication = publisher.publish(event("concurrent-1")).unwrap();

    assert_eq!(publication.subscription_count(), 2);

    assert_eq!(
        first_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        second_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Accepted
    );

    let first_result = receive(&receiver);
    let second_result = receive(&receiver);

    assert_ne!(first_result, second_result);
    assert!([first_result, second_result].contains(&'a'));
    assert!([first_result, second_result].contains(&'b'));

    dispatcher.shutdown();
}

// ---------------------------------------------------------------------------
// Context + Security integration
// ---------------------------------------------------------------------------

#[test]
fn event_context_reaches_subscriber_through_delivery() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let (sender, receiver) = mpsc::channel();

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let operation_context = operation_context("operation-1", "correlation-1");
    let producer_security = security_context("producer-engine");
    let event_context = EventContext::empty()
        .with_operation_context(operation_context.clone())
        .with_security_context(producer_security.clone());

    let event = Event::new_with_context(
        EventId::new("context-1").unwrap(),
        EVENT_TYPE,
        scope(),
        event_context,
    )
    .unwrap();

    let subscription = publisher
        .subscribe(subscription(
            move |received: &Event| {
                let operation_id = received
                    .context()
                    .operation_context()
                    .unwrap()
                    .operation
                    .id
                    .as_str()
                    .to_owned();

                let correlation_id = received
                    .context()
                    .operation_context()
                    .unwrap()
                    .operation
                    .correlation_id
                    .as_str()
                    .to_owned();

                let producer_id = received
                    .context()
                    .security_context()
                    .unwrap()
                    .principal()
                    .principal_id()
                    .as_str()
                    .to_owned();

                sender
                    .send((operation_id, correlation_id, producer_id))
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    let publication = publisher.publish(event).unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    assert_eq!(
        receive(&receiver),
        (
            "operation-1".to_owned(),
            "correlation-1".to_owned(),
            "producer-engine".to_owned()
        )
    );

    dispatcher.shutdown();
}

#[test]
fn unauthorized_matching_subscription_does_not_receive_event() {
    struct DenyAuthorizer;

    impl Authorizer for DenyAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Deny)
        }
    }

    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let handler_calls = Arc::new(AtomicUsize::new(0));

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let calls = Arc::clone(&handler_calls);
    let subscription = publisher
        .subscribe(
            EventSubscription::new(
                EVENT_TYPE,
                scope(),
                move |_event: &Event| {
                    calls.fetch_add(1, Ordering::SeqCst);
                },
                &owner,
            )
            .unwrap()
            .with_security_context(security_context("subscriber"))
            .with_authorizer(Arc::new(DenyAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let publication = publisher.publish(event("secure-deny-1")).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Unauthorized
    );

    assert_eq!(handler_calls.load(Ordering::SeqCst), 0);

    dispatcher.shutdown();
}

#[test]
fn authorization_failure_stops_event_delivery() {
    struct FailingAuthorizer;

    impl Authorizer for FailingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Err(AuthorizationError::Failed)
        }
    }

    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let handler_calls = Arc::new(AtomicUsize::new(0));

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let calls = Arc::clone(&handler_calls);
    let subscription = publisher
        .subscribe(
            EventSubscription::new(
                EVENT_TYPE,
                scope(),
                move |_event: &Event| {
                    calls.fetch_add(1, Ordering::SeqCst);
                },
                &owner,
            )
            .unwrap()
            .with_security_context(security_context("subscriber"))
            .with_authorizer(Arc::new(FailingAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let publication = publisher.publish(event("secure-fail-1")).unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())),
        Err(DeliveryError::AuthorizationFailed(
            AuthorizationError::Failed
        ))
    );

    assert_eq!(handler_calls.load(Ordering::SeqCst), 0);

    dispatcher.shutdown();
}

// ---------------------------------------------------------------------------
// Cancellation + reentrancy integration
// ---------------------------------------------------------------------------

#[test]
fn cancelled_subscription_does_not_consume_already_published_event() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let handler_calls = Arc::new(AtomicUsize::new(0));

    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let calls = Arc::clone(&handler_calls);
    let subscription = publisher
        .subscribe(subscription(
            move |_event: &Event| {
                calls.fetch_add(1, Ordering::SeqCst);
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let publication = publisher.publish(event("cancelled-1")).unwrap();

    subscription.cancel().unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Cancelled
    );

    assert_eq!(handler_calls.load(Ordering::SeqCst), 0);

    dispatcher.shutdown();
}

#[test]
fn reentrant_publication_is_serialized_through_delivery() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let publisher = Arc::new(EventPublisher::new(Arc::clone(&lifecycle), &owner));
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let handle_cell: Arc<Mutex<Option<nizaam_core::events::DeliveryHandle>>> =
        Arc::new(Mutex::new(None));

    let publisher_for_handler = Arc::clone(&publisher);
    let handle_cell_for_handler = Arc::clone(&handle_cell);
    let sender_for_handler = sender.clone();

    let subscription = publisher
        .subscribe(subscription(
            move |received: &Event| {
                sender_for_handler
                    .send(received.event_id().as_str().to_owned())
                    .unwrap();

                if received.event_id().as_str() == "e1" {
                    let handle = handle_cell_for_handler
                        .lock()
                        .expect("delivery handle lock poisoned")
                        .as_ref()
                        .expect("delivery handle must be registered before delivery")
                        .clone();

                    let nested_publication = publisher_for_handler
                        .publish(event("e2"))
                        .expect("reentrant publication should remain admissible");

                    assert_eq!(nested_publication.subscription_count(), 1);

                    handle
                        .enqueue(Arc::clone(nested_publication.event()))
                        .expect("nested event should be accepted");
                }
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4, 1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    *handle_cell.lock().expect("delivery handle lock poisoned") = Some(handle.clone());

    let publication = publisher.publish(event("e1")).unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    assert_eq!(receive(&receiver), "e1");
    assert_eq!(receive(&receiver), "e2");

    dispatcher.shutdown();
}
