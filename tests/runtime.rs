//! Integration tests for Phase 8: engine runtime execution infrastructure.
//!
//! These tests exercise the public runtime surface together with the already
//! implemented contract, operation, capability, and error foundations.
//!
//! The tests intentionally compose the primitives the way an engine would:
//! a universal request is structurally validated, its operation context is
//! associated with an EngineContext, cancellation/deadline state is honored,
//! the capability registry resolves the requested capability, and dispatch
//! invokes the engine-owned handler.
//!
//! These tests do not test lifecycle transition rules in isolation. That
//! coverage belongs to `tests/lifecycle.rs`.

use std::sync::{
    Arc, Barrier, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityError, CapabilityInvocation, CapabilityOutcome,
    CapabilityRegistry, arc_handler, dispatch,
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse,
    validation::validate_request,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::{ProvenanceContext, SecurityContext, Status, Version};
use nizaam_core::runtime::{
    BackgroundTasks, Deadline, EngineContext, EngineRuntime, ExecutionPipeline, LifecycleState,
    TaskScope,
};

// ---------------------------------------------------------------------------
// Shared construction helpers
// ---------------------------------------------------------------------------

const ENGINE: &str = "runtime-test-engine";
const CAPABILITY: &str = "runtime.lookup";

fn operation_context(operation_id: &str, correlation_id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(operation_id).unwrap(),
        CorrelationId::new(correlation_id).unwrap(),
    ))
}

fn request(
    message_id: &str,
    capability: &str,
    payload: &[u8],
    operation_context: OperationContext,
) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new(format!("{capability}.contract")).unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("caller-engine").unwrap(),
            EngineId::new(ENGINE).unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message_id).unwrap(),
        operation_context,
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new();

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

    runtime
}

fn invocation_from_request(request: &UniversalRequest) -> CapabilityInvocation {
    CapabilityInvocation::new(
        request.envelope.metadata.descriptor.capability_id.clone(),
        request.envelope.metadata.descriptor.contract_id.clone(),
        request.envelope.payload.bytes().to_vec(),
    )
}

fn registry_with_handler(
    handler: Arc<dyn nizaam_core::capability::CapabilityHandler>,
) -> (CapabilityRegistry, CapabilityId) {
    let registry = CapabilityRegistry::new();
    let capability_id = CapabilityId::new(CAPABILITY).unwrap();

    let definition = CapabilityDefinition::new(
        capability_id.clone(),
        EngineId::new(ENGINE).unwrap(),
        "Runtime Lookup",
    )
    .unwrap();

    registry.register(definition, handler).unwrap();

    (registry, capability_id)
}

// ---------------------------------------------------------------------------
// Runtime + contract + context + capability integration
// ---------------------------------------------------------------------------

#[test]
fn serving_runtime_can_execute_a_validated_universal_request() {
    let runtime = serving_runtime();
    assert_eq!(runtime.state(), LifecycleState::Serving);

    let observed_operation = Arc::new(Mutex::new(None));
    let observed_operation_by_handler = Arc::clone(&observed_operation);

    let handler = arc_handler(
        move |context: &EngineContext, invocation: &CapabilityInvocation| {
            *observed_operation_by_handler.lock().unwrap() =
                Some(context.operation().operation.id.clone());

            Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
        },
    );

    let (registry, _capability_id) = registry_with_handler(handler);

    let operation = operation_context("runtime-op-1", "runtime-corr-1");
    let request = request(
        "runtime-msg-1",
        CAPABILITY,
        b"runtime payload",
        operation.clone(),
    );

    // Contract validation is a distinct step before capability dispatch.
    assert!(request.has_request_interaction());
    validate_request(&request).expect("the request must pass structural validation");

    // Runtime execution context is associated with the trusted operation
    // context carried by the universal request.
    let context = EngineContext::new(request.envelope.operation_context.clone());

    let invocation = invocation_from_request(&request);

    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_ok());
    assert_eq!(
        result.into_outcome().unwrap().into_bytes(),
        b"runtime payload"
    );
    assert_eq!(
        observed_operation.lock().unwrap().as_ref(),
        Some(&operation.operation.id)
    );

    runtime.shutdown().unwrap();
    assert_eq!(runtime.state(), LifecycleState::Stopped);
}

