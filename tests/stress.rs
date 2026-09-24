//! Phase 16 bounded stress and resource-isolation tests.
//!
//! Workloads are deliberately finite and deterministic. These are correctness
//! stress tests, not benchmarks.

use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;

use nizaam_core::client::UniversalClient;
use nizaam_core::config::{
    resolution::ConfigurationResolver,
    snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
    validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse, Version,
};
use nizaam_core::control_plane::membership::Membership;
use nizaam_core::control_plane::registration::{
    Endpoint, EngineRegistration, RuntimeRegistrationMetadata,
};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventName, EventPublisher,
    EventSubscription, Scope,
};
use nizaam_core::health::ReadinessReport;
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, EventId, MessageId,
    OperationId,
};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::retry::{Attempt, AttemptLifecycleState, RetryBudget};
use nizaam_core::runtime::{
    ConcurrencyConfig, ConcurrencyState, EngineContext, EngineRuntime, LifecycleState, TaskScope,
};
use nizaam_core::status::Status;
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamItem, StreamLifecycleState,
};
use nizaam_core::transport::InMemoryTransport;

fn operation_context(index: usize) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(format!("stress-operation-{index}")).unwrap(),
        CorrelationId::new(format!("stress-correlation-{index}")).unwrap(),
    ))
}

fn request(target: &EngineId, instance: &EngineInstanceId, index: usize) -> UniversalRequest {
    let payload =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("stress.contract").unwrap(),
        CapabilityId::new("stress.echo").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload.clone(),
    );
    let participants = Participants::new(EngineId::new("stress-client").unwrap(), target.clone())
        .with_target_instance(instance.clone());
    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("stress-message-{index}")).unwrap(),
        operation_context(index),
        ContractMetadata::new(descriptor, participants),
        EncodedPayload::new(payload, index.to_le_bytes()),
    ))
}

fn configuration(id: u64) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(std::collections::BTreeMap::new());
    parsed.insert(
        "stress.value".to_owned(),
        ConfigurationValue::Integer(id as i64),
    );
    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();
    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}

fn registration(instance: &str) -> EngineRegistration {
    EngineRegistration::new(
        EngineId::new("stress-engine").unwrap(),
        EngineInstanceId::new(instance).unwrap(),
    )
    .with_endpoint(Endpoint::new(format!("memory://{instance}")).unwrap())
    .with_runtime_metadata(
        RuntimeRegistrationMetadata::new()
            .with_lifecycle(LifecycleState::Serving)
            .with_readiness(ReadinessReport::ready()),
    )
}

#[test]
fn bounded_active_concurrency_never_exceeds_configuration() {
    let config = ConcurrencyConfig::new(2, 2).unwrap();
    let state = Arc::new(Mutex::new(ConcurrencyState::new()));
    let barrier = Arc::new(Barrier::new(5));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);
        let peak = Arc::clone(&peak);
        handles.push(thread::spawn(move || {
            barrier.wait();
            let acquired = {
                let mut state = state.lock().unwrap();
                state.try_acquire_active(&config).is_ok()
            };
            if acquired {
                let active = state.lock().unwrap().active();
                peak.fetch_max(active, Ordering::SeqCst);
                let mut state = state.lock().unwrap();
                state.release_active().unwrap();
            }
        }));
    }
    barrier.wait();
    for handle in handles {
        handle.join().unwrap();
    }

    assert!(peak.load(Ordering::SeqCst) <= config.max_active());
    assert!(state.lock().unwrap().is_empty());
}

#[test]
fn bounded_queue_rejects_admission_after_capacity_is_full() {
    let config = ConcurrencyConfig::new(1, 2).unwrap();
    let mut state = ConcurrencyState::new();
    state.try_enqueue(&config).unwrap();
    state.try_enqueue(&config).unwrap();
    assert!(state.try_enqueue(&config).is_err());
    assert_eq!(state.queued(), 2);
}

#[test]
fn capacity_recovers_without_counter_drift() {
    let config = ConcurrencyConfig::new(1, 2).unwrap();
    let mut state = ConcurrencyState::new();
    state.try_acquire_active(&config).unwrap();
    state.try_enqueue(&config).unwrap();
    state.release_active().unwrap();
    state.dequeue().unwrap();
    assert!(state.is_empty());
    state.try_acquire_active(&config).unwrap();
    state.release_active().unwrap();
    assert!(state.is_empty());
}

