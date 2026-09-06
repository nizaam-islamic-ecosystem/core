//! Integration tests for the Capability System.
//!
//! These tests verify the end-to-end pipeline of registering, looking up,
//! and dispatching capabilities through the registry.

use std::sync::Arc;
use std::time::Duration;

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityDispatchResult, CapabilityEntry, CapabilityError,
    CapabilityHandler, CapabilityInvocation, CapabilityOutcome, CapabilityRegistry, RegistryError,
    arc_handler, dispatch,
};
use nizaam_core::identity::{CapabilityId, ContractId, EngineId};
use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::{Deadline, EngineContext};

/// Helper to create a test engine context.
fn make_context() -> EngineContext {
    let operation = Operation::new(
        OperationId::new("test-op").unwrap(),
        CorrelationId::new("test-corr").unwrap(),
    );
    EngineContext::new(OperationContext::new(operation))
}

/// Helper to create a test invocation.
fn make_invocation(cap_id: CapabilityId) -> CapabilityInvocation {
    CapabilityInvocation::new(
        cap_id,
        ContractId::new("test.contract").unwrap(),
        b"request payload".to_vec(),
    )
}

/// Helper to create a simple capability handler that returns a response.
fn simple_handler() -> Arc<dyn CapabilityHandler> {
    arc_handler(|_ctx: &EngineContext, _inv: &CapabilityInvocation| {
        Ok(CapabilityOutcome::new(b"simple response".to_vec()))
    })
}

#[test]
fn test_registry_register_and_lookup() {
    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("test.cap").unwrap(),
        EngineId::new("test.engine").unwrap(),
        "Test Capability",
    )
    .unwrap();

    registry.register(def.clone(), simple_handler()).unwrap();
    assert!(registry.contains(def.capability_id()));

    let entry = registry.get(def.capability_id()).unwrap();
    assert_eq!(entry.definition().name(), "Test Capability");
}

#[test]
fn test_registry_rejects_duplicate() {
    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("dup.cap").unwrap(),
        EngineId::new("test.engine").unwrap(),
        "Duplicate",
    )
    .unwrap();

    registry.register(def.clone(), simple_handler()).unwrap();
    let result = registry.register(def, simple_handler());
    assert!(matches!(result, Err(RegistryError::AlreadyRegistered(_))));
}

#[test]
fn test_registry_unregister() {
    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("remove.cap").unwrap(),
        EngineId::new("test.engine").unwrap(),
        "To Remove",
    )
    .unwrap();

    registry.register(def.clone(), simple_handler()).unwrap();
    registry.unregister(def.capability_id()).unwrap();
    assert!(!registry.contains(def.capability_id()));
}

#[test]
fn test_registry_iter() {
    let registry = CapabilityRegistry::new();
    let def1 = CapabilityDefinition::new(
        CapabilityId::new("iter.a").unwrap(),
        EngineId::new("engine.1").unwrap(),
        "Iter A",
    )
    .unwrap();
    let def2 = CapabilityDefinition::new(
        CapabilityId::new("iter.b").unwrap(),
        EngineId::new("engine.2").unwrap(),
        "Iter B",
    )
    .unwrap();

    registry.register(def1, simple_handler()).unwrap();
    registry.register(def2, simple_handler()).unwrap();

    let entries: Vec<_> = registry.iter().collect();
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_dispatch_happy_path() {
    let registry = CapabilityRegistry::new();
    let cap_id = CapabilityId::new("happy.cap").unwrap();
    let def = CapabilityDefinition::new(
        cap_id.clone(),
        EngineId::new("engine").unwrap(),
        "Happy Path",
    )
    .unwrap();
    registry.register(def, simple_handler()).unwrap();

    let context = make_context();
    let invocation = make_invocation(cap_id);
    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_ok());
    let outcome = result.into_outcome().unwrap();
    assert_eq!(outcome.into_bytes(), b"simple response");
}

#[test]
fn test_dispatch_unknown_capability() {
    let registry = CapabilityRegistry::new();
    let context = make_context();
    let invocation = make_invocation(CapabilityId::new("unknown").unwrap());

    let result = dispatch(&registry, &context, &invocation);
    assert!(result.is_err());
    assert!(matches!(result.as_error(), Some(CapabilityError::Unknown)));
}

#[test]
fn test_dispatch_cancelled_context() {
    let registry = CapabilityRegistry::new();
    let cap_id = CapabilityId::new("cancel.cap").unwrap();
    let def = CapabilityDefinition::new(
        cap_id.clone(),
        EngineId::new("engine").unwrap(),
        "Cancel Test",
    )
    .unwrap();
    registry.register(def, simple_handler()).unwrap();

    let context = make_context();
    context.cancellation().cancel();

    let invocation = make_invocation(cap_id);
    let result = dispatch(&registry, &context, &invocation);
    assert!(matches!(
        result.as_error(),
        Some(CapabilityError::Cancelled)
    ));
}

