//! Control Plane engine registry.
//!
//! The registry is the discovery and metadata authority for known engine
//! instances. It is deliberately distinct from routing membership:
//!
//! ```text
//! Engine Runtime
//!       |
//!       | EngineRegistration
//!       v
//!     Registry
//!       |
//!       | known engine metadata
//!       v
//!   Membership
//!       |
//!       | routing snapshot
//!       v
//! routing / resolution
//! ```
//!
//! The registry answers questions such as:
//!
//! - Which concrete engine instances are known?
//! - Which logical engine owns an instance?
//! - What registration metadata has been advertised by an instance?
//!
//! The registry does not own:
//!
//! - routing membership decisions;
//! - lifecycle transitions;
//! - health evaluation;
//! - routing policy;
//! - destination selection;
//! - transport connections;
//! - capability execution;
//! - retry semantics.
//!
//! `membership.rs` owns the current in-memory routing membership. The registry
//! owns discovery information so that discovery and routing remain separate
//! architectural responsibilities.
//!
//! Persistent registry storage is intentionally not implemented in this first
//! boundary. The intended persistent location for a future storage layer is:
//!
//! ```text
//! ~/.nizaam/core/registry/
//! ```
//!
//! Keeping persistence behind this boundary allows the storage representation
//! to be introduced later without coupling the registry authority to a
//! particular filesystem or serialization format.

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::control_plane::registration::{EngineRegistration, RegistrationValidationError};
use crate::identity::{EngineId, EngineInstanceId};

/// Errors produced by registry operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The supplied engine instance is already registered to the same logical
    /// engine.
    AlreadyRegistered(EngineInstanceId),

    /// The supplied concrete instance is already registered to a different
    /// logical engine.
    EngineIdentityMismatch {
        instance_id: EngineInstanceId,
        registered_engine_id: EngineId,
        requested_engine_id: EngineId,
    },

    /// The supplied registration failed structural validation.
    InvalidRegistration(RegistrationValidationError),

    /// The requested concrete engine instance is not known to the registry.
    NotFound(EngineInstanceId),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered(instance_id) => {
                write!(
                    formatter,
                    "engine instance {instance_id} is already registered"
                )
            }

            Self::EngineIdentityMismatch {
                instance_id,
                registered_engine_id,
                requested_engine_id,
            } => write!(
                formatter,
                "engine instance {instance_id} is registered to engine \
                 {registered_engine_id}, but registration specifies \
                 engine {requested_engine_id}"
            ),

            Self::InvalidRegistration(error) => {
                write!(formatter, "invalid engine registration: {error}")
            }

            Self::NotFound(instance_id) => {
                write!(formatter, "engine instance {instance_id} is not registered")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<RegistrationValidationError> for RegistryError {
    fn from(error: RegistrationValidationError) -> Self {
        Self::InvalidRegistration(error)
    }
}

/// Result type used by registry operations.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// One engine registration known to the registry.
///
/// The complete [`EngineRegistration`] is retained rather than duplicating
/// individual registration fields. This keeps the registry aligned with the
/// existing registration model as that model evolves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryRecord {
    registration: EngineRegistration,
}

impl RegistryRecord {
    /// Creates a registry record from an engine registration.
    #[must_use]
    pub fn new(registration: EngineRegistration) -> Self {
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

    /// Returns the complete engine registration.
    #[must_use]
    pub fn registration(&self) -> &EngineRegistration {
        &self.registration
    }
}

/// Authoritative registry of known engine instances.
///
/// The registry is keyed by [`EngineInstanceId`] because a concrete runtime
/// instance is the smallest independently addressable engine participant in
/// the ecosystem.
///
/// Multiple instances may belong to the same logical [`EngineId`]:
///
/// ```text
/// QuranEngine
/// ├── quran-01
/// ├── quran-02
/// └── quran-03
/// ```
///
/// The registry does not infer or assign routing priority from storage order.
#[derive(Debug)]
pub struct EngineRegistry {
    state: RwLock<BTreeMap<EngineInstanceId, RegistryRecord>>,
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRegistry {
    /// Creates an empty engine registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(BTreeMap::new()),
        }
    }

