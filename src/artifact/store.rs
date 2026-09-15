//! Storage abstraction for artifact versions and their lifecycle metadata.
//!
//! `ArtifactStore` owns storage mechanics, not artifact domain semantics.
//! The in-memory implementation exists for Core tests and deterministic
//! behavior. Persistent or remote storage implementations can be supplied
//! by infrastructure layers.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::artifact::integrity::IntegrityProof;
use crate::artifact::lifecycle::{LifecycleState, transition};
use crate::artifact::version::{
    ArtifactVersion, ArtifactVersionRecord, ArtifactVersionRestoreError,
};
use crate::identity::ArtifactId;

/// Errors produced by artifact storage operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// The artifact version already exists.
    AlreadyExists,

    /// The requested artifact version does not exist.
    NotFound,

    /// The supplied alias is empty or whitespace-only.
    InvalidAlias,

    /// The supplied version is empty or whitespace-only.
    InvalidVersion,

    /// The requested lifecycle transition is not allowed.
    InvalidLifecycleTransition {
        from: LifecycleState,
        to: LifecycleState,
    },

    /// Publishing must use the dedicated publication operation.
    PublicationRequired,

    /// The artifact content reference is invalid.
    InvalidContentReference,

    /// The persisted artifact version could not be trusted and restored.
    RestorationFailed,

    /// The supplied integrity proof does not match the recorded content.
    IntegrityFailure,

    /// Internal synchronization state could not be acquired.
    LockPoisoned,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists => write!(formatter, "artifact version already exists"),
            Self::NotFound => write!(formatter, "artifact version not found"),
            Self::InvalidAlias => write!(formatter, "artifact alias is invalid"),
            Self::InvalidVersion => write!(formatter, "artifact version is invalid"),
            Self::InvalidLifecycleTransition { from, to } => write!(
                formatter,
                "invalid artifact lifecycle transition from {:?} to {:?}",
                from, to
            ),
            Self::PublicationRequired => write!(
                formatter,
                "artifact publication requires the dedicated publication operation"
            ),
            Self::InvalidContentReference => {
                write!(formatter, "artifact content reference is invalid")
            }
            Self::RestorationFailed => write!(formatter, "artifact version restoration failed"),
            Self::IntegrityFailure => {
                write!(formatter, "artifact content integrity verification failed")
            }
            Self::LockPoisoned => write!(formatter, "artifact store lock is poisoned"),
        }
    }
}

impl std::error::Error for StoreError {}

fn map_restoration_error(error: ArtifactVersionRestoreError) -> StoreError {
    match error {
        ArtifactVersionRestoreError::InvalidContentReference => StoreError::InvalidContentReference,
        ArtifactVersionRestoreError::InvalidVersion
        | ArtifactVersionRestoreError::InvalidLifecycleTransition { .. } => {
            StoreError::RestorationFailed
        }
    }
}

/// Storage abstraction for artifact versions.
pub trait ArtifactStore: Send + Sync {
    /// Stores a new artifact version.
    fn store(&self, version: ArtifactVersion) -> Result<(), StoreError>;

    /// Retrieves one exact version.
    fn get(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
    ) -> Result<Option<ArtifactVersion>, StoreError>;

    /// Lists all versions belonging to an artifact.
    ///
    /// No ordering semantics are implied.
    fn list_versions(&self, artifact_id: &ArtifactId) -> Result<Vec<ArtifactVersion>, StoreError>;

    /// Atomically applies a valid lifecycle transition and returns the
    /// resulting version.
    ///
    /// The `Validated → Published` transition is reserved for
    /// [`ArtifactStore::publish_validated`].
    fn transition_lifecycle(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
        target: LifecycleState,
    ) -> Result<ArtifactVersion, StoreError>;

    /// Atomically publishes a version only when its stored state is
    /// `Validated`.
    ///
    /// This compare-and-transition primitive is the only store operation that
    /// can enter `Published`, preventing callers from bypassing publication
    /// prerequisites through a generic lifecycle update.
    fn publish_validated(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
        proof: &IntegrityProof,
    ) -> Result<ArtifactVersion, StoreError>;

    /// Associates an alias with one exact version.
    fn set_alias(
        &self,
        artifact_id: &ArtifactId,
        alias: &str,
        version: &str,
    ) -> Result<(), StoreError>;