#[test]
fn independent_task_scopes_cancel_without_cross_scope_cancellation() {
    let parent = CancellationToken::new();
    let first = TaskScope::new(&parent);
    let second = TaskScope::new(&parent);
    first.cancel();
    assert!(first.is_cancelled());
    assert!(!second.is_cancelled());
    assert!(!parent.is_cancelled());
}

#[test]
fn runtime_shutdown_cleans_active_background_work() {
    let runtime = EngineRuntime::new(
        EngineId::new("stress-runtime").unwrap(),
        EngineInstanceId::new("stress-runtime-instance").unwrap(),
    );
    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime.transition(state).unwrap();
    }
    let finished = Arc::new(AtomicUsize::new(0));
    for _ in 0..3 {
        let finished = Arc::clone(&finished);
        runtime
            .background_tasks()
            .spawn(move |token| {
                while !token.is_cancelled() {
                    thread::yield_now();
                }
                finished.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
    }

    assert!(runtime.shutdown().unwrap());
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert_eq!(finished.load(Ordering::SeqCst), 3);
}

#[test]
fn repeated_runtime_lifecycle_cycles_remain_isolated() {
    for index in 0..5 {
        let runtime = EngineRuntime::new(
            EngineId::new(format!("stress-engine-{index}")).unwrap(),
            EngineInstanceId::new(format!("stress-instance-{index}")).unwrap(),
        );
        for state in [
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
            LifecycleState::Registering,
            LifecycleState::Ready,
            LifecycleState::Serving,
            LifecycleState::Draining,
        ] {
            runtime.transition(state).unwrap();
        }

        assert!(runtime.shutdown().unwrap());
        assert_eq!(runtime.state(), LifecycleState::Stopped);
    }
}

#[test]
fn bounded_batch_of_real_client_requests_preserves_message_pairing() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("stress-client-engine").unwrap();
    let instance = EngineInstanceId::new("stress-client-instance").unwrap();
    transport.register(engine.clone(), instance.clone(), |request| {
        UniversalResponse::new(request.event.envelope, Status::Success)
    });
    let client = UniversalClient::new(transport);

    for index in 0..24 {
        let response =
            futures::executor::block_on(client.send(&instance, request(&engine, &instance, index)))
                .unwrap();
        assert_eq!(response.status, Status::Success);
        assert_eq!(
            response.event.envelope.message_id.as_str(),
            format!("stress-message-{index}")
        );
    }
}

#[test]
fn multiple_streams_keep_item_order_and_terminal_state_isolated() {
    let mut streams = Vec::new();
    for index in 0..4 {
        let stream: Stream<u32> = Stream::new(
            &EngineContext::new(operation_context(index)),
            BackpressureConfig::new(3, BackpressurePolicy::Reject).unwrap(),
        )
        .unwrap();
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();
        stream
            .publish(StreamItem::partial(0, index as u32))
            .unwrap();
        stream
            .publish(StreamItem::final_item(1, 100 + index as u32))
            .unwrap();
        streams.push((stream, consumer));
    }

    for (stream, consumer) in streams {
        assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 0);
        assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 1);
        assert_eq!(consumer.next_item().unwrap(), None);
        assert_eq!(stream.state(), StreamLifecycleState::Completed);
    }
}

#[test]
fn slow_consumer_pressure_rejects_without_dropping_already_accepted_item() {
    let stream: Stream<u32> = Stream::new(
        &EngineContext::new(operation_context(99)),
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 7)).unwrap();
    assert!(stream.publish(StreamItem::partial(1, 8)).is_err());
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &7);
}

#[test]
fn retry_budget_pressure_stops_at_configured_limit() {
    let mut budget = RetryBudget::new(3);
    for _ in 0..3 {
        budget.try_consume().unwrap();
    }
    assert!(budget.is_exhausted());
    assert!(budget.try_consume().is_err());
    assert_eq!(budget.consumed(), 3);
}

#[test]
fn membership_churn_preserves_snapshot_immutability() {
    let membership = Membership::new();
    membership
        .register(registration("stress-member-1"))
        .unwrap();
    membership
        .register(registration("stress-member-2"))
        .unwrap();
    let first = membership.snapshot();
    membership
        .unregister(&EngineInstanceId::new("stress-member-1").unwrap())
        .unwrap();
    let second = membership.snapshot();

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    assert!(first.contains(&EngineInstanceId::new("stress-member-1").unwrap()));
    assert!(!second.contains(&EngineInstanceId::new("stress-member-1").unwrap()));
}

