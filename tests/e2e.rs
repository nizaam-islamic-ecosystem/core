//! Phase 16 complete Core E2E execution tests.
//!
//! These tests exercise the public Control Plane boundaries as one composed
//! system. They deliberately cross registration, registry, membership,
//! observations, destination eligibility, routing policy, resolution,
//! attempts, concrete routing, and Control Plane communication.
//!
//! The current Phase 15 implementation does not expose one stateful
//! `ControlPlane::execute` operation. The tests therefore compose the
//! authoritative subsystem owners explicitly rather than inventing a missing
//! orchestration API.
//!
//! The principal execution path covered here is:
//!
//! ```text
//! EngineRegistration
//!       ↓
//! EngineRegistry + Membership
//!       ↓
//! ObservationSnapshot
//!       ↓
//! destination eligibility
//!       ↓
//! RoutingPolicy
//!       ↓
//! Resolution
//!       ↓
//! Attempt
//!       ↓
//! RoutingDecision
//!       ↓
//! ControlPlaneCommunication
//!       ↓
//! Engine Runtime / Transport
//! ```
//!
//! These tests do not duplicate unit-level validation of individual modules,
//! and they do not claim that currently-unimplemented lifecycle, retry,
//! observability, provenance, or replanning orchestration exists.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nizaam_core::capability::CapabilityError;
use nizaam_core::contracts::UniversalRequest;
use nizaam_core::contracts::descriptor::Interaction;
use nizaam_core::control_plane::{
    ControlPlaneCommunication, DestinationEligibilityInput, DestinationRequest, EngineRegistry,
    FallbackPolicy, Membership, Observations, eligible_destinations,
};
use nizaam_core::identity::{AttemptId, ContractId, CorrelationId, NodeId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::LifecycleState;
use nizaam_core::transport::InMemoryTransport;

mod common;

use common::control_plane::*;
use common::reference_engine::{ReferenceBehavior, ReferenceDispatchError, ReferenceEngine};

fn e2e_engine_context(request: &UniversalRequest) -> nizaam_core::runtime::EngineContext {
    nizaam_core::runtime::EngineContext::new(request.event.envelope.operation_context.clone())
}

#[test]
fn e2e_happy_path_reaches_reference_engine_runtime_and_capability() {
    let engine = ReferenceEngine::new(engine_id("e2e-engine-b"), instance_id("e2e-engine-b-1"));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let operation = operation_id("e2e-happy-operation");
    let attempt = attempt(&operation, "e2e-happy-attempt", 1);
    let context = e2e_engine_context(&request_for(
        &engine_id("e2e-engine-b"),
        &instance_id("e2e-engine-b-1"),
        &operation,
        attempt.attempt_id(),
        "e2e-happy-message",
    ));

    let outcome = engine
        .dispatch(&context, CAPABILITY, CONTRACT, b"hello-e2e")
        .unwrap();
    assert_eq!(outcome.as_bytes(), b"hello-e2e");
    assert_eq!(engine.invocation_count(CAPABILITY), 1);
}

#[test]
fn e2e_large_opaque_payload_is_preserved_through_transport_framing() {
    let payload = vec![b'x'; 4096];
    let transport = InMemoryTransport::new();
    let received = Arc::new(Mutex::new(Vec::<u8>::new()));
    let received_for_handler = Arc::clone(&received);

    transport.register(
        engine_id("e2e-large-engine"),
        instance_id("e2e-large-instance"),
        move |request| {
            *received_for_handler.lock().unwrap() = request.event.envelope.payload.bytes().to_vec();
            response_for(request, b"ok".to_vec())
        },
    );

    let operation = operation_id("e2e-large-operation");
    let attempt = attempt(&operation, "e2e-large-attempt", 1);
    let request = request_with_payload(
        &engine_id("e2e-large-engine"),
        &instance_id("e2e-large-instance"),
        &operation,
        attempt.attempt_id(),
        "e2e-large-message",
        &payload,
    );
    // The transport path itself owns framing. This assertion deliberately uses
    // the public communication boundary rather than reimplementing framing.
    let communication = ControlPlaneCommunication::new(transport);
    let _ = futures::executor::block_on(
        communication.send_to_engine(&instance_id("e2e-large-instance"), request),
    )
    .unwrap();
    let received = received.lock().unwrap();
    assert_eq!(received.len(), 4096);
    assert_eq!(*received, payload);
}

#[test]
fn e2e_multiple_instances_keep_engine_and_instance_identity_distinct() {
    let membership = Membership::new();
    let observations = Observations::new();

    let registry = EngineRegistry::new();
    for instance in ["e2e-b-1", "e2e-b-2"] {
        let registered = registration("e2e-engine-b", instance, CAPABILITY);
        assert_eq!(registered.engine_id().as_str(), "e2e-engine-b");
        assert_ne!(
            registered.engine_instance_id().as_str(),
            registered.engine_id().as_str()
        );
        registry.register(registered).unwrap();
        membership
            .register(registration("e2e-engine-b", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("e2e-engine-b", instance))
            .unwrap();
    }

    let candidates = eligible_candidates(
        &membership,
        &observations,
        &DestinationRequest::hard_logical(capability_requirement(CAPABILITY)),
    );
    assert_eq!(candidates.len(), 2);
    assert_ne!(candidates[0].instance_id(), candidates[1].instance_id());
    for candidate in &candidates {
        let record = registry.get(candidate.instance_id()).unwrap();
        assert_eq!(record.engine_id().as_str(), "e2e-engine-b");
        assert_eq!(record.engine_instance_id(), candidate.instance_id());
    }
}

#[test]
fn e2e_hard_destination_failure_does_not_fallback() {
    let membership = Membership::new();
    let observations = Observations::new();
    membership
        .register(registration("e2e-hard", "e2e-hard-1", CAPABILITY))
        .unwrap();
    observations
        .update(healthy_observation("e2e-hard", "e2e-hard-1"))
        .unwrap();

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &DestinationRequest::hard_explicit(instance_id("e2e-hard-missing")),
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ));
    assert!(result.is_err());
}