    /// Resolves an alias to its exact version identifier.
    fn resolve_alias(
        &self,
        artifact_id: &ArtifactId,
        alias: &str,
    ) -> Result<Option<String>, StoreError>;

    /// Removes an alias without removing the underlying version.
    fn remove_alias(&self, artifact_id: &ArtifactId, alias: &str) -> Result<bool, StoreError>;
}

/// Deterministic in-memory artifact store.
#[derive(Clone, Default)]
pub struct InMemoryArtifactStore {
    versions: Arc<RwLock<BTreeMap<(String, String), ArtifactVersionRecord>>>,
    aliases: Arc<RwLock<BTreeMap<(String, String), String>>>,
}

impl InMemoryArtifactStore {
    /// Creates an empty in-memory artifact store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ArtifactStore for InMemoryArtifactStore {
    fn store(&self, version: ArtifactVersion) -> Result<(), StoreError> {
        if version.version().trim().is_empty() {
            return Err(StoreError::InvalidVersion);
        }

        if !version.content().is_valid() {
            return Err(StoreError::InvalidContentReference);
        }

        let key = (
            version.artifact_id().as_str().to_string(),
            version.version().to_string(),
        );

        let mut versions = self
            .versions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        if versions.contains_key(&key) {
            return Err(StoreError::AlreadyExists);
        }

        versions.insert(key, (&version).into());
        Ok(())
    }

    fn get(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
    ) -> Result<Option<ArtifactVersion>, StoreError> {
        if version.trim().is_empty() {
            return Err(StoreError::InvalidVersion);
        }

        let versions = self.versions.read().map_err(|_| StoreError::LockPoisoned)?;

        versions
            .get(&(artifact_id.as_str().to_string(), version.to_string()))
            .cloned()
            .map(ArtifactVersion::restore)
            .transpose()
            .map_err(map_restoration_error)
    }

    fn list_versions(&self, artifact_id: &ArtifactId) -> Result<Vec<ArtifactVersion>, StoreError> {
        let versions = self.versions.read().map_err(|_| StoreError::LockPoisoned)?;

        versions
            .iter()
            .filter(|((stored_artifact_id, _), _)| stored_artifact_id == artifact_id.as_str())
            .map(|(_, record)| {
                ArtifactVersion::restore(record.clone()).map_err(map_restoration_error)
            })
            .collect()
    }

    fn transition_lifecycle(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
        target: LifecycleState,
    ) -> Result<ArtifactVersion, StoreError> {
        if version.trim().is_empty() {
            return Err(StoreError::InvalidVersion);
        }

        let key = (artifact_id.as_str().to_string(), version.to_string());
        let mut versions = self
            .versions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        let current_record = versions.get(&key).ok_or(StoreError::NotFound)?;
        let current =
            ArtifactVersion::restore(current_record.clone()).map_err(map_restoration_error)?;

        if *current.lifecycle() == LifecycleState::Validated && target == LifecycleState::Published
        {
            return Err(StoreError::PublicationRequired);
        }

        let next_state = transition(*current.lifecycle(), target).map_err(|error| {
            StoreError::InvalidLifecycleTransition {
                from: error.from(),
                to: error.to(),
            }
        })?;

        let mut updated = current;
        updated.set_lifecycle(next_state);
        versions.insert(key, (&updated).into());

        Ok(updated)
    }

    fn publish_validated(
        &self,
        artifact_id: &ArtifactId,
        version: &str,
        proof: &IntegrityProof,
    ) -> Result<ArtifactVersion, StoreError> {
        if version.trim().is_empty() {
            return Err(StoreError::InvalidVersion);
        }

        let key = (artifact_id.as_str().to_string(), version.to_string());
        let mut versions = self
            .versions
            .write()
            .map_err(|_| StoreError::LockPoisoned)?;

        let current_record = versions.get(&key).ok_or(StoreError::NotFound)?;
        let current =
            ArtifactVersion::restore(current_record.clone()).map_err(map_restoration_error)?;

        if *current.lifecycle() != LifecycleState::Validated {
            return Err(StoreError::InvalidLifecycleTransition {
                from: *current.lifecycle(),
                to: LifecycleState::Published,
            });
        }

        if !current.content().is_valid() {
            return Err(StoreError::InvalidContentReference);
        }

        if !proof.matches(current.digest(), current.size()) {
            return Err(StoreError::IntegrityFailure);
        }

        let next_state =
            transition(LifecycleState::Validated, LifecycleState::Published).map_err(|error| {
                StoreError::InvalidLifecycleTransition {
                    from: error.from(),
                    to: error.to(),
                }
            })?;

        let mut updated = current;
        updated.set_lifecycle(next_state);
        versions.insert(key, (&updated).into());

        Ok(updated)
    }