    /// Registers one concrete engine instance.
    ///
    /// The concrete [`EngineInstanceId`] is the registry identity key.
    ///
    /// Registering the same instance twice is rejected. In particular, the
    /// registry never silently changes the logical engine associated with an
    /// existing concrete instance.
    ///
    /// Registration validation remains owned by `EngineRegistration`.
    pub fn register(&self, registration: EngineRegistration) -> RegistryResult<()> {
        registration.validate()?;

        let instance_id = registration.engine_instance_id().clone();
        let engine_id = registration.engine_id().clone();

        let mut state = self
            .state
            .write()
            .expect("Control Plane registry state lock poisoned");

        if let Some(existing) = state.get(&instance_id) {
            if existing.engine_id() == &engine_id {
                return Err(RegistryError::AlreadyRegistered(instance_id));
            }

            return Err(RegistryError::EngineIdentityMismatch {
                instance_id,
                registered_engine_id: existing.engine_id().clone(),
                requested_engine_id: engine_id,
            });
        }

        state.insert(instance_id, RegistryRecord::new(registration));

        Ok(())
    }

    /// Replaces the registration for an existing concrete engine instance.
    ///
    /// The logical [`EngineId`] is immutable for an existing
    /// [`EngineInstanceId`]. A different logical engine identity is rejected.
    pub fn update(&self, registration: EngineRegistration) -> RegistryResult<()> {
        registration.validate()?;

        let instance_id = registration.engine_instance_id().clone();
        let engine_id = registration.engine_id().clone();

        let mut state = self
            .state
            .write()
            .expect("Control Plane registry state lock poisoned");

        let existing = state
            .get(&instance_id)
            .ok_or_else(|| RegistryError::NotFound(instance_id.clone()))?;

        if existing.engine_id() != &engine_id {
            return Err(RegistryError::EngineIdentityMismatch {
                instance_id,
                registered_engine_id: existing.engine_id().clone(),
                requested_engine_id: engine_id,
            });
        }

        state.insert(instance_id, RegistryRecord::new(registration));

        Ok(())
    }

    /// Removes a concrete engine instance from the registry.
    ///
    /// The removed record is returned so callers can preserve it for
    /// diagnostics, observability, or higher-level coordination.
    pub fn unregister(&self, instance_id: &EngineInstanceId) -> RegistryResult<RegistryRecord> {
        let mut state = self
            .state
            .write()
            .expect("Control Plane registry state lock poisoned");

        state
            .remove(instance_id)
            .ok_or_else(|| RegistryError::NotFound(instance_id.clone()))
    }

    /// Looks up one concrete engine instance.
    ///
    /// The record is cloned so the registry lock is released before the caller
    /// uses the returned value.
    #[must_use]
    pub fn get(&self, instance_id: &EngineInstanceId) -> Option<RegistryRecord> {
        let state = self
            .state
            .read()
            .expect("Control Plane registry state lock poisoned");

        state.get(instance_id).cloned()
    }

    /// Returns whether a concrete engine instance is known to the registry.
    #[must_use]
    pub fn contains(&self, instance_id: &EngineInstanceId) -> bool {
        let state = self
            .state
            .read()
            .expect("Control Plane registry state lock poisoned");

        state.contains_key(instance_id)
    }

    /// Returns the number of known concrete engine instances.
    #[must_use]
    pub fn len(&self) -> usize {
        let state = self
            .state
            .read()
            .expect("Control Plane registry state lock poisoned");

        state.len()
    }

    /// Returns whether the registry contains no engine instances.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns all known engine registrations in deterministic
    /// `EngineInstanceId` order.
    ///
    /// Ordering is only an iteration guarantee. The registry does not assign
    /// routing priority.
    #[must_use]
    pub fn records(&self) -> Vec<RegistryRecord> {
        let state = self
            .state
            .read()
            .expect("Control Plane registry state lock poisoned");

        state.values().cloned().collect()
    }

    /// Returns all registered instances belonging to one logical engine.
    ///
    /// Results are returned in deterministic `EngineInstanceId` order.
    #[must_use]
    pub fn instances_for_engine(&self, engine_id: &EngineId) -> Vec<RegistryRecord> {
        let state = self
            .state
            .read()
            .expect("Control Plane registry state lock poisoned");

        state
            .values()
            .filter(|record| record.engine_id() == engine_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_plane::registration::EngineRegistration;

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("valid engine id")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("valid instance id")
    }

    fn registration(engine: &str, instance: &str) -> EngineRegistration {
        EngineRegistration::new(engine_id(engine), instance_id(instance))
    }

    #[test]
    fn new_registry_is_empty() {
        let registry = EngineRegistry::new();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.records().is_empty());
    }

    #[test]
    fn default_matches_new() {
        let first = EngineRegistry::new();
        let second = EngineRegistry::default();

        assert_eq!(first.records(), second.records());
    }

    #[test]
    fn register_adds_concrete_instance() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("registration succeeds");

        assert_eq!(registry.len(), 1);
        assert!(registry.contains(&instance_id("quran-01")));

        let record = registry
            .get(&instance_id("quran-01"))
            .expect("registered instance");

