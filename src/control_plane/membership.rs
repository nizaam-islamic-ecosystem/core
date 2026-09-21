//! Authoritative in-memory Control Plane routing membership.
//!
//! `membership.rs` owns the Control Plane's current view of concrete engine
//! instances that have been accepted into routing membership. It deliberately
//! does not own engine lifecycle transitions, health evaluation, routing
//! policy, destination selection, transport connections, retries, persistence,
//! or capability execution.
//!
//! The central concurrency invariant is that routing consumers obtain one
//! [`MembershipSnapshot`] and evaluate their entire destination decision from
//! that immutable state. A successful membership mutation creates a new
//! monotonically increasing membership version; previously-created snapshots
//! remain unchanged.
//!
//! ```text
//! Engine Runtime
//!       |
//!       | EngineRegistration
//!       v
//!   Membership
//!       |
//!       | snapshot()
//!       v
//! MembershipSnapshot
//!       |
//!       v
//! routing / resolution
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::sync::RwLock;

use crate::control_plane::registration::{EngineRegistration, RegistrationValidationError};
use crate::identity::{EngineId, EngineInstanceId};

/// Errors produced by Control Plane membership operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MembershipError {
    /// The supplied registration failed its own structural validation.
    InvalidRegistration(RegistrationValidationError),

    /// A registration was submitted for an instance that is already present.
    AlreadyRegistered(EngineInstanceId),

    /// A requested instance does not exist in membership.
    NotFound(EngineInstanceId),

    /// An update attempted to move an existing concrete instance to another
    /// logical engine identity.
    EngineIdentityMismatch {
        instance_id: EngineInstanceId,
        registered_engine_id: EngineId,
        updated_engine_id: EngineId,
    },
}

impl fmt::Display for MembershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegistration(error) => {
                write!(formatter, "invalid engine registration: {error}")
            }
            Self::AlreadyRegistered(instance_id) => {
                write!(
                    formatter,
                    "engine instance {instance_id} is already registered"
                )
            }
            Self::NotFound(instance_id) => {
                write!(formatter, "engine instance {instance_id} is not registered")
            }
            Self::EngineIdentityMismatch {
                instance_id,
                registered_engine_id,
                updated_engine_id,
            } => write!(
                formatter,
                "engine instance {instance_id} belongs to engine {registered_engine_id}, \
                 but update specifies engine {updated_engine_id}"
            ),
        }
    }
}

impl std::error::Error for MembershipError {}

impl From<RegistrationValidationError> for MembershipError {
    fn from(error: RegistrationValidationError) -> Self {
        Self::InvalidRegistration(error)
    }
}

/// Result alias for membership-local operations.
pub type MembershipResult<T> = Result<T, MembershipError>;

/// One accepted concrete engine instance in routing membership.
///
/// The record intentionally wraps the declarative [`EngineRegistration`]
/// instead of duplicating its fields. Membership owns participation in the
/// routing view; registration owns the registration data itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipRecord {
    registration: EngineRegistration,
}

impl MembershipRecord {
    /// Creates a membership record from an already validated registration.
    ///
    /// This constructor is crate-public because records are created by the
    /// membership authority rather than by external callers.
    pub(crate) fn new(registration: EngineRegistration) -> Self {
        Self { registration }
    }

    /// Returns the logical engine identifier.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        self.registration.engine_id()
    }

    /// Returns the concrete engine instance identifier.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        self.registration.engine_instance_id()
    }

    /// Returns the accepted declarative engine registration.
    #[must_use]
    pub fn registration(&self) -> &EngineRegistration {
        &self.registration
    }
}

/// Immutable, point-in-time view of routing membership.
///
/// A snapshot owns its records. Later registration, update, or unregister
/// operations cannot mutate an existing snapshot, which lets a routing
/// decision remain tied to the membership state from which it was resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MembershipSnapshot {
    version: u64,
    members: BTreeMap<EngineInstanceId, MembershipRecord>,
}

