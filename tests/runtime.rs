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
use nizaam_core::config::{
    resolution::ConfigurationResolver,
    snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
    validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
};
use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse,
    validation::validate_request,
};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventPublisher, EventSubscription,
    Scope,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::{ProvenanceContext, Status, Version};
use nizaam_core::retry::{Attempt, AttemptLifecycleState};
use nizaam_core::runtime::pipeline::RequestPipelineError;
use nizaam_core::runtime::{
    BackgroundTasks, Deadline, EngineContext, EngineRuntime, ExecutionPipeline, LifecycleState,
    TaskScope,
};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
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

fn configuration_snapshot() -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::empty();
    parsed.insert(
        "runtime.retry.mode",
        ConfigurationValue::String("deterministic".to_owned()),
    );

    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();

    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(13),
        resolved,
    ))
}

fn serving_runtime() -> EngineRuntime {
    let runtime = EngineRuntime::new(
        nizaam_core::identity::EngineId::new(ENGINE).unwrap(),
        nizaam_core::identity::EngineInstanceId::new("runtime-test-instance").unwrap(),
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

    runtime
}

fn invocation_from_request(request: &UniversalRequest) -> CapabilityInvocation {
    CapabilityInvocation::new(
        request
            .event
            .envelope
            .metadata
            .descriptor
            .capability_id
            .clone(),
        request
            .event
            .envelope
            .metadata
            .descriptor
            .contract_id
            .clone(),
        request.event.envelope.payload.bytes().to_vec(),
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

    let observed_context = Arc::new(Mutex::new(None));
    let observed_context_by_handler = Arc::clone(&observed_context);

    let handler = arc_handler(
        move |context: &EngineContext, invocation: &CapabilityInvocation| {
            *observed_context_by_handler.lock().unwrap() = Some((
                context.operation().operation.id.clone(),
                context.operation().attempt_id.clone(),
                context.operation().node_id.clone(),
            ));

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
    let base_context = EngineContext::new(request.event.envelope.operation_context.clone());

    let attempt = Attempt::new(
        operation.operation.id.clone(),
        nizaam_core::identity::AttemptId::new("runtime-attempt-1").unwrap(),
        1,
    )
    .unwrap();

    let context = base_context.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-node-1").unwrap(),
        attempt.attempt_id().clone(),
    );

    let invocation = invocation_from_request(&request);

    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_ok());
    assert_eq!(
        result.into_outcome().unwrap().into_bytes(),
        b"runtime payload"
    );

    assert_eq!(
        observed_context.lock().unwrap().as_ref(),
        Some(&(
            operation.operation.id.clone(),
            Some(attempt.attempt_id().clone()),
            Some(nizaam_core::identity::NodeId::new("runtime-node-1").unwrap()),
        ))
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

    let context = EngineContext::new(request.event.envelope.operation_context.clone()).for_attempt(
        nizaam_core::identity::NodeId::new("runtime-routing-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-routing-attempt").unwrap(),
    );

    let invocation = invocation_from_request(&request);

    let result = dispatch(&registry, &context, &invocation);

    assert_eq!(result.into_outcome().unwrap().into_bytes(), b"requested");
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
    .with_security(SecurityContext::new(
        PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("runtime-test-user").unwrap(),
        ),
        None,
    ))
    .with_provenance(provenance.clone())
    .with_deadline(Deadline::from_now(Duration::from_secs(5)).unwrap())
    .with_configuration(configuration_snapshot());

    let attempt_id = nizaam_core::identity::AttemptId::new("runtime-context-attempt").unwrap();

    let attempt_context = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-context-node").unwrap(),
        attempt_id.clone(),
    );

    let observed = Arc::new(Mutex::new(None));
    let observed_by_handler = Arc::clone(&observed);

    let handler = arc_handler(
        move |context: &EngineContext, _invocation: &CapabilityInvocation| {
            *observed_by_handler.lock().unwrap() = Some((
                context.operation().operation.id.clone(),
                context.operation().attempt_id.clone(),
                context.operation().node_id.clone(),
                context.provenance().attribute("source").map(str::to_owned),
                context
                    .provenance()
                    .attribute("component")
                    .map(str::to_owned),
                context.security().cloned(),
                context.deadline(),
                context.configuration().map(|snapshot| snapshot.id()),
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

    let result = dispatch(&registry, &attempt_context, &invocation);

    assert!(result.is_ok());
    assert_eq!(
        observed.lock().unwrap().as_ref(),
        Some(&(
            OperationId::new("runtime-context-op").unwrap(),
            Some(attempt_id),
            Some(nizaam_core::identity::NodeId::new("runtime-context-node").unwrap()),
            Some("runtime-integration".to_owned()),
            Some("test-engine".to_owned()),
            Some(parent.security().unwrap().clone()),
            parent.deadline(),
            Some(ConfigurationSnapshotId::new(13)),
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

    let base_context = EngineContext::new(operation_context(
        "runtime-cancel-op",
        "runtime-cancel-corr",
    ));

    let context = base_context.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-cancel-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-cancel-attempt").unwrap(),
    );

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

    let base_context = EngineContext::new(operation_context(
        "runtime-deadline-op",
        "runtime-deadline-corr",
    ))
    .with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

    let context = base_context.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-deadline-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-deadline-attempt").unwrap(),
    );

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
            first_observed.lock().unwrap().push(format!(
                "{}:{}",
                context.operation().operation.id,
                context.operation().attempt_id.as_ref().unwrap()
            ));
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
    .with_provenance(ProvenanceContext::new().with_attribute("stage", "second"))
    .for_attempt(
        nizaam_core::identity::NodeId::new("runtime-pipeline-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-pipeline-attempt").unwrap(),
    );

    pipeline.run(&context).unwrap();

    assert_eq!(
        observed.lock().unwrap().as_slice(),
        [
            "runtime-pipeline-op:runtime-pipeline-attempt",
            "runtime-pipeline-op:second",
        ]
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

    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));

    let finished_by_task = Arc::clone(&finished);

    runtime
        .background_tasks()
        .spawn(move |cancellation| {
            started_sender
                .send(())
                .expect("test thread must still be waiting for task start");

            while !cancellation.is_cancelled() {
                thread::yield_now();
            }

            finished_by_task.store(true, Ordering::SeqCst);
        })
        .unwrap();

    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("owned background task must start before runtime shutdown is tested");

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
// Runtime + Event lifecycle/cancellation integration
// ---------------------------------------------------------------------------

#[test]
fn runtime_shutdown_closes_event_publisher_owned_by_runtime() {
    let runtime = serving_runtime();
    let publisher = EventPublisher::new(runtime.event_lifecycle(), runtime.shutdown_token());

    publisher.activate().unwrap();
    publisher
        .publish(
            Event::new(
                nizaam_core::identity::EventId::new("runtime-owned-before-shutdown").unwrap(),
                nizaam_core::events::EventName::new("runtime.event").unwrap(),
                "runtime.event",
                Scope::new("runtime:test").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    runtime.transition(LifecycleState::Draining).unwrap();

    assert!(matches!(
        publisher.publish(
            Event::new(
                nizaam_core::identity::EventId::new("runtime-owned-during-draining").unwrap(),
                nizaam_core::events::EventName::new("runtime.event").unwrap(),
                "runtime.event",
                Scope::new("runtime:test").unwrap(),
            )
            .unwrap(),
        ),
        Err(nizaam_core::events::PublisherError::EventSubsystemUnavailable)
    ));
    assert!(publisher.is_active());

    runtime.shutdown().unwrap();

    assert!(matches!(
        publisher.publish(
            Event::new(
                nizaam_core::identity::EventId::new("runtime-owned-after-shutdown").unwrap(),
                nizaam_core::events::EventName::new("runtime.event").unwrap(),
                "runtime.event",
                Scope::new("runtime:test").unwrap(),
            )
            .unwrap(),
        ),
        Err(nizaam_core::events::PublisherError::Closed)
    ));
    assert!(publisher.is_closed());
}

#[test]
fn runtime_shutdown_cancels_runtime_owned_event_subscription() {
    let runtime = serving_runtime();
    let subscription = EventSubscription::new(
        nizaam_core::events::EventName::new("runtime.event").unwrap(),
        "runtime.event",
        Scope::new("runtime:test").unwrap(),
        |_event: &Event| {},
        runtime.shutdown_token(),
    )
    .unwrap();

    subscription.activate().unwrap();
    assert!(subscription.is_active());

    runtime.shutdown().unwrap();

    assert!(subscription.is_cancelled());
}

#[test]
fn runtime_shutdown_cancels_runtime_owned_event_delivery() {
    let runtime = serving_runtime();
    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let (finished_sender, finished_receiver) = std::sync::mpsc::channel();
    let runtime_cancellation = runtime.shutdown_token().clone();

    let handler_cancellation = runtime_cancellation.clone();

    let subscription = Arc::new(
        EventSubscription::new(
            nizaam_core::events::EventName::new("runtime.event").unwrap(),
            "runtime.event",
            Scope::new("runtime:test").unwrap(),
            move |_event: &Event| {
                started_sender
                    .send(())
                    .expect("test thread must still be waiting for handler start");

                while !handler_cancellation.is_cancelled() {
                    thread::yield_now();
                }

                finished_sender
                    .send(())
                    .expect("test thread must still be waiting for handler completion");
            },
            runtime.shutdown_token(),
        )
        .unwrap(),
    );
    subscription.activate().unwrap();

    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(4, 1, 1).unwrap(), runtime_cancellation)
            .unwrap();

    let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

    assert_eq!(
        handle
            .enqueue(Arc::new(
                Event::new(
                    nizaam_core::identity::EventId::new("runtime-delivery-event").unwrap(),
                    nizaam_core::events::EventName::new("runtime.event").unwrap(),
                    "runtime.event",
                    Scope::new("runtime:test").unwrap(),
                )
                .unwrap(),
            ))
            .unwrap(),
        DeliveryOutcome::Accepted
    );

    started_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("Event delivery handler must start before runtime shutdown is tested");

    runtime.shutdown().unwrap();

    finished_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("runtime shutdown cancellation must reach an active Event delivery handler");

    assert!(subscription.is_cancelled());

    dispatcher.shutdown();
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

    let context = EngineContext::new(request.event.envelope.operation_context.clone()).for_attempt(
        nizaam_core::identity::NodeId::new("runtime-response-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-response-attempt").unwrap(),
    );

    let invocation = CapabilityInvocation::new(
        capability_id,
        request
            .event
            .envelope
            .metadata
            .descriptor
            .contract_id
            .clone(),
        request.event.envelope.payload.bytes().to_vec(),
    );

    let outcome = dispatch(&registry, &context, &invocation)
        .into_outcome()
        .expect("capability must produce an outcome");

    let mut envelope = request.event.envelope;
    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        outcome.into_bytes(),
    );

    let response = UniversalResponse::new(envelope, Status::Success);

    assert!(response.has_response_interaction());
    assert_eq!(response.status, Status::Success);
    assert_eq!(response.event.envelope.payload.bytes(), b"runtime response");

    runtime.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// Phase 13: retry-aware runtime integration
// ---------------------------------------------------------------------------

#[test]
fn runtime_dispatch_can_run_sequential_attempts_for_one_operation() {
    let runtime = serving_runtime();

    let invocation = CapabilityInvocation::new(
        CapabilityId::new(CAPABILITY).unwrap(),
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"retry payload".to_vec(),
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let calls_by_handler = Arc::clone(&calls);
    let attempts_by_handler = Arc::clone(&observed_attempts);

    let handler = arc_handler(move |context: &EngineContext, _: &CapabilityInvocation| {
        let call = calls_by_handler.fetch_add(1, Ordering::SeqCst);

        attempts_by_handler
            .lock()
            .unwrap()
            .push(context.operation().attempt_id.clone());

        if call == 0 {
            Err(CapabilityError::HandlerFailed(
                "transient failure".to_owned(),
            ))
        } else {
            Ok(CapabilityOutcome::new(b"retry success".to_vec()))
        }
    });

    let (registry, _) = registry_with_handler(handler);

    let engine = EngineContext::new(operation_context("runtime-retry-op", "runtime-retry-corr"));

    let first = Attempt::new(
        OperationId::new("runtime-retry-op").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-attempt-1").unwrap(),
        1,
    )
    .unwrap();

    first.start().unwrap();

    let first_context = engine.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        first.attempt_id().clone(),
    );

    let first_result = dispatch(&registry, &first_context, &invocation);

    assert!(matches!(
        first_result.as_error(),
        Some(CapabilityError::HandlerFailed(_))
    ));

    first.fail().unwrap();

    let second = Attempt::new(
        OperationId::new("runtime-retry-op").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-attempt-2").unwrap(),
        2,
    )
    .unwrap();

    second.start().unwrap();

    let second_context = engine.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        second.attempt_id().clone(),
    );

    let second_result = dispatch(&registry, &second_context, &invocation);

    assert_eq!(
        second_result.into_outcome().unwrap().into_bytes(),
        b"retry success"
    );

    second.succeed().unwrap();

    assert_eq!(first.state(), AttemptLifecycleState::Failed);
    assert_eq!(second.state(), AttemptLifecycleState::Succeeded);
    assert_eq!(first.operation_id(), second.operation_id());
    assert_ne!(first.attempt_id(), second.attempt_id());
    assert_eq!(first.attempt_number(), 1);
    assert_eq!(second.attempt_number(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let observed = observed_attempts.lock().unwrap();

    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0].as_ref(), Some(first.attempt_id()));
    assert_eq!(observed[1].as_ref(), Some(second.attempt_id()));

    runtime.shutdown().unwrap();
}

#[test]
fn retry_attempt_context_preserves_runtime_security_provenance_deadline_and_configuration() {
    let provenance = ProvenanceContext::new()
        .with_attribute("source", "runtime-retry")
        .with_attribute("component", "runtime-test");

    let security = SecurityContext::new(
        user_principal("runtime-retry-user"),
        Some(service_principal("runtime-retry-service")),
    );

    let configuration = configuration_snapshot();

    let parent = EngineContext::new(operation_context(
        "runtime-retry-context-op",
        "runtime-retry-context-corr",
    ))
    .with_security(security.clone())
    .with_provenance(provenance.clone())
    .with_deadline(Deadline::from_now(Duration::from_secs(5)).unwrap())
    .with_configuration(Arc::clone(&configuration));

    let first = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-context-attempt-1").unwrap(),
    );

    let second = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-context-attempt-2").unwrap(),
    );

    assert_eq!(
        first.operation().operation.id,
        second.operation().operation.id
    );
    assert_ne!(first.operation().attempt_id, second.operation().attempt_id);

    assert_eq!(first.security(), Some(&security));
    assert_eq!(second.security(), Some(&security));

    assert_eq!(first.provenance(), &provenance);
    assert_eq!(second.provenance(), &provenance);

    assert_eq!(first.deadline(), parent.deadline());
    assert_eq!(second.deadline(), parent.deadline());

    assert_eq!(first.configuration(), Some(configuration.as_ref()));
    assert_eq!(second.configuration(), Some(configuration.as_ref()));
}

#[test]
fn retry_attempts_share_operation_cancellation_authority() {
    let parent = EngineContext::new(operation_context(
        "runtime-retry-cancel-op",
        "runtime-retry-cancel-corr",
    ));

    let first = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-cancel-attempt-1").unwrap(),
    );

    let second = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-retry-node").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-retry-cancel-attempt-2").unwrap(),
    );

    assert!(!first.cancellation().is_cancelled());
    assert!(!second.cancellation().is_cancelled());

    parent.cancellation().cancel();

    assert!(parent.cancellation().is_cancelled());
    assert!(first.cancellation().is_cancelled());
    assert!(second.cancellation().is_cancelled());
}

#[test]
fn retry_attempts_preserve_authenticated_delegation_during_dispatch() {
    let runtime = serving_runtime();

    let principal = user_principal("runtime-delegated-user");
    let calling_service = service_principal("runtime-delegated-service");

    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_by_handler = Arc::clone(&observed);

    let handler = arc_handler(move |context: &EngineContext, _: &CapabilityInvocation| {
        let security = context
            .security()
            .expect("security context must be present");

        observed_by_handler.lock().unwrap().push((
            context.operation().operation.id.clone(),
            context.operation().attempt_id.clone(),
            security.principal().clone(),
            security.calling_service().cloned(),
        ));

        Ok(CapabilityOutcome::new(b"delegated".to_vec()))
    });

    let (registry, capability_id) = registry_with_handler(handler);

    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"delegated payload".to_vec(),
    );

    let parent = EngineContext::new(operation_context(
        "runtime-delegated-op",
        "runtime-delegated-corr",
    ))
    .with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));

    for attempt_name in ["runtime-delegated-attempt-1", "runtime-delegated-attempt-2"] {
        let context = parent.for_attempt(
            nizaam_core::identity::NodeId::new("runtime-delegated-node").unwrap(),
            nizaam_core::identity::AttemptId::new(attempt_name).unwrap(),
        );

        let result = dispatch(&registry, &context, &invocation);

        assert_eq!(result.into_outcome().unwrap().into_bytes(), b"delegated");
    }

    let observed = observed.lock().unwrap();

    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0].0.as_str(), "runtime-delegated-op");
    assert_eq!(observed[1].0.as_str(), "runtime-delegated-op");
    assert_ne!(observed[0].1, observed[1].1);

    assert_eq!(observed[0].2, principal);
    assert_eq!(observed[1].2, user_principal("runtime-delegated-user"));

    assert_eq!(observed[0].3, Some(calling_service.clone()));
    assert_eq!(observed[1].3, Some(calling_service));

    runtime.shutdown().unwrap();
}

