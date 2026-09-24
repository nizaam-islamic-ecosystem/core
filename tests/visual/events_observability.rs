use crate::support::{section, separator, show_arrow, step, success};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventName, EventPublisher,
    EventSubscription, Scope,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, CorrelationId, EngineId, EngineInstanceId, EventId, MessageId,
    OperationId,
};
use nizaam_core::operation::CancellationToken;
use std::sync::{Arc, mpsc};

#[test]
fn visual_events_and_observability_lineage() {
    section("NIZAAM CORE — EVENTS + OBSERVABILITY");
    let owner = CancellationToken::new();
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();
    let (sender, receiver) = mpsc::channel();

    step(1, "event creation with causal identities");
    let operation = OperationId::new("visual-event-operation").unwrap();
    let attempt = AttemptId::new("visual-event-attempt").unwrap();
    let engine = EngineId::new("visual-engine").unwrap();
    let instance = EngineInstanceId::new("visual-engine-instance").unwrap();
    let capability = CapabilityId::new("visual.events").unwrap();
    let message = MessageId::new("visual-message").unwrap();
    let correlation = CorrelationId::new("visual-correlation").unwrap();
    let event = Event::new(
        EventId::new("visual-event").unwrap(),
        EventName::new("visual.completed").unwrap(),
        "visual.completed",
        Scope::new("engine:visual").unwrap(),
    )
    .unwrap();
    println!("  OperationId       : {operation}");
    println!("  AttemptId         : {attempt}");
    println!("  EngineId          : {engine}");
    println!("  EngineInstanceId  : {instance}");
    println!("  CapabilityId      : {capability}");
    println!("  MessageId         : {message}");
    println!("  CorrelationId     : {correlation}");
    println!("  EventId           : {}", event.event_id());
    show_arrow("Operation / attempt / capability", "Event publication");

    step(2, "matching subscriber");
    let healthy_sender = sender.clone();
    let subscription = EventSubscription::new(
        EventName::new("visual.completed").unwrap(),
        "visual.completed",
        Scope::new("engine:visual").unwrap(),
        move |event: &Event| {
            healthy_sender.send(event.event_id().clone()).unwrap();
        },
        &owner,
    )
    .unwrap();
    let subscription = publisher.subscribe(subscription).unwrap();
    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(4, 1, 4).unwrap(), owner.clone()).unwrap();
    let handle = dispatcher.register(subscription).unwrap();
    let publication = publisher.publish(event).unwrap();
    assert_eq!(publication.subscription_count(), 1);
    assert_eq!(
        handle.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap()
            .as_str(),
        "visual-event"
    );
    success("matching subscriber receives one local event occurrence");

    step(3, "subscription lifecycle");
    let second_owner = CancellationToken::new();
    let second = EventSubscription::new(
        EventName::new("visual.completed").unwrap(),
        "visual.completed",
        Scope::new("engine:visual").unwrap(),
        |_event: &Event| {},
        &second_owner,
    )
    .unwrap();
    let second = publisher.subscribe(second).unwrap();
    second.cancel().unwrap();
    let publication = publisher
        .publish(
            Event::new(
                EventId::new("visual-event-2").unwrap(),
                EventName::new("visual.completed").unwrap(),
                "visual.completed",
                Scope::new("engine:visual").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let delivery = handle.enqueue(Arc::clone(publication.event())).unwrap();
    assert_eq!(delivery, DeliveryOutcome::Accepted);
    assert!(second.is_cancelled());
    println!("  cancelled subscription state: {:?}", second.state());
    success("subscription cancellation is local to its lifecycle");
    dispatcher.shutdown();
    separator();
}
