//! Idempotency state and in-process state management for Nizaam Core.
//!
//! This module owns the logical lifecycle of an idempotent submission and the
//! provider-neutral in-process store used to coordinate duplicate handling.
//!
//! An idempotency state describes the state of the logical action, not the
//! lifecycle of any single retry attempt. Retries remain owned by the retry
//! system and create new attempts under the same logical [`OperationId`].
//!
//! Expiration is deliberately not a lifecycle state. It is represented by the
//! `expires_at` metadata on [`crate::idempotency::record::IdempotencyRecord`]
//! and observed by this module during lookup/reservation.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::idempotency::key::IdempotencyIdentity;
use crate::idempotency::record::{IdempotencyRecord, RecordedOutcome};
use crate::status::Status;

/// Logical state of an idempotent submission.
///
/// `Unknown` means Core cannot currently establish whether the externally
/// observable effect completed. It is intentionally distinct from `Failed` and
/// requires reconciliation or an explicit safe-repeat decision before another
/// side-effecting execution is created.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
    Default,
)]
pub enum IdempotencyState {
    /// The logical action is currently associated with an execution.
    #[default]
    InFlight,
    /// The logical action has a known successful outcome.
    Succeeded,
    /// The logical action has a known failed or timed-out outcome.
    Failed,
    /// The logical action has a known cancellation outcome.
    Cancelled,
    /// The logical outcome cannot currently be established.
    Unknown,
}

impl IdempotencyState {
    /// Returns whether this state represents a completed logical outcome.
    ///
    /// `Unknown` is deliberately not terminal because reconciliation may still
    /// establish the actual outcome or an explicit safe continuation path.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    /// Returns whether the logical action is currently in flight.
    pub const fn is_in_flight(self) -> bool {
        matches!(self, Self::InFlight)
    }

    /// Returns whether the logical outcome is currently unknown.
    pub const fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// Returns whether a logical idempotency state transition is permitted.
    ///
    /// Terminal states remain terminal. `Unknown` may move to a resolved
    /// outcome or back to `InFlight` when a higher-level reconciliation or
    /// safe-continuation mechanism explicitly establishes that transition.
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::InFlight => matches!(
                next,
                Self::InFlight | Self::Succeeded | Self::Failed | Self::Cancelled | Self::Unknown
            ),
            Self::Unknown => matches!(
                next,
                Self::InFlight | Self::Succeeded | Self::Failed | Self::Cancelled | Self::Unknown
            ),
            Self::Succeeded => matches!(next, Self::Succeeded),
            Self::Failed => matches!(next, Self::Failed),
            Self::Cancelled => matches!(next, Self::Cancelled),
        }
    }
}

/// Returns whether an idempotency lifecycle transition is permitted.
pub const fn can_transition(from: IdempotencyState, to: IdempotencyState) -> bool {
    from.can_transition_to(to)
}

/// Error returned when an idempotency state transition is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyStateTransitionError {
    from: IdempotencyState,
    to: IdempotencyState,
}

impl IdempotencyStateTransitionError {
    fn new(from: IdempotencyState, to: IdempotencyState) -> Self {
        Self { from, to }
    }

    /// Returns the state before the rejected transition.
    pub const fn from(&self) -> IdempotencyState {
        self.from
    }

    /// Returns the requested state.
    pub const fn to(&self) -> IdempotencyState {
        self.to
    }
}

impl std::fmt::Display for IdempotencyStateTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid idempotency state transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for IdempotencyStateTransitionError {}

/// Result of looking up an idempotency identity.
///
/// Expiration is kept distinct from absence so that expired state is not
/// silently treated as proof that the earlier logical action never occurred.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyLookup {
    /// No stored record exists for the identity.
    Missing,
    /// A non-expired record exists.
    Present(IdempotencyRecord),
    /// A stored record exists, but its explicit retention window has expired.
    Expired(IdempotencyRecord),
}

/// Result of attempting to reserve a new idempotent logical action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyReservation {
    /// The logical action was newly established in the store.
    Created(IdempotencyRecord),
    /// An existing record already represents the same logical action.
    Duplicate(IdempotencyRecord),
    /// The key/scope identity is already bound to a different logical action.
    Conflict(IdempotencyRecord),
    /// A stored record exists but is outside its explicit retention window.
    Expired(IdempotencyRecord),
}

