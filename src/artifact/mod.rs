//! Phase 10 boundary for domain agnostic artifact identity, versioning,
//! references, content access, integrity, lifecycle, and publication
//! mechanisms.
//!
//! Core owns artifact mechanisms and historical relationships. Core does NOT
//! own domain semantics, domain-specific validation, engine storage, or
//! artifact creation logic specific to an engine.

mod content;
mod identity;
mod integrity;
mod lifecycle;
mod publication;
mod reference;
mod resolution;
mod store;
mod version;

pub use content::ContentReference;
pub use identity::Artifact;
pub use integrity::{ContentDigest, IntegrityError, SHA256_DIGEST_LENGTH, verify_integrity};
pub use lifecycle::{LifecycleError, LifecycleState, can_transition, transition};
pub use publication::{PublicationError, publish};
pub use reference::{ArtifactReference, VersionSelector};
pub use resolution::{ResolutionError, resolve};
pub use store::{ArtifactStore, InMemoryArtifactStore, StoreError};
pub use version::{
    ArtifactVersion, ArtifactVersionError, ArtifactVersionRecord, ArtifactVersionRestoreError,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ArtifactId;

    fn artifact_id() -> ArtifactId {
        ArtifactId::new("artifact-module-integration").unwrap()
    }

    fn new_version(version: &str) -> ArtifactVersion {
        let content = b"artifact module integration content";

        ArtifactVersion::new(
            artifact_id(),
            version,
            ContentReference::new("test-provider", format!("artifacts/{version}")),
            ContentDigest::new(content),
            content.len() as u64,
        )
    }

    fn validate_version(store: &InMemoryArtifactStore, version: &str) {
        store.store(new_version(version)).unwrap();

        store
            .transition_lifecycle(&artifact_id(), version, LifecycleState::Validating)
            .unwrap();

        store
            .transition_lifecycle(&artifact_id(), version, LifecycleState::Validated)
            .unwrap();
    }

    #[test]
    fn artifact_submodules_work_together_for_one_version() {
        let artifact = Artifact::new(artifact_id());

        let version = new_version("v1");

        let content = b"artifact module integration content";
        assert_eq!(artifact.id(), version.artifact_id());
        assert_eq!(version.version(), "v1");

        assert!(version.content().is_valid());
        assert_eq!(ContentDigest::algorithm(), "sha-256");
        assert_eq!(version.digest().as_bytes().len(), SHA256_DIGEST_LENGTH);
        assert_eq!(version.size(), content.len() as u64);
        assert_eq!(version.lifecycle(), &LifecycleState::Created);
    }

    #[test]
    fn lifecycle_and_store_work_together() {
        let store = InMemoryArtifactStore::new();

        store.store(new_version("v1")).unwrap();

        let created = store.get(&artifact_id(), "v1").unwrap().unwrap();
        assert_eq!(created.lifecycle(), &LifecycleState::Created);

        let validating = store
            .transition_lifecycle(&artifact_id(), "v1", LifecycleState::Validating)
            .unwrap();

        assert_eq!(validating.lifecycle(), &LifecycleState::Validating);

        let validated = store
            .transition_lifecycle(&artifact_id(), "v1", LifecycleState::Validated)
            .unwrap();

        assert_eq!(validated.lifecycle(), &LifecycleState::Validated);

        publish(&artifact_id(), "v1", &store).unwrap();

        let published = store.get(&artifact_id(), "v1").unwrap().unwrap();

        assert_eq!(published.lifecycle(), &LifecycleState::Published);

        assert_eq!(
            transition(LifecycleState::Published, LifecycleState::Superseded).unwrap(),
            LifecycleState::Superseded
        );
    }

    #[test]
    fn publication_uses_store_lifecycle_transition() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");

        publish(&artifact_id(), "v1", &store).unwrap();

        let version = store.get(&artifact_id(), "v1").unwrap().unwrap();

        assert_eq!(version.lifecycle(), &LifecycleState::Published);
    }

    #[test]
    fn publication_rejects_unvalidated_versions() {
        let store = InMemoryArtifactStore::new();

        store.store(new_version("v1")).unwrap();

        let result = publish(&artifact_id(), "v1", &store);

        assert_eq!(
            result,
            Err(PublicationError::InvalidState {
                actual: LifecycleState::Created
            })
        );

        let stored = store.get(&artifact_id(), "v1").unwrap().unwrap();

        assert_eq!(stored.lifecycle(), &LifecycleState::Created);
    }

    #[test]
    fn exact_reference_resolves_published_version() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");
        publish(&artifact_id(), "v1", &store).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        assert!(reference.is_exact());
        assert!(reference.is_valid());

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
        assert_eq!(resolved.lifecycle(), &LifecycleState::Published);
    }

    #[test]
    fn alias_reference_resolves_through_store_mapping() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");
        validate_version(&store, "v2");

        publish(&artifact_id(), "v1", &store).unwrap();
        publish(&artifact_id(), "v2", &store).unwrap();

        store.set_alias(&artifact_id(), "latest", "v2").unwrap();

        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        assert!(reference.is_alias());
        assert!(reference.is_valid());

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v2");
        assert_eq!(resolved.lifecycle(), &LifecycleState::Published);
    }

    #[test]
    fn exact_reference_remains_bound_when_alias_changes() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");
        validate_version(&store, "v2");

        publish(&artifact_id(), "v1", &store).unwrap();
        publish(&artifact_id(), "v2", &store).unwrap();

        store.set_alias(&artifact_id(), "latest", "v1").unwrap();

        let exact = ArtifactReference::new(artifact_id(), "v1");

        store.set_alias(&artifact_id(), "latest", "v2").unwrap();

        let resolved = resolve(&exact, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
    }

    #[test]
    fn integrity_binds_content_to_artifact_version() {
        let content = b"artifact module integration content";

        let digest = ContentDigest::new(content);

        let version = ArtifactVersion::new(
            artifact_id(),
            "v1",
            ContentReference::new("test-provider", "artifacts/v1"),
            digest.clone(),
            content.len() as u64,
        );

        assert_eq!(version.digest(), &digest);
        assert!(verify_integrity(content, version.digest()).is_ok());
        assert!(verify_integrity(b"modified artifact module content", version.digest()).is_err());
    }

    #[test]
    fn revoked_version_is_not_resolvable() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");
        publish(&artifact_id(), "v1", &store).unwrap();

        store
            .transition_lifecycle(&artifact_id(), "v1", LifecycleState::Revoked)
            .unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        assert_eq!(
            resolve(&reference, &store),
            Err(ResolutionError::RevokedArtifact)
        );
    }

    #[test]
    fn superseded_version_remains_resolvable_by_exact_identity() {
        let store = InMemoryArtifactStore::new();

        validate_version(&store, "v1");
        publish(&artifact_id(), "v1", &store).unwrap();

        store
            .transition_lifecycle(&artifact_id(), "v1", LifecycleState::Superseded)
            .unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
        assert_eq!(resolved.lifecycle(), &LifecycleState::Superseded);
    }
}
