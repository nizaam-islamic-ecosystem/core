//! Integration tests for the Phase 13 idempotency boundary.
//!
//! These tests intentionally exercise idempotency together with other Core
//! modules. Local identity/record/state behavior belongs to the implementation
//! unit tests, while key/record/state composition belongs to
//! `src/idempotency/mod.rs`.

use nizaam_core::{
    artifact::{ArtifactReference, VersionSelector},
    capability::CapabilityDefinition,
    contracts::Version,
    error::{
        ErrorClass, ErrorCode, ErrorContext, ErrorDefinition, ErrorOwner, ErrorSystem, Severity,
    },
    idempotency::{
        IdempotencyIdentity, IdempotencyKey, IdempotencyRecord, IdempotencyReservation,
        IdempotencyScope, IdempotencyState, IdempotencyStateStore, RecordedOutcome,
    },
    identity::{ArtifactId, CapabilityId, CorrelationId, OperationId},
    observability::CorrelationContext,
    operation::{Operation, OperationContext},
    runtime::EngineContext,
    status::{Retryability, Status},
};

fn operation_id(value: &str) -> OperationId {
    OperationId::new(value).unwrap()
}

fn operation(value: &str) -> Operation {
    Operation::new(
        operation_id(value),
        CorrelationId::new(format!("correlation-{value}")).unwrap(),
    )
}

fn engine_context(value: &str) -> EngineContext {
    EngineContext::new(OperationContext::new(operation(value)))
}

fn idempotency_identity(scope: &str, key: &str) -> IdempotencyIdentity {
    IdempotencyIdentity::new(
        IdempotencyScope::new(scope).unwrap(),
        IdempotencyKey::new(key).unwrap(),
    )
}

fn record(
    identity: IdempotencyIdentity,
    operation_id: OperationId,
    capability_id: Option<CapabilityId>,
) -> IdempotencyRecord {
    IdempotencyRecord::new(
        identity,
        operation_id,
        capability_id,
        IdempotencyState::InFlight,
        None,
        None,
        1_000,
    )
}

fn error_definition(retryability: Retryability) -> ErrorDefinition {
    ErrorDefinition::new(
        ErrorCode::new("CORE.EXECUTION.101").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        Version::new(1, 0, 0),
        ErrorClass::Execution,
        Severity::Error,
        "idempotency integration test failure",
        retryability,
    )
    .unwrap()
}

#[test]
fn runtime_operation_identity_flows_into_idempotency_record() {
    let context = engine_context("idempotency-operation-1");
    let identity = idempotency_identity("service-a", "request-1");

    let record = record(identity, context.operation().operation.id.clone(), None);

    assert_eq!(record.operation_id(), &context.operation().operation.id);
    assert_eq!(record.identity().scope().as_str(), "service-a");
    assert_eq!(record.identity().key().as_str(), "request-1");
}

#[test]
fn capability_identity_participates_in_idempotency_duplicate_and_conflict_detection() {
    let first_capability = CapabilityDefinition::new(
        CapabilityId::new("capability.lookup").unwrap(),
        nizaam_core::identity::EngineId::new("engine-a").unwrap(),
        "Lookup capability",
    )
    .unwrap();

    let second_capability = CapabilityDefinition::new(
        CapabilityId::new("capability.update").unwrap(),
        nizaam_core::identity::EngineId::new("engine-a").unwrap(),
        "Update capability",
    )
    .unwrap();

    let operation_id = operation_id("idempotency-operation-2");
    let identity = idempotency_identity("service-a", "request-2");
    let store = IdempotencyStateStore::new();

    let first = record(
        identity.clone(),
        operation_id.clone(),
        Some(first_capability.capability_id().clone()),
    );

    assert_eq!(
        store.reserve(first.clone(), 100).unwrap(),
        IdempotencyReservation::Created(first.clone())
    );

    let duplicate = record(
        identity.clone(),
        operation_id.clone(),
        Some(first_capability.capability_id().clone()),
    );

    assert_eq!(
        store.reserve(duplicate, 200).unwrap(),
        IdempotencyReservation::Duplicate(first.clone())
    );

    let conflicting_capability = record(
        identity,
        operation_id,
        Some(second_capability.capability_id().clone()),
    );

    assert_eq!(
        store.reserve(conflicting_capability, 200).unwrap(),
        IdempotencyReservation::Conflict(first)
    );
}

#[test]
fn error_system_failure_becomes_recorded_idempotent_failure() {
    let operation = operation("idempotency-operation-3");
    let operation_id = operation.id.clone();
    let identity = idempotency_identity("service-a", "request-3");

    let mut error_system = ErrorSystem::new();
    let definition = error_definition(Retryability::NonRetryable);
    let error_code = definition.code.clone();

    error_system.register(definition).unwrap();

    let error_event = error_system
        .instance()
        .report(
            &error_code,
            ErrorContext::new(OperationContext::new(operation)),
        )
        .unwrap();

    let reference = error_event.reference();

    let store = IdempotencyStateStore::new();
    let initial = record(identity.clone(), operation_id, None);

    store.reserve(initial, 100).unwrap();

    let completed = store
        .transition(
            &identity,
            IdempotencyState::Failed,
            Some(RecordedOutcome::new(
                Status::Failure,
                Some(reference.clone()),
            )),
            None,
        )
        .unwrap();

    assert_eq!(*completed.state(), IdempotencyState::Failed);
    assert_eq!(completed.outcome().unwrap().status(), Status::Failure);
    assert_eq!(
        completed.outcome().unwrap().error_reference(),
        Some(&reference)
    );
}