/// Errors produced by the in-process idempotency state store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyStateError {
    /// The backing lock was poisoned by a panic in another thread.
    LockPoisoned,
    /// A new reservation must begin in the `InFlight` state.
    InvalidInitialState(IdempotencyState),
    /// The requested record does not exist.
    MissingRecord,
    /// The requested lifecycle transition is not permitted.
    InvalidTransition(IdempotencyStateTransitionError),
    /// An `InFlight` or `Unknown` record cannot carry a final recorded outcome
    /// or result reference.
    InvalidInFlightMetadata(IdempotencyState),
    /// A terminal idempotency state must carry a matching recorded outcome.
    InvalidTerminalMetadata(IdempotencyState),
    /// An in-flight or unknown logical action cannot be removed from the store.
    RemovalNotAllowed(IdempotencyState),
}

impl std::fmt::Display for IdempotencyStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LockPoisoned => write!(formatter, "the idempotency state lock is poisoned"),
            Self::InvalidInitialState(state) => write!(
                formatter,
                "a new idempotency reservation must start in InFlight, got {:?}",
                state
            ),
            Self::MissingRecord => write!(formatter, "the idempotency record does not exist"),
            Self::InvalidTransition(error) => error.fmt(formatter),
            Self::InvalidInFlightMetadata(state) => write!(
                formatter,
                "idempotency state {:?} cannot carry a recorded outcome or result reference",
                state
            ),
            Self::InvalidTerminalMetadata(state) => write!(
                formatter,
                "terminal idempotency state {:?} requires a matching recorded outcome",
                state
            ),
            Self::RemovalNotAllowed(state) => write!(
                formatter,
                "idempotency state {:?} cannot be removed while unresolved",
                state
            ),
        }
    }
}

impl std::error::Error for IdempotencyStateError {}

/// Thread-safe provider-neutral in-process idempotency state store.
///
/// The store performs reservation as one write-locked check-and-insert
/// operation, preventing concurrent callers from independently observing a
/// missing identity and both creating competing logical actions.
///
/// It is intentionally an in-process storage mechanism rather than a distributed
/// persistence provider. A durable or distributed backend remains an
/// implementation choice outside this module.
#[derive(Clone, Debug, Default)]
pub struct IdempotencyStateStore {
    records: Arc<RwLock<BTreeMap<IdempotencyIdentity, IdempotencyRecord>>>,
}

impl IdempotencyStateStore {
    /// Creates an empty idempotency state store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of stored idempotency records.
    pub fn len(&self) -> Result<usize, IdempotencyStateError> {
        let records = self
            .records
            .read()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;
        Ok(records.len())
    }

    /// Returns whether the store contains no records.
    pub fn is_empty(&self) -> Result<bool, IdempotencyStateError> {
        Ok(self.len()? == 0)
    }

