//! Publication behavior for artifact versions.
//!
//! Publication conceptually performs:
//!
//! VALIDATED → PUBLISH → PUBLISHED
//!
//! Publication must behave as an atomic externally visible transition.
//!
//! A version must not become published unless the required publication
//! invariants are satisfied:
//!
//! - the artifact version exists;
//! - the version is in `Validated` state;
//! - a content reference exists and is valid;
//! - integrity information exists;
//! - the lifecycle transition to `Published` is valid.
//!
//! Authorization is intentionally not implemented here. Artifact access
//! authorization is provided by the Phase 9 security boundary.

use crate::artifact::integrity::IntegrityProof;
use crate::artifact::lifecycle::LifecycleState;
use crate::artifact::store::{ArtifactStore, StoreError};
use crate::identity::ArtifactId;

/// Error during publication of an artifact version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationError {
    /// The requested artifact version does not exist.
    NotFound,

    /// The artifact version is not in the required `Validated` state.
    InvalidState { actual: LifecycleState },

    /// The content reference is invalid.
    InvalidContentReference,

    /// The supplied integrity evidence does not match the recorded content.
    IntegrityFailure,

    /// The underlying artifact store failed.
    Failed,
}

impl std::fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(formatter, "artifact version not found"),
            Self::InvalidState { actual } => write!(
                formatter,
                "artifact version is not publishable from state {:?}",
                actual
            ),
            Self::InvalidContentReference => {
                write!(formatter, "artifact content reference is invalid")
            }
            Self::IntegrityFailure => write!(
                formatter,
                "artifact publication failed integrity verification"
            ),
            Self::Failed => write!(formatter, "artifact publication failed"),
        }
    }
}

impl std::error::Error for PublicationError {}

/// Publishes a validated artifact version.
///
/// The operation verifies publication prerequisites and then uses the store's
/// atomic `Validated → Published` compare-and-transition operation.
pub fn publish(
    artifact_id: &ArtifactId,
    version: &str,
    store: &impl ArtifactStore,
    proof: &IntegrityProof,
) -> Result<(), PublicationError> {
    let artifact_version = store
        .get(artifact_id, version)
        .map_err(map_store_error)?
        .ok_or(PublicationError::NotFound)?;

    if artifact_version.lifecycle() != &LifecycleState::Validated {
        return Err(PublicationError::InvalidState {
            actual: *artifact_version.lifecycle(),
        });
    }

    if !artifact_version.content().is_valid() {
        return Err(PublicationError::InvalidContentReference);
    }

    store
        .publish_validated(artifact_id, version, proof)
        .map_err(map_store_error)?;

    Ok(())
}

