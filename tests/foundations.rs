//! Integration tests for the Phase 1 public surface.
//!
//! These tests exercise `nizaam_core::prelude::*` end-to-end across the
//! identity, status, and operation modules. They live in the `tests/`
//! directory so they only see the public API.

use nizaam_core::prelude::*;
use std::str::FromStr;

// -------------------------------------------------------------------------
// Identity types from the prelude
// -------------------------------------------------------------------------

#[test]
fn prelude_exposes_all_phase_one_identity_types() {
    // Construct each identity type and confirm its as_str round-trip.
    assert_eq!(ArtifactId::new("a1").unwrap().as_str(), "a1");
    assert_eq!(AttemptId::new("at1").unwrap().as_str(), "at1");
    assert_eq!(CapabilityId::new("c1").unwrap().as_str(), "c1");
    assert_eq!(ContractId::new("ct1").unwrap().as_str(), "ct1");
    assert_eq!(CorrelationId::new("co1").unwrap().as_str(), "co1");
    assert_eq!(EngineId::new("e1").unwrap().as_str(), "e1");
    assert_eq!(EngineInstanceId::new("ei1").unwrap().as_str(), "ei1");
    assert_eq!(MessageId::new("m1").unwrap().as_str(), "m1");
    assert_eq!(NodeId::new("n1").unwrap().as_str(), "n1");
    assert_eq!(OperationId::new("op1").unwrap().as_str(), "op1");
    assert_eq!(PlanId::new("pl1").unwrap().as_str(), "pl1");
}

#[test]
fn prelude_identity_types_reject_empty_values() {
    assert!(ArtifactId::new("").is_err());
    assert!(AttemptId::new("").is_err());
    assert!(CapabilityId::new("").is_err());
    assert!(ContractId::new("").is_err());
    assert!(CorrelationId::new("").is_err());
    assert!(EngineId::new("").is_err());
    assert!(EngineInstanceId::new("").is_err());
    assert!(MessageId::new("").is_err());
    assert!(NodeId::new("").is_err());
    assert!(OperationId::new("").is_err());
    assert!(PlanId::new("").is_err());
}

#[test]
fn prelude_identity_types_round_trip_via_from_str() {
    assert_eq!(ArtifactId::from_str("a1").unwrap().as_str(), "a1");
    assert_eq!(AttemptId::from_str("at1").unwrap().as_str(), "at1");
    assert_eq!(CapabilityId::from_str("c1").unwrap().as_str(), "c1");
    assert_eq!(ContractId::from_str("ct1").unwrap().as_str(), "ct1");
    assert_eq!(CorrelationId::from_str("co1").unwrap().as_str(), "co1");
    assert_eq!(EngineId::from_str("e1").unwrap().as_str(), "e1");
    assert_eq!(EngineInstanceId::from_str("ei1").unwrap().as_str(), "ei1");
    assert_eq!(MessageId::from_str("m1").unwrap().as_str(), "m1");
    assert_eq!(NodeId::from_str("n1").unwrap().as_str(), "n1");
    assert_eq!(OperationId::from_str("op1").unwrap().as_str(), "op1");
    assert_eq!(PlanId::from_str("pl1").unwrap().as_str(), "pl1");
}

#[test]
fn prelude_identity_types_implement_display() {
    assert_eq!(ArtifactId::new("a1").unwrap().to_string(), "a1");
    assert_eq!(OperationId::new("op1").unwrap().to_string(), "op1");
    assert_eq!(EngineId::new("e1").unwrap().to_string(), "e1");
}

// -------------------------------------------------------------------------
// Status / shared result primitives
// -------------------------------------------------------------------------

#[test]
fn status_variants_distinguish_outcomes() {
    // The four variants exist and are distinct.
    assert_ne!(Status::Success, Status::Failure);
    assert_ne!(Status::Cancelled, Status::TimedOut);
    assert_eq!(Status::Success, Status::Success);

    assert_eq!(Retryability::Retryable, Retryability::Retryable);
    assert_ne!(Retryability::Retryable, Retryability::NonRetryable);

    assert_eq!(Compatibility::Compatible, Compatibility::Compatible);
    assert_ne!(Compatibility::Compatible, Compatibility::Incompatible);
    assert_ne!(Compatibility::Compatible, Compatibility::Unknown);
}

#[test]
fn error_reference_requires_a_value() {
    assert!(ErrorReference::new("").is_none());
    assert!(ErrorReference::new("   ").is_none());
    let err = ErrorReference::new("error-event-1").unwrap();
    assert_eq!(err.as_str(), "error-event-1");
}

#[test]
fn artifact_reference_preserves_artifact_id_and_optional_version() {
    let id = ArtifactId::new("artifact-1").unwrap();
    let without_version = ArtifactReference {
        artifact_id: id.clone(),
        version: None,
    };
    let with_version = ArtifactReference {
        artifact_id: id.clone(),
        version: Some("v1.0.0".to_string()),
    };

    assert_eq!(without_version.artifact_id, id);
    assert_eq!(without_version.version, None);
    assert_eq!(with_version.version.as_deref(), Some("v1.0.0"));
}

// -------------------------------------------------------------------------
// Operation and OperationContext composition
// -------------------------------------------------------------------------

#[test]
fn a_consumer_can_construct_a_domain_agnostic_operation_context() {
    let operation = Operation::new(
        OperationId::new("operation-42").unwrap(),
        CorrelationId::new("correlation-42").unwrap(),
    )
    .with_plan(PlanId::new("plan-1").unwrap());

    let context = OperationContext::new(operation).for_attempt(
        NodeId::new("node-2").unwrap(),
        AttemptId::new("attempt-5").unwrap(),
    );

    assert_eq!(context.operation.plan_id.unwrap().as_str(), "plan-1");
    assert_eq!(context.node_id.unwrap().as_str(), "node-2");
}

#[test]
fn operation_context_without_for_attempt_has_no_node_or_attempt() {
    let operation = Operation::new(
        OperationId::new("op-1").unwrap(),
        CorrelationId::new("corr-1").unwrap(),
    );
    let context = OperationContext::new(operation);

    assert!(context.node_id.is_none());
    assert!(context.attempt_id.is_none());
}

#[test]
fn operation_supports_parent_chain() {
    let parent = Operation::new(
        OperationId::new("parent-op").unwrap(),
        CorrelationId::new("parent-corr").unwrap(),
    );
    let child = Operation::new(
        OperationId::new("child-op").unwrap(),
        CorrelationId::new("parent-corr").unwrap(),
    )
    .with_parent(parent.id.clone())
    .with_plan(PlanId::new("shared-plan").unwrap());

    assert_eq!(child.parent_operation_id.unwrap().as_str(), "parent-op");
    assert_eq!(
        child.correlation_id.as_str(),
        parent.correlation_id.as_str()
    );
    assert_eq!(child.plan_id.unwrap().as_str(), "shared-plan");
}
