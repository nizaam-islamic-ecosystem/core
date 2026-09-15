//! Resolution of artifact references to exact artifact versions.
//!
//! Resolution identifies exactly one `ArtifactVersion` from an
//! `ArtifactReference`.
//!
//! Exact version references are resolved directly.
//!
//! Mutable aliases are resolved through the artifact store's explicit alias
//! mapping. Resolution never infers version ordering from version identifiers.
//!
//! The store boundary restores persisted records into validated live versions
//! before they are returned here. Once resolution returns an exact version,
//! that version is the concrete execution reference and must not silently
//! change during the operation.

use crate::artifact::reference::{ArtifactReference, VersionSelector};
use crate::artifact::store::{ArtifactStore, StoreError};
use crate::artifact::version::ArtifactVersion;

/// Failure during artifact reference resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionError {
    /// The artifact reference contains an invalid selector.
    InvalidReference,

    /// An exact artifact version could not be found.
    VersionNotFound,

    /// A mutable alias could not be resolved.
    AliasNotFound,

    /// The resolved artifact version has been revoked and must not be used
    /// through the normal trusted resolution path.
    RevokedArtifact,

    /// The underlying store could not complete the resolution operation.
    ResolutionFailure,
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidReference => write!(formatter, "invalid artifact reference"),
            Self::VersionNotFound => write!(formatter, "artifact version not found"),
            Self::AliasNotFound => write!(formatter, "artifact alias not found"),
            Self::RevokedArtifact => write!(formatter, "artifact version is revoked"),
            Self::ResolutionFailure => write!(formatter, "artifact resolution failed"),
        }
    }
}

impl std::error::Error for ResolutionError {}

/// Resolves an [`ArtifactReference`] to exactly one [`ArtifactVersion`].
///
/// Exact references perform a direct version lookup.
///
/// Alias references use the store's explicit alias mapping. The resolver does
/// not infer aliases such as `latest` from version ordering.
pub fn resolve(
    reference: &ArtifactReference,
    store: &impl ArtifactStore,
) -> Result<ArtifactVersion, ResolutionError> {
    if !reference.is_valid() {
        return Err(ResolutionError::InvalidReference);
    }

    match reference.version() {
        VersionSelector::Exact(version) => resolve_exact(reference.artifact_id(), version, store),
        VersionSelector::Alias(alias) => resolve_alias(reference.artifact_id(), alias, store),
    }
}

/// Resolves an exact version identifier.
fn resolve_exact(
    artifact_id: &crate::identity::ArtifactId,
    version: &str,
    store: &impl ArtifactStore,
) -> Result<ArtifactVersion, ResolutionError> {
    let resolved = store
        .get(artifact_id, version)
        .map_err(map_store_error)?
        .ok_or(ResolutionError::VersionNotFound)?;

    ensure_resolvable(resolved)
}

/// Resolves a mutable alias through the explicit alias mapping maintained by
/// the artifact store.
fn resolve_alias(
    artifact_id: &crate::identity::ArtifactId,
    alias: &str,
    store: &impl ArtifactStore,
) -> Result<ArtifactVersion, ResolutionError> {
    let version = store
        .resolve_alias(artifact_id, alias)
        .map_err(map_store_error)?
        .ok_or(ResolutionError::AliasNotFound)?;

    let resolved = store
        .get(artifact_id, &version)
        .map_err(map_store_error)?
        .ok_or(ResolutionError::VersionNotFound)?;

    ensure_resolvable(resolved)
}

/// Applies the normal trusted-resolution lifecycle constraints.
///
/// Unpublished versions must not be returned through the normal published
/// resolution path. Revoked versions are never returned as trusted results.
///
/// Superseded and archived versions remain resolvable by exact identity,
/// subject to the later security/access policy.
fn ensure_resolvable(version: ArtifactVersion) -> Result<ArtifactVersion, ResolutionError> {
    match version.lifecycle() {
        crate::artifact::lifecycle::LifecycleState::Revoked => {
            Err(ResolutionError::RevokedArtifact)
        }
        crate::artifact::lifecycle::LifecycleState::Published
        | crate::artifact::lifecycle::LifecycleState::Superseded
        | crate::artifact::lifecycle::LifecycleState::Archived => Ok(version),

        crate::artifact::lifecycle::LifecycleState::Created
        | crate::artifact::lifecycle::LifecycleState::Validating
        | crate::artifact::lifecycle::LifecycleState::Validated => {
            Err(ResolutionError::VersionNotFound)
        }
    }
}