#[test]
fn failed_runtime_attempt_can_be_followed_by_success_without_new_operation_identity() {
    let runtime = serving_runtime();

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_by_handler = Arc::clone(&calls);

    let handler = arc_handler(move |_: &EngineContext, _: &CapabilityInvocation| {
        if calls_by_handler.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(CapabilityError::HandlerFailed(
                "first attempt failed".to_owned(),
            ))
        } else {
            Ok(CapabilityOutcome::new(b"second attempt succeeded".to_vec()))
        }
    });

    let (registry, capability_id) = registry_with_handler(handler);

    let invocation = CapabilityInvocation::new(
        capability_id,
        ContractId::new("runtime.lookup.contract").unwrap(),
        b"lineage payload".to_vec(),
    );

    let parent = EngineContext::new(operation_context(
        "runtime-lineage-op",
        "runtime-lineage-corr",
    ));

    let first = Attempt::new(
        OperationId::new("runtime-lineage-op").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-lineage-attempt-1").unwrap(),
        1,
    )
    .unwrap();

    first.start().unwrap();

    let first_context = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-lineage-node").unwrap(),
        first.attempt_id().clone(),
    );

    assert!(matches!(
        dispatch(&registry, &first_context, &invocation).as_error(),
        Some(CapabilityError::HandlerFailed(_))
    ));

    first.fail().unwrap();

    let second = Attempt::new(
        OperationId::new("runtime-lineage-op").unwrap(),
        nizaam_core::identity::AttemptId::new("runtime-lineage-attempt-2").unwrap(),
        2,
    )
    .unwrap();

    second.start().unwrap();

    let second_context = parent.for_attempt(
        nizaam_core::identity::NodeId::new("runtime-lineage-node").unwrap(),
        second.attempt_id().clone(),
    );

    assert_eq!(
        dispatch(&registry, &second_context, &invocation)
            .into_outcome()
            .unwrap()
            .into_bytes(),
        b"second attempt succeeded"
    );

    second.succeed().unwrap();

    assert_eq!(first.operation_id(), second.operation_id());
    assert_eq!(first.attempt_number(), 1);
    assert_eq!(second.attempt_number(), 2);
    assert_eq!(first.state(), AttemptLifecycleState::Failed);
    assert_eq!(second.state(), AttemptLifecycleState::Succeeded);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    runtime.shutdown().unwrap();
}

