use crate::support::{section, show_arrow, step, success};
use nizaam_core::identity::{EngineId, EngineInstanceId};
use nizaam_core::runtime::{EngineRuntime, LifecycleState};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

#[test]
fn visual_runtime_lifecycle_and_admission() {
    section("NIZAAM CORE — LIFECYCLE + RUNTIME");
    let runtime = EngineRuntime::new(
        EngineId::new("visual-engine").unwrap(),
        EngineInstanceId::new("visual-instance").unwrap(),
    );
    let states = [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ];
    step(1, "ordered lifecycle");
    for state in states {
        runtime.transition(state).unwrap();
        println!("  → {state:?}");
    }
    assert_eq!(runtime.state(), LifecycleState::Serving);
    runtime.admit_request().unwrap();
    success("Serving admits normal request work");

    step(2, "invalid transition does not mutate state");
    assert!(runtime.transition(LifecycleState::Created).is_err());
    assert_eq!(runtime.state(), LifecycleState::Serving);
    success("invalid/backward transition is rejected locally");

    step(3, "draining rejects new admission");
    runtime.transition(LifecycleState::Draining).unwrap();
    assert!(runtime.admit_request().is_err());
    println!("  Serving → Draining → admission rejected");
    show_arrow("Draining", "No new normal request");

    step(4, "shutdown owns background cleanup");
    let finished = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&finished);
    runtime
        .background_tasks()
        .spawn(move |token| {
            while !token.is_cancelled() {
                thread::yield_now();
            }
            observed.store(true, Ordering::SeqCst);
        })
        .unwrap();
    assert!(runtime.shutdown().unwrap());
    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(finished.load(Ordering::SeqCst));
    success("shutdown signals cancellation, joins runtime-owned work, and reaches Stopped");
}
