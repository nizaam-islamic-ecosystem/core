//! Idempotency support for Nizaam Core.
//!
//! The idempotency module is divided into three layers:
//!
//! ```text
//! key.rs
//!     -> validated scope/key identity
//! record.rs
//!     -> immutable logical idempotency record
//! state.rs
//!     -> logical lifecycle and state coordination
//! ```
//!
//! An idempotency identity is the composite of a contract-specific scope and
//! key. The identity is associated with an [`IdempotencyRecord`], which carries
//! the logical [`crate::identity::OperationId`], optional capability identity,
//! current state, recorded outcome, result reference, and expiry metadata.
//!
//! [`IdempotencyStateStore`] coordinates reservations and logical state
//! transitions. Retries remain owned by the retry subsystem: a retry creates a
//! new attempt under the same logical operation rather than creating a new
//! idempotency identity or replacing the operation identity.
//!
//! This module does not provide distributed persistence, retention cleanup,
//! reconciliation workflows, speculative execution, or retry policy.

pub mod key;
pub mod record;
pub mod state;

pub use key::{IdempotencyIdentity, IdempotencyKey, IdempotencyScope};
pub use record::{IdempotencyRecord, RecordedOutcome};
pub use state::{
    IdempotencyLookup, IdempotencyReservation, IdempotencyState, IdempotencyStateError,
    IdempotencyStateStore, IdempotencyStateTransitionError, can_transition,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CapabilityId, OperationId};
    use crate::status::{ErrorReference, Status};

    fn identity(scope: &str, key: &str) -> IdempotencyIdentity {
        IdempotencyIdentity::new(
            IdempotencyScope::new(scope).unwrap(),
            IdempotencyKey::new(key).unwrap(),
        )
    }

    fn operation(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn capability(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    fn in_flight_record(
        scope: &str,
        key: &str,
        operation_id: &str,
        capability_id: Option<&str>,
        expires_at: u64,
    ) -> IdempotencyRecord {
        IdempotencyRecord::new(
            identity(scope, key),
            operation(operation_id),
            capability_id.map(capability),
            IdempotencyState::InFlight,
            None,
            None,
            expires_at,
        )
    }

    #[test]
    fn identity_and_record_form_one_logical_submission() {
        let identity = identity("service-a", "request-1");
        let operation = operation("operation-1");
        let capability = capability("capability-1");

        let record = IdempotencyRecord::new(
            identity.clone(),
            operation.clone(),
            Some(capability.clone()),
            IdempotencyState::InFlight,
            None,
            None,
            1_000,
        );

        assert_eq!(record.identity(), &identity);
        assert_eq!(record.operation_id(), &operation);
        assert_eq!(record.capability_id(), Some(&capability));
        assert_eq!(*record.state(), IdempotencyState::InFlight);
    }

    #[test]
    fn reservation_flow_connects_identity_record_and_state_store() {
        let store = IdempotencyStateStore::new();
        let record = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);

        let reservation = store.reserve(record.clone(), 500).unwrap();

        assert_eq!(reservation, IdempotencyReservation::Created(record.clone()));
        assert_eq!(store.get(record.identity()).unwrap(), Some(record));
    }

    #[test]
    fn repeated_submission_with_same_logical_identity_is_duplicate() {
        let store = IdempotencyStateStore::new();
        let first = in_flight_record(
            "service-a",
            "request-1",
            "operation-1",
            Some("cap-1"),
            1_000,
        );
        let second = in_flight_record(
            "service-a",
            "request-1",
            "operation-1",
            Some("cap-1"),
            2_000,
        );

        store.reserve(first.clone(), 500).unwrap();

        assert_eq!(
            store.reserve(second, 600).unwrap(),
            IdempotencyReservation::Duplicate(first)
        );
    }

    #[test]
    fn reused_identity_for_different_logical_operation_is_conflict() {
        let store = IdempotencyStateStore::new();
        let first = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);
        let second = in_flight_record("service-a", "request-1", "operation-2", None, 2_000);

        store.reserve(first.clone(), 500).unwrap();

        assert_eq!(
            store.reserve(second, 600).unwrap(),
            IdempotencyReservation::Conflict(first)
        );
    }

    #[test]
    fn same_key_in_different_scopes_is_independent() {
        let store = IdempotencyStateStore::new();
        let first = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);
        let second = in_flight_record("service-b", "request-1", "operation-2", None, 1_000);

        assert!(matches!(
            store.reserve(first, 500).unwrap(),
            IdempotencyReservation::Created(_)
        ));
        assert!(matches!(
            store.reserve(second, 500).unwrap(),
            IdempotencyReservation::Created(_)
        ));
        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn successful_completion_preserves_identity_and_records_result() {
        let store = IdempotencyStateStore::new();
        let initial = in_flight_record(
            "service-a",
            "request-1",
            "operation-1",
            Some("cap-1"),
            1_000,
        );

        store.reserve(initial.clone(), 500).unwrap();

        let updated = store
            .transition(
                initial.identity(),
                IdempotencyState::Succeeded,
                Some(RecordedOutcome::new(Status::Success, None)),
                Some("result://operation-1".to_owned()),
            )
            .unwrap();

        assert_eq!(updated.identity(), initial.identity());
        assert_eq!(updated.operation_id(), initial.operation_id());
        assert_eq!(updated.capability_id(), initial.capability_id());
        assert_eq!(*updated.state(), IdempotencyState::Succeeded);
        assert_eq!(
            updated.outcome().map(RecordedOutcome::status),
            Some(Status::Success)
        );
        assert_eq!(updated.result_reference(), Some("result://operation-1"));
        assert_eq!(updated.expires_at(), initial.expires_at());
    }

    #[test]
    fn failed_completion_records_error_reference() {
        let store = IdempotencyStateStore::new();
        let initial = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);
        let error = ErrorReference::new("error-event-1").unwrap();

        store.reserve(initial.clone(), 500).unwrap();

        let updated = store
            .transition(
                initial.identity(),
                IdempotencyState::Failed,
                Some(RecordedOutcome::new(Status::Failure, Some(error.clone()))),
                None,
            )
            .unwrap();

        assert_eq!(*updated.state(), IdempotencyState::Failed);
        assert_eq!(
            updated.outcome().map(RecordedOutcome::status),
            Some(Status::Failure)
        );
        assert_eq!(
            updated.outcome().and_then(RecordedOutcome::error_reference),
            Some(&error)
        );
        assert!(updated.result_reference().is_none());
    }

    #[test]
    fn unknown_outcome_can_be_reconciled_back_to_in_flight() {
        let store = IdempotencyStateStore::new();
        let initial = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);

        store.reserve(initial.clone(), 500).unwrap();

        let unknown = store
            .transition(initial.identity(), IdempotencyState::Unknown, None, None)
            .unwrap();
        assert_eq!(*unknown.state(), IdempotencyState::Unknown);
        assert!(unknown.outcome().is_none());
        assert!(unknown.result_reference().is_none());

        let resumed = store
            .transition(initial.identity(), IdempotencyState::InFlight, None, None)
            .unwrap();

        assert_eq!(*resumed.state(), IdempotencyState::InFlight);
        assert_eq!(resumed.identity(), initial.identity());
        assert_eq!(resumed.operation_id(), initial.operation_id());
    }

    #[test]
    fn terminal_completion_cannot_reopen_logical_execution() {
        let store = IdempotencyStateStore::new();
        let initial = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);

        store.reserve(initial.clone(), 500).unwrap();
        store
            .transition(
                initial.identity(),
                IdempotencyState::Succeeded,
                Some(RecordedOutcome::new(Status::Success, None)),
                None,
            )
            .unwrap();

        let result = store.transition(initial.identity(), IdempotencyState::InFlight, None, None);

        assert!(matches!(
            result,
            Err(IdempotencyStateError::InvalidTransition(_))
        ));
    }

    #[test]
    fn expiry_is_observed_by_lookup_and_reservation_without_silent_replacement() {
        let store = IdempotencyStateStore::new();
        let existing = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);
        let replacement = in_flight_record("service-a", "request-1", "operation-2", None, 2_000);

        store.reserve(existing.clone(), 500).unwrap();

        assert_eq!(
            store.lookup(existing.identity(), 999).unwrap(),
            IdempotencyLookup::Present(existing.clone())
        );
        assert_eq!(
            store.lookup(existing.identity(), 1_000).unwrap(),
            IdempotencyLookup::Expired(existing.clone())
        );
        assert_eq!(
            store.reserve(replacement, 1_000).unwrap(),
            IdempotencyReservation::Expired(existing)
        );
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn complete_record_serialization_round_trips_across_the_module_boundary() {
        let record = IdempotencyRecord::new(
            identity("service-a", "request-1"),
            operation("operation-1"),
            Some(capability("cap-1")),
            IdempotencyState::Failed,
            Some(RecordedOutcome::new(
                Status::Failure,
                Some(ErrorReference::new("error-event-1").unwrap()),
            )),
            Some("artifact://result-1".to_owned()),
            10_000,
        );

        let encoded = serde_json::to_string(&record).unwrap();
        let decoded: IdempotencyRecord = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn cloned_state_stores_share_authoritative_idempotency_state() {
        let first = IdempotencyStateStore::new();
        let second = first.clone();
        let record = in_flight_record("service-a", "request-1", "operation-1", None, 1_000);

        first.reserve(record.clone(), 500).unwrap();

        assert_eq!(
            second.lookup(record.identity(), 600).unwrap(),
            IdempotencyLookup::Present(record)
        );
    }
}