// ---------------------------------------------------------------------------
// Phase 9: runtime request security boundary
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct TestCredentialExtractor {
    credentials: Option<Vec<u8>>,
}

impl CredentialExtractor for TestCredentialExtractor {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.credentials.clone()
    }
}

#[derive(Clone, Debug)]
struct TestAuthenticator {
    result: Result<PrincipalIdentity, AuthenticationError>,
}

impl Authenticator for TestAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.result.clone()
    }
}

#[derive(Clone, Debug)]
struct TestAuthorizer {
    result: Result<AuthorizationDecision, AuthorizationError>,
}

impl Authorizer for TestAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        self.result
    }
}

struct RecordingAuthorizer {
    expected_capability: CapabilityId,
    expected_principal: PrincipalIdentity,
    expected_calling_service: Option<PrincipalIdentity>,
    observed: Arc<Mutex<bool>>,
    result: AuthorizationDecision,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.capability(), &self.expected_capability);
        assert_eq!(request.principal(), &self.expected_principal);
        assert_eq!(
            request.calling_service(),
            self.expected_calling_service.as_ref(),
        );

        *self.observed.lock().unwrap() = true;

        Ok(self.result)
    }
}

fn response_for_request(request: &UniversalRequest, payload: &[u8]) -> UniversalResponse {
    let mut envelope = request.event.envelope.clone();

    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        payload.to_vec(),
    );

    UniversalResponse::new(envelope, Status::Success)
}