    fn set_alias(
        &self,
        artifact_id: &ArtifactId,
        alias: &str,
        version: &str,
    ) -> Result<(), StoreError> {
        if alias.trim().is_empty() {
            return Err(StoreError::InvalidAlias);
        }

        if version.trim().is_empty() {
            return Err(StoreError::InvalidVersion);
        }

        let version_key = (artifact_id.as_str().to_string(), version.to_string());

        {
            let versions = self.versions.read().map_err(|_| StoreError::LockPoisoned)?;

            if !versions.contains_key(&version_key) {
                return Err(StoreError::NotFound);
            }
        }

        let alias_key = (artifact_id.as_str().to_string(), alias.to_string());

        let mut aliases = self.aliases.write().map_err(|_| StoreError::LockPoisoned)?;

        aliases.insert(alias_key, version.to_string());

        Ok(())
    }

    fn resolve_alias(
        &self,
        artifact_id: &ArtifactId,
        alias: &str,
    ) -> Result<Option<String>, StoreError> {
        if alias.trim().is_empty() {
            return Err(StoreError::InvalidAlias);
        }

        let aliases = self.aliases.read().map_err(|_| StoreError::LockPoisoned)?;

        Ok(aliases
            .get(&(artifact_id.as_str().to_string(), alias.to_string()))
            .cloned())
    }

