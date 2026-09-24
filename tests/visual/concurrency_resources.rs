use crate::support::{section, show_arrow, step, success};
use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::runtime::{
    BackgroundTasks, ConcurrencyConfig, ConcurrencyState, EngineContext, TaskScope,
};
use nizaam_core::streaming::{BackpressureConfig, BackpressurePolicy, Stream, StreamItem};
use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;

fn context(index: usize) -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new(format!("visual-resource-op-{index}")).unwrap(),
        CorrelationId::new(format!("visual-resource-corr-{index}")).unwrap(),
    )))
}

#[test]
fn visual_bounded_resources_and_cleanup() {
    section("NIZAAM CORE — CONCURRENCY + RESOURCES");
    step(1, "bounded active concurrency");
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
                let mut guard = state.lock().unwrap();
                guard.try_acquire_active(&config).is_ok()
            };
            if acquired {
                peak.fetch_max(state.lock().unwrap().active(), Ordering::SeqCst);
                state.lock().unwrap().release_active().unwrap();
            }
        }));
    }
    barrier.wait();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(peak.load(Ordering::SeqCst) <= config.max_active());
    assert!(state.lock().unwrap().is_empty());
    println!("  max active : {}", config.max_active());
    println!("  observed peak: {}", peak.load(Ordering::SeqCst));
    success("active work never exceeds configured bound");

    step(2, "queue saturation and recovery");
    let mut queue = ConcurrencyState::new();
    queue.try_enqueue(&config).unwrap();
    queue.try_enqueue(&config).unwrap();
    assert!(queue.try_enqueue(&config).is_err());
    assert_eq!(queue.queued(), config.max_queued());
    queue.dequeue().unwrap();
    assert_eq!(queue.queued(), config.max_queued() - 1);
    queue.dequeue().unwrap();
    assert_eq!(queue.queued(), 0);
    println!("  queue: max / max → max-1 / max → 0");
    show_arrow("queue full", "dequeue → capacity available");
    success("queue accounting returns to baseline");

    step(3, "bounded stream pressure");
    let stream: Stream<u8> = Stream::new(
        &context(1),
        BackpressureConfig::new(1, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream.publish(StreamItem::partial(0, 1)).unwrap();
    assert!(stream.publish(StreamItem::partial(1, 2)).is_err());
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &1);
    stream.publish(StreamItem::final_item(1, 2)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().payload(), &2);
    success("consumer-driven recovery releases bounded stream capacity");

    step(4, "runtime-owned background cleanup");
    let tasks = BackgroundTasks::with_concurrency(
        CancellationToken::new(),
        ConcurrencyConfig::new(2, 1).unwrap(),
    );
    let finished = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let finished = Arc::clone(&finished);
        tasks
            .spawn_bounded(move |token| {
                while !token.is_cancelled() {
                    thread::yield_now();
                }
                finished.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
    }
    tasks.shutdown();
    assert_eq!(finished.load(Ordering::SeqCst), 2);
    success("shutdown cancels and joins runtime-owned tasks");

    step(5, "independent task scopes");
    let parent = CancellationToken::new();
    let first = TaskScope::new(&parent);
    let second = TaskScope::new(&parent);
    first.cancel();
    assert!(first.is_cancelled());
    assert!(!second.is_cancelled());
    assert!(!parent.is_cancelled());
    success("cancellation remains scoped and does not leak to siblings or parent");
}