fn user_principal(id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(PrincipalType::User, PrincipalId::new(id).unwrap())
}

fn service_principal(id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new(id).unwrap())
}

#[test]
fn runtime_request_pipeline_authenticates_authorizes_and_dispatches() {
    let runtime = serving_runtime();

    let request = request(
        "runtime-security-happy-msg",
        CAPABILITY,
        b"secure payload",
        operation_context("runtime-security-happy-op", "runtime-security-happy-corr"),
    );

    let principal = user_principal("runtime-security-user");
    let calling_service = service_principal("runtime-security-service");

    let authorization_observed = Arc::new(Mutex::new(false));

    let authorizer = RecordingAuthorizer {
        expected_capability: CapabilityId::new(CAPABILITY).unwrap(),
        expected_principal: principal.clone(),
        expected_calling_service: Some(calling_service.clone()),
        observed: Arc::clone(&authorization_observed),
        result: AuthorizationDecision::Allow,
    };

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(principal.clone()),
        },
        authorizer,
        TestCredentialExtractor {
            credentials: Some(b"opaque credentials".to_vec()),
        },
    );

    let initial_context = EngineContext::new(request.event.envelope.operation_context.clone())
        .with_security(SecurityContext::new(
            service_principal("initial-caller-placeholder"),
            Some(calling_service.clone()),
        ))
        .for_attempt(
            nizaam_core::identity::NodeId::new("runtime-security-node").unwrap(),
            nizaam_core::identity::AttemptId::new("runtime-security-attempt").unwrap(),
        );

    let mut context = initial_context;
    let mut request = request;

    let handler_observed = Arc::new(Mutex::new(false));
    let handler_observed_by_downstream = Arc::clone(&handler_observed);
    let principal_observed_by_downstream = principal.clone();
    let calling_service_observed_by_downstream = calling_service.clone();

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);

    let result: Result<UniversalResponse, RequestPipelineError<_>> = pipeline.run_request(
        &mut context,
        &mut request,
        move |context, request| -> Result<UniversalResponse, ()> {
            *handler_observed_by_downstream.lock().unwrap() = true;

            let security = context
                .security()
                .expect("successful authentication must establish security context");

            assert_eq!(security.principal(), &principal_observed_by_downstream);

            assert_eq!(
                security.calling_service(),
                Some(&calling_service_observed_by_downstream),
            );

            assert_eq!(
                context.operation().operation.id.as_str(),
                "runtime-security-happy-op",
            );

            assert_eq!(
                context.operation().attempt_id.as_ref().unwrap().as_str(),
                "runtime-security-attempt",
            );

            assert_eq!(
                context.operation().node_id.as_ref().unwrap().as_str(),
                "runtime-security-node",
            );

            Ok(response_for_request(request, b"secure response"))
        },
    );

    let response = result.expect("authenticated and authorized request must dispatch");

    assert_eq!(response.status, Status::Success);
    assert_eq!(response.event.envelope.payload.bytes(), b"secure response");
    assert!(*authorization_observed.lock().unwrap());
    assert!(*handler_observed.lock().unwrap());

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_request_pipeline_rejects_authentication_failure_before_dispatch() {
    let runtime = serving_runtime();

    let request = request(
        "runtime-security-auth-reject-msg",
        CAPABILITY,
        b"secure payload",
        operation_context(
            "runtime-security-auth-reject-op",
            "runtime-security-auth-reject-corr",
        ),
    );

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Err(AuthenticationError::InvalidCredentials),
        },
        TestAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        TestCredentialExtractor {
            credentials: Some(b"invalid credentials".to_vec()),
        },
    );

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone())
        .for_attempt(
            nizaam_core::identity::NodeId::new("runtime-security-node").unwrap(),
            nizaam_core::identity::AttemptId::new("runtime-security-attempt").unwrap(),
        );

    let mut request = request;

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_downstream = Arc::clone(&handler_called);

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);

    let result: Result<UniversalResponse, RequestPipelineError<_>> = pipeline.run_request(
        &mut context,
        &mut request,
        move |_context, _request| -> Result<UniversalResponse, ()> {
            handler_called_by_downstream.store(true, Ordering::SeqCst);
            Ok(response_for_request(_request, b"must not dispatch"))
        },
    );

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));

    assert!(!handler_called.load(Ordering::SeqCst));
    assert!(context.security().is_none());

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_request_pipeline_fails_on_authentication_subsystem_failure_before_dispatch() {
    let runtime = serving_runtime();

    let request = request(
        "runtime-security-auth-fail-msg",
        CAPABILITY,
        b"secure payload",
        operation_context(
            "runtime-security-auth-fail-op",
            "runtime-security-auth-fail-corr",
        ),
    );

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Err(AuthenticationError::Failed),
        },
        TestAuthorizer {
            result: Ok(AuthorizationDecision::Allow),
        },
        TestCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    );

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone())
        .for_attempt(
            nizaam_core::identity::NodeId::new("runtime-security-node").unwrap(),
            nizaam_core::identity::AttemptId::new("runtime-security-attempt").unwrap(),
        );

    let mut request = request;

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_downstream = Arc::clone(&handler_called);

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);

    let result = pipeline.run_request(
        &mut context,
        &mut request,
        move |_context, _request| -> Result<UniversalResponse, ()> {
            handler_called_by_downstream.store(true, Ordering::SeqCst);
            Ok(response_for_request(_request, b"must not dispatch"))
        },
    );

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Middleware(_)
        ))
    ));

    assert!(!handler_called.load(Ordering::SeqCst));
    assert!(context.security().is_none());

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_request_pipeline_rejects_authorization_deny_before_dispatch() {
    let runtime = serving_runtime();

    let request = request(
        "runtime-security-authz-deny-msg",
        CAPABILITY,
        b"secure payload",
        operation_context(
            "runtime-security-authz-deny-op",
            "runtime-security-authz-deny-corr",
        ),
    );

    let principal = user_principal("runtime-security-denied-user");

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(principal),
        },
        TestAuthorizer {
            result: Ok(AuthorizationDecision::Deny),
        },
        TestCredentialExtractor {
            credentials: Some(b"valid credentials".to_vec()),
        },
    );

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone())
        .for_attempt(
            nizaam_core::identity::NodeId::new("runtime-security-node").unwrap(),
            nizaam_core::identity::AttemptId::new("runtime-security-attempt").unwrap(),
        );

    let mut request = request;

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_downstream = Arc::clone(&handler_called);

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);

    let result: Result<UniversalResponse, RequestPipelineError<_>> = pipeline.run_request(
        &mut context,
        &mut request,
        move |_context, _request| -> Result<UniversalResponse, ()> {
            handler_called_by_downstream.store(true, Ordering::SeqCst);
            Ok(response_for_request(_request, b"must not dispatch"))
        },
    );

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));

    assert!(!handler_called.load(Ordering::SeqCst));

    let security = context
        .security()
        .expect("authentication succeeds before authorization denial");

    assert_eq!(
        security.principal(),
        &user_principal("runtime-security-denied-user"),
    );

    runtime.shutdown().unwrap();
}