    fn remove_alias(&self, artifact_id: &ArtifactId, alias: &str) -> Result<bool, StoreError> {
        if alias.trim().is_empty() {
            return Err(StoreError::InvalidAlias);
        }

        let mut aliases = self.aliases.write().map_err(|_| StoreError::LockPoisoned)?;

        Ok(aliases
            .remove(&(artifact_id.as_str().to_string(), alias.to_string()))
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::artifact::content::ContentReference;
    use crate::artifact::integrity::{ContentDigest, IntegrityProof};

    fn artifact_id() -> ArtifactId {
        ArtifactId::new("test-artifact").unwrap()
    }

    fn version(version: &str) -> ArtifactVersion {
        ArtifactVersion::new(
            artifact_id(),
            version,
            ContentReference::new("provider", "location"),
            ContentDigest::new(b"content"),
            7,
        )
    }

    #[test]
    fn stores_and_retrieves_version() {
        let store = InMemoryArtifactStore::new();
        let item = version("v1");

        store.store(item.clone()).unwrap();

        assert_eq!(store.get(&artifact_id(), "v1").unwrap(), Some(item));
    }

    #[test]
    fn duplicate_version_is_rejected() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v1")).unwrap();

        assert_eq!(store.store(version("v1")), Err(StoreError::AlreadyExists));
    }

    #[test]
    fn lists_versions_without_implying_version_order() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v2")).unwrap();
        store.store(version("v10")).unwrap();

        let versions = store.list_versions(&artifact_id()).unwrap();

        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|item| item.version() == "v2"));
        assert!(versions.iter().any(|item| item.version() == "v10"));
    }

    #[test]
    fn lifecycle_transition_cannot_bypass_publication() {
        let store = InMemoryArtifactStore::new();

        let mut item = version("v1");
        item.set_lifecycle(LifecycleState::Validated);
        store.store(item).unwrap();

        assert_eq!(
            store.transition_lifecycle(&artifact_id(), "v1", LifecycleState::Published),
            Err(StoreError::PublicationRequired)
        );
    }

    #[test]
    fn publish_validated_atomically_updates_existing_version() {
        let store = InMemoryArtifactStore::new();

        let mut item = version("v1");
        item.set_lifecycle(LifecycleState::Validated);
        store.store(item).unwrap();

        let content = b"content";
        let proof = IntegrityProof::verify(content, &ContentDigest::new(content)).unwrap();
        let updated = store
            .publish_validated(&artifact_id(), "v1", &proof)
            .unwrap();

        assert_eq!(updated.lifecycle(), &LifecycleState::Published);

        let stored = store.get(&artifact_id(), "v1").unwrap().unwrap();
        assert_eq!(stored.lifecycle(), &LifecycleState::Published);
    }

    #[test]
    fn publish_validated_rejects_an_integrity_mismatch() {
        let store = InMemoryArtifactStore::new();

        let mut item = version("v1");
        item.set_lifecycle(LifecycleState::Validated);
        store.store(item).unwrap();

        let proof = IntegrityProof::verify(
            b"tampered content",
            &ContentDigest::new(b"tampered content"),
        )
        .unwrap();

        assert_eq!(
            store.publish_validated(&artifact_id(), "v1", &proof),
            Err(StoreError::IntegrityFailure)
        );
        assert_eq!(
            store
                .get(&artifact_id(), "v1")
                .unwrap()
                .unwrap()
                .lifecycle(),
            &LifecycleState::Validated
        );
    }

    #[test]
    fn invalid_lifecycle_transition_is_rejected() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v1")).unwrap();

        let result = store.transition_lifecycle(&artifact_id(), "v1", LifecycleState::Published);

        assert!(matches!(
            result,
            Err(StoreError::InvalidLifecycleTransition {
                from: LifecycleState::Created,
                to: LifecycleState::Published
            })
        ));
    }

    #[test]
    fn publish_validated_rejects_a_stale_state() {
        let store = InMemoryArtifactStore::new();

        let mut item = version("v1");
        item.set_lifecycle(LifecycleState::Validated);
        store.store(item).unwrap();

        let content = b"content";
        let proof = IntegrityProof::verify(content, &ContentDigest::new(content)).unwrap();

        store
            .publish_validated(&artifact_id(), "v1", &proof)
            .unwrap();

        assert_eq!(
            store.publish_validated(&artifact_id(), "v1", &proof),
            Err(StoreError::InvalidLifecycleTransition {
                from: LifecycleState::Published,
                to: LifecycleState::Published,
            })
        );
    }

    #[test]
    fn missing_version_cannot_transition() {
        let store = InMemoryArtifactStore::new();

        assert_eq!(
            store.transition_lifecycle(&artifact_id(), "missing", LifecycleState::Published),
            Err(StoreError::NotFound)
        );
    }

    #[test]
    fn aliases_resolve_to_exact_versions() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v1")).unwrap();
        store.set_alias(&artifact_id(), "latest", "v1").unwrap();

        assert_eq!(
            store.resolve_alias(&artifact_id(), "latest").unwrap(),
            Some("v1".to_string())
        );
    }

    #[test]
    fn aliases_can_be_updated_without_replacing_versions() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v1")).unwrap();
        store.store(version("v2")).unwrap();

        store.set_alias(&artifact_id(), "latest", "v1").unwrap();
        store.set_alias(&artifact_id(), "latest", "v2").unwrap();

        assert_eq!(
            store.resolve_alias(&artifact_id(), "latest").unwrap(),
            Some("v2".to_string())
        );

        assert!(store.get(&artifact_id(), "v1").unwrap().is_some());
        assert!(store.get(&artifact_id(), "v2").unwrap().is_some());
    }

    #[test]
    fn removing_alias_does_not_remove_version() {
        let store = InMemoryArtifactStore::new();

        store.store(version("v1")).unwrap();
        store.set_alias(&artifact_id(), "latest", "v1").unwrap();

        assert!(store.remove_alias(&artifact_id(), "latest").unwrap());

        assert_eq!(store.resolve_alias(&artifact_id(), "latest").unwrap(), None);

        assert!(store.get(&artifact_id(), "v1").unwrap().is_some());
    }

    #[test]
    fn invalid_version_is_rejected() {
        let store = InMemoryArtifactStore::new();

        assert_eq!(store.store(version("   ")), Err(StoreError::InvalidVersion));
    }

    #[test]
    fn invalid_alias_is_rejected() {
        let store = InMemoryArtifactStore::new();

        assert_eq!(
            store.set_alias(&artifact_id(), "   ", "v1"),
            Err(StoreError::InvalidAlias)
        );
    }

    #[test]
    fn alias_requires_existing_target_version() {
        let store = InMemoryArtifactStore::new();

        assert_eq!(
            store.set_alias(&artifact_id(), "latest", "missing"),
            Err(StoreError::NotFound)
        );
    }
}
