//! Phase 16 reference/conformance engine tests.
//!
//! The shared `ReferenceEngine` implementation lives in `tests/common` so
//! E2E and cross-module integration tests reuse the same deterministic engine
//! without importing this test-bearing crate.

use std::time::Duration;

use nizaam_core::capability::CapabilityError;
use nizaam_core::identity::{CorrelationId, EngineId, EngineInstanceId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::{EngineContext, LifecycleState, RequestAdmissionError};
use nizaam_core::streaming::BackpressurePolicy;

mod common;

use common::reference_engine::{ReferenceBehavior, ReferenceDispatchError, ReferenceEngine};

fn new_engine() -> ReferenceEngine {
    ReferenceEngine::new(
        EngineId::new("phase16-engine-a").unwrap(),
        EngineInstanceId::new("phase16-instance-a").unwrap(),
    )
}

fn test_context() -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new("phase16-test-operation").unwrap(),
        CorrelationId::new("phase16-test-correlation").unwrap(),
    )))
}

#[test]
fn reference_engine_preserves_explicit_engine_identity() {
    let engine = new_engine();

    assert!(!engine.engine_id().as_str().is_empty());
    assert!(!engine.instance_id().as_str().is_empty());
    assert_eq!(engine.runtime().state(), LifecycleState::Created);
    assert_eq!(engine.state(), LifecycleState::Created);
}

#[test]
fn reference_engine_registers_capability_in_real_registry() {
    let engine = new_engine();

    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();

    assert!(engine.has_capability("conformance.echo"));
    assert_eq!(engine.registry().len(), 1);
}

#[test]
fn echo_behavior_preserves_opaque_payload() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let payload = [0_u8, 255, 17, 42, 128];
    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.echo",
            "conformance.contract",
            &payload,
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), payload);
    assert_eq!(engine.invocation_count("conformance.echo"), 1);
}

#[test]
fn unknown_capability_is_rejected_by_real_dispatch() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.missing",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(CapabilityError::Unknown))
    ));
    assert_eq!(engine.invocation_count("conformance.echo"), 0);
}

#[test]
fn handler_failure_remains_capability_failure() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail", ReferenceBehavior::Fail)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.fail",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.fail"), 1);
}

#[test]
fn fail_once_behavior_is_deterministic() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail_once", ReferenceBehavior::FailOnce)
        .unwrap();
    engine.serving().unwrap();

    let first = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );
    assert!(matches!(
        first,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));

    let second = engine
        .dispatch(
            &test_context(),
            "conformance.fail_once",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(second.as_bytes(), b"payload");
    assert_eq!(engine.invocation_count("conformance.fail_once"), 2);
}

#[test]
fn already_expired_deadline_is_rejected_before_reference_dispatch() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.delay",
            ReferenceBehavior::Delay(Duration::from_millis(5)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context()
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::ZERO).unwrap());

    let result = engine.dispatch(
        &context,
        "conformance.delay",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::DeadlineExpired
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.delay"), 0);
}

#[test]
fn live_deadline_shorter_than_delay_rejects_after_handler_starts() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.live-deadline",
            ReferenceBehavior::Delay(Duration::from_millis(50)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context()
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::from_millis(5)).unwrap());

    let result = engine.dispatch(
        &context,
        "conformance.live-deadline",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::DeadlineExpired
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.live-deadline"), 1);
}

#[test]
fn cancelled_context_is_rejected_before_handler_execution() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.delay",
            ReferenceBehavior::Delay(Duration::from_millis(5)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context();
    context.cancellation().cancel();

    let result = engine.dispatch(
        &context,
        "conformance.delay",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::Cancelled
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.delay"), 0);
}

#[test]
fn large_payload_behavior_returns_test_supplied_bytes_unchanged() {
    let payload = vec![42_u8; 4096];
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.large",
            ReferenceBehavior::LargePayload(payload.clone()),
        )
        .unwrap();
    engine.serving().unwrap();

    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.large",
            "conformance.contract",
            b"ignored",
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), payload.as_slice());
}

#[test]
fn side_effect_behavior_records_actual_handler_execution() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.side_effect",
            ReferenceBehavior::SideEffect(b"committed".to_vec()),
        )
        .unwrap();
    engine.serving().unwrap();

    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.side_effect",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), b"committed");
    assert_eq!(engine.invocation_count("conformance.side_effect"), 1);
    assert_eq!(engine.side_effect_count("conformance.side_effect"), 1);
}

#[test]
fn runtime_admission_prevents_dispatch_before_serving() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.echo",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Created)
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.echo"), 0);
}

#[test]
fn shutdown_is_delegated_to_real_runtime_and_is_terminal() {
    let engine = new_engine();
    engine.serving().unwrap();

    assert!(engine.shutdown().unwrap());
    assert_eq!(engine.state(), LifecycleState::Stopped);
    assert!(engine.shutdown().unwrap());
}

#[test]
fn separate_reference_engines_keep_handler_state_independent() {
    let first = new_engine();
    let second = ReferenceEngine::new(
        EngineId::new("phase16-engine-b").unwrap(),
        EngineInstanceId::new("phase16-instance-b").unwrap(),
    );

    first
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    second
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    first.serving().unwrap();
    second.serving().unwrap();

    first
        .dispatch(
            &test_context(),
            "conformance.echo",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(first.invocation_count("conformance.echo"), 1);
    assert_eq!(second.invocation_count("conformance.echo"), 0);
}

#[test]
fn dispatched_context_is_observable_at_the_real_handler_boundary() {
    let engine = new_engine();
    engine
        .register_capability("conformance.context", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let context = test_context()
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::from_secs(1)).unwrap());

    engine
        .dispatch(
            &context,
            "conformance.context",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    let observed = engine.last_context("conformance.context").unwrap();
    assert!(observed.deadline().is_some());
    assert!(!observed.is_expired());
}

#[test]
fn reference_stream_uses_real_stream_api_and_operation_ownership() {
    let engine = new_engine();
    let context = test_context();

    let stream = engine
        .open_stream::<Vec<u8>>(&context, 2, BackpressurePolicy::Reject)
        .unwrap();

    assert_eq!(stream.capacity(), 2);
    assert_eq!(stream.backpressure_policy(), BackpressurePolicy::Reject);
    assert!(!stream.has_observable_output());
    assert!(!stream.owner().operation_id().as_str().is_empty());

    stream.open().unwrap();
    assert_eq!(
        stream.state(),
        nizaam_core::streaming::StreamLifecycleState::Open
    );
}

#[test]
fn fail_once_state_can_be_explicitly_reset() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail_once", ReferenceBehavior::FailOnce)
        .unwrap();
    engine.serving().unwrap();

    let _ = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );
    engine.reset_fail_once("conformance.fail_once");

    let result = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));
}
