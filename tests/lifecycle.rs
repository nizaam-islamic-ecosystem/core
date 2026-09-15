//! Integration tests for the Phase 8 engine lifecycle.
//!
//! These tests exercise the lifecycle through the public crate API and also
//! verify that `EngineRuntime` exposes and enforces the same lifecycle rules.
//!
//! The lifecycle itself is intentionally tested separately from the runtime
//! execution pipeline in `tests/runtime.rs`.

use std::sync::Arc;
use std::thread;

use nizaam_core::error::InvalidTransition;
use nizaam_core::runtime::{EngineRuntime, Lifecycle, LifecycleState};

fn complete_lifecycle(lifecycle: &mut Lifecycle) {
    lifecycle.transition(LifecycleState::Starting).unwrap();
    lifecycle.transition(LifecycleState::Configuring).unwrap();
    lifecycle.transition(LifecycleState::Dependencies).unwrap();
    lifecycle.transition(LifecycleState::Capabilities).unwrap();
    lifecycle.transition(LifecycleState::Registering).unwrap();
    lifecycle.transition(LifecycleState::Ready).unwrap();
    lifecycle.transition(LifecycleState::Serving).unwrap();
    lifecycle.transition(LifecycleState::Draining).unwrap();
    lifecycle.transition(LifecycleState::Stopped).unwrap();
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new();

    runtime.transition(LifecycleState::Starting).unwrap();
    runtime.transition(LifecycleState::Configuring).unwrap();
    runtime.transition(LifecycleState::Dependencies).unwrap();
    runtime.transition(LifecycleState::Capabilities).unwrap();
    runtime.transition(LifecycleState::Registering).unwrap();
    runtime.transition(LifecycleState::Ready).unwrap();
    runtime.transition(LifecycleState::Serving).unwrap();

    runtime
}

#[test]
fn lifecycle_starts_created_and_reaches_stopped_through_every_required_stage() {
    let mut lifecycle = Lifecycle::new();

    assert_eq!(lifecycle.state(), LifecycleState::Created);

    complete_lifecycle(&mut lifecycle);

    assert_eq!(lifecycle.state(), LifecycleState::Stopped);
}

#[test]
fn lifecycle_requires_the_exact_startup_and_shutdown_order() {
    let mut lifecycle = Lifecycle::new();

    let expected_order = [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
        LifecycleState::Draining,
        LifecycleState::Stopped,
    ];

    for expected_state in expected_order {
        lifecycle.transition(expected_state).unwrap();
        assert_eq!(lifecycle.state(), expected_state);
    }
}

#[test]
fn lifecycle_rejects_skipping_any_required_stage() {
    let cases = [
        (LifecycleState::Created, LifecycleState::Configuring),
        (LifecycleState::Starting, LifecycleState::Dependencies),
        (LifecycleState::Configuring, LifecycleState::Capabilities),
        (LifecycleState::Dependencies, LifecycleState::Registering),
        (LifecycleState::Capabilities, LifecycleState::Ready),
        (LifecycleState::Registering, LifecycleState::Serving),
        (LifecycleState::Ready, LifecycleState::Draining),
    ];

    for (from, to) in cases {
        let mut lifecycle = Lifecycle::new();

        match from {
            LifecycleState::Created => {}
            LifecycleState::Starting => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
            }
            LifecycleState::Configuring => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
                lifecycle.transition(LifecycleState::Configuring).unwrap();
            }
            LifecycleState::Dependencies => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
                lifecycle.transition(LifecycleState::Configuring).unwrap();
                lifecycle.transition(LifecycleState::Dependencies).unwrap();
            }
            LifecycleState::Capabilities => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
                lifecycle.transition(LifecycleState::Configuring).unwrap();
                lifecycle.transition(LifecycleState::Dependencies).unwrap();
                lifecycle.transition(LifecycleState::Capabilities).unwrap();
            }
            LifecycleState::Registering => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
                lifecycle.transition(LifecycleState::Configuring).unwrap();
                lifecycle.transition(LifecycleState::Dependencies).unwrap();
                lifecycle.transition(LifecycleState::Capabilities).unwrap();
                lifecycle.transition(LifecycleState::Registering).unwrap();
            }
            LifecycleState::Ready => {
                lifecycle.transition(LifecycleState::Starting).unwrap();
                lifecycle.transition(LifecycleState::Configuring).unwrap();
                lifecycle.transition(LifecycleState::Dependencies).unwrap();
                lifecycle.transition(LifecycleState::Capabilities).unwrap();
                lifecycle.transition(LifecycleState::Registering).unwrap();
                lifecycle.transition(LifecycleState::Ready).unwrap();
            }
            LifecycleState::Serving | LifecycleState::Draining | LifecycleState::Stopped => {
                unreachable!()
            }
        }

        assert_eq!(
            lifecycle.transition(to),
            Err(InvalidTransition::new(
                format!("{from:?}"),
                format!("{to:?}"),
            ))
        );
        assert_eq!(lifecycle.state(), from);
    }
}

#[test]
fn lifecycle_rejects_direct_terminal_transitions_before_draining() {
    let mut lifecycle = Lifecycle::new();

    assert_eq!(
        lifecycle.transition(LifecycleState::Stopped),
        Err(InvalidTransition::new("Created", "Stopped"))
    );

    lifecycle.transition(LifecycleState::Starting).unwrap();

    assert_eq!(
        lifecycle.transition(LifecycleState::Stopped),
        Err(InvalidTransition::new("Starting", "Stopped"))
    );

    lifecycle.transition(LifecycleState::Configuring).unwrap();
    lifecycle.transition(LifecycleState::Dependencies).unwrap();
    lifecycle.transition(LifecycleState::Capabilities).unwrap();
    lifecycle.transition(LifecycleState::Registering).unwrap();
    lifecycle.transition(LifecycleState::Ready).unwrap();

    assert_eq!(
        lifecycle.transition(LifecycleState::Stopped),
        Err(InvalidTransition::new("Ready", "Stopped"))
    );
    assert_eq!(lifecycle.state(), LifecycleState::Ready);
}

