//! Immutable idempotency records describing the known state of one logical
//! submission.
//!
//! An [`IdempotencyRecord`] combines the validated idempotency identity with the
//! logical [`crate::identity::OperationId`], applicable capability identity,
//! current idempotency state, any recorded outcome, an opaque result reference,
//! and explicit expiry metadata.
//!
//! This module stores record information only. Duplicate lookup, reservation,
//! state transitions, retention cleanup, reconciliation, and execution policy
//! remain owned by the idempotency state layer and higher Core systems.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::idempotency::key::IdempotencyIdentity;
use crate::idempotency::state::IdempotencyState;
use crate::identity::{CapabilityId, OperationId};
use crate::status::{ErrorReference, Status};

/// A recorded technical outcome associated with an idempotency record.
///
/// The outcome is intentionally reference-based rather than carrying an
/// arbitrary result or error body. This allows completed operations to retain
/// a stable logical outcome without requiring indefinite in-memory retention of
/// large responses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedOutcome {
    status: Status,
    error_reference: Option<ErrorReference>,
}

impl RecordedOutcome {
    /// Creates a recorded outcome from the shared Core status and an optional
    /// stable error reference.
    pub fn new(status: Status, error_reference: Option<ErrorReference>) -> Self {
        Self {
            status,
            error_reference,
        }
    }

    /// Returns the technical status recorded for the logical operation.
    pub const fn status(&self) -> Status {
        self.status
    }

    /// Returns the stable error reference when one was recorded.
    pub fn error_reference(&self) -> Option<&ErrorReference> {
        self.error_reference.as_ref()
    }
}

impl Serialize for RecordedOutcome {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut record = serializer.serialize_struct("RecordedOutcome", 2)?;
        record.serialize_field("status", &self.status)?;
        record.serialize_field(
            "error_reference",
            &self.error_reference.as_ref().map(ErrorReference::as_str),
        )?;
        record.end()
    }
}

impl<'de> Deserialize<'de> for RecordedOutcome {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RecordedOutcomeWire {
            status: Status,
            error_reference: Option<String>,
        }

        let value = RecordedOutcomeWire::deserialize(deserializer)?;
        let error_reference =
            match value.error_reference {
                Some(reference) => Some(ErrorReference::new(reference).ok_or_else(|| {
                    serde::de::Error::custom("an error reference must not be empty")
                })?),
                None => None,
            };

        Ok(Self::new(value.status, error_reference))
    }
}

/// Immutable idempotency information for one logical submission.
///
/// The record contains enough information for higher idempotency layers to
/// recognize and safely handle repeated submissions without embedding duplicate
/// detection or storage behavior into this value object.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IdempotencyRecord {
    identity: IdempotencyIdentity,
    operation_id: OperationId,
    capability_id: Option<CapabilityId>,
    state: IdempotencyState,
    outcome: Option<RecordedOutcome>,
    result_reference: Option<String>,
    expires_at: u64,
}

impl IdempotencyRecord {
    /// Creates an immutable idempotency record.
    ///
    /// `expires_at` is an absolute Unix-epoch timestamp in seconds. The record
    /// does not calculate expiry or consult a clock; the applicable retention
    /// policy supplies the value.
    pub fn new(
        identity: IdempotencyIdentity,
        operation_id: OperationId,
        capability_id: Option<CapabilityId>,
        state: IdempotencyState,
        outcome: Option<RecordedOutcome>,
        result_reference: Option<String>,
        expires_at: u64,
    ) -> Self {
        Self {
            identity,
            operation_id,
            capability_id,
            state,
            outcome,
            result_reference,
            expires_at,
        }
    }

    /// Returns the composite idempotency identity.
    pub fn identity(&self) -> &IdempotencyIdentity {
        &self.identity
    }

    /// Returns the logical operation identity associated with this record.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the applicable capability identity, when one exists.
    pub fn capability_id(&self) -> Option<&CapabilityId> {
        self.capability_id.as_ref()
    }

    /// Returns the current idempotency state.
    pub fn state(&self) -> &IdempotencyState {
        &self.state
    }

    /// Returns the recorded outcome, when one is available.
    pub fn outcome(&self) -> Option<&RecordedOutcome> {
        self.outcome.as_ref()
    }

    /// Returns the opaque result or durable-result reference, when available.
    ///
    /// The reference is not interpreted by Core's idempotency record layer.
    pub fn result_reference(&self) -> Option<&str> {
        self.result_reference.as_deref()
    }

