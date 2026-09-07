use std::time::Duration;

use nizaam_core::prelude::*;

#[test]
fn consumer_can_propagate_engine_context_to_downstream_work() {
    let operation = Operation::new(
        OperationId::new("operation-5").unwrap(),
        CorrelationId::new("correlation-5").unwrap(),
    );
    let provenance = ProvenanceContext::new().with_attribute("source", "test");
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_secs(1)).unwrap())
        .with_security(SecurityContext::new())
        .with_provenance(provenance);
    let child = context.child_with_deadline(Deadline::from_now(Duration::from_secs(2)).unwrap());

    assert_eq!(child.operation().operation.id.as_str(), "operation-5");
    assert_eq!(child.security(), context.security());
    assert_eq!(child.provenance().attribute("source"), Some("test"));
    assert!(!child.cancellation().is_cancelled());
    assert!(!child.is_expired());
    context.cancellation().cancel();
    assert!(child.cancellation().is_cancelled());
}

#[test]
fn child_context_derives_from_parent_operation() {
    let operation = Operation::new(
        OperationId::new("parent-op").unwrap(),
        CorrelationId::new("parent-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let child = context.child();
    assert_eq!(child.operation().operation.id.as_str(), "parent-op");
}

#[test]
fn child_context_inherits_parent_cancellation_token() {
    let operation = Operation::new(
        OperationId::new("cancel-inherit-op").unwrap(),
        CorrelationId::new("cancel-inherit-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let child = context.child();
    context.cancellation().cancel();
    assert!(child.cancellation().is_cancelled());
}

#[test]
fn parent_cancellation_propagates_to_child() {
    let operation = Operation::new(
        OperationId::new("cancel-parent-op").unwrap(),
        CorrelationId::new("cancel-parent-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let child = context.child();
    context.cancellation().cancel();
    assert!(child.cancellation().is_cancelled());
}

#[test]
fn child_cancellation_does_not_affect_parent() {
    let operation = Operation::new(
        OperationId::new("cancel-child-op").unwrap(),
        CorrelationId::new("cancel-child-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let child = context.child();
    child.cancellation().cancel();
    assert!(!context.cancellation().is_cancelled());
    assert!(child.cancellation().is_cancelled());
}

#[test]
fn sibling_children_have_independent_cancellation() {
    let operation = Operation::new(
        OperationId::new("sibling-op").unwrap(),
        CorrelationId::new("sibling-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    let child1 = context.child();
    let child2 = context.child();
    child1.cancellation().cancel();
    assert!(child1.cancellation().is_cancelled());
    assert!(!child2.cancellation().is_cancelled());
    assert!(!context.cancellation().is_cancelled());
}

#[test]
fn context_with_deadline_reports_not_expired() {
    let operation = Operation::new(
        OperationId::new("deadline-op").unwrap(),
        CorrelationId::new("deadline-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_secs(60)).unwrap());
    assert!(!context.is_expired());
    assert!(context.deadline().is_some());
}

#[test]
fn expired_context_reports_is_expired() {
    let operation = Operation::new(
        OperationId::new("expired-op").unwrap(),
        CorrelationId::new("expired-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::ZERO).unwrap());
    assert!(context.is_expired());
}

#[test]
fn child_context_inherits_parent_deadline() {
    let operation = Operation::new(
        OperationId::new("deadline-child-op").unwrap(),
        CorrelationId::new("deadline-child-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_secs(30)).unwrap());
    let child = context.child();
    assert_eq!(child.deadline(), context.deadline());
}

#[test]
fn child_with_deadline_takes_earlier_deadline() {
    let operation = Operation::new(
        OperationId::new("earlier-deadline-op").unwrap(),
        CorrelationId::new("earlier-deadline-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_millis(50)).unwrap());
    let child = context.child_with_deadline(Deadline::from_now(Duration::from_secs(10)).unwrap());
    assert!(child.deadline().unwrap().remaining() <= Duration::from_secs(1));
}

#[test]
fn context_without_deadline_is_never_expired() {
    let operation = Operation::new(
        OperationId::new("no-deadline-op").unwrap(),
        CorrelationId::new("no-deadline-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation));
    assert!(context.deadline().is_none());
    assert!(!context.is_expired());
}

#[test]
fn provenance_attributes_propagate_to_child() {
    let operation = Operation::new(
        OperationId::new("provenance-op").unwrap(),
        CorrelationId::new("provenance-corr").unwrap(),
    );
    let provenance = ProvenanceContext::new()
        .with_attribute("source", "engine")
        .with_attribute("version", "1.0");
    let context = EngineContext::new(OperationContext::new(operation)).with_provenance(provenance);
    let child = context.child();
    assert_eq!(child.provenance().attribute("source"), Some("engine"));
    assert_eq!(child.provenance().attribute("version"), Some("1.0"));
}

#[test]
fn security_context_propagates_to_child() {
    let operation = Operation::new(
        OperationId::new("security-op").unwrap(),
        CorrelationId::new("security-corr").unwrap(),
    );
    let context =
        EngineContext::new(OperationContext::new(operation)).with_security(SecurityContext::new());
    let child = context.child();
    assert_eq!(child.security(), context.security());
}

#[test]
fn expired_context_produces_error() {
    let operation = Operation::new(
        OperationId::new("expiry-op").unwrap(),
        CorrelationId::new("expiry-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::ZERO).unwrap());
    let definition = ErrorDefinition::new(
        ErrorCode::new("CORE.EXECUTION.001").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        Version::new(1, 0, 0),
        ErrorClass::Execution,
        Severity::Error,
        "Execution deadline expired",
        Retryability::NonRetryable,
    )
    .unwrap();
    let error = context.expiration_error(&definition);
    assert!(error.is_some());
    assert_eq!(error.unwrap().code.as_str(), "CORE.EXECUTION.001");
}

#[test]
fn non_expired_context_returns_none_for_expiration_error() {
    let operation = Operation::new(
        OperationId::new("not-expired-op").unwrap(),
        CorrelationId::new("not-expired-corr").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_secs(60)).unwrap());
    let definition = ErrorDefinition::new(
        ErrorCode::new("CORE.EXECUTION.001").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        Version::new(1, 0, 0),
        ErrorClass::Execution,
        Severity::Error,
        "Execution deadline expired",
        Retryability::NonRetryable,
    )
    .unwrap();
    assert!(context.expiration_error(&definition).is_none());
}

#[test]
fn operation_context_can_include_attempt_identity() {
    let operation = Operation::new(
        OperationId::new("attempt-op").unwrap(),
        CorrelationId::new("attempt-corr").unwrap(),
    );
    let context = OperationContext::new(operation).for_attempt(
        NodeId::new("node-1").unwrap(),
        AttemptId::new("attempt-1").unwrap(),
    );
    assert_eq!(context.operation.id.as_str(), "attempt-op");
    assert_eq!(context.node_id.as_ref().unwrap().as_str(), "node-1");
    assert_eq!(context.attempt_id.as_ref().unwrap().as_str(), "attempt-1");
}