#[test]
fn e2e_preferred_destination_falls_back_to_an_eligible_instance() {
    let membership = Membership::new();
    let observations = Observations::new();
    for instance in ["e2e-pref-1", "e2e-pref-2"] {
        membership
            .register(registration("e2e-pref", instance, CAPABILITY))
            .unwrap();
        observations
            .update(healthy_observation("e2e-pref", instance))
            .unwrap();
    }

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &DestinationRequest::preferred_explicit(
            instance_id("e2e-pref-missing"),
            FallbackPolicy::Allowed,
        ),
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id(CAPABILITY),
        &descriptor_for(CAPABILITY, Interaction::Request),
    ))
    .unwrap();

    assert!(!result.is_empty());
}

#[test]
fn e2e_dispatch_accepts_the_supplied_contract_id_without_contract_validation() {
    let engine = ReferenceEngine::new(
        engine_id("e2e-contract-engine"),
        instance_id("e2e-contract-instance"),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let operation = operation_id("e2e-contract-operation");
    let attempt = attempt(&operation, "e2e-contract-attempt", 1);
    let ctx = e2e_engine_context(&request_for(
        engine.engine_id(),
        engine.instance_id(),
        &operation,
        attempt.attempt_id(),
        "e2e-contract-message",
    ));

    let incompatible = ContractId::new("incompatible.e2e.contract").unwrap();
    let result = engine.dispatch(&ctx, CAPABILITY, incompatible.as_str(), b"payload");
    assert!(result.is_ok());
    assert_eq!(engine.invocation_count(CAPABILITY), 1);
}

#[test]
fn e2e_runtime_rejects_after_routing_when_engine_is_draining() {
    let engine = ReferenceEngine::new(
        engine_id("e2e-draining"),
        instance_id("e2e-draining-instance"),
    );
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();
    engine
        .runtime()
        .transition(LifecycleState::Draining)
        .unwrap();

    let ctx = context_for_e2e("e2e-draining-operation");
    let result = engine.dispatch(&ctx, CAPABILITY, CONTRACT, b"late");
    assert!(result.is_err());
}

fn context_for_e2e(id: &str) -> nizaam_core::runtime::EngineContext {
    nizaam_core::runtime::EngineContext::new(OperationContext::new(Operation::new(
        operation_id(id),
        CorrelationId::new(format!("corr-{id}")).unwrap(),
    )))
}

#[test]
fn e2e_retry_can_move_from_one_reference_instance_to_another() {
    let first = ReferenceEngine::new(engine_id("e2e-retry"), instance_id("e2e-retry-1"));
    let second = ReferenceEngine::new(engine_id("e2e-retry"), instance_id("e2e-retry-2"));
    first
        .register_capability(CAPABILITY, ReferenceBehavior::Fail)
        .unwrap();
    second
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    first.serving().unwrap();
    second.serving().unwrap();

    let operation = operation_id("e2e-retry-operation");
    let first_context = context_for_e2e("e2e-retry-operation").for_attempt(
        NodeId::new("node-1").unwrap(),
        AttemptId::new("e2e-retry-attempt-1").unwrap(),
    );
    let second_context = context_for_e2e("e2e-retry-operation").for_attempt(
        NodeId::new("node-2").unwrap(),
        AttemptId::new("e2e-retry-attempt-2").unwrap(),
    );

    assert!(
        first
            .dispatch(&first_context, CAPABILITY, CONTRACT, b"retry")
            .is_err()
    );
    assert!(
        second
            .dispatch(&second_context, CAPABILITY, CONTRACT, b"retry")
            .is_ok()
    );
    assert_eq!(
        first
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .operation
            .id,
        operation
    );
    assert_eq!(
        second
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .operation
            .id,
        operation
    );
    assert_ne!(
        first
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .attempt_id
            .as_ref(),
        second
            .last_context(CAPABILITY)
            .unwrap()
            .operation()
            .attempt_id
            .as_ref()
    );
}

#[test]
fn e2e_idempotent_side_effect_counter_remains_observable() {
    let engine = ReferenceEngine::new(engine_id("e2e-idempotent"), instance_id("e2e-idempotent-1"));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::SideEffect(b"done".to_vec()))
        .unwrap();
    engine.serving().unwrap();

    let ctx = context_for_e2e("e2e-idempotent-operation");
    engine
        .dispatch(&ctx, CAPABILITY, CONTRACT, b"side-effect")
        .unwrap();
    assert_eq!(engine.side_effect_count(CAPABILITY), 1);
}