#[test]
fn lifecycle_rejects_serving_after_draining() {
    let mut lifecycle = Lifecycle::new();

    lifecycle.transition(LifecycleState::Starting).unwrap();
    lifecycle.transition(LifecycleState::Configuring).unwrap();
    lifecycle.transition(LifecycleState::Dependencies).unwrap();
    lifecycle.transition(LifecycleState::Capabilities).unwrap();
    lifecycle.transition(LifecycleState::Registering).unwrap();
    lifecycle.transition(LifecycleState::Ready).unwrap();
    lifecycle.transition(LifecycleState::Serving).unwrap();
    lifecycle.transition(LifecycleState::Draining).unwrap();

    assert_eq!(
        lifecycle.transition(LifecycleState::Serving),
        Err(InvalidTransition::new("Draining", "Serving"))
    );
    assert_eq!(lifecycle.state(), LifecycleState::Draining);
}

#[test]
fn lifecycle_stopped_is_terminal() {
    let mut lifecycle = Lifecycle::new();

    complete_lifecycle(&mut lifecycle);

    for state in [
        LifecycleState::Created,
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
        LifecycleState::Draining,
        LifecycleState::Stopped,
    ] {
        assert!(
            lifecycle.transition(state).is_err(),
            "STOPPED must reject transition to {state:?}"
        );
    }

    assert_eq!(lifecycle.state(), LifecycleState::Stopped);
}

#[test]
fn lifecycle_error_preserves_source_and_destination_states() {
    let mut lifecycle = Lifecycle::new();

    lifecycle.transition(LifecycleState::Starting).unwrap();

    let error = lifecycle
        .transition(LifecycleState::Ready)
        .expect_err("READY cannot be entered directly from STARTING");

    assert_eq!(error.from, "Starting");
    assert_eq!(error.to, "Ready");
    assert_eq!(lifecycle.state(), LifecycleState::Starting);
}

#[test]
fn engine_runtime_exposes_the_same_ordered_lifecycle() {
    let runtime = EngineRuntime::new();

    assert_eq!(runtime.state(), LifecycleState::Created);

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
        assert_eq!(runtime.state(), state);
    }

    runtime.shutdown().unwrap();
    assert_eq!(runtime.state(), LifecycleState::Stopped);
}

#[test]
fn engine_runtime_rejects_invalid_transition_without_changing_state() {
    let runtime = EngineRuntime::new();

    let error = runtime
        .transition(LifecycleState::Serving)
        .expect_err("runtime must not skip startup states");

    assert_eq!(error, InvalidTransition::new("Created", "Serving"));
    assert_eq!(runtime.state(), LifecycleState::Created);

    runtime.transition(LifecycleState::Starting).unwrap();

    let error = runtime
        .transition(LifecycleState::Ready)
        .expect_err("runtime must not skip startup states");

    assert_eq!(error, InvalidTransition::new("Starting", "Ready"));
    assert_eq!(runtime.state(), LifecycleState::Starting);
}

#[test]
fn engine_runtime_shutdown_moves_serving_through_draining_to_stopped() {
    let runtime = serving_runtime();

    assert_eq!(runtime.state(), LifecycleState::Serving);

    runtime.shutdown().unwrap();

    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.shutdown_token().is_cancelled());
}

#[test]
fn engine_runtime_cannot_reenter_serving_after_draining() {
    let runtime = serving_runtime();

    runtime.transition(LifecycleState::Draining).unwrap();

    assert_eq!(
        runtime.transition(LifecycleState::Serving),
        Err(InvalidTransition::new("Draining", "Serving"))
    );
    assert_eq!(runtime.state(), LifecycleState::Draining);

    runtime.shutdown().unwrap();
    assert_eq!(runtime.state(), LifecycleState::Stopped);
}

#[test]
fn engine_runtime_keeps_stopped_terminal_and_shutdown_is_idempotent() {
    let runtime = serving_runtime();

    runtime.shutdown().unwrap();
    runtime.shutdown().unwrap();

    assert_eq!(runtime.state(), LifecycleState::Stopped);

    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
        LifecycleState::Draining,
        LifecycleState::Stopped,
    ] {
        assert!(runtime.transition(state).is_err());
    }
}

#[test]
fn independent_engine_runtimes_keep_lifecycle_state_isolated() {
    let runtime_a = Arc::new(serving_runtime());
    let runtime_b = Arc::new(serving_runtime());

    let worker_a = {
        let runtime = Arc::clone(&runtime_a);
        thread::spawn(move || {
            runtime.transition(LifecycleState::Draining).unwrap();
            runtime.shutdown().unwrap();
        })
    };

    let worker_b = {
        let runtime = Arc::clone(&runtime_b);
        thread::spawn(move || {
            runtime.transition(LifecycleState::Draining).unwrap();
            runtime.shutdown().unwrap();
        })
    };

    worker_a.join().unwrap();
    worker_b.join().unwrap();

    assert_eq!(runtime_a.state(), LifecycleState::Stopped);
    assert_eq!(runtime_b.state(), LifecycleState::Stopped);
    assert!(runtime_a.shutdown_token().is_cancelled());
    assert!(runtime_b.shutdown_token().is_cancelled());
}