#[test]
fn runtime_request_pipeline_fails_on_authorization_subsystem_failure_before_dispatch() {
    let runtime = serving_runtime();

    let request = request(
        "runtime-security-authz-fail-msg",
        CAPABILITY,
        b"secure payload",
        operation_context(
            "runtime-security-authz-fail-op",
            "runtime-security-authz-fail-corr",
        ),
    );

    let principal = user_principal("runtime-security-failing-user");

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(principal.clone()),
        },
        TestAuthorizer {
            result: Err(AuthorizationError::Failed),
        },
        TestCredentialExtractor {
            credentials: Some(b"valid credentials".to_vec()),
        },
    );

    let mut context = EngineContext::new(request.event.envelope.operation_context.clone())
        .for_attempt(
            nizaam_core::identity::NodeId::new("runtime-security-node").unwrap(),
            nizaam_core::identity::AttemptId::new("runtime-security-attempt").unwrap(),
        );

    let mut request = request;

    let handler_called = Arc::new(AtomicBool::new(false));
    let handler_called_by_downstream = Arc::clone(&handler_called);

    let pipeline = ExecutionPipeline::new().with_middleware(middleware);

    let result: Result<UniversalResponse, RequestPipelineError<_>> = pipeline.run_request(
        &mut context,
        &mut request,
        move |_context, _request| -> Result<UniversalResponse, ()> {
            handler_called_by_downstream.store(true, Ordering::SeqCst);
            Ok(response_for_request(_request, b"must not dispatch"))
        },
    );

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Middleware(_)
        ))
    ));

    assert!(!handler_called.load(Ordering::SeqCst));

    let security = context
        .security()
        .expect("authentication succeeds before authorization failure");

    assert_eq!(security.principal(), &principal);

    runtime.shutdown().unwrap();
}