#[test]
fn runtime_dispatch_uses_request_capability_for_handler_selection() {
    let runtime = serving_runtime();

    let requested_capability = "runtime.requested";
    let registered_capability = "runtime.registered";

    let requested_handler_called = Arc::new(AtomicBool::new(false));
    let registered_handler_called = Arc::new(AtomicBool::new(false));

    let requested_called = Arc::clone(&requested_handler_called);
    let registered_called = Arc::clone(&registered_handler_called);

    let registry = CapabilityRegistry::new();

    let requested_definition = CapabilityDefinition::new(
        CapabilityId::new(requested_capability).unwrap(),
        EngineId::new(ENGINE).unwrap(),
        "Requested Capability",
    )
    .unwrap();

    registry
        .register(
            requested_definition,
            arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
                requested_called.store(true, Ordering::SeqCst);
                Ok(CapabilityOutcome::new(b"requested".to_vec()))
            }),
        )
        .unwrap();

    let registered_definition = CapabilityDefinition::new(
        CapabilityId::new(registered_capability).unwrap(),
        EngineId::new(ENGINE).unwrap(),
        "Registered Capability",
    )
    .unwrap();

    registry
        .register(
            registered_definition,
            arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
                registered_called.store(true, Ordering::SeqCst);
                Ok(CapabilityOutcome::new(b"registered".to_vec()))
            }),
        )
        .unwrap();

    let request = request(
        "runtime-routing-msg",
        requested_capability,
        b"routing payload",
        operation_context("runtime-routing-op", "runtime-routing-corr"),
    );
    let context = EngineContext::new(request.envelope.operation_context.clone());
    let invocation = invocation_from_request(&request);

    let result = dispatch(&registry, &context, &invocation);

    assert_eq!(
        result.into_outcome().unwrap().into_bytes(),
        b"requested"
    );
    assert!(requested_handler_called.load(Ordering::SeqCst));
    assert!(!registered_handler_called.load(Ordering::SeqCst));

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_context_preserves_operation_security_and_provenance_during_dispatch() {
    let runtime = serving_runtime();

    let provenance = ProvenanceContext::new()
        .with_attribute("source", "runtime-integration")
        .with_attribute("component", "test-engine");

    let parent = EngineContext::new(operation_context(
        "runtime-context-op",
        "runtime-context-corr",
    ))
    .with_security(SecurityContext::new())
    .with_provenance(provenance.clone())
    .with_deadline(Deadline::from_now(Duration::from_secs(5)).unwrap());

    let child = parent.child();

    let observed = Arc::new(Mutex::new(None));
    let observed_by_handler = Arc::clone(&observed);

    let handler = arc_handler(
        move |context: &EngineContext, _invocation: &CapabilityInvocation| {
            *observed_by_handler.lock().unwrap() = Some((
                context.operation().operation.id.clone(),
                context.provenance().attribute("source").map(str::to_owned),
                context
                    .provenance()
                    .attribute("component")
                    .map(str::to_owned),
            ));

            Ok(CapabilityOutcome::new(b"context-ok".to_vec()))
        },
    );

    let (registry, capability_id) = registry_with_handler(handler);
    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"payload".to_vec(),
    );

    let result = dispatch(&registry, &child, &invocation);

    assert!(result.is_ok());
    assert_eq!(
        observed.lock().unwrap().as_ref(),
        Some(&(
            OperationId::new("runtime-context-op").unwrap(),
            Some("runtime-integration".to_owned()),
            Some("test-engine".to_owned()),
        ))
    );

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_dispatch_honors_cancellation_before_capability_resolution() {
    let runtime = serving_runtime();

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_handler = Arc::clone(&handler_called);

    let handler = arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
        handler_called_by_handler.store(true, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(b"must-not-run".to_vec()))
    });

    let (registry, capability_id) = registry_with_handler(handler);

    let context = EngineContext::new(operation_context(
        "runtime-cancel-op",
        "runtime-cancel-corr",
    ));
    context.cancellation().cancel();

    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"payload".to_vec(),
    );

    let result = dispatch(&registry, &context, &invocation);

    assert!(matches!(
        result.as_error(),
        Some(CapabilityError::Cancelled)
    ));
    assert!(
        !handler_called.load(Ordering::SeqCst),
        "cancelled work must not reach the engine handler"
    );

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_dispatch_honors_deadline_before_capability_resolution() {
    let runtime = serving_runtime();

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_handler = Arc::clone(&handler_called);

    let handler = arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
        handler_called_by_handler.store(true, Ordering::SeqCst);
        Ok(CapabilityOutcome::new(b"must-not-run".to_vec()))
    });

    let (registry, capability_id) = registry_with_handler(handler);

    let context = EngineContext::new(operation_context(
        "runtime-deadline-op",
        "runtime-deadline-corr",
    ))
    .with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"payload".to_vec(),
    );

    let result = dispatch(&registry, &context, &invocation);

    assert!(matches!(
        result.as_error(),
        Some(CapabilityError::DeadlineExpired)
    ));
    assert!(
        !handler_called.load(Ordering::SeqCst),
        "expired work must not reach the engine handler"
    );

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_pipeline_runs_execution_stages_with_the_same_engine_context() {
    let runtime = serving_runtime();

    let observed = Arc::new(Mutex::new(Vec::<String>::new()));
    let first_observed = Arc::clone(&observed);
    let second_observed = Arc::clone(&observed);

    let pipeline = ExecutionPipeline::new()
        .with_stage(Box::new(move |context: &EngineContext| {
            first_observed
                .lock()
                .unwrap()
                .push(context.operation().operation.id.to_string());
            Ok(())
        }))
        .with_stage(Box::new(move |context: &EngineContext| {
            second_observed.lock().unwrap().push(format!(
                "{}:{}",
                context.operation().operation.id,
                context.provenance().attribute("stage").unwrap_or("missing")
            ));
            Ok(())
        }));

    let context = EngineContext::new(operation_context(
        "runtime-pipeline-op",
        "runtime-pipeline-corr",
    ))
    .with_provenance(ProvenanceContext::new().with_attribute("stage", "second"));

    pipeline.run(&context).unwrap();

    assert_eq!(
        observed.lock().unwrap().as_slice(),
        ["runtime-pipeline-op", "runtime-pipeline-op:second"]
    );

    runtime.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// Runtime concurrency and task scopes
// ---------------------------------------------------------------------------

#[test]
fn independent_runtime_task_scopes_execute_concurrently_without_shared_cancellation() {
    let runtime = serving_runtime();

    let barrier = Arc::new(Barrier::new(3));
    let completed = Arc::new(AtomicUsize::new(0));

    let scope_a = TaskScope::new(runtime.shutdown_token());
    let scope_b = TaskScope::new(runtime.shutdown_token());

    let barrier_a = Arc::clone(&barrier);
    let completed_a = Arc::clone(&completed);
    let token_a = scope_a.cancellation().clone();

    let barrier_b = Arc::clone(&barrier);
    let completed_b = Arc::clone(&completed);
    let token_b = scope_b.cancellation().clone();

    let handle_a = thread::spawn(move || {
        barrier_a.wait();
        assert!(!token_a.is_cancelled());
        completed_a.fetch_add(1, Ordering::SeqCst);
    });

    let handle_b = thread::spawn(move || {
        barrier_b.wait();
        assert!(!token_b.is_cancelled());
        completed_b.fetch_add(1, Ordering::SeqCst);
    });

    // Both worker threads and this test thread must reach the barrier before
    // either worker is allowed to continue.
    barrier.wait();

    handle_a.join().unwrap();
    handle_b.join().unwrap();

    assert_eq!(completed.load(Ordering::SeqCst), 2);

    // Cancelling one task scope must not cancel its sibling.
    scope_a.cancel();
    assert!(scope_a.is_cancelled());
    assert!(!scope_b.is_cancelled());
    assert!(!runtime.shutdown_token().is_cancelled());

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_shutdown_cancels_and_joins_owned_background_work() {
    let runtime = Arc::new(serving_runtime());

    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));

    let started_by_task = Arc::clone(&started);
    let finished_by_task = Arc::clone(&finished);

    runtime
        .background_tasks()
        .spawn(move |cancellation| {
            started_by_task.store(true, Ordering::SeqCst);

            while !cancellation.is_cancelled() {
                thread::yield_now();
            }

            finished_by_task.store(true, Ordering::SeqCst);
        })
        .unwrap();

    while !started.load(Ordering::SeqCst) {
        thread::yield_now();
    }

    runtime.shutdown().unwrap();

    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.shutdown_token().is_cancelled());
    assert!(
        finished.load(Ordering::SeqCst),
        "runtime shutdown must not return before owned background work finishes"
    );
}