#[test]
fn test_dispatch_expired_deadline() {
    let registry = CapabilityRegistry::new();
    let cap_id = CapabilityId::new("expire.cap").unwrap();
    let def = CapabilityDefinition::new(
        cap_id.clone(),
        EngineId::new("engine").unwrap(),
        "Expire Test",
    )
    .unwrap();
    registry.register(def, simple_handler()).unwrap();

    let context = make_context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

    let invocation = make_invocation(cap_id);
    let result = dispatch(&registry, &context, &invocation);
    assert!(matches!(
        result.as_error(),
        Some(CapabilityError::DeadlineExpired)
    ));
}

#[test]
fn test_dispatch_handler_failure() {
    let failing_handler: Arc<dyn CapabilityHandler> =
        arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
            Err(CapabilityError::HandlerFailed("internal error".into()))
        });

    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("fail.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "Failing Handler",
    )
    .unwrap();
    registry.register(def, failing_handler).unwrap();

    let context = make_context();
    let invocation = make_invocation(CapabilityId::new("fail.cap").unwrap());
    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_err());
    if let Some(CapabilityError::HandlerFailed(msg)) = result.into_error() {
        assert_eq!(msg, "internal error");
    } else {
        panic!("Expected HandlerFailed error");
    }
}

#[test]
fn test_function_adapter_integration() {
    // Verify that plain functions can be adapted into handlers
    let handler: Arc<dyn CapabilityHandler> =
        arc_handler(|ctx: &EngineContext, inv: &CapabilityInvocation| {
            let response = format!(
                "echo: {:?} from op {}",
                inv.payload_bytes(),
                ctx.operation().operation.id
            );
            Ok(CapabilityOutcome::new(response.into_bytes()))
        });

    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("echo.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "Echo Handler",
    )
    .unwrap();
    registry.register(def, handler).unwrap();

    let context = make_context();
    let invocation = make_invocation(CapabilityId::new("echo.cap").unwrap());
    let result = dispatch(&registry, &context, &invocation);

    assert!(result.is_ok());
    let response = String::from_utf8(result.into_outcome().unwrap().into_bytes()).unwrap();
    assert!(response.contains("request") || response.contains("114, 101, 113"));
}

#[test]
fn test_dispatch_result_accessors() {
    let outcome = CapabilityDispatchResult::Outcome(CapabilityOutcome::new(b"ok".to_vec()));
    assert!(outcome.is_ok());
    assert!(!outcome.is_err());
    assert!(outcome.as_outcome().is_some());
    assert!(outcome.as_error().is_none());
    assert_eq!(outcome.into_outcome().unwrap().into_bytes(), b"ok");

    let error = CapabilityDispatchResult::Error(CapabilityError::Unknown);
    assert!(!error.is_ok());
    assert!(error.is_err());
    assert!(error.as_outcome().is_none());
    assert!(error.as_error().is_some());
    assert!(matches!(error.into_error(), Some(CapabilityError::Unknown)));
}

#[test]
fn test_capability_response_conversion() {
    let response = nizaam_core::capability::CapabilityResponse::new(b"test".to_vec());
    let bytes: Vec<u8> = response.into();
    assert_eq!(bytes, b"test");
}

#[test]
fn test_capability_entry_debug() {
    let def = CapabilityDefinition::new(
        CapabilityId::new("debug.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "Debug Test",
    )
    .unwrap();
    let entry = CapabilityEntry::new(def, simple_handler());

    // Verify Debug can be formatted (should not panic)
    let debug_str = format!("{:?}", entry);
    assert!(debug_str.contains("CapabilityEntry"));
}

#[test]
fn test_registry_debug() {
    let registry = CapabilityRegistry::new();
    let debug_str = format!("{:?}", registry);
    assert!(debug_str.contains("CapabilityRegistry"));
}

#[test]
fn test_registry_clone_is_independent() {
    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("clone.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "Clone Test",
    )
    .unwrap();
    registry.register(def.clone(), simple_handler()).unwrap();

    // Clone the registry
    let cloned = registry.clone();

    // Verify both point to the same logical state
    assert!(registry.contains(def.capability_id()));
    assert!(cloned.contains(def.capability_id()));

    // Unregister from original
    registry.unregister(def.capability_id()).unwrap();

    // Cloned should still have it (it's a logical copy, not a reference)
    assert!(cloned.contains(def.capability_id()));
}