/// Converts a provider-neutral store failure into a resolution failure
/// without incorrectly treating storage failures as authorization decisions.
fn map_store_error(error: StoreError) -> ResolutionError {
    match error {
        StoreError::InvalidAlias | StoreError::InvalidVersion => ResolutionError::InvalidReference,
        StoreError::NotFound => ResolutionError::VersionNotFound,
        StoreError::AlreadyExists
        | StoreError::InvalidLifecycleTransition { .. }
        | StoreError::RestorationFailed
        | StoreError::PublicationRequired
        | StoreError::InvalidContentReference
        | StoreError::LockPoisoned => ResolutionError::ResolutionFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::artifact::content::ContentReference;
    use crate::artifact::integrity::ContentDigest;
    use crate::artifact::lifecycle::LifecycleState;
    use crate::artifact::reference::{ArtifactReference, VersionSelector};
    use crate::artifact::store::{ArtifactStore, InMemoryArtifactStore};
    use crate::identity::ArtifactId;

    fn artifact_id() -> ArtifactId {
        ArtifactId::new("test-artifact").unwrap()
    }

    fn sample_version(version: &str) -> ArtifactVersion {
        let mut artifact = ArtifactVersion::new(
            artifact_id(),
            version,
            ContentReference::new("provider", format!("location-{version}")),
            ContentDigest::new(b"digest"),
            100,
        );

        artifact.set_lifecycle(LifecycleState::Published);
        artifact
    }

    #[test]
    fn exact_reference_resolves_to_specific_version() {
        let store = InMemoryArtifactStore::new();
        store.store(sample_version("v5")).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v5");

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v5");
    }

    #[test]
    fn exact_reference_does_not_depend_on_version_ordering() {
        let store = InMemoryArtifactStore::new();

        store.store(sample_version("v10")).unwrap();
        store.store(sample_version("v2")).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v2");

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v2");
    }

    #[test]
    fn exact_reference_returns_version_not_found_for_missing_version() {
        let store = InMemoryArtifactStore::new();

        let reference = ArtifactReference::new(artifact_id(), "missing");

        let result = resolve(&reference, &store);

        assert!(matches!(result, Err(ResolutionError::VersionNotFound)));
    }

    #[test]
    fn alias_reference_uses_explicit_store_mapping() {
        let store = InMemoryArtifactStore::new();

        store.store(sample_version("v1")).unwrap();
        store.store(sample_version("v2")).unwrap();
        store.store(sample_version("v10")).unwrap();

        store.set_alias(&artifact_id(), "latest", "v2").unwrap();

        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v2");
    }

    #[test]
    fn alias_resolution_does_not_infer_highest_version() {
        let store = InMemoryArtifactStore::new();

        store.store(sample_version("v1")).unwrap();
        store.store(sample_version("v2")).unwrap();
        store.store(sample_version("v10")).unwrap();

        // Deliberately map `latest` to v1. Resolution must respect the
        // explicit alias mapping rather than infer ordering.
        store.set_alias(&artifact_id(), "latest", "v1").unwrap();

        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
    }

    #[test]
    fn missing_alias_returns_alias_not_found() {
        let store = InMemoryArtifactStore::new();

        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        let result = resolve(&reference, &store);

        assert!(matches!(result, Err(ResolutionError::AliasNotFound)));
    }

    #[test]
    fn alias_target_missing_version_returns_version_not_found() {
        let store = InMemoryArtifactStore::new();

        store.store(sample_version("v1")).unwrap();

        // The store normally prevents this mapping, so this test verifies
        // resolution remains defensive if a provider returns an invalid
        // alias target.
        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        assert!(matches!(
            resolve(&reference, &store),
            Err(ResolutionError::AliasNotFound)
        ));
    }

    #[test]
    fn invalid_reference_is_rejected_before_store_lookup() {
        let store = InMemoryArtifactStore::new();

        let reference = ArtifactReference::new(artifact_id(), "");

        let result = resolve(&reference, &store);

        assert!(matches!(result, Err(ResolutionError::InvalidReference)));
    }

    #[test]
    fn unpublished_version_is_not_returned_by_normal_resolution() {
        let store = InMemoryArtifactStore::new();

        let version = ArtifactVersion::new(
            artifact_id(),
            "v1",
            ContentReference::new("provider", "location"),
            ContentDigest::new(b"digest"),
            100,
        );

        store.store(version).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        let result = resolve(&reference, &store);

        assert!(matches!(result, Err(ResolutionError::VersionNotFound)));
    }

    #[test]
    fn superseded_version_remains_resolvable() {
        let store = InMemoryArtifactStore::new();

        let mut version = sample_version("v1");
        version.set_lifecycle(LifecycleState::Superseded);

        store.store(version).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
        assert_eq!(resolved.lifecycle(), &LifecycleState::Superseded);
    }

    #[test]
    fn archived_version_remains_resolvable() {
        let store = InMemoryArtifactStore::new();

        let mut version = sample_version("v1");
        version.set_lifecycle(LifecycleState::Archived);

        store.store(version).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v1");
        assert_eq!(resolved.lifecycle(), &LifecycleState::Archived);
    }

    #[test]
    fn revoked_version_is_not_resolvable() {
        let store = InMemoryArtifactStore::new();

        let mut version = sample_version("v1");
        version.set_lifecycle(LifecycleState::Revoked);

        store.store(version).unwrap();

        let reference = ArtifactReference::new(artifact_id(), "v1");

        let result = resolve(&reference, &store);

        assert!(matches!(result, Err(ResolutionError::RevokedArtifact)));
    }

    #[test]
    fn resolution_returns_exact_version_not_alias() {
        let store = InMemoryArtifactStore::new();

        store.store(sample_version("v1")).unwrap();
        store.store(sample_version("v2")).unwrap();
        store.set_alias(&artifact_id(), "latest", "v2").unwrap();

        let reference =
            ArtifactReference::with_selector(artifact_id(), VersionSelector::alias("latest"));

        let resolved = resolve(&reference, &store).unwrap();

        assert_eq!(resolved.version(), "v2");
    }
}