#[test]
fn runtime_background_tasks_share_runtime_shutdown_cancellation() {
    let runtime = Arc::new(serving_runtime());

    let task_cancelled = Arc::new(AtomicBool::new(false));
    let task_cancelled_by_task = Arc::clone(&task_cancelled);

    runtime
        .background_tasks()
        .spawn(move |cancellation| {
            while !cancellation.is_cancelled() {
                thread::yield_now();
            }

            task_cancelled_by_task.store(true, Ordering::SeqCst);
        })
        .unwrap();

    runtime.shutdown().unwrap();

    assert!(runtime.shutdown_token().is_cancelled());
    assert!(runtime.background_tasks().cancellation().is_cancelled());
    assert!(task_cancelled.load(Ordering::SeqCst));
}

// ---------------------------------------------------------------------------
// Background task primitive used independently by runtime consumers
// ---------------------------------------------------------------------------

#[test]
fn background_tasks_reject_new_work_after_shutdown() {
    let cancellation = nizaam_core::runtime::CancellationToken::new();
    let tasks = BackgroundTasks::new(cancellation);

    tasks.shutdown();

    assert!(
        tasks.spawn(|_| {}).is_err(),
        "closed background task ownership must reject late work"
    );
}

#[test]
fn runtime_shutdown_is_idempotent_after_background_work_has_stopped() {
    let runtime = serving_runtime();

    runtime
        .background_tasks()
        .spawn(|cancellation| {
            while !cancellation.is_cancelled() {
                thread::yield_now();
            }
        })
        .unwrap();

    runtime.shutdown().unwrap();
    runtime.shutdown().unwrap();

    assert_eq!(runtime.state(), LifecycleState::Stopped);
    assert!(runtime.shutdown_token().is_cancelled());
}