impl MembershipSnapshot {
    /// Returns the membership version represented by this snapshot.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Looks up one concrete engine instance in this snapshot.
    #[must_use]
    pub fn get(&self, instance_id: &EngineInstanceId) -> Option<&MembershipRecord> {
        self.members.get(instance_id)
    }

    /// Returns whether an engine instance exists in this snapshot.
    #[must_use]
    pub fn contains(&self, instance_id: &EngineInstanceId) -> bool {
        self.members.contains_key(instance_id)
    }

    /// Returns the number of registered engine instances in this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Returns whether this snapshot contains no engine instances.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Enumerates all registered instances in deterministic `EngineInstanceId`
    /// order.
    ///
    /// Ordering here is only an iteration guarantee. Membership does not
    /// assign routing priority to any candidate.
    pub fn candidates(&self) -> impl Iterator<Item = &MembershipRecord> {
        self.members.values()
    }

    /// Enumerates instances belonging to one logical engine in deterministic
    /// `EngineInstanceId` order.
    pub fn instances_for_engine(
        &self,
        engine_id: &EngineId,
    ) -> impl Iterator<Item = &MembershipRecord> {
        self.members
            .values()
            .filter(move |record| record.engine_id() == engine_id)
    }
}

/// Authoritative in-memory routing membership for concrete engine instances.
///
/// Membership uses one map keyed by [`EngineInstanceId`] so all registration
/// metadata for a concrete runtime participant changes together. The live
/// state is protected by a synchronous read/write lock because membership
/// operations are bounded state mutations/reads and do not require an async
/// execution context.
#[derive(Debug)]
pub struct Membership {
    state: RwLock<MembershipState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MembershipState {
    version: u64,
    members: BTreeMap<EngineInstanceId, MembershipRecord>,
}

impl Default for Membership {
    fn default() -> Self {
        Self::new()
    }
}

impl Membership {
    /// Creates empty membership at revision zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(MembershipState {
                version: 0,
                members: BTreeMap::new(),
            }),
        }
    }

    /// Registers one new concrete engine instance.
    ///
    /// Registration is deliberately distinct from update semantics: an
    /// already-present `EngineInstanceId` is rejected rather than silently
    /// replaced. Use [`Self::update`] to replace an existing instance's
    /// registration view.
    ///
    /// A successful registration increments the membership version once.
    pub fn register(&self, registration: EngineRegistration) -> MembershipResult<()> {
        registration.validate()?;

        let instance_id = registration.engine_instance_id().clone();
        let mut state = self
            .state
            .write()
            .expect("Control Plane membership state lock poisoned");

        if state.members.contains_key(&instance_id) {
            return Err(MembershipError::AlreadyRegistered(instance_id));
        }

        state
            .members
            .insert(instance_id, MembershipRecord::new(registration));
        state.version = state
            .version
            .checked_add(1)
            .expect("Control Plane membership version exhausted");

        Ok(())
    }

    /// Updates an existing concrete engine instance's registration view.
    ///
    /// The `EngineInstanceId` is the stable membership key. The logical
    /// [`EngineId`] associated with that instance cannot be changed by an
    /// update.
    ///
    /// A successful update increments the membership version once.
    pub fn update(&self, registration: EngineRegistration) -> MembershipResult<()> {
        registration.validate()?;

        let instance_id = registration.engine_instance_id().clone();
        let mut state = self
            .state
            .write()
            .expect("Control Plane membership state lock poisoned");

        let existing = state
            .members
            .get(&instance_id)
            .ok_or_else(|| MembershipError::NotFound(instance_id.clone()))?;

        if existing.engine_id() != registration.engine_id() {
            return Err(MembershipError::EngineIdentityMismatch {
                instance_id,
                registered_engine_id: existing.engine_id().clone(),
                updated_engine_id: registration.engine_id().clone(),
            });
        }

        state
            .members
            .insert(instance_id, MembershipRecord::new(registration));
        state.version = state
            .version
            .checked_add(1)
            .expect("Control Plane membership version exhausted");

        Ok(())
    }

    /// Removes one concrete engine instance from membership.
    ///
    /// The removed record is returned to the caller for diagnostics,
    /// observability, or optional external notification.
    ///
    /// A successful removal increments the membership version once.
    pub fn unregister(&self, instance_id: &EngineInstanceId) -> MembershipResult<MembershipRecord> {
        let mut state = self
            .state
            .write()
            .expect("Control Plane membership state lock poisoned");

        let record = state
            .members
            .remove(instance_id)
            .ok_or_else(|| MembershipError::NotFound(instance_id.clone()))?;

        state.version = state
            .version
            .checked_add(1)
            .expect("Control Plane membership version exhausted");

        Ok(record)
    }

    /// Looks up one concrete engine instance in the current live membership.
    ///
    /// The returned record is cloned so the lock is released before the caller
    /// uses the value. Routing code should normally prefer [`Self::snapshot`]
    /// so a complete decision is evaluated against one coherent state.
    #[must_use]
    pub fn get(&self, instance_id: &EngineInstanceId) -> Option<MembershipRecord> {
        let state = self
            .state
            .read()
            .expect("Control Plane membership state lock poisoned");
        state.members.get(instance_id).cloned()
    }

    /// Returns whether an instance exists in current live membership.
    #[must_use]
    pub fn contains(&self, instance_id: &EngineInstanceId) -> bool {
        let state = self
            .state
            .read()
            .expect("Control Plane membership state lock poisoned");
        state.members.contains_key(instance_id)
    }

    /// Returns the number of registered engine instances.
    #[must_use]
    pub fn len(&self) -> usize {
        let state = self
            .state
            .read()
            .expect("Control Plane membership state lock poisoned");
        state.members.len()
    }

    /// Returns whether live membership contains no engine instances.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current live membership revision.
    #[must_use]
    pub fn version(&self) -> u64 {
        let state = self
            .state
            .read()
            .expect("Control Plane membership state lock poisoned");
        state.version
    }

    /// Captures one immutable, coherent membership snapshot.
    ///
    /// All routing candidate evaluation for one destination decision should be
    /// performed from one snapshot instead of mixing independent live lookups.
    #[must_use]
    pub fn snapshot(&self) -> MembershipSnapshot {
        let state = self
            .state
            .read()
            .expect("Control Plane membership state lock poisoned");

        MembershipSnapshot {
            version: state.version,
            members: state.members.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityDefinition;
    use crate::contracts::descriptor::{Interaction, PayloadDescriptor};
    use crate::contracts::{ContractDescriptor, Version};
    use crate::control_plane::registration::Endpoint;
    use crate::identity::{CapabilityId, ContractId};
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("valid engine id")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("valid instance id")
    }

    fn capability(id: &str, owner: &str) -> CapabilityDefinition {
        CapabilityDefinition::new(
            CapabilityId::new(id).expect("valid capability id"),
            engine_id(owner),
            "Test Capability",
        )
        .expect("valid capability definition")
    }

    fn contract(capability_id: &str, version: Version) -> ContractDescriptor {
        ContractDescriptor::new(
            ContractId::new("test.contract").expect("valid contract id"),
            CapabilityId::new(capability_id).expect("valid capability id"),
            version.clone(),
            Interaction::Request,
            PayloadDescriptor::new("application/json", version).expect("valid payload descriptor"),
        )
    }

    fn registration(engine: &str, instance: &str) -> EngineRegistration {
        EngineRegistration::new(engine_id(engine), instance_id(instance))
    }

    #[test]
    fn new_membership_is_empty_at_version_zero() {
        let membership = Membership::new();

        assert_eq!(membership.version(), 0);
        assert_eq!(membership.len(), 0);
        assert!(membership.is_empty());
        assert!(membership.snapshot().is_empty());
    }

    #[test]
    fn default_matches_new() {
        let membership = Membership::default();
        assert_eq!(membership.snapshot(), Membership::new().snapshot());
    }

    #[test]
    fn register_accepts_valid_instance_and_advances_version() {
        let membership = Membership::new();

        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration succeeds");

        assert_eq!(membership.version(), 1);
        assert_eq!(membership.len(), 1);
        assert!(membership.contains(&instance_id("arabic-01")));
        assert_eq!(
            membership
                .get(&instance_id("arabic-01"))
                .expect("registered instance")
                .engine_id()
                .as_str(),
            "arabic"
        );
    }

    #[test]
    fn multiple_instances_can_share_one_logical_engine() {
        let membership = Membership::new();

        membership
            .register(registration("arabic", "arabic-01"))
            .expect("first registration");
        membership
            .register(registration("arabic", "arabic-02"))
            .expect("second registration");
        membership
            .register(registration("arabic", "arabic-03"))
            .expect("third registration");

        let snapshot = membership.snapshot();
        let instances = snapshot
            .instances_for_engine(&engine_id("arabic"))
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(instances, vec!["arabic-01", "arabic-02", "arabic-03"]);
    }

    #[test]
    fn one_instance_can_carry_multiple_capabilities_and_contracts() {
        let membership = Membership::new();
        let registration = registration("arabic", "arabic-01")
            .with_capability(capability("arabic.tokenize", "arabic"))
            .expect("first capability")
            .with_capability(capability("arabic.normalize", "arabic"))
            .expect("second capability")
            .with_contract(contract("arabic.tokenize", Version::new(1, 0, 0)))
            .with_contract(contract("arabic.tokenize", Version::new(2, 0, 0)))
            .with_endpoint(Endpoint::new("grpc://arabic-01:50051").expect("endpoint"));

        membership.register(registration).expect("registration");

        let record = membership
            .get(&instance_id("arabic-01"))
            .expect("registered instance");

        assert_eq!(record.registration().capabilities().len(), 2);
        assert_eq!(record.registration().contracts().len(), 2);
        assert_eq!(
            record
                .registration()
                .endpoint()
                .expect("endpoint")
                .address(),
            "grpc://arabic-01:50051"
        );
    }

    #[test]
    fn register_rejects_duplicate_instance_without_mutating_state() {
        let membership = Membership::new();
        let original = registration("arabic", "arabic-01");
        let replacement = registration("arabic", "arabic-01")
            .with_capability(capability("arabic.normalize", "arabic"))
            .expect("capability");

        membership.register(original).expect("first registration");
        let version_before = membership.version();

        assert_eq!(
            membership.register(replacement),
            Err(MembershipError::AlreadyRegistered(instance_id("arabic-01")))
        );
        assert_eq!(membership.version(), version_before);
        assert!(
            membership
                .get(&instance_id("arabic-01"))
                .expect("original registration")
                .registration()
                .capabilities()
                .is_empty()
        );
    }

    #[test]
    fn update_replaces_existing_registration_and_advances_version() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");

        let updated = registration("arabic", "arabic-01")
            .with_capability(capability("arabic.normalize", "arabic"))
            .expect("capability")
            .with_routing_metadata("region", "ap-south")
            .expect("routing metadata");

        membership.update(updated).expect("update");

        assert_eq!(membership.version(), 2);
        let record = membership
            .get(&instance_id("arabic-01"))
            .expect("updated instance");
        assert_eq!(record.registration().capabilities().len(), 1);
        assert_eq!(
            record.registration().routing_metadata().get("region"),
            Some(&"ap-south".to_string())
        );
    }

    #[test]
    fn update_rejects_unknown_instance_without_mutating_state() {
        let membership = Membership::new();
        let registration = registration("arabic", "arabic-01");

        assert_eq!(
            membership.update(registration),
            Err(MembershipError::NotFound(instance_id("arabic-01")))
        );
        assert_eq!(membership.version(), 0);
        assert!(membership.is_empty());
    }

    #[test]
    fn update_cannot_change_logical_engine_identity() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "shared-01"))
            .expect("registration");
        let version_before = membership.version();

        assert_eq!(
            membership.update(registration("quran", "shared-01")),
            Err(MembershipError::EngineIdentityMismatch {
                instance_id: instance_id("shared-01"),
                registered_engine_id: engine_id("arabic"),
                updated_engine_id: engine_id("quran"),
            })
        );

        assert_eq!(membership.version(), version_before);
        assert_eq!(
            membership
                .get(&instance_id("shared-01"))
                .expect("existing record")
                .engine_id()
                .as_str(),
            "arabic"
        );
    }

    #[test]
    fn unregister_returns_record_and_advances_version() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");

        let removed = membership
            .unregister(&instance_id("arabic-01"))
            .expect("unregister");

        assert_eq!(removed.engine_id().as_str(), "arabic");
        assert_eq!(removed.engine_instance_id().as_str(), "arabic-01");
        assert_eq!(membership.version(), 2);
        assert!(!membership.contains(&instance_id("arabic-01")));
        assert!(membership.is_empty());
    }

    #[test]
    fn unregister_rejects_unknown_instance_without_mutating_state() {
        let membership = Membership::new();

        assert_eq!(
            membership.unregister(&instance_id("missing-01")),
            Err(MembershipError::NotFound(instance_id("missing-01")))
        );
        assert_eq!(membership.version(), 0);
    }

    #[test]
    fn same_capability_id_can_be_advertised_by_multiple_logical_engines() {
        let membership = Membership::new();

        membership
            .register(
                registration("arabic", "arabic-01")
                    .with_capability(capability("shared.search", "arabic"))
                    .expect("arabic capability"),
            )
            .expect("arabic registration");
        membership
            .register(
                registration("quran", "quran-01")
                    .with_capability(capability("shared.search", "quran"))
                    .expect("quran capability"),
            )
            .expect("quran registration");

        let snapshot = membership.snapshot();
        assert_eq!(snapshot.len(), 2);

        let advertised = snapshot
            .candidates()
            .filter(|record| {
                record
                    .registration()
                    .capabilities()
                    .iter()
                    .any(|capability| capability.capability_id().as_str() == "shared.search")
            })
            .map(|record| record.engine_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(advertised, vec!["arabic", "quran"]);
    }

    #[test]
    fn snapshot_captures_one_coherent_membership_version() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("first registration");
        membership
            .register(registration("quran", "quran-01"))
            .expect("second registration");

        let snapshot = membership.snapshot();

        assert_eq!(snapshot.version(), 2);
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.contains(&instance_id("arabic-01")));
        assert!(snapshot.contains(&instance_id("quran-01")));
    }

    #[test]
    fn snapshot_is_stable_after_later_membership_changes() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("first registration");
        membership
            .register(registration("quran", "quran-01"))
            .expect("second registration");

        let old_snapshot = membership.snapshot();

        membership
            .update(
                registration("arabic", "arabic-01")
                    .with_routing_metadata("region", "ap-south")
                    .expect("metadata"),
            )
            .expect("update");
        membership
            .unregister(&instance_id("quran-01"))
            .expect("unregister");
        membership
            .register(registration("hadith", "hadith-01"))
            .expect("third registration");

        assert_eq!(old_snapshot.version(), 2);
        assert_eq!(old_snapshot.len(), 2);
        assert!(old_snapshot.contains(&instance_id("quran-01")));
        assert!(!old_snapshot.contains(&instance_id("hadith-01")));
        assert!(
            old_snapshot
                .get(&instance_id("arabic-01"))
                .expect("old record")
                .registration()
                .routing_metadata()
                .is_empty()
        );

        let new_snapshot = membership.snapshot();
        assert_eq!(new_snapshot.version(), 5);
        assert_eq!(new_snapshot.len(), 2);
        assert!(!new_snapshot.contains(&instance_id("quran-01")));
        assert!(new_snapshot.contains(&instance_id("hadith-01")));
        assert_eq!(
            new_snapshot
                .get(&instance_id("arabic-01"))
                .expect("new record")
                .registration()
                .routing_metadata()
                .get("region"),
            Some(&"ap-south".to_string())
        );
    }

    #[test]
    fn candidate_order_is_deterministic_and_not_routing_priority() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-03"))
            .expect("registration");
        membership
            .register(registration("quran", "quran-02"))
            .expect("registration");
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");

        let ids = membership
            .snapshot()
            .candidates()
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["arabic-01", "arabic-03", "quran-02"]);
    }

    #[test]
    fn instances_for_engine_filters_without_collapsing_other_logical_engines() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");
        membership
            .register(registration("arabic", "arabic-02"))
            .expect("registration");
        membership
            .register(registration("quran", "quran-01"))
            .expect("registration");

        let arabic = membership
            .snapshot()
            .instances_for_engine(&engine_id("arabic"))
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();
        let quran = membership
            .snapshot()
            .instances_for_engine(&engine_id("quran"))
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(arabic, vec!["arabic-01", "arabic-02"]);
        assert_eq!(quran, vec!["quran-01"]);
    }

    #[test]
    fn concurrent_registrations_of_distinct_instances_are_safe() {
        let membership = Arc::new(Membership::new());
        let barrier = Arc::new(Barrier::new(3));

        let first_membership = Arc::clone(&membership);
        let first_barrier = Arc::clone(&barrier);
        let first = thread::spawn(move || {
            first_barrier.wait();
            first_membership
                .register(registration("arabic", "arabic-01"))
                .expect("first concurrent registration");
        });

        let second_membership = Arc::clone(&membership);
        let second_barrier = Arc::clone(&barrier);
        let second = thread::spawn(move || {
            second_barrier.wait();
            second_membership
                .register(registration("arabic", "arabic-02"))
                .expect("second concurrent registration");
        });

        barrier.wait();
        first.join().expect("first thread joins");
        second.join().expect("second thread joins");

        assert_eq!(membership.len(), 2);
        assert_eq!(membership.version(), 2);
        assert!(membership.contains(&instance_id("arabic-01")));
        assert!(membership.contains(&instance_id("arabic-02")));
    }

    #[test]
    fn snapshot_remains_valid_while_membership_is_updated_concurrently() {
        let membership = Arc::new(Membership::new());
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");

        let snapshot_before_update = membership.snapshot();
        let barrier = Arc::new(Barrier::new(2));

        let updater_membership = Arc::clone(&membership);
        let updater_barrier = Arc::clone(&barrier);
        let updater = thread::spawn(move || {
            updater_barrier.wait();
            updater_membership
                .update(
                    registration("arabic", "arabic-01")
                        .with_routing_metadata("region", "ap-south")
                        .expect("metadata"),
                )
                .expect("update succeeds");
        });

        barrier.wait();
        updater.join().expect("updater joins");

        assert_eq!(snapshot_before_update.version(), 1);
        assert!(
            snapshot_before_update
                .get(&instance_id("arabic-01"))
                .expect("old record")
                .registration()
                .routing_metadata()
                .is_empty()
        );

        assert_eq!(membership.version(), 2);
        assert_eq!(
            membership
                .snapshot()
                .get(&instance_id("arabic-01"))
                .expect("new record")
                .registration()
                .routing_metadata()
                .get("region"),
            Some(&"ap-south".to_string())
        );
    }

    #[test]
    fn membership_record_exposes_registration_without_mutable_state() {
        let membership = Membership::new();
        membership
            .register(registration("arabic", "arabic-01"))
            .expect("registration");

        let snapshot = membership.snapshot();
        let record = snapshot.get(&instance_id("arabic-01")).expect("record");

        assert_eq!(record.engine_id().as_str(), "arabic");
        assert_eq!(record.engine_instance_id().as_str(), "arabic-01");
        assert_eq!(record.registration().engine_id().as_str(), "arabic");
    }
}
