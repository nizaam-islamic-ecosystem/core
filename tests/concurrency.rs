use nizaam_core::runtime::{
    CancellationToken, ConcurrencyConfig, ConcurrencyError, ConcurrencyState, TaskScope,
};
use std::{
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

#[test]
fn concurrency_config_preserves_limits() {
    let config = ConcurrencyConfig::new(4, 8).unwrap();

    assert_eq!(config.max_active(), 4);
    assert_eq!(config.max_queued(), 8);
}

#[test]
fn zero_limits_are_rejected() {
    assert_eq!(
        ConcurrencyConfig::new(0, 8),
        Err(ConcurrencyError::ZeroActiveLimit)
    );
    assert_eq!(
        ConcurrencyConfig::new(4, 0),
        Err(ConcurrencyError::ZeroQueueLimit)
    );
}

#[test]
fn active_work_is_bounded_by_the_configured_limit() {
    let config = ConcurrencyConfig::new(2, 4).unwrap();
    let mut state = ConcurrencyState::new();

    assert_eq!(state.try_acquire_active(&config), Ok(()));
    assert_eq!(state.try_acquire_active(&config), Ok(()));
    assert_eq!(
        state.try_acquire_active(&config),
        Err(ConcurrencyError::ActiveLimitReached)
    );
    assert_eq!(state.active(), 2);
}

#[test]
fn releasing_active_work_restores_capacity() {
    let config = ConcurrencyConfig::new(1, 1).unwrap();
    let mut state = ConcurrencyState::new();

    state.try_acquire_active(&config).unwrap();
    assert_eq!(
        state.try_acquire_active(&config),
        Err(ConcurrencyError::ActiveLimitReached)
    );

    state.release_active().unwrap();

    assert_eq!(state.active(), 0);
    assert_eq!(state.try_acquire_active(&config), Ok(()));
    assert_eq!(state.active(), 1);
}

#[test]
fn queued_work_is_bounded_and_dequeue_restores_capacity() {
    let config = ConcurrencyConfig::new(2, 2).unwrap();
    let mut state = ConcurrencyState::new();

    assert_eq!(state.try_enqueue(&config), Ok(()));
    assert_eq!(state.try_enqueue(&config), Ok(()));
    assert_eq!(
        state.try_enqueue(&config),
        Err(ConcurrencyError::QueueLimitReached)
    );
    assert_eq!(state.queued(), 2);

    state.dequeue().unwrap();

    assert_eq!(state.queued(), 1);
    assert_eq!(state.try_enqueue(&config), Ok(()));
    assert_eq!(state.queued(), 2);
}

#[test]
fn rejected_admission_does_not_mutate_occupancy() {
    let config = ConcurrencyConfig::new(1, 1).unwrap();
    let mut state = ConcurrencyState::new();

    state.try_acquire_active(&config).unwrap();
    assert_eq!(
        state.try_acquire_active(&config),
        Err(ConcurrencyError::ActiveLimitReached)
    );
    assert_eq!(state.active(), 1);

    state.try_enqueue(&config).unwrap();
    assert_eq!(
        state.try_enqueue(&config),
        Err(ConcurrencyError::QueueLimitReached)
    );
    assert_eq!(state.queued(), 1);
}

#[test]
fn empty_releases_are_explicit() {
    let mut state = ConcurrencyState::new();

    assert_eq!(
        state.release_active(),
        Err(ConcurrencyError::ActiveUnderflow)
    );
    assert_eq!(state.dequeue(), Err(ConcurrencyError::QueueUnderflow));
    assert!(state.is_empty());
}

#[test]
fn concurrent_active_admission_never_exceeds_the_limit() {
    let config = ConcurrencyConfig::new(2, 4).unwrap();
    let state = Arc::new(Mutex::new(ConcurrencyState::new()));
    let barrier = Arc::new(Barrier::new(5));
    let successful = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for _ in 0..4 {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);
        let successful = Arc::clone(&successful);

        handles.push(thread::spawn(move || {
            barrier.wait();

            let mut state = state.lock().unwrap();
            if state.try_acquire_active(&config).is_ok() {
                successful.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    assert!(successful.load(Ordering::SeqCst) <= config.max_active());
    assert!(
        state.lock().unwrap().active() <= config.max_active(),
        "active occupancy exceeded configured limit"
    );
}

#[test]
fn parallel_workers_can_execute_when_capacity_is_available() {
    let config = ConcurrencyConfig::new(2, 2).unwrap();
    let state = Arc::new(Mutex::new(ConcurrencyState::new()));
    let barrier = Arc::new(Barrier::new(3));
    let running = Arc::new(AtomicUsize::new(0));
    let peak_running = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for _ in 0..2 {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);
        let running = Arc::clone(&running);
        let peak_running = Arc::clone(&peak_running);

        handles.push(thread::spawn(move || {
            {
                let mut state = state.lock().unwrap();
                state.try_acquire_active(&config).unwrap();
            }

            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak_running.fetch_max(now, Ordering::SeqCst);

            barrier.wait();
            thread::yield_now();

            running.fetch_sub(1, Ordering::SeqCst);

            let mut state = state.lock().unwrap();
            state.release_active().unwrap();
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(peak_running.load(Ordering::SeqCst), 2);
    assert!(state.lock().unwrap().is_empty());
}

#[test]
fn concurrent_workers_preserve_admission_state_invariants() {
    let config = ConcurrencyConfig::new(4, 4).unwrap();
    let state = Arc::new(Mutex::new(ConcurrencyState::new()));
    let barrier = Arc::new(Barrier::new(9));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);

        handles.push(thread::spawn(move || {
            barrier.wait();

            let mut state = state.lock().unwrap();
            let _ = state.try_acquire_active(&config);
            let _ = state.try_enqueue(&config);
        }));
    }

    for _ in 0..4 {
        let state = Arc::clone(&state);
        let barrier = Arc::clone(&barrier);

        handles.push(thread::spawn(move || {
            barrier.wait();

            let mut state = state.lock().unwrap();
            let _ = state.release_active();
            let _ = state.dequeue();
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    let state = state.lock().unwrap();
    assert!(state.active() <= config.max_active());
    assert!(state.queued() <= config.max_queued());
}

#[test]
fn parent_cancellation_propagates_to_concurrent_task_scopes() {
    let parent = CancellationToken::new();
    let scope_a = TaskScope::new(&parent);
    let scope_b = TaskScope::new(&parent);

    parent.cancel();

    assert!(scope_a.is_cancelled());
    assert!(scope_b.is_cancelled());
}

#[test]
fn child_cancellation_isolated_from_sibling_and_parent_scopes() {
    let parent = CancellationToken::new();
    let scope_a = TaskScope::new(&parent);
    let scope_b = TaskScope::new(&parent);

    scope_a.cancel();

    assert!(scope_a.is_cancelled());
    assert!(!scope_b.is_cancelled());
    assert!(!parent.is_cancelled());
}

#[test]
fn cancelled_work_can_release_capacity_without_leaking_resources() {
    let config = ConcurrencyConfig::new(1, 1).unwrap();
    let cancellation = CancellationToken::new();
    let scope = TaskScope::new(&cancellation);
    let mut state = ConcurrencyState::new();

    state.try_acquire_active(&config).unwrap();
    assert_eq!(state.active(), 1);

    scope.cancel();
    assert!(scope.is_cancelled());

    state.release_active().unwrap();

    assert_eq!(state.active(), 0);
    assert!(state.is_empty());
}

#[test]
fn bounded_state_can_be_reused_after_full_occupancy_is_released() {
    let config = ConcurrencyConfig::new(2, 2).unwrap();
    let mut state = ConcurrencyState::new();

    state.try_acquire_active(&config).unwrap();
    state.try_acquire_active(&config).unwrap();
    state.try_enqueue(&config).unwrap();
    state.try_enqueue(&config).unwrap();

    assert!(!state.is_empty());

    state.release_active().unwrap();
    state.release_active().unwrap();
    state.dequeue().unwrap();
    state.dequeue().unwrap();

    assert!(state.is_empty());
    assert_eq!(state.try_acquire_active(&config), Ok(()));
    assert_eq!(state.try_enqueue(&config), Ok(()));
}

#[test]
fn task_scope_is_send_and_sync_through_the_public_runtime_api() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<TaskScope>();
}
