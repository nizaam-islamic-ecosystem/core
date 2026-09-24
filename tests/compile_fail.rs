//! Phase 16 compile-time API boundary checks.
//!
//! The current repository has no approved compile-fail harness or additional
//! dev dependency. These tests therefore anchor the existing public type
//! contracts. The invalid substitutions themselves are documented as deferred
//! compile-fail cases rather than inserted into executable code.

use nizaam_core::capability::{
    CapabilityError, CapabilityHandler, CapabilityInvocation, CapabilityOutcome,
};
use nizaam_core::contracts::{UniversalRequest, UniversalResponse};
use nizaam_core::identity::{
    AttemptId, CapabilityId, CorrelationId, EngineId, EngineInstanceId, EventId, MessageId,
    OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;
use nizaam_core::status::Status;
use nizaam_core::streaming::StreamContext;
use nizaam_core::transport::{BoxedFuture, Transport, TransportError};

fn accepts_operation(_: OperationId) {}
fn accepts_attempt(_: AttemptId) {}
fn accepts_message(_: MessageId) {}
fn accepts_event(_: EventId) {}
fn accepts_engine(_: EngineId) {}
fn accepts_instance(_: EngineInstanceId) {}
fn accepts_capability(_: CapabilityId) {}
fn accepts_operation_context(_: &OperationContext) {}
fn accepts_engine_context(_: &EngineContext) {}
fn accepts_stream_context(_: &StreamContext) {}
fn accepts_capability_handler(_: &dyn CapabilityHandler) {}
fn accepts_transport(_: &dyn Transport) {}

#[test]
fn identity_roles_have_distinct_public_signatures() {
    accepts_operation(OperationId::new("compile-operation-id").unwrap());
    accepts_attempt(AttemptId::new("compile-attempt-id").unwrap());
    accepts_message(MessageId::new("compile-message-id").unwrap());
    accepts_event(EventId::new("compile-event-id").unwrap());
    accepts_engine(EngineId::new("compile-engine").unwrap());
    accepts_instance(EngineInstanceId::new("compile-instance").unwrap());
    accepts_capability(CapabilityId::new("compile-capability-id").unwrap());
}

#[test]
fn identity_roles_have_distinct_runtime_type_ids() {
    use std::any::TypeId;

    assert_ne!(TypeId::of::<OperationId>(), TypeId::of::<AttemptId>());

    assert_ne!(TypeId::of::<MessageId>(), TypeId::of::<OperationId>());
    assert_ne!(TypeId::of::<MessageId>(), TypeId::of::<AttemptId>());

    assert_ne!(TypeId::of::<EventId>(), TypeId::of::<OperationId>());
    assert_ne!(TypeId::of::<EventId>(), TypeId::of::<AttemptId>());
    assert_ne!(TypeId::of::<EventId>(), TypeId::of::<MessageId>());

    assert_ne!(TypeId::of::<EngineId>(), TypeId::of::<OperationId>());
    assert_ne!(TypeId::of::<EngineId>(), TypeId::of::<AttemptId>());
    assert_ne!(TypeId::of::<EngineId>(), TypeId::of::<MessageId>());
    assert_ne!(TypeId::of::<EngineId>(), TypeId::of::<EventId>());

    assert_ne!(
        TypeId::of::<EngineInstanceId>(),
        TypeId::of::<OperationId>()
    );
    assert_ne!(TypeId::of::<EngineInstanceId>(), TypeId::of::<AttemptId>());
    assert_ne!(TypeId::of::<EngineInstanceId>(), TypeId::of::<MessageId>());
    assert_ne!(TypeId::of::<EngineInstanceId>(), TypeId::of::<EventId>());
    assert_ne!(TypeId::of::<EngineInstanceId>(), TypeId::of::<EngineId>());

    assert_ne!(TypeId::of::<CapabilityId>(), TypeId::of::<OperationId>());
    assert_ne!(TypeId::of::<CapabilityId>(), TypeId::of::<AttemptId>());
    assert_ne!(TypeId::of::<CapabilityId>(), TypeId::of::<MessageId>());
    assert_ne!(TypeId::of::<CapabilityId>(), TypeId::of::<EventId>());
    assert_ne!(TypeId::of::<CapabilityId>(), TypeId::of::<EngineId>());
    assert_ne!(
        TypeId::of::<CapabilityId>(),
        TypeId::of::<EngineInstanceId>()
    );
}

#[test]
fn context_roles_have_distinct_public_signatures() {
    let operation = Operation::new(
        OperationId::new("compile-operation").unwrap(),
        CorrelationId::new("compile-correlation").unwrap(),
    );
    let engine = EngineContext::new(OperationContext::new(operation));
    let stream = StreamContext::from_engine_context(&engine);

    accepts_operation_context(engine.operation());
    accepts_engine_context(&engine);
    accepts_stream_context(&stream);
}

#[test]
fn capability_and_transport_traits_accept_only_their_own_contracts() {
    struct NoopCapability;

    impl CapabilityHandler for NoopCapability {
        fn invoke(
            &self,
            _context: &EngineContext,
            _invocation: &CapabilityInvocation,
        ) -> Result<CapabilityOutcome, CapabilityError> {
            Ok(CapabilityOutcome::new(Vec::new()))
        }
    }

    struct NoopTransport;

    impl Transport for NoopTransport {
        fn call(
            &self,
            _target: &EngineInstanceId,
            request: UniversalRequest,
        ) -> BoxedFuture<UniversalResponse, TransportError> {
            Box::pin(async move {
                Ok(UniversalResponse::new(
                    request.event.envelope,
                    Status::Success,
                ))
            })
        }

        fn is_connected(&self, _target: &EngineInstanceId) -> bool {
            true
        }

        fn connected_targets(&self) -> Vec<EngineInstanceId> {
            Vec::new()
        }
    }

    let capability = NoopCapability;
    let transport = NoopTransport;
    accepts_capability_handler(&capability);
    accepts_transport(&transport);
}

// Deferred negative cases, intentionally not executable without a compile-fail
// harness approved by the repository plan:
//
// accepts_operation(AttemptId::new("compile-attempt-id").unwrap());
// accepts_attempt(OperationId::new("compile-operation-id").unwrap());
// accepts_engine(EngineInstanceId::new("x").unwrap());
// accepts_operation_context(&EngineContext::new(/* ... */));
// accepts_stream_context(&EngineContext::new(/* ... */));
// accepts_transport(&NoopCapability);
//
// Rust's normal type checker rejects these substitutions because the public
// signatures require distinct concrete roles.