#[test]
fn event_delivery_pressure_keeps_subscriber_delivery_bounded_and_isolated() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();

    let (slow_started_tx, slow_started_rx) = std::sync::mpsc::channel();
    let (slow_release_tx, slow_release_rx) = std::sync::mpsc::channel();
    let slow_release_rx = Arc::new(Mutex::new(slow_release_rx));
    let slow_subscription = EventSubscription::new(
        EventName::new("stress.event").unwrap(),
        "stress.event",
        Scope::new("engine:stress").unwrap(),
        {
            let slow_release_rx = Arc::clone(&slow_release_rx);
            move |event: &Event| {
                if event.event_id().as_str() == "stress-event-0" {
                    slow_started_tx.send(()).unwrap();
                    let _ = slow_release_rx
                        .lock()
                        .expect("slow release receiver lock should not be poisoned")
                        .recv();
                }
            }
        },
        &owner,
    )
    .unwrap();
    let slow_subscription = publisher.subscribe(slow_subscription).unwrap();

    let (healthy_tx, healthy_rx) = std::sync::mpsc::channel();
    let (healthy_started_tx, healthy_started_rx) = std::sync::mpsc::channel();
    let healthy_subscription = EventSubscription::new(
        EventName::new("stress.event").unwrap(),
        "stress.event",
        Scope::new("engine:stress").unwrap(),
        move |event: &Event| {
            healthy_started_tx.send(()).unwrap();
            healthy_tx
                .send(event.event_id().as_str().to_owned())
                .unwrap();
        },
        &owner,
    )
    .unwrap();
    let healthy_subscription = publisher.subscribe(healthy_subscription).unwrap();

    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(1, 2, 8).unwrap(), owner.clone()).unwrap();

    struct SlowReleaseGuard(Option<std::sync::mpsc::Sender<()>>);

    impl SlowReleaseGuard {
        fn release(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    impl Drop for SlowReleaseGuard {
        fn drop(&mut self) {
            self.release();
        }
    }

    let mut slow_release_guard = SlowReleaseGuard(Some(slow_release_tx));
    let slow_handle = dispatcher.register(slow_subscription).unwrap();
    let healthy_handle = dispatcher.register(healthy_subscription).unwrap();

    let event = |index: usize| {
        Event::new(
            EventId::new(format!("stress-event-{index}")).unwrap(),
            EventName::new("stress.event").unwrap(),
            "stress.event",
            Scope::new("engine:stress").unwrap(),
        )
        .unwrap()
    };

    let first = publisher.publish(event(0)).unwrap();
    assert_eq!(
        slow_handle.enqueue(Arc::clone(first.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        healthy_handle.enqueue(Arc::clone(first.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    slow_started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();

    let second = publisher.publish(event(1)).unwrap();
    let slow_second = slow_handle.enqueue(Arc::clone(second.event())).unwrap();

    let third = publisher.publish(event(2)).unwrap();
    let slow_third = slow_handle.enqueue(Arc::clone(third.event())).unwrap();

    assert_eq!(slow_second, DeliveryOutcome::Accepted);
    assert_eq!(slow_third, DeliveryOutcome::Dropped);

    assert_eq!(
        healthy_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        "stress-event-0"
    );
    healthy_started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();

    assert_eq!(
        healthy_handle.enqueue(Arc::clone(second.event())).unwrap(),
        DeliveryOutcome::Accepted
    );

    assert_eq!(
        healthy_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        "stress-event-1"
    );

    slow_release_guard.release();
    dispatcher.shutdown();
}

#[test]
fn repeated_configuration_snapshots_do_not_accumulate_duplicate_runtime_state() {
    let contexts: Vec<_> = (0..10)
        .map(|index| {
            EngineContext::new(operation_context(index))
                .with_configuration(configuration(index as u64 + 1))
        })
        .collect();

    assert_eq!(contexts.len(), 10);
    for (index, context) in contexts.iter().enumerate() {
        assert_eq!(
            context.configuration().unwrap().id(),
            ConfigurationSnapshotId::new(index as u64 + 1)
        );
    }
}

#[test]
fn repeated_attempts_keep_one_logical_operation_identity() {
    let operation = OperationId::new("stress-logical-operation").unwrap();
    let mut attempts = Vec::new();
    for number in 1..=8 {
        attempts.push(
            Attempt::new(
                operation.clone(),
                nizaam_core::identity::AttemptId::new(format!("stress-attempt-{number}")).unwrap(),
                number,
            )
            .unwrap(),
        );
    }

    for (index, attempt) in attempts.iter().enumerate() {
        assert_eq!(attempt.operation_id(), &operation);
        assert_eq!(attempt.attempt_number(), index as u32 + 1);
        assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    }
}