    /// Returns the absolute Unix-epoch expiry timestamp in seconds.
    pub const fn expires_at(&self) -> u64 {
        self.expires_at
    }

    /// Returns whether the record has reached its explicit expiry timestamp.
    ///
    /// Expiration is a pure observation. It does not mutate the record or imply
    /// that the logical operation never occurred.
    pub const fn is_expired(&self, now: u64) -> bool {
        now >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CapabilityId, OperationId};

    fn operation_id() -> OperationId {
        OperationId::new("operation-1").unwrap()
    }

    fn capability_id() -> CapabilityId {
        CapabilityId::new("capability-1").unwrap()
    }

    fn identity() -> IdempotencyIdentity {
        IdempotencyIdentity::new(
            crate::idempotency::key::IdempotencyScope::new("service-a").unwrap(),
            crate::idempotency::key::IdempotencyKey::new("request-1").unwrap(),
        )
    }

    #[test]
    fn recorded_outcome_preserves_status_and_error_reference() {
        let error = ErrorReference::new("error-event-1").unwrap();
        let outcome = RecordedOutcome::new(Status::Failure, Some(error.clone()));

        assert_eq!(outcome.status(), Status::Failure);
        assert_eq!(outcome.error_reference(), Some(&error));
    }

    #[test]
    fn recorded_outcome_supports_success_without_error_reference() {
        let outcome = RecordedOutcome::new(Status::Success, None);

        assert_eq!(outcome.status(), Status::Success);
        assert!(outcome.error_reference().is_none());
    }

    #[test]
    fn recorded_outcome_serialization_round_trips_error_reference() {
        let outcome = RecordedOutcome::new(
            Status::Failure,
            Some(ErrorReference::new("error-event-2").unwrap()),
        );

        let encoded = serde_json::to_string(&outcome).unwrap();
        let decoded: RecordedOutcome = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, outcome);
    }

    #[test]
    fn recorded_outcome_deserialization_rejects_empty_error_reference() {
        let result = serde_json::from_str::<RecordedOutcome>(
            r#"{"status":"Failure","error_reference":"   "}"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn record_preserves_all_supplied_fields() {
        let record = IdempotencyRecord::new(
            identity(),
            operation_id(),
            Some(capability_id()),
            IdempotencyState::default(),
            Some(RecordedOutcome::new(Status::Success, None)),
            Some("result://operation-1".to_owned()),
            500,
        );

        assert_eq!(record.identity().key().as_str(), "request-1");
        assert_eq!(record.operation_id().as_str(), "operation-1");
        assert_eq!(
            record.capability_id().map(CapabilityId::as_str),
            Some("capability-1")
        );
        assert_eq!(
            record.outcome().map(RecordedOutcome::status),
            Some(Status::Success)
        );
        assert_eq!(record.result_reference(), Some("result://operation-1"));
        assert_eq!(record.expires_at(), 500);
    }

    #[test]
    fn record_supports_missing_optional_fields() {
        let record = IdempotencyRecord::new(
            identity(),
            operation_id(),
            None,
            IdempotencyState::default(),
            None,
            None,
            1_000,
        );

        assert!(record.capability_id().is_none());
        assert!(record.outcome().is_none());
        assert!(record.result_reference().is_none());
    }

    #[test]
    fn record_expiration_is_boundary_inclusive() {
        let record = IdempotencyRecord::new(
            identity(),
            operation_id(),
            None,
            IdempotencyState::default(),
            None,
            None,
            1_000,
        );

        assert!(!record.is_expired(999));
        assert!(record.is_expired(1_000));
        assert!(record.is_expired(1_001));
    }

    #[test]
    fn record_serialization_round_trips() {
        let record = IdempotencyRecord::new(
            identity(),
            operation_id(),
            Some(capability_id()),
            IdempotencyState::default(),
            Some(RecordedOutcome::new(
                Status::Failure,
                Some(ErrorReference::new("error-event-3").unwrap()),
            )),
            Some("artifact://result-1".to_owned()),
            10_000,
        );

        let encoded = serde_json::to_string(&record).unwrap();
        let decoded: IdempotencyRecord = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn cloned_records_remain_equal() {
        let record = IdempotencyRecord::new(
            identity(),
            operation_id(),
            None,
            IdempotencyState::default(),
            Some(RecordedOutcome::new(Status::Cancelled, None)),
            None,
            25,
        );

        assert_eq!(record.clone(), record);
    }
}