    /// Returns a stored record without applying retention/expiry interpretation.
    ///
    /// This is a raw state lookup. Use [`Self::lookup`] when the caller needs
    /// expiry-aware semantics.
    pub fn get(
        &self,
        identity: &IdempotencyIdentity,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStateError> {
        let records = self
            .records
            .read()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;

        Ok(records.get(identity).cloned())
    }

    /// Performs an expiry-aware lookup.
    pub fn lookup(
        &self,
        identity: &IdempotencyIdentity,
        now: u64,
    ) -> Result<IdempotencyLookup, IdempotencyStateError> {
        let records = self
            .records
            .read()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;

        let Some(record) = records.get(identity) else {
            return Ok(IdempotencyLookup::Missing);
        };

        if record.is_expired(now) {
            Ok(IdempotencyLookup::Expired(record.clone()))
        } else {
            Ok(IdempotencyLookup::Present(record.clone()))
        }
    }

    /// Atomically reserves a new logical idempotency action.
    ///
    /// A new reservation must begin in `InFlight`. An existing non-expired
    /// record for the same scope/key is classified as either a duplicate or a
    /// conflict by comparing the logical operation and capability identity.
    ///
    /// Expired records are returned as `Expired` and are never silently
    /// overwritten or deleted by reservation.
    pub fn reserve(
        &self,
        record: IdempotencyRecord,
        now: u64,
    ) -> Result<IdempotencyReservation, IdempotencyStateError> {
        if !record.state().is_in_flight() {
            return Err(IdempotencyStateError::InvalidInitialState(*record.state()));
        }

        if record.outcome().is_some() || record.result_reference().is_some() {
            return Err(IdempotencyStateError::InvalidInFlightMetadata(
                *record.state(),
            ));
        }

        let identity = record.identity().clone();

        let mut records = self
            .records
            .write()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;

        let Some(existing) = records.get(&identity) else {
            records.insert(identity, record.clone());
            return Ok(IdempotencyReservation::Created(record));
        };

        if existing.is_expired(now) {
            return Ok(IdempotencyReservation::Expired(existing.clone()));
        }

        if same_logical_action(existing, &record) {
            Ok(IdempotencyReservation::Duplicate(existing.clone()))
        } else {
            Ok(IdempotencyReservation::Conflict(existing.clone()))
        }
    }

    /// Transitions an existing logical idempotency record and replaces the
    /// stored snapshot atomically.
    ///
    /// Identity, logical operation, capability identity, and expiry metadata
    /// remain unchanged. Higher layers decide whether a transition is justified;
    /// this method only enforces the state machine and record consistency rules.
    pub fn transition(
        &self,
        identity: &IdempotencyIdentity,
        next_state: IdempotencyState,
        outcome: Option<RecordedOutcome>,
        result_reference: Option<String>,
    ) -> Result<IdempotencyRecord, IdempotencyStateError> {
        match next_state {
            IdempotencyState::InFlight | IdempotencyState::Unknown => {
                if outcome.is_some() || result_reference.is_some() {
                    return Err(IdempotencyStateError::InvalidInFlightMetadata(next_state));
                }
            }
            IdempotencyState::Succeeded
            | IdempotencyState::Failed
            | IdempotencyState::Cancelled => {
                let Some(outcome) = outcome.as_ref() else {
                    return Err(IdempotencyStateError::InvalidTerminalMetadata(next_state));
                };

                let status_matches = match next_state {
                    IdempotencyState::Succeeded => outcome.status() == Status::Success,
                    IdempotencyState::Failed => {
                        matches!(outcome.status(), Status::Failure | Status::TimedOut)
                    }
                    IdempotencyState::Cancelled => outcome.status() == Status::Cancelled,
                    IdempotencyState::InFlight | IdempotencyState::Unknown => unreachable!(),
                };

                if !status_matches {
                    return Err(IdempotencyStateError::InvalidTerminalMetadata(next_state));
                }
            }
        }

        let mut records = self
            .records
            .write()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;

        let Some(existing) = records.get(identity).cloned() else {
            return Err(IdempotencyStateError::MissingRecord);
        };

        let current_state = *existing.state();
        if !current_state.can_transition_to(next_state) {
            return Err(IdempotencyStateError::InvalidTransition(
                IdempotencyStateTransitionError::new(current_state, next_state),
            ));
        }

        let updated = IdempotencyRecord::new(
            existing.identity().clone(),
            existing.operation_id().clone(),
            existing.capability_id().cloned(),
            next_state,
            outcome,
            result_reference,
            existing.expires_at(),
        );

        records.insert(identity.clone(), updated.clone());
        Ok(updated)
    }

    /// Explicitly removes a resolved terminal record from the store.
    ///
    /// `InFlight` and `Unknown` records are retained until they are resolved;
    /// removal is never performed implicitly by [`Self::reserve`] or
    /// [`Self::lookup`].
    pub fn remove(
        &self,
        identity: &IdempotencyIdentity,
    ) -> Result<Option<IdempotencyRecord>, IdempotencyStateError> {
        let mut records = self
            .records
            .write()
            .map_err(|_| IdempotencyStateError::LockPoisoned)?;

        let Some(existing) = records.get(identity) else {
            return Ok(None);
        };

        if matches!(
            existing.state(),
            IdempotencyState::InFlight | IdempotencyState::Unknown
        ) {
            return Err(IdempotencyStateError::RemovalNotAllowed(*existing.state()));
        }

        Ok(records.remove(identity))
    }
}

fn same_logical_action(existing: &IdempotencyRecord, incoming: &IdempotencyRecord) -> bool {
    existing.operation_id() == incoming.operation_id()
        && existing.capability_id() == incoming.capability_id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idempotency::key::{IdempotencyIdentity, IdempotencyKey, IdempotencyScope};
    use crate::identity::{CapabilityId, OperationId};
    use crate::status::Status;

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

    fn record(
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
    fn default_state_is_in_flight() {
        assert_eq!(IdempotencyState::default(), IdempotencyState::InFlight);
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(IdempotencyState::Succeeded.is_terminal());
        assert!(IdempotencyState::Failed.is_terminal());
        assert!(IdempotencyState::Cancelled.is_terminal());

        assert!(!IdempotencyState::InFlight.is_terminal());
        assert!(!IdempotencyState::Unknown.is_terminal());
    }

    #[test]
    fn in_flight_transitions_to_known_or_unknown_outcomes() {
        assert!(can_transition(
            IdempotencyState::InFlight,
            IdempotencyState::Succeeded
        ));
        assert!(can_transition(
            IdempotencyState::InFlight,
            IdempotencyState::Failed
        ));
        assert!(can_transition(
            IdempotencyState::InFlight,
            IdempotencyState::Cancelled
        ));
        assert!(can_transition(
            IdempotencyState::InFlight,
            IdempotencyState::Unknown
        ));
    }

    #[test]
    fn unknown_can_be_reconciled_or_return_to_in_flight() {
        assert!(IdempotencyState::Unknown.can_transition_to(IdempotencyState::Succeeded));
        assert!(IdempotencyState::Unknown.can_transition_to(IdempotencyState::Failed));
        assert!(IdempotencyState::Unknown.can_transition_to(IdempotencyState::Cancelled));
        assert!(IdempotencyState::Unknown.can_transition_to(IdempotencyState::InFlight));
        assert!(IdempotencyState::Unknown.can_transition_to(IdempotencyState::Unknown));
    }

    #[test]
    fn terminal_states_cannot_reopen() {
        for state in [
            IdempotencyState::Succeeded,
            IdempotencyState::Failed,
            IdempotencyState::Cancelled,
        ] {
            assert!(!state.can_transition_to(IdempotencyState::InFlight));
            assert!(!state.can_transition_to(IdempotencyState::Unknown));
        }
    }

    #[test]
    fn store_starts_empty() {
        let store = IdempotencyStateStore::new();

        assert_eq!(store.len().unwrap(), 0);
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn reservation_creates_new_in_flight_record() {
        let store = IdempotencyStateStore::new();
        let record = record("service-a", "key-1", "operation-1", None, 100);

        let result = store.reserve(record.clone(), 50).unwrap();

        assert_eq!(result, IdempotencyReservation::Created(record.clone()));
        assert_eq!(store.get(record.identity()).unwrap(), Some(record));
    }

    #[test]
    fn reservation_rejects_non_in_flight_initial_state() {
        let store = IdempotencyStateStore::new();
        let record = IdempotencyRecord::new(
            identity("service-a", "key-1"),
            operation("operation-1"),
            None,
            IdempotencyState::Succeeded,
            Some(RecordedOutcome::new(Status::Success, None)),
            None,
            100,
        );

        assert_eq!(
            store.reserve(record, 50),
            Err(IdempotencyStateError::InvalidInitialState(
                IdempotencyState::Succeeded
            ))
        );
    }

    #[test]
    fn same_logical_action_is_a_duplicate() {
        let store = IdempotencyStateStore::new();
        let first = record("service-a", "key-1", "operation-1", Some("cap-1"), 100);
        let second = record("service-a", "key-1", "operation-1", Some("cap-1"), 200);

        store.reserve(first.clone(), 50).unwrap();

        assert_eq!(
            store.reserve(second, 60).unwrap(),
            IdempotencyReservation::Duplicate(first)
        );
    }

    #[test]
    fn different_operation_is_a_conflict() {
        let store = IdempotencyStateStore::new();
        let first = record("service-a", "key-1", "operation-1", None, 100);
        let second = record("service-a", "key-1", "operation-2", None, 200);

        store.reserve(first.clone(), 50).unwrap();

        assert_eq!(
            store.reserve(second, 60).unwrap(),
            IdempotencyReservation::Conflict(first)
        );
    }

    #[test]
    fn different_capability_is_a_conflict() {
        let store = IdempotencyStateStore::new();
        let first = record("service-a", "key-1", "operation-1", Some("cap-1"), 100);
        let second = record("service-a", "key-1", "operation-1", Some("cap-2"), 200);

        store.reserve(first.clone(), 50).unwrap();

        assert_eq!(
            store.reserve(second, 60).unwrap(),
            IdempotencyReservation::Conflict(first)
        );
    }

    #[test]
    fn same_key_in_different_scopes_is_independent() {
        let store = IdempotencyStateStore::new();
        let first = record("service-a", "same-key", "operation-1", None, 100);
        let second = record("service-b", "same-key", "operation-2", None, 100);

        assert!(matches!(
            store.reserve(first, 50).unwrap(),
            IdempotencyReservation::Created(_)
        ));
        assert!(matches!(
            store.reserve(second, 50).unwrap(),
            IdempotencyReservation::Created(_)
        ));

        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn expired_record_is_not_silently_overwritten() {
        let store = IdempotencyStateStore::new();
        let first = record("service-a", "key-1", "operation-1", None, 100);
        let replacement = record("service-a", "key-1", "operation-2", None, 200);

        store.reserve(first.clone(), 50).unwrap();

        assert_eq!(
            store.reserve(replacement, 100).unwrap(),
            IdempotencyReservation::Expired(first)
        );
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn lookup_distinguishes_missing_present_and_expired() {
        let store = IdempotencyStateStore::new();
        let current = record("service-a", "key-1", "operation-1", None, 100);

        assert_eq!(
            store.lookup(current.identity(), 50).unwrap(),
            IdempotencyLookup::Missing
        );

        store.reserve(current.clone(), 50).unwrap();

        assert_eq!(
            store.lookup(current.identity(), 99).unwrap(),
            IdempotencyLookup::Present(current.clone())
        );
        assert_eq!(
            store.lookup(current.identity(), 100).unwrap(),
            IdempotencyLookup::Expired(current)
        );
    }

    #[test]
    fn transition_updates_only_logical_state_metadata() {
        let store = IdempotencyStateStore::new();
        let initial = record("service-a", "key-1", "operation-1", Some("cap-1"), 500);

        store.reserve(initial.clone(), 100).unwrap();

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
        assert_eq!(updated.expires_at(), 500);
    }

    #[test]
    fn terminal_states_require_matching_recorded_outcomes() {
        let cases = [
            (IdempotencyState::Succeeded, Status::Failure),
            (IdempotencyState::Failed, Status::Success),
            (IdempotencyState::Cancelled, Status::Failure),
        ];

        for (state, status) in cases {
            let store = IdempotencyStateStore::new();
            let initial = record("service-a", "key-1", "operation-1", None, 500);
            store.reserve(initial.clone(), 100).unwrap();

            let result = store.transition(
                initial.identity(),
                state,
                Some(RecordedOutcome::new(status, None)),
                None,
            );

            assert_eq!(
                result,
                Err(IdempotencyStateError::InvalidTerminalMetadata(state))
            );
            assert_eq!(store.get(initial.identity()).unwrap(), Some(initial));
        }
    }

    #[test]
    fn terminal_states_accept_matching_recorded_outcomes() {
        let cases = [
            (IdempotencyState::Succeeded, Status::Success),
            (IdempotencyState::Failed, Status::Failure),
            (IdempotencyState::Cancelled, Status::Cancelled),
        ];

        for (state, status) in cases {
            let store = IdempotencyStateStore::new();
            let initial = record("service-a", "key-1", "operation-1", None, 500);
            store.reserve(initial.clone(), 100).unwrap();

            let updated = store
                .transition(
                    initial.identity(),
                    state,
                    Some(RecordedOutcome::new(status, None)),
                    None,
                )
                .unwrap();

            assert_eq!(*updated.state(), state);
            assert_eq!(updated.outcome().map(RecordedOutcome::status), Some(status));
        }
    }

    #[test]
    fn terminal_states_cannot_be_completed_without_an_outcome() {
        for state in [
            IdempotencyState::Succeeded,
            IdempotencyState::Failed,
            IdempotencyState::Cancelled,
        ] {
            let store = IdempotencyStateStore::new();
            let initial = record("service-a", "key-1", "operation-1", None, 500);
            store.reserve(initial.clone(), 100).unwrap();

            let result = store.transition(initial.identity(), state, None, None);

            assert_eq!(
                result,
                Err(IdempotencyStateError::InvalidTerminalMetadata(state))
            );
            assert_eq!(store.get(initial.identity()).unwrap(), Some(initial));
        }
    }

    #[test]
    fn invalid_terminal_reopen_is_rejected() {
        let store = IdempotencyStateStore::new();
        let initial = record("service-a", "key-1", "operation-1", None, 500);

        store.reserve(initial.clone(), 100).unwrap();
        store
            .transition(
                initial.identity(),
                IdempotencyState::Succeeded,
                Some(RecordedOutcome::new(Status::Success, None)),
                None,
            )
            .unwrap();

        let result = store.transition(initial.identity(), IdempotencyState::InFlight, None, None);

        assert_eq!(
            result,
            Err(IdempotencyStateError::InvalidTransition(
                IdempotencyStateTransitionError::new(
                    IdempotencyState::Succeeded,
                    IdempotencyState::InFlight
                )
            ))
        );
    }

    #[test]
    fn unknown_can_return_to_in_flight_without_final_metadata() {
        let store = IdempotencyStateStore::new();
        let initial = record("service-a", "key-1", "operation-1", None, 500);

        store.reserve(initial.clone(), 100).unwrap();

        store
            .transition(initial.identity(), IdempotencyState::Unknown, None, None)
            .unwrap();

        let updated = store
            .transition(initial.identity(), IdempotencyState::InFlight, None, None)
            .unwrap();

        assert_eq!(*updated.state(), IdempotencyState::InFlight);
        assert!(updated.outcome().is_none());
        assert!(updated.result_reference().is_none());
    }

    #[test]
    fn in_flight_metadata_is_rejected() {
        let store = IdempotencyStateStore::new();
        let initial = record("service-a", "key-1", "operation-1", None, 500);

        store.reserve(initial.clone(), 100).unwrap();

        let result = store.transition(
            initial.identity(),
            IdempotencyState::InFlight,
            Some(RecordedOutcome::new(Status::Success, None)),
            None,
        );

        assert_eq!(
            result,
            Err(IdempotencyStateError::InvalidInFlightMetadata(
                IdempotencyState::InFlight
            ))
        );
    }

    #[test]
    fn missing_transition_target_is_reported() {
        let store = IdempotencyStateStore::new();
        let identity = identity("service-a", "missing");

        assert_eq!(
            store.transition(
                &identity,
                IdempotencyState::Succeeded,
                Some(RecordedOutcome::new(Status::Success, None)),
                None,
            ),
            Err(IdempotencyStateError::MissingRecord)
        );
    }

    #[test]
    fn remove_requires_a_resolved_terminal_record() {
        let store = IdempotencyStateStore::new();
        let initial = record("service-a", "key-1", "operation-1", None, 100);

        store.reserve(initial.clone(), 50).unwrap();

        assert_eq!(
            store.remove(initial.identity()),
            Err(IdempotencyStateError::RemovalNotAllowed(
                IdempotencyState::InFlight
            ))
        );

        store
            .transition(initial.identity(), IdempotencyState::Unknown, None, None)
            .unwrap();
        assert_eq!(
            store.remove(initial.identity()),
            Err(IdempotencyStateError::RemovalNotAllowed(
                IdempotencyState::Unknown
            ))
        );

        let resolved = store
            .transition(
                initial.identity(),
                IdempotencyState::Succeeded,
                Some(RecordedOutcome::new(Status::Success, None)),
                None,
            )
            .unwrap();

        assert_eq!(store.remove(resolved.identity()).unwrap(), Some(resolved));
        assert!(store.get(initial.identity()).unwrap().is_none());
    }

    #[test]
    fn cloned_stores_share_authoritative_state() {
        let first = IdempotencyStateStore::new();
        let second = first.clone();
        let record = record("service-a", "key-1", "operation-1", None, 100);

        first.reserve(record.clone(), 50).unwrap();

        assert_eq!(second.get(record.identity()).unwrap(), Some(record));
    }

    #[test]
    fn transition_errors_expose_both_states() {
        let error = IdempotencyStateTransitionError::new(
            IdempotencyState::Succeeded,
            IdempotencyState::Failed,
        );

        assert_eq!(error.from(), IdempotencyState::Succeeded);
        assert_eq!(error.to(), IdempotencyState::Failed);
        assert!(error.to_string().contains("Succeeded"));
        assert!(error.to_string().contains("Failed"));
    }
}