#[test]
fn e2e_streaming_preserves_logical_item_boundaries() {
    let engine = ReferenceEngine::new(engine_id("e2e-stream"), instance_id("e2e-stream-1"));
    engine.serving().unwrap();
    let ctx = context_for_e2e("e2e-stream-operation");
    let stream = engine
        .open_stream::<Vec<u8>>(&ctx, 4, nizaam_core::streaming::BackpressurePolicy::Reject)
        .unwrap();
    stream.open().unwrap();
    let consumer = stream.consumer().unwrap();
    stream
        .publish(nizaam_core::streaming::StreamItem::partial(
            0,
            b"one".to_vec(),
        ))
        .unwrap();
    stream
        .publish(nizaam_core::streaming::StreamItem::final_item(
            1,
            b"two".to_vec(),
        ))
        .unwrap();
    assert_eq!(
        consumer.next_item().unwrap().unwrap().into_payload(),
        b"one".to_vec()
    );
    assert_eq!(
        consumer.next_item().unwrap().unwrap().into_payload(),
        b"two".to_vec()
    );
}

#[test]
fn e2e_event_can_observe_execution_without_becoming_routing_state() {
    use nizaam_core::events::{Event, EventContext, EventName, Scope};
    use nizaam_core::identity::EventId;

    let context = context_for_e2e("e2e-event-operation");
    let event = Event::new_with_context(
        EventId::new("e2e-event-id").unwrap(),
        EventName::new("e2e.execution").unwrap(),
        "execution",
        Scope::new("engine:e2e").unwrap(),
        EventContext::empty().with_operation_context(context.operation().clone()),
    )
    .unwrap();

    assert_eq!(
        event.context().operation_context().unwrap().operation.id,
        OperationId::new("e2e-event-operation").unwrap()
    );
}

#[test]
fn e2e_failure_can_be_observed_without_replacing_core_error() {
    let engine = ReferenceEngine::new(engine_id("e2e-error"), instance_id("e2e-error-1"));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Fail)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &context_for_e2e("e2e-error-operation"),
        CAPABILITY,
        CONTRACT,
        b"fail",
    );
    assert!(result.is_err());
    assert_eq!(engine.invocation_count(CAPABILITY), 1);
}

