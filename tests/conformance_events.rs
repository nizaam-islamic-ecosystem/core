use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

use nizaam_core::{
    events::{
        DeliveryConfig, DeliveryDispatcher, DeliveryError, DeliveryOutcome, Event, EventContext,
        EventLifecycle, EventName, EventPublisher, EventSubscriber, EventSubscription, Scope,
    },
    identity::{AttemptId, CapabilityId, CorrelationId, EventId, OperationId},
    operation::{CancellationToken, Operation, OperationContext},
    retry::{Attempt, AttemptLifecycleState},
    security::{
        AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer, PrincipalId,
        PrincipalIdentity, PrincipalType, SecurityContext,
    },
};

const EVENT_TYPE: &str = "conformance.event";
const EVENT_SCOPE: &str = "engine:conformance";

fn scope() -> Scope {
    Scope::new(EVENT_SCOPE).unwrap()
}

fn event(id: &str) -> Event {
    Event::new(
        EventId::new(id).unwrap(),
        EventName::new(EVENT_TYPE).unwrap(),
        EVENT_TYPE,
        scope(),
    )
    .unwrap()
}

fn publisher(owner: &CancellationToken) -> EventPublisher {
    EventPublisher::new(Arc::new(EventLifecycle::new()), owner)
}

fn dispatcher(capacity: usize) -> DeliveryDispatcher {
    DeliveryDispatcher::new(
        DeliveryConfig::new(capacity, 1, 16).unwrap(),
        CancellationToken::new(),
    )
    .unwrap()
}

fn ignore_event(_: &Event) {}

fn panic_unexpected_event(_: &Event) {
    panic!("unexpected event delivery");
}

struct CountingSubscriber(Arc<AtomicUsize>);

impl EventSubscriber for CountingSubscriber {
    fn handle(&self, _: &Event) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn subscription(
    handler: impl Fn(&Event) + Send + Sync + 'static,
    owner: &CancellationToken,
) -> EventSubscription {
    EventSubscription::new(
        EventName::new(EVENT_TYPE).unwrap(),
        EVENT_TYPE,
        scope(),
        handler,
        owner,
    )
    .unwrap()
}

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new("event-conformance-operation").unwrap(),
        CorrelationId::new("event-conformance-correlation").unwrap(),
    ))
}

fn engine_security(id: &str) -> SecurityContext {
    SecurityContext::new(
        PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new(id).unwrap()),
        None,
    )
}

fn receive<T>(receiver: &mpsc::Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for event delivery")
}

struct AllowAuthorizer;

impl Authorizer for AllowAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}

struct DenyAuthorizer;

impl Authorizer for DenyAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Deny)
    }
}

struct FailingAuthorizer;

impl Authorizer for FailingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Err(AuthorizationError::Failed)
    }
}

#[test]
fn publisher_lifecycle_requires_activation_and_shutdown_is_terminal() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);

    assert_eq!(
        publisher.state(),
        nizaam_core::events::PublisherLifecycleState::Created
    );
    assert!(
        publisher
            .publish(event("lifecycle-before-activate"))
            .is_err()
    );

    publisher.activate().unwrap();
    assert!(publisher.is_active());

    publisher.close();
    assert!(publisher.is_closed());
    assert!(publisher.publish(event("lifecycle-after-close")).is_err());
}