/// Converts storage failures into publication failures.
fn map_store_error(error: StoreError) -> PublicationError {
    match error {
        StoreError::NotFound | StoreError::InvalidVersion => PublicationError::NotFound,
        StoreError::InvalidContentReference => PublicationError::InvalidContentReference,
        StoreError::IntegrityFailure => PublicationError::IntegrityFailure,
        StoreError::InvalidLifecycleTransition { from, .. } => {
            PublicationError::InvalidState { actual: from }
        }
        StoreError::InvalidAlias
        | StoreError::AlreadyExists
        | StoreError::PublicationRequired
        | StoreError::RestorationFailed
        | StoreError::LockPoisoned => PublicationError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::artifact::content::ContentReference;
    use crate::artifact::integrity::{ContentDigest, IntegrityProof};
    use crate::artifact::store::{ArtifactStore, InMemoryArtifactStore};
    use crate::artifact::version::ArtifactVersion;

    fn artifact_id() -> ArtifactId {
        ArtifactId::new("test-artifact").unwrap()
    }

    fn content() -> &'static [u8] {
        b"artifact content"
    }

    fn proof() -> IntegrityProof {
        IntegrityProof::verify(content(), &ContentDigest::new(content())).unwrap()
    }

    fn validated_version() -> ArtifactVersion {
        let mut version = ArtifactVersion::new(
            artifact_id(),
            "v1",
            ContentReference::new("provider", "location"),
            ContentDigest::new(b"artifact content"),
            16,
        );

        version.set_lifecycle(LifecycleState::Validated);
        version
    }

    #[test]
    fn publishes_validated_version() {
        let store = InMemoryArtifactStore::new();
        store.store(validated_version()).unwrap();

        assert_eq!(publish(&artifact_id(), "v1", &store, &proof()), Ok(()));

        let published = store.get(&artifact_id(), "v1").unwrap().unwrap();

        assert_eq!(published.lifecycle(), &LifecycleState::Published);
    }

    #[test]
    fn missing_version_returns_not_found() {
        let store = InMemoryArtifactStore::new();

        assert_eq!(
            publish(&artifact_id(), "missing", &store, &proof()),
            Err(PublicationError::NotFound)
        );
    }

    #[test]
    fn created_version_cannot_be_published() {
        let store = InMemoryArtifactStore::new();

        store
            .store(ArtifactVersion::new(
                artifact_id(),
                "v1",
                ContentReference::new("provider", "location"),
                ContentDigest::new(b"artifact content"),
                16,
            ))
            .unwrap();

        assert_eq!(
            publish(&artifact_id(), "v1", &store, &proof()),
            Err(PublicationError::InvalidState {
                actual: LifecycleState::Created
            })
        );
    }

    #[test]
    fn published_version_cannot_be_published_again() {
        let store = InMemoryArtifactStore::new();

        let mut version = validated_version();
        version.set_lifecycle(LifecycleState::Published);

        store.store(version).unwrap();

        assert_eq!(
            publish(&artifact_id(), "v1", &store, &proof()),
            Err(PublicationError::InvalidState {
                actual: LifecycleState::Published
            })
        );
    }

    #[test]
    fn revoked_version_cannot_be_published() {
        let store = InMemoryArtifactStore::new();

        let mut version = validated_version();
        version.set_lifecycle(LifecycleState::Revoked);

        store.store(version).unwrap();

        assert_eq!(
            publish(&artifact_id(), "v1", &store, &proof()),
            Err(PublicationError::InvalidState {
                actual: LifecycleState::Revoked
            })
        );
    }

    #[test]
    fn concurrent_publication_allows_only_one_winner() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let store = Arc::new(InMemoryArtifactStore::new());
        store.store(validated_version()).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();

        let proof = Arc::new(proof());

        for _ in 0..2 {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let proof = Arc::clone(&proof);

            handles.push(thread::spawn(move || {
                barrier.wait();
                publish(&artifact_id(), "v1", store.as_ref(), proof.as_ref())
            }));
        }

        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert!(results.iter().any(|result| {
            matches!(
                result,
                Err(PublicationError::InvalidState {
                    actual: LifecycleState::Published
                })
            )
        }));
        assert_eq!(
            store
                .get(&artifact_id(), "v1")
                .unwrap()
                .unwrap()
                .lifecycle(),
            &LifecycleState::Published
        );
    }

    #[test]
    fn publication_rejects_unverified_content() {
        let store = InMemoryArtifactStore::new();
        store.store(validated_version()).unwrap();

        let wrong_proof = IntegrityProof::verify(
            b"tampered content",
            &ContentDigest::new(b"artifact content"),
        );

        assert!(wrong_proof.is_err());

        let proof_for_wrong_digest = IntegrityProof::verify(
            b"tampered content",
            &ContentDigest::new(b"tampered content"),
        )
        .unwrap();

        assert_eq!(
            publish(&artifact_id(), "v1", &store, &proof_for_wrong_digest),
            Err(PublicationError::IntegrityFailure)
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
    fn publication_rejects_a_stale_validated_expectation() {
        let store = InMemoryArtifactStore::new();
        store.store(validated_version()).unwrap();

        publish(&artifact_id(), "v1", &store, &proof()).unwrap();

        assert_eq!(
            publish(&artifact_id(), "v1", &store, &proof()),
            Err(PublicationError::InvalidState {
                actual: LifecycleState::Published
            })
        );
    }

    #[test]
    fn publication_preserves_version_identity_and_content() {
        let store = InMemoryArtifactStore::new();
        let version = validated_version();

        store.store(version.clone()).unwrap();
        publish(&artifact_id(), "v1", &store, &proof()).unwrap();

        let published = store.get(&artifact_id(), "v1").unwrap().unwrap();

        assert_eq!(published.artifact_id(), version.artifact_id());
        assert_eq!(published.version(), version.version());
        assert_eq!(published.content(), version.content());
        assert_eq!(published.digest(), version.digest());
        assert_eq!(published.size(), version.size());
        assert_eq!(published.metadata(), version.metadata());
    }
}
