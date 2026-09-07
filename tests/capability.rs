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
use nizaam_core::contracts::Version;
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

#[test]
fn test_dispatch_uses_context_operation_metadata() {
    // Verify the handler receives the correct operation context.
    let received_context = std::sync::Arc::new(std::sync::Mutex::new(None));
    let received_clone = std::sync::Arc::clone(&received_context);
    let handler: Arc<dyn CapabilityHandler> =
        arc_handler(move |ctx: &EngineContext, _: &CapabilityInvocation| {
            let op_id = ctx.operation().operation.id.clone();
            *received_clone.lock().unwrap() = Some(op_id);
            Ok(CapabilityOutcome::new(b"ok".to_vec()))
        });

    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("ctx.op").unwrap(),
        EngineId::new("engine").unwrap(),
        "Context Op Test",
    )
    .unwrap();
    registry.register(def, handler).unwrap();

    let operation = Operation::new(
        OperationId::new("custom-op-id").unwrap(),
        CorrelationId::new("custom-corr-id").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let invocation = make_invocation(CapabilityId::new("ctx.op").unwrap());

    let result = dispatch(&registry, &context, &invocation);
    assert!(result.is_ok());

    let captured = received_context.lock().unwrap().take();
    let op_id = captured.expect("handler should have been called");
    assert_eq!(op_id.as_str(), "custom-op-id");
}

#[test]
fn test_multiple_handlers_different_capabilities() {
    let registry = CapabilityRegistry::new();

    let handler1 = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
        Ok(CapabilityOutcome::new(b"response1".to_vec()))
    });
    let handler2 = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
        Ok(CapabilityOutcome::new(b"response2".to_vec()))
    });

    let def1 = CapabilityDefinition::new(
        CapabilityId::new("multi.1").unwrap(),
        EngineId::new("engine").unwrap(),
        "Multi 1",
    )
    .unwrap();
    let def2 = CapabilityDefinition::new(
        CapabilityId::new("multi.2").unwrap(),
        EngineId::new("engine").unwrap(),
        "Multi 2",
    )
    .unwrap();

    registry.register(def1, handler1).unwrap();
    registry.register(def2, handler2).unwrap();

    let context = make_context();

    let result1 = dispatch(
        &registry,
        &context,
        &make_invocation(CapabilityId::new("multi.1").unwrap()),
    );
    let result2 = dispatch(
        &registry,
        &context,
        &make_invocation(CapabilityId::new("multi.2").unwrap()),
    );

    assert_eq!(result1.into_outcome().unwrap().into_bytes(), b"response1");
    assert_eq!(result2.into_outcome().unwrap().into_bytes(), b"response2");
}

#[test]
fn test_definition_with_all_metadata_in_pipeline() {
    let registry = CapabilityRegistry::new();

    let handler = arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
        Ok(CapabilityOutcome::new(b"full-metadata".to_vec()))
    });

    let def = CapabilityDefinition::new(
        CapabilityId::new("full.meta").unwrap(),
        EngineId::new("meta.engine").unwrap(),
        "Full Metadata Capability",
    )
    .unwrap()
    .with_description("A capability with all metadata fields set")
    .unwrap()
    .with_version(Version::new(1, 0, 0));

    assert_eq!(def.name(), "Full Metadata Capability");
    assert_eq!(
        def.description(),
        Some("A capability with all metadata fields set")
    );
    assert_eq!(def.version().unwrap().major(), 1);

    registry.register(def, handler).unwrap();

    let context = make_context();
    let result = dispatch(
        &registry,
        &context,
        &make_invocation(CapabilityId::new("full.meta").unwrap()),
    );

    assert!(result.is_ok());
    assert_eq!(
        result.into_outcome().unwrap().into_bytes(),
        b"full-metadata"
    );
}

#[test]
fn test_dispatch_unknown_before_handler_invocation() {
    // Even if a handler is registered, an unknown capability should return Unknown
    // (not attempt to invoke any handler).
    let handler: Arc<dyn CapabilityHandler> =
        arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
            panic!("This handler should never be called");
        });

    let registry = CapabilityRegistry::new();
    let def = CapabilityDefinition::new(
        CapabilityId::new("known.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "Known Capability",
    )
    .unwrap();
    registry.register(def, handler).unwrap();

    let context = make_context();
    let result = dispatch(
        &registry,
        &context,
        &make_invocation(CapabilityId::new("unknown.cap").unwrap()),
    );

    assert!(result.is_err());
    assert!(matches!(result.as_error(), Some(CapabilityError::Unknown)));
}