#[test]
fn e2e_artifact_output_has_integrity_and_provenance_relations() {
    use nizaam_core::artifact::{ArtifactVersion, ContentDigest, ContentReference, IntegrityProof};
    use nizaam_core::identity::ArtifactId;
    use nizaam_core::provenance::ProvenanceContext;

    let id = ArtifactId::new("e2e-artifact").unwrap();
    let bytes = b"e2e-artifact";
    let version = ArtifactVersion::new(
        id.clone(),
        "v1",
        ContentReference::new("e2e-provider", "content/v1"),
        ContentDigest::new(bytes),
        bytes.len() as u64,
    );
    let proof = IntegrityProof::verify(bytes, &ContentDigest::new(bytes)).unwrap();
    let provenance = ProvenanceContext::new().with_attribute("operation", "e2e-artifact-operation");

    assert_eq!(version.artifact_id(), &id);
    assert!(proof.matches(&ContentDigest::new(bytes), bytes.len() as u64));
    assert_eq!(
        provenance.attribute("operation"),
        Some("e2e-artifact-operation")
    );
}

#[test]
fn e2e_cancellation_interrupts_in_flight_reference_execution() {
    let engine = Arc::new(ReferenceEngine::new(
        engine_id("e2e-cancel"),
        instance_id("e2e-cancel-1"),
    ));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Delay(Duration::from_secs(1)))
        .unwrap();
    engine.serving().unwrap();

    let ctx = context_for_e2e("e2e-cancel-operation");
    let cancellation = ctx.cancellation().clone();
    let worker = {
        let engine = Arc::clone(&engine);
        let ctx = ctx.clone();
        std::thread::spawn(move || engine.dispatch(&ctx, CAPABILITY, CONTRACT, b"slow"))
    };
    std::thread::sleep(Duration::from_millis(20));
    cancellation.cancel();
    assert!(matches!(
        worker.join().unwrap(),
        Err(ReferenceDispatchError::Capability(
            CapabilityError::Cancelled
        ))
    ));
}

#[test]
fn e2e_deadline_aborts_slow_execution() {
    let engine = ReferenceEngine::new(engine_id("e2e-deadline"), instance_id("e2e-deadline-1"));
    engine
        .register_capability(
            CAPABILITY,
            ReferenceBehavior::Delay(Duration::from_millis(50)),
        )
        .unwrap();
    engine.serving().unwrap();

    let ctx = context_for_e2e("e2e-deadline-operation")
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::from_millis(1)).unwrap());
    std::thread::sleep(Duration::from_millis(2));
    assert!(
        engine
            .dispatch(&ctx, CAPABILITY, CONTRACT, b"deadline")
            .is_err()
    );
}

#[test]
fn e2e_transport_disconnect_is_a_bounded_failure() {
    let transport = InMemoryTransport::new();
    let communication = ControlPlaneCommunication::new(transport);
    let operation = operation_id("e2e-disconnect-operation");
    let attempt = attempt(&operation, "e2e-disconnect-attempt", 1);

    let result = futures::executor::block_on(communication.send_to_engine(
        &instance_id("e2e-disconnected"),
        request_for(
            &engine_id("e2e-disconnected-engine"),
            &instance_id("e2e-disconnected"),
            &operation,
            attempt.attempt_id(),
            "e2e-disconnect-message",
        ),
    ));
    assert!(result.is_err());
}

#[test]
fn e2e_engine_shutdown_rejects_subsequent_work() {
    let engine = ReferenceEngine::new(engine_id("e2e-shutdown"), instance_id("e2e-shutdown-1"));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();
    assert!(engine.shutdown().unwrap());
    assert!(
        engine
            .dispatch(
                &context_for_e2e("e2e-shutdown-operation"),
                CAPABILITY,
                CONTRACT,
                b"after-shutdown"
            )
            .is_err()
    );
}

#[test]
fn e2e_two_sequential_requests_remain_paired_by_their_contexts() {
    let engine = ReferenceEngine::new(engine_id("e2e-concurrent"), instance_id("e2e-concurrent-1"));
    engine
        .register_capability(CAPABILITY, ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let first = context_for_e2e("e2e-concurrent-one");
    let second = context_for_e2e("e2e-concurrent-two");

    let first_engine = &engine;
    let first_result = first_engine
        .dispatch(&first, CAPABILITY, CONTRACT, b"one")
        .unwrap();
    let second_result = engine
        .dispatch(&second, CAPABILITY, CONTRACT, b"two")
        .unwrap();

    assert_eq!(first_result.as_bytes(), b"one");
    assert_eq!(second_result.as_bytes(), b"two");
    assert_eq!(engine.invocation_count(CAPABILITY), 2);
}