// ---------------------------------------------------------------------------
// Universal response construction at the runtime boundary
// ---------------------------------------------------------------------------

#[test]
fn dispatched_capability_outcome_can_become_a_structurally_valid_universal_response() {
    let runtime = serving_runtime();

    let handler = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
        Ok(CapabilityOutcome::new(b"runtime response".to_vec()))
    });

    let (registry, capability_id) = registry_with_handler(handler);

    let request = request(
        "runtime-response-msg",
        CAPABILITY,
        b"runtime request",
        operation_context("runtime-response-op", "runtime-response-corr"),
    );

    validate_request(&request).unwrap();

    let context = EngineContext::new(request.envelope.operation_context.clone());
    let invocation = CapabilityInvocation::new(
        capability_id,
        request.envelope.metadata.descriptor.contract_id.clone(),
        request.envelope.payload.bytes().to_vec(),
    );

    let outcome = dispatch(&registry, &context, &invocation)
        .into_outcome()
        .expect("capability must produce an outcome");

    let mut envelope = request.envelope;
    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        outcome.into_bytes(),
    );

    let response = UniversalResponse::new(envelope, Status::Success);

    assert!(response.has_response_interaction());
    assert_eq!(response.status, Status::Success);
    assert_eq!(response.envelope.payload.bytes(), b"runtime response");

    runtime.shutdown().unwrap();
}