#[test]
fn completed_duplicate_returns_the_established_successful_outcome() {
    let operation_id = operation_id("idempotency-operation-4");
    let identity = idempotency_identity("service-a", "request-4");
    let store = IdempotencyStateStore::new();

    let initial = record(identity.clone(), operation_id.clone(), None);

    store.reserve(initial.clone(), 100).unwrap();

    let completed = store
        .transition(
            &identity,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some("result://operation-4".to_owned()),
        )
        .unwrap();

    let repeated_submission = record(identity, operation_id, None);

    let duplicate = store.reserve(repeated_submission, 200).unwrap();

    assert_eq!(
        duplicate,
        IdempotencyReservation::Duplicate(completed.clone())
    );

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        unreachable!();
    };

    assert_eq!(*existing.state(), IdempotencyState::Succeeded);
    assert_eq!(
        existing.outcome().map(RecordedOutcome::status),
        Some(Status::Success)
    );
    assert_eq!(existing.result_reference(), Some("result://operation-4"));
}

#[test]
fn failed_duplicate_returns_the_established_failure() {
    let operation = operation("idempotency-operation-5");
    let identity = idempotency_identity("service-a", "request-5");

    let mut error_system = ErrorSystem::new();
    let definition = error_definition(Retryability::NonRetryable);
    let error_code = definition.code.clone();

    error_system.register(definition).unwrap();

    let error_event = error_system
        .instance()
        .report(
            &error_code,
            ErrorContext::new(OperationContext::new(operation.clone())),
        )
        .unwrap();

    let reference = error_event.reference();

    let store = IdempotencyStateStore::new();

    let initial = record(identity.clone(), operation.id.clone(), None);

    store.reserve(initial, 100).unwrap();

    let failed = store
        .transition(
            &identity,
            IdempotencyState::Failed,
            Some(RecordedOutcome::new(
                Status::Failure,
                Some(reference.clone()),
            )),
            None,
        )
        .unwrap();

    let repeated_submission = record(identity, operation.id, None);

    let duplicate = store.reserve(repeated_submission, 200).unwrap();

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        panic!("expected the repeated failed submission to be a duplicate");
    };

    assert_eq!(existing, failed);
    assert_eq!(*existing.state(), IdempotencyState::Failed);
    assert_eq!(
        existing.outcome().unwrap().error_reference(),
        Some(&reference)
    );
}

#[test]
fn unknown_outcome_remains_distinct_and_is_not_replaced() {
    let context = engine_context("idempotency-operation-6");
    let identity = idempotency_identity("service-a", "request-6");
    let operation_id = context.operation().operation.id.clone();

    let store = IdempotencyStateStore::new();

    let initial = record(identity.clone(), operation_id.clone(), None);

    store.reserve(initial, 100).unwrap();

    let unknown = store
        .transition(&identity, IdempotencyState::Unknown, None, None)
        .unwrap();

    assert_eq!(*unknown.state(), IdempotencyState::Unknown);
    assert!(unknown.outcome().is_none());
    assert!(unknown.result_reference().is_none());

    let repeated_submission = record(identity, operation_id, None);

    let duplicate = store.reserve(repeated_submission, 200).unwrap();

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        panic!("expected the unknown logical submission to remain represented");
    };

    assert_eq!(existing, unknown);
    assert_eq!(*existing.state(), IdempotencyState::Unknown);
}

#[test]
fn idempotency_and_observability_share_operation_identity() {
    let context = engine_context("idempotency-operation-7");

    let record = record(
        idempotency_identity("service-a", "request-7"),
        context.operation().operation.id.clone(),
        None,
    );

    let correlation = CorrelationContext::from_operation_context(context.operation());

    assert_eq!(record.operation_id(), correlation.operation_id().unwrap());
    assert_eq!(record.operation_id().as_str(), "idempotency-operation-7");
}

#[test]
fn artifact_reference_is_preserved_as_opaque_result_reference() {
    let artifact_id = ArtifactId::new("artifact.idempotency.result").unwrap();
    let artifact_reference =
        ArtifactReference::with_selector(artifact_id.clone(), VersionSelector::exact("v1"));

    let serialized_reference = serde_json::to_string(&artifact_reference).unwrap();

    let operation_id = operation_id("idempotency-operation-8");
    let identity = idempotency_identity("service-a", "request-8");
    let store = IdempotencyStateStore::new();

    let initial = record(identity.clone(), operation_id.clone(), None);

    store.reserve(initial, 100).unwrap();

    let completed = store
        .transition(
            &identity,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            Some(serialized_reference.clone()),
        )
        .unwrap();

    assert_eq!(
        completed.result_reference(),
        Some(serialized_reference.as_str())
    );

    let restored_reference: ArtifactReference =
        serde_json::from_str(completed.result_reference().unwrap()).unwrap();

    assert_eq!(restored_reference.artifact_id(), &artifact_id);
    assert!(restored_reference.is_exact());
    assert_eq!(restored_reference.version().as_str(), "v1");

    let duplicate_submission = record(identity, operation_id, None);

    let duplicate = store.reserve(duplicate_submission, 200).unwrap();

    let IdempotencyReservation::Duplicate(existing) = duplicate else {
        panic!("expected the completed artifact-producing submission to be a duplicate");
    };

    assert_eq!(
        existing.result_reference(),
        Some(serialized_reference.as_str())
    );
}