        assert_eq!(record.engine_id(), &engine_id("quran"));
        assert_eq!(record.engine_instance_id(), &instance_id("quran-01"));
    }

    #[test]
    fn multiple_instances_can_belong_to_one_engine() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("first registration");

        registry
            .register(registration("quran", "quran-02"))
            .expect("second registration");

        registry
            .register(registration("quran", "quran-03"))
            .expect("third registration");

        let instances = registry
            .instances_for_engine(&engine_id("quran"))
            .into_iter()
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(instances, vec!["quran-01", "quran-02", "quran-03"]);
    }

    #[test]
    fn duplicate_registration_is_rejected_without_mutating_state() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("initial registration");

        let result = registry.register(registration("quran", "quran-01"));

        assert_eq!(
            result,
            Err(RegistryError::AlreadyRegistered(instance_id("quran-01")))
        );

        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn instance_cannot_change_logical_engine_identity() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "shared-instance"))
            .expect("initial registration");

        let result = registry.register(registration("hadith", "shared-instance"));

        assert_eq!(
            result,
            Err(RegistryError::EngineIdentityMismatch {
                instance_id: instance_id("shared-instance"),
                registered_engine_id: engine_id("quran"),
                requested_engine_id: engine_id("hadith"),
            })
        );

        assert_eq!(
            registry
                .get(&instance_id("shared-instance"))
                .expect("existing record")
                .engine_id(),
            &engine_id("quran")
        );
    }

    #[test]
    fn update_replaces_existing_registration() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("initial registration");

        let updated = registration("quran", "quran-01");

        registry.update(updated).expect("update succeeds");

        let record = registry
            .get(&instance_id("quran-01"))
            .expect("updated record");

        assert_eq!(record.engine_id(), &engine_id("quran"));
    }

    #[test]
    fn update_rejects_unknown_instance() {
        let registry = EngineRegistry::new();

        let result = registry.update(registration("quran", "quran-01"));

        assert_eq!(
            result,
            Err(RegistryError::NotFound(instance_id("quran-01")))
        );
    }

    #[test]
    fn update_rejects_logical_engine_identity_change() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("initial registration");

        let result = registry.update(registration("hadith", "quran-01"));

        assert_eq!(
            result,
            Err(RegistryError::EngineIdentityMismatch {
                instance_id: instance_id("quran-01"),
                registered_engine_id: engine_id("quran"),
                requested_engine_id: engine_id("hadith"),
            })
        );

        assert_eq!(
            registry
                .get(&instance_id("quran-01"))
                .expect("existing record")
                .engine_id(),
            &engine_id("quran")
        );
    }

    #[test]
    fn unregister_removes_and_returns_record() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("registration succeeds");

        let removed = registry
            .unregister(&instance_id("quran-01"))
            .expect("unregister succeeds");

        assert_eq!(removed.engine_id(), &engine_id("quran"));
        assert!(!registry.contains(&instance_id("quran-01")));
        assert!(registry.is_empty());
    }

    #[test]
    fn unregister_unknown_instance_is_rejected() {
        let registry = EngineRegistry::new();

        let result = registry.unregister(&instance_id("missing"));

        assert_eq!(result, Err(RegistryError::NotFound(instance_id("missing"))));
    }

    #[test]
    fn records_are_returned_in_instance_id_order() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-03"))
            .expect("registration");

        registry
            .register(registration("quran", "quran-01"))
            .expect("registration");

        registry
            .register(registration("quran", "quran-02"))
            .expect("registration");

        let instances = registry
            .records()
            .into_iter()
            .map(|record| record.engine_instance_id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(instances, vec!["quran-01", "quran-02", "quran-03"]);
    }

    #[test]
    fn records_preserve_complete_registration() {
        let registry = EngineRegistry::new();
        let registration = registration("quran", "quran-01");

        registry
            .register(registration.clone())
            .expect("registration succeeds");

        assert_eq!(
            registry
                .get(&instance_id("quran-01"))
                .expect("registered record")
                .registration(),
            &registration
        );
    }

    #[test]
    fn unregistering_one_instance_does_not_affect_other_instances() {
        let registry = EngineRegistry::new();

        registry
            .register(registration("quran", "quran-01"))
            .expect("first registration");

        registry
            .register(registration("quran", "quran-02"))
            .expect("second registration");

        registry
            .unregister(&instance_id("quran-01"))
            .expect("unregister succeeds");

        assert!(!registry.contains(&instance_id("quran-01")));
        assert!(registry.contains(&instance_id("quran-02")));
        assert_eq!(registry.len(), 1);
    }
}