#[test]
fn matching_subscription_receives_one_event() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let subscription = publisher
        .subscribe(
            subscription(
                move |received| {
                    sender
                        .send(received.event_id().as_str().to_owned())
                        .unwrap();
                },
                &owner,
            )
            .with_security_context(engine_security("allowed"))
            .with_authorizer(Arc::new(AllowAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    let publication = publisher.publish(event("matching-1")).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(receive(&receiver), "matching-1");

    dispatcher.shutdown();
}

#[test]
fn non_matching_subscriptions_are_not_selected() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let matching = publisher
        .subscribe(subscription(ignore_event, &owner))
        .unwrap();

    let other_type = publisher
        .subscribe(
            EventSubscription::new(
                EventName::new("other.event").unwrap(),
                "other.event",
                scope(),
                panic_unexpected_event,
                &owner,
            )
            .unwrap(),
        )
        .unwrap();

    let other_scope = publisher
        .subscribe(
            EventSubscription::new(
                EventName::new(EVENT_TYPE).unwrap(),
                EVENT_TYPE,
                Scope::new("engine:other").unwrap(),
                panic_unexpected_event,
                &owner,
            )
            .unwrap(),
        )
        .unwrap();

    let publication = publisher.publish(event("matching-2")).unwrap();

    assert_eq!(publication.subscription_count(), 1);
    assert!(Arc::ptr_eq(&publication.subscriptions()[0], &matching));
    assert!(
        !publication
            .subscriptions()
            .iter()
            .any(|s| Arc::ptr_eq(s, &other_type))
    );
    assert!(
        !publication
            .subscriptions()
            .iter()
            .any(|s| Arc::ptr_eq(s, &other_scope))
    );
}

#[test]
fn one_event_occurrence_reaches_multiple_matching_subscribers() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let first_sender = sender.clone();

    let first = publisher
        .subscribe(subscription(
            move |event| {
                first_sender
                    .send(("first", event.event_id().to_string()))
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let second = publisher
        .subscribe(subscription(
            move |event| {
                sender
                    .send(("second", event.event_id().to_string()))
                    .unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let first_handle = dispatcher.register(Arc::clone(&first)).unwrap();
    let second_handle = dispatcher.register(Arc::clone(&second)).unwrap();

    let publication = publisher.publish(event("fanout-1")).unwrap();
    assert_eq!(publication.subscription_count(), 2);

    first_handle
        .enqueue(Arc::clone(publication.event()))
        .unwrap();
    second_handle
        .enqueue(Arc::clone(publication.event()))
        .unwrap();

    let mut observed = [receive(&receiver), receive(&receiver)];
    observed.sort();

    assert_eq!(
        observed,
        [
            ("first", "fanout-1".to_owned()),
            ("second", "fanout-1".to_owned())
        ]
    );

    dispatcher.shutdown();
}

#[test]
fn publication_is_a_delivery_handoff_not_inline_subscriber_execution() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let subscription = publisher
        .subscribe(subscription(
            move |_| {
                sender.send(()).unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let publication = publisher.publish(event("handoff-1")).unwrap();

    // Publication only selects the subscriber; it must not execute the
    // subscriber inline.
    assert!(receiver.try_recv().is_err());

    handle.enqueue(Arc::clone(publication.event())).unwrap();

    // The delivery worker is the only code that can produce this signal.
    receive(&receiver);

    dispatcher.shutdown();
}

#[test]
fn one_subscription_preserves_fifo_order() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let subscription = publisher
        .subscribe(subscription(
            move |event| {
                sender.send(event.event_id().to_string()).unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(8);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    for id in ["fifo-1", "fifo-2", "fifo-3", "fifo-4"] {
        let publication = publisher.publish(event(id)).unwrap();
        assert_eq!(
            handle.enqueue(Arc::clone(publication.event())).unwrap(),
            DeliveryOutcome::Accepted
        );
    }

    let received = [
        receive(&receiver),
        receive(&receiver),
        receive(&receiver),
        receive(&receiver),
    ];

    assert_eq!(received, ["fifo-1", "fifo-2", "fifo-3", "fifo-4"]);
    dispatcher.shutdown();
}

#[test]
fn subscriber_cancellation_prevents_future_delivery() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_in_handler = Arc::clone(&calls);

    let subscription = publisher
        .subscribe(subscription(
            move |_| {
                calls_in_handler.fetch_add(1, Ordering::SeqCst);
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let publication = publisher.publish(event("cancel-1")).unwrap();
    subscription.cancel().unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Cancelled
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let later = publisher.publish(event("cancel-2")).unwrap();
    assert_eq!(later.subscription_count(), 0);

    dispatcher.shutdown();
}

#[test]
fn owner_cancellation_closes_publisher_owned_activity() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let subscription = publisher
        .subscribe(subscription(ignore_event, &owner))
        .unwrap();

    owner.cancel();

    assert!(publisher.publish(event("owner-cancel")).is_err());
    assert!(subscription.is_cancelled() || subscription.is_closed());
}

#[test]
fn publisher_close_terminates_registered_subscriptions() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let subscription = publisher
        .subscribe(subscription(ignore_event, &owner))
        .unwrap();

    publisher.close();

    assert!(publisher.is_closed());
    assert!(subscription.is_terminal());
    assert!(publisher.publish(event("closed-1")).is_err());
}

#[test]
fn bounded_delivery_drops_newest_event_when_subscription_queue_is_full() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (started_sender, started_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let release_receiver_for_handler = Arc::clone(&release_receiver);

    let subscription = publisher
        .subscribe(subscription(
            move |received| {
                if received.event_id().as_str() == "bounded-1" {
                    started_sender.send(()).unwrap();
                    release_receiver_for_handler
                        .lock()
                        .unwrap()
                        .recv()
                        .expect("test must release the blocked worker");
                }
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(1);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    let first = publisher.publish(event("bounded-1")).unwrap();
    assert_eq!(
        handle.enqueue(Arc::clone(first.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    // Wait until the worker is definitely inside the first callback. The
    // callback remains blocked, leaving one queue slot available.
    receive(&started_receiver);

    let second = publisher.publish(event("bounded-2")).unwrap();
    assert_eq!(
        handle.enqueue(Arc::clone(second.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    let third = publisher.publish(event("bounded-3")).unwrap();
    assert_eq!(
        handle.enqueue(Arc::clone(third.event())).unwrap(),
        DeliveryOutcome::Dropped
    );

    // Let the callback finish before shutting the dispatcher down.
    release_sender.send(()).unwrap();
    dispatcher.shutdown();
}

#[test]
fn unauthorized_subscription_is_not_delivered() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let calls = Arc::new(AtomicUsize::new(0));

    let subscription = publisher
        .subscribe(
            EventSubscription::new(
                EventName::new(EVENT_TYPE).unwrap(),
                EVENT_TYPE,
                scope(),
                CountingSubscriber(Arc::clone(&calls)),
                &owner,
            )
            .unwrap()
            .with_security_context(engine_security("unauthorized"))
            .with_authorizer(Arc::new(DenyAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    let publication = publisher.publish(event("security-deny")).unwrap();

    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Unauthorized
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    dispatcher.shutdown();
}

#[test]
fn authorization_failure_is_distinct_from_authorization_denial() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let denied = publisher
        .subscribe(
            EventSubscription::new(
                EventName::new(EVENT_TYPE).unwrap(),
                EVENT_TYPE,
                scope(),
                ignore_event,
                &owner,
            )
            .unwrap()
            .with_security_context(engine_security("denied"))
            .with_authorizer(Arc::new(DenyAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let failed = publisher
        .subscribe(
            EventSubscription::new(
                EventName::new(EVENT_TYPE).unwrap(),
                EVENT_TYPE,
                scope(),
                ignore_event,
                &owner,
            )
            .unwrap()
            .with_security_context(engine_security("failed"))
            .with_authorizer(Arc::new(FailingAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap()),
        )
        .unwrap();

    let dispatcher = dispatcher(4);
    let denied_handle = dispatcher.register(Arc::clone(&denied)).unwrap();
    let failed_handle = dispatcher.register(Arc::clone(&failed)).unwrap();
    let publication = publisher.publish(event("security-outcomes")).unwrap();

    assert_eq!(
        denied_handle
            .enqueue(Arc::clone(publication.event()))
            .unwrap(),
        DeliveryOutcome::Unauthorized
    );
    assert_eq!(
        failed_handle.enqueue(Arc::clone(publication.event())),
        Err(DeliveryError::AuthorizationFailed(
            AuthorizationError::Failed
        ))
    );

    dispatcher.shutdown();
}

#[test]
fn event_context_is_preserved_through_delivery() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let context = EventContext::empty()
        .with_operation_context(operation_context())
        .with_security_context(engine_security("producer"));

    let event = Event::new_with_context(
        EventId::new("context-event").unwrap(),
        EventName::new(EVENT_TYPE).unwrap(),
        EVENT_TYPE,
        scope(),
        context,
    )
    .unwrap();

    let subscription = publisher
        .subscribe(subscription(
            move |received| {
                let operation_id = received
                    .context()
                    .operation_context()
                    .unwrap()
                    .operation
                    .id
                    .to_string();
                let principal = received
                    .context()
                    .security_context()
                    .unwrap()
                    .principal()
                    .principal_id()
                    .to_string();
                sender.send((operation_id, principal)).unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    let publication = publisher.publish(event).unwrap();
    handle.enqueue(Arc::clone(publication.event())).unwrap();

    assert_eq!(
        receive(&receiver),
        (
            "event-conformance-operation".to_owned(),
            "producer".to_owned()
        )
    );

    dispatcher.shutdown();
}

#[test]
fn event_publication_does_not_mutate_an_unrelated_attempt() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let attempt = Attempt::new(
        OperationId::new("event-retry-boundary").unwrap(),
        AttemptId::new("event-retry-attempt").unwrap(),
        1,
    )
    .unwrap();

    let before = attempt.state();
    let publication = publisher.publish(event("no-retry-side-effect")).unwrap();

    assert_eq!(publication.subscription_count(), 0);
    assert_eq!(attempt.state(), before);
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
}

#[test]
fn event_publication_preserves_operation_context_without_creating_execution_state() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let context = EventContext::empty().with_operation_context(operation_context());
    let event = Event::new_with_context(
        EventId::new("pipeline-independent").unwrap(),
        EventName::new(EVENT_TYPE).unwrap(),
        EVENT_TYPE,
        scope(),
        context,
    )
    .unwrap();

    let publication = publisher.publish(event).unwrap();

    assert_eq!(
        publication
            .event()
            .context()
            .operation_context()
            .unwrap()
            .operation
            .id
            .as_str(),
        "event-conformance-operation"
    );
    assert_eq!(publication.event().event_type(), EVENT_TYPE);
}

#[test]
fn reentrant_publication_is_serialized_without_recursion_into_publish() {
    let owner = CancellationToken::new();
    let publisher = Arc::new(publisher(&owner));
    publisher.activate().unwrap();

    let (sender, receiver) = mpsc::channel();
    let handle_cell: Arc<Mutex<Option<nizaam_core::events::DeliveryHandle>>> =
        Arc::new(Mutex::new(None));

    let nested_publisher = Arc::clone(&publisher);
    let nested_handle = Arc::clone(&handle_cell);

    let subscription = publisher
        .subscribe(subscription(
            move |received| {
                sender.send(received.event_id().to_string()).unwrap();

                if received.event_id().as_str() == "reentrant-1" {
                    let handle = nested_handle.lock().unwrap().as_ref().unwrap().clone();

                    let nested = nested_publisher
                        .publish(event("reentrant-2"))
                        .expect("nested publication should be accepted");

                    handle.enqueue(Arc::clone(nested.event())).unwrap();
                }
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    *handle_cell.lock().unwrap() = Some(handle.clone());

    let publication = publisher.publish(event("reentrant-1")).unwrap();
    handle.enqueue(Arc::clone(publication.event())).unwrap();

    assert_eq!(receive(&receiver), "reentrant-1");
    assert_eq!(receive(&receiver), "reentrant-2");

    dispatcher.shutdown();
}

#[test]
fn closing_from_a_callback_does_not_leave_publisher_active() {
    let owner = CancellationToken::new();
    let publisher = Arc::new(publisher(&owner));
    publisher.activate().unwrap();

    let publisher_for_handler = Arc::clone(&publisher);
    let (closed_sender, closed_receiver) = mpsc::channel();

    let subscription = publisher
        .subscribe(subscription(
            move |_| {
                publisher_for_handler.close();
                closed_sender.send(()).unwrap();
            },
            &owner,
        ))
        .unwrap();

    let dispatcher = dispatcher(4);
    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();
    let publication = publisher.publish(event("callback-close")).unwrap();

    handle.enqueue(Arc::clone(publication.event())).unwrap();

    // Wait for the callback itself instead of polling. This makes the test
    // deterministic: the assertion runs only after close() has been invoked
    // from the delivery worker.
    receive(&closed_receiver);

    assert!(publisher.is_closed());
    dispatcher.shutdown();
}

#[test]
fn logging_event_keeps_event_identity_and_scoped_logging_semantics() {
    use nizaam_core::events::EventName;
    use nizaam_core::logging::{LogContext, LogEvent, LogEventType, LogLevel, LogScope, LogSource};

    let event = LogEvent::new(
        EventName::new("logging.conformance").unwrap(),
        LogLevel::Info,
        LogSource::ControlPlane,
        LogScope::Global,
        "control-plane",
        LogContext::new(operation_context()),
        "event conformance",
        LogEventType::RequestReceived,
    )
    .unwrap();

    assert_eq!(
        event.universal_event().event_name().as_str(),
        "logging.conformance"
    );
    assert_eq!(event.universal_event().event_type(), "request.received");
    assert!(!event.event_id().as_str().is_empty());
    assert!(!event.message_id().as_str().is_empty());
}

#[test]
fn event_system_does_not_change_retry_attempt_or_routing_inputs() {
    let owner = CancellationToken::new();
    let publisher = publisher(&owner);
    publisher.activate().unwrap();

    let attempt = Attempt::new(
        OperationId::new("event-routing-boundary").unwrap(),
        AttemptId::new("event-routing-attempt").unwrap(),
        1,
    )
    .unwrap();

    let event = Event::new_with_context(
        EventId::new("routing-boundary-event").unwrap(),
        EventName::new(EVENT_TYPE).unwrap(),
        EVENT_TYPE,
        scope(),
        EventContext::empty().with_operation_context(operation_context()),
    )
    .unwrap();

    let publication = publisher.publish(event).unwrap();

    assert_eq!(publication.subscription_count(), 0);
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    assert_eq!(attempt.attempt_number(), 1);
    assert_eq!(
        publication
            .event()
            .context()
            .operation_context()
            .unwrap()
            .operation
            .id
            .as_str(),
        "event-conformance-operation"
    );
}