#[test]
fn test_function_adapter_error_types() {
    let handler: Arc<dyn CapabilityHandler> =
        arc_handler(|_: &EngineContext, _: &CapabilityInvocation| {
            Err(CapabilityError::HandlerFailed("test error".to_string()))
        });

    let context = make_context();
    let invocation = make_invocation(CapabilityId::new("err.cap").unwrap());
    let result = handler.invoke(&context, &invocation);

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.to_string(), "capability handler failed: test error");
}

#[test]
fn test_capability_outcome_with_various_payloads() {
    // Test that CapabilityOutcome can handle various payload types.
    let outcome_vec = CapabilityOutcome::new(b"binary".to_vec());
    assert_eq!(outcome_vec.as_bytes(), b"binary");

    let outcome_string = CapabilityOutcome::new("string payload".to_string());
    assert_eq!(outcome_string.as_bytes(), b"string payload");

    let outcome_slice = CapabilityOutcome::new(&b"slice payload"[..]);
    assert_eq!(outcome_slice.as_bytes(), b"slice payload");

    let outcome_empty = CapabilityOutcome::new(Vec::<u8>::new());
    assert!(outcome_empty.as_bytes().is_empty());
}

#[test]
fn test_dispatch_result_equality() {
    let outcome1 = CapabilityDispatchResult::Outcome(CapabilityOutcome::new(b"same".to_vec()));
    let outcome2 = CapabilityDispatchResult::Outcome(CapabilityOutcome::new(b"same".to_vec()));
    let error1 = CapabilityDispatchResult::Error(CapabilityError::Unknown);
    let error2 = CapabilityDispatchResult::Error(CapabilityError::Unknown);

    assert_eq!(outcome1, outcome2);
    assert_eq!(error1, error2);

    let mixed = CapabilityDispatchResult::Outcome(CapabilityOutcome::new(b"diff".to_vec()));
    assert_ne!(outcome1, mixed);
}

#[test]
fn test_definition_rejects_whitespace_only_name() {
    let result = CapabilityDefinition::new(
        CapabilityId::new("ws.cap").unwrap(),
        EngineId::new("engine").unwrap(),
        "   \t\n  ",
    );
    assert!(result.is_err());
}

#[test]
fn test_definition_rejects_whitespace_only_description() {
    let result = CapabilityDefinition::new(
        CapabilityId::new("ws.desc").unwrap(),
        EngineId::new("engine").unwrap(),
        "Valid Name",
    )
    .unwrap()
    .with_description("\t   \n");

    assert!(result.is_err());
}

#[test]
fn test_registry_clear_removes_all_entries() {
    let registry = CapabilityRegistry::new();

    for i in 0..5 {
        let def = CapabilityDefinition::new(
            CapabilityId::new(format!("clear.{}", i)).unwrap(),
            EngineId::new("engine").unwrap(),
            format!("Clear {}", i),
        )
        .unwrap();
        registry.register(def, simple_handler()).unwrap();
    }

    assert_eq!(registry.len(), 5);

    // Unregister all one by one
    for i in 0..5 {
        registry
            .unregister(&CapabilityId::new(format!("clear.{}", i)).unwrap())
            .unwrap();
    }

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_capability_invocation_debug() {
    let invocation = CapabilityInvocation::new(
        CapabilityId::new("debug.test").unwrap(),
        ContractId::new("debug.contract").unwrap(),
        b"debug payload".to_vec(),
    );
    let debug_str = format!("{:?}", invocation);
    assert!(debug_str.contains("CapabilityInvocation"));
}

#[test]
fn test_capability_error_debug() {
    let errors = [
        CapabilityError::Unknown,
        CapabilityError::Cancelled,
        CapabilityError::DeadlineExpired,
        CapabilityError::InvalidDefinition,
        CapabilityError::HandlerFailed("debug".to_string()),
    ];

    for error in &errors {
        let debug_str = format!("{:?}", error);
        assert!(
            !debug_str.is_empty(),
            "Debug should produce non-empty output for {:?}",
            error
        );
    }
}
