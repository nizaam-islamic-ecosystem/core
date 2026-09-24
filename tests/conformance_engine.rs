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
use nizaam_core::streaming::{BackpressurePolicy, StreamItem};

#[path = "common/reference_engine.rs"]
pub mod reference_engine;

use reference_engine::{ReferenceBehavior, ReferenceDispatchError, ReferenceEngine};

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
            ReferenceBehavior::Delay(Duration::from_millis(500)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context().with_deadline(
        nizaam_core::runtime::Deadline::from_now(Duration::from_millis(100)).unwrap(),
    );

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

#[test]
fn reference_engine_exercises_all_behaviors_and_boundaries() {
    fn context(id: &str) -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new(id).expect("test operation id must be valid"),
            CorrelationId::new(format!("{id}-correlation"))
                .expect("test correlation id must be valid"),
        )))
    }

    let engine = ReferenceEngine::new(
        EngineId::new("reference-test-engine").expect("test engine id must be valid"),
        EngineInstanceId::new("reference-test-instance").expect("test instance id must be valid"),
    );

    assert_eq!(engine.engine_id().as_str(), "reference-test-engine");
    assert_eq!(engine.instance_id().as_str(), "reference-test-instance");
    assert_eq!(engine.state(), LifecycleState::Created);
    assert_eq!(engine.runtime().state(), LifecycleState::Created);
    assert!(!engine.has_capability("missing.capability"));
    assert!(
        !engine.registry().contains(
            &nizaam_core::identity::CapabilityId::new("missing.capability")
                .expect("test capability id must be valid")
        )
    );

    engine
        .register_capability("reference.echo", ReferenceBehavior::Echo)
        .unwrap();
    engine
        .register_capability("reference.fail", ReferenceBehavior::Fail)
        .unwrap();
    engine
        .register_capability("reference.fail_once", ReferenceBehavior::FailOnce)
        .unwrap();
    engine
        .register_capability("reference.delay", ReferenceBehavior::Delay(Duration::ZERO))
        .unwrap();
    engine
        .register_capability(
            "reference.large",
            ReferenceBehavior::LargePayload(b"large-output".to_vec()),
        )
        .unwrap();
    engine
        .register_capability(
            "reference.side_effect",
            ReferenceBehavior::SideEffect(b"side-effect-output".to_vec()),
        )
        .unwrap();

    assert!(engine.has_capability("reference.echo"));
    assert!(engine.has_capability("reference.fail"));
    assert!(engine.has_capability("reference.fail_once"));
    assert!(engine.has_capability("reference.delay"));
    assert!(engine.has_capability("reference.large"));
    assert!(engine.has_capability("reference.side_effect"));

    engine.serving().unwrap();
    assert_eq!(engine.state(), LifecycleState::Serving);
    assert_eq!(engine.runtime().state(), LifecycleState::Serving);

    let echo_context = context("reference-echo");
    let echo = engine
        .dispatch(
            &echo_context,
            "reference.echo",
            "reference.contract",
            b"echo-payload",
        )
        .unwrap();
    assert_eq!(echo.as_bytes(), b"echo-payload");
    assert_eq!(engine.invocation_count("reference.echo"), 1);
    assert_eq!(
        engine
            .last_context("reference.echo")
            .expect("echo context must be recorded")
            .operation()
            .operation
            .id
            .as_str(),
        "reference-echo"
    );

    let fail = engine.dispatch(
        &context("reference-fail"),
        "reference.fail",
        "reference.contract",
        b"fail-payload",
    );
    match fail {
        Err(ReferenceDispatchError::Capability(CapabilityError::HandlerFailed(message))) => {
            assert_eq!(message, "reference engine failure");
        }
        other => panic!("expected reference capability failure, got {other:?}"),
    }
    assert_eq!(engine.invocation_count("reference.fail"), 1);

    let first_fail_once = engine.dispatch(
        &context("reference-fail-once"),
        "reference.fail_once",
        "reference.contract",
        b"first",
    );
    match first_fail_once {
        Err(ReferenceDispatchError::Capability(CapabilityError::HandlerFailed(message))) => {
            assert_eq!(message, "reference engine fail-once");
        }
        other => panic!("expected first fail-once error, got {other:?}"),
    }

    let second_fail_once = engine
        .dispatch(
            &context("reference-fail-once"),
            "reference.fail_once",
            "reference.contract",
            b"second",
        )
        .unwrap();
    assert_eq!(second_fail_once.as_bytes(), b"second");
    assert_eq!(engine.invocation_count("reference.fail_once"), 2);

    engine.reset_fail_once("reference.fail_once");
    let reset_fail_once = engine.dispatch(
        &context("reference-fail-once"),
        "reference.fail_once",
        "reference.contract",
        b"after-reset",
    );
    assert!(matches!(
        reset_fail_once,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));

    let delayed = engine
        .dispatch(
            &context("reference-delay"),
            "reference.delay",
            "reference.contract",
            b"delay-payload",
        )
        .unwrap();
    assert_eq!(delayed.as_bytes(), b"delay-payload");

    let large = engine
        .dispatch(
            &context("reference-large"),
            "reference.large",
            "reference.contract",
            b"ignored-input",
        )
        .unwrap();
    assert_eq!(large.as_bytes(), b"large-output");

    let side_effect = engine
        .dispatch(
            &context("reference-side-effect"),
            "reference.side_effect",
            "reference.contract",
            b"ignored-input",
        )
        .unwrap();
    assert_eq!(side_effect.as_bytes(), b"side-effect-output");
    assert_eq!(engine.side_effect_count("reference.side_effect"), 1);

    let stream_context = context("reference-stream");
    let stream = engine
        .open_stream::<u32>(&stream_context, 2, BackpressurePolicy::Reject)
        .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();

    stream.publish(StreamItem::partial(0, 1)).unwrap();
    stream.publish(StreamItem::final_item(1, 1)).unwrap();
    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 0);
    assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 1);

    assert!(engine.shutdown().unwrap());
    assert_eq!(engine.state(), LifecycleState::Stopped);

    let admission = engine.dispatch(
        &context("reference-after-shutdown"),
        "reference.echo",
        "reference.contract",
        b"rejected",
    );
    match admission {
        Err(ReferenceDispatchError::Admission(error)) => {
            let _ = format!("{error:?}");
        }
        other => panic!("expected admission rejection after shutdown, got {other:?}"),
    }
}
