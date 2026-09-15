//! Integration tests for the public Phase 10 artifact boundary.
//!
//! These tests exercise the Artifact subsystem as an external Core consumer
//! would use it. Individual implementation details are covered by the unit
//! tests inside `src/artifact/*`; this file focuses on behavior across the
//! public artifact identity, versioning, storage, lifecycle, integrity,
//! publication, reference, and resolution boundaries.

use nizaam_core::artifact::{
    Artifact, ArtifactReference, ArtifactStore, ArtifactVersion, ContentDigest, ContentReference,
    InMemoryArtifactStore, LifecycleState, PublicationError, ResolutionError, StoreError,
    VersionSelector, publish, resolve, verify_integrity,
};
use nizaam_core::identity::ArtifactId;

fn artifact_id(value: &str) -> ArtifactId {
    ArtifactId::new(value).unwrap()
}

fn content(value: &str) -> ContentReference {
    ContentReference::new("test-provider", format!("content/{value}"))
}

fn version(artifact_id: &ArtifactId, version: &str, bytes: &[u8]) -> ArtifactVersion {
    ArtifactVersion::new(
        artifact_id.clone(),
        version,
        content(version),
        ContentDigest::new(bytes),
        bytes.len() as u64,
    )
}

fn validated_version(artifact_id: &ArtifactId, version_id: &str, bytes: &[u8]) -> ArtifactVersion {
    let artifact_version = version(artifact_id, version_id, bytes)
        .with_metadata("environment", "integration-test")
        .unwrap();

    // `set_lifecycle` is intentionally crate-visible, so repository-level
    // integration tests must use the public store lifecycle transition API.
    let store = InMemoryArtifactStore::new();
    store.store(artifact_version).unwrap();
    store
        .transition_lifecycle(artifact_id, version_id, LifecycleState::Validating)
        .unwrap();
    store
        .transition_lifecycle(artifact_id, version_id, LifecycleState::Validated)
        .unwrap();

    store.get(artifact_id, version_id).unwrap().unwrap()
}

#[test]
fn logical_artifact_identity_is_stable_across_multiple_versions() {
    let id = artifact_id("dataset.example");
    let artifact = Artifact::new(id.clone());

    let v1 = version(&id, "v1", b"first version");
    let v2 = version(&id, "v2", b"second version");

    assert_eq!(artifact.id(), &id);
    assert_eq!(v1.artifact_id(), &id);
    assert_eq!(v2.artifact_id(), &id);
    assert_ne!(v1.version(), v2.version());
}

#[test]
fn distinct_logical_artifacts_remain_distinct_even_with_identical_content() {
    let first_id = artifact_id("artifact.first");
    let second_id = artifact_id("artifact.second");
    let bytes = b"shared content";

    let first = version(&first_id, "v1", bytes);
    let second = version(&second_id, "v1", bytes);

    assert_eq!(first.digest(), second.digest());
    assert_ne!(first.artifact_id(), second.artifact_id());
    assert_eq!(first.version(), second.version());
}

#[test]
fn artifact_store_preserves_multiple_versions_and_their_metadata() {
    let id = artifact_id("model.example");
    let store = InMemoryArtifactStore::new();

    let v1 = version(&id, "v1", b"model-v1")
        .with_metadata("format", "binary")
        .unwrap();
    let v2 = version(&id, "v2", b"model-v2")
        .with_metadata("format", "binary")
        .unwrap();

    store.store(v1.clone()).unwrap();
    store.store(v2.clone()).unwrap();

    let stored_v1 = store.get(&id, "v1").unwrap().unwrap();
    let stored_v2 = store.get(&id, "v2").unwrap().unwrap();

    assert_eq!(stored_v1, v1);
    assert_eq!(stored_v2, v2);

    let versions = store.list_versions(&id).unwrap();

    assert_eq!(versions.len(), 2);
    assert!(versions.iter().any(|item| item.version() == "v1"));
    assert!(versions.iter().any(|item| item.version() == "v2"));
}

#[test]
fn duplicate_artifact_version_is_rejected_without_replacing_existing_state() {
    let id = artifact_id("artifact.duplicate");
    let store = InMemoryArtifactStore::new();

    let original = version(&id, "v1", b"original");
    let replacement = version(&id, "v1", b"replacement");

    store.store(original.clone()).unwrap();

    let result = store.store(replacement);

    assert_eq!(result, Err(StoreError::AlreadyExists));
    assert_eq!(store.get(&id, "v1").unwrap().unwrap(), original);
}

#[test]
fn exact_reference_resolves_to_the_requested_published_version() {
    let id = artifact_id("artifact.exact");
    let store = InMemoryArtifactStore::new();

    let validated = validated_version(&id, "v1", b"exact-version");
    store.store(validated).unwrap();

    publish(&id, "v1", &store).unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v1");
    assert_eq!(resolved.lifecycle(), &LifecycleState::Published);
}

#[test]
fn alias_resolution_uses_explicit_store_mapping() {
    let id = artifact_id("artifact.alias");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"version-one"))
        .unwrap();
    store
        .store(validated_version(&id, "v2", b"version-two"))
        .unwrap();

    publish(&id, "v1", &store).unwrap();
    publish(&id, "v2", &store).unwrap();

    store.set_alias(&id, "latest", "v2").unwrap();

    let reference = ArtifactReference::with_selector(id.clone(), VersionSelector::alias("latest"));

    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v2");

    // Changing the alias changes future alias resolution, not the identity
    // of an already resolved exact version.
    store.set_alias(&id, "latest", "v1").unwrap();

    let resolved_again = resolve(&reference, &store).unwrap();

    assert_eq!(resolved_again.version(), "v1");
}

#[test]
fn exact_version_remains_bound_after_alias_target_changes() {
    let id = artifact_id("artifact.binding");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"version-one"))
        .unwrap();
    store
        .store(validated_version(&id, "v2", b"version-two"))
        .unwrap();

    publish(&id, "v1", &store).unwrap();
    publish(&id, "v2", &store).unwrap();

    store.set_alias(&id, "latest", "v2").unwrap();

    let alias_reference =
        ArtifactReference::with_selector(id.clone(), VersionSelector::alias("latest"));

    let resolved_from_alias = resolve(&alias_reference, &store).unwrap();

    assert_eq!(resolved_from_alias.version(), "v2");

    let exact_reference =
        ArtifactReference::new(id.clone(), resolved_from_alias.version().to_string());

    store.set_alias(&id, "latest", "v1").unwrap();

    let exact_resolved = resolve(&exact_reference, &store).unwrap();

    assert_eq!(exact_resolved.version(), "v2");
}

#[test]
fn unpublished_versions_are_outside_the_normal_resolution_path() {
    let id = artifact_id("artifact.unpublished");
    let store = InMemoryArtifactStore::new();

    store
        .store(version(&id, "v1", b"not-yet-published"))
        .unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let result = resolve(&reference, &store);

    assert_eq!(result, Err(ResolutionError::VersionNotFound));
}

#[test]
fn publication_transitions_validated_version_without_changing_immutable_data() {
    let id = artifact_id("artifact.publication");
    let store = InMemoryArtifactStore::new();
    let bytes = b"publication-content";

    let validated = validated_version(&id, "v1", bytes);
    store.store(validated.clone()).unwrap();

    publish(&id, "v1", &store).unwrap();

    let published = store.get(&id, "v1").unwrap().unwrap();

    assert_eq!(published.artifact_id(), validated.artifact_id());
    assert_eq!(published.version(), validated.version());
    assert_eq!(published.content(), validated.content());
    assert_eq!(published.digest(), validated.digest());
    assert_eq!(published.size(), validated.size());
    assert_eq!(published.metadata(), validated.metadata());
    assert_eq!(published.lifecycle(), &LifecycleState::Published);
}

#[test]
fn generic_lifecycle_transition_cannot_publish_a_validated_version() {
    let id = artifact_id("artifact.publication-boundary");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"validated-content"))
        .unwrap();

    assert_eq!(
        store.transition_lifecycle(&id, "v1", LifecycleState::Published),
        Err(StoreError::PublicationRequired)
    );

    assert_eq!(
        store.get(&id, "v1").unwrap().unwrap().lifecycle(),
        &LifecycleState::Validated
    );
}

#[test]
fn publishing_a_created_version_fails_without_changing_lifecycle() {
    let id = artifact_id("artifact.invalid-publication");
    let store = InMemoryArtifactStore::new();

    store.store(version(&id, "v1", b"created-content")).unwrap();

    let result = publish(&id, "v1", &store);

    assert_eq!(
        result,
        Err(PublicationError::InvalidState {
            actual: LifecycleState::Created,
        })
    );

    let stored = store.get(&id, "v1").unwrap().unwrap();

    assert_eq!(stored.lifecycle(), &LifecycleState::Created);
}

#[test]
fn superseded_exact_version_remains_resolvable() {
    let id = artifact_id("artifact.superseded");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"old-version"))
        .unwrap();
    store
        .store(validated_version(&id, "v2", b"new-version"))
        .unwrap();

    publish(&id, "v1", &store).unwrap();
    publish(&id, "v2", &store).unwrap();

    store
        .transition_lifecycle(&id, "v1", LifecycleState::Superseded)
        .unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.version(), "v1");
    assert_eq!(resolved.lifecycle(), &LifecycleState::Superseded);
}

#[test]
fn superseded_version_can_be_revoked_and_then_is_rejected_by_resolution() {
    let id = artifact_id("artifact.superseded-revoked");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"old-version"))
        .unwrap();
    publish(&id, "v1", &store).unwrap();

    store
        .transition_lifecycle(&id, "v1", LifecycleState::Superseded)
        .unwrap();
    store
        .transition_lifecycle(&id, "v1", LifecycleState::Revoked)
        .unwrap();

    let reference = ArtifactReference::new(id, "v1");

    assert_eq!(
        resolve(&reference, &store),
        Err(ResolutionError::RevokedArtifact)
    );
}

#[test]
fn revoked_version_is_rejected_by_normal_resolution() {
    let id = artifact_id("artifact.revoked");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"unsafe-version"))
        .unwrap();

    publish(&id, "v1", &store).unwrap();

    store
        .transition_lifecycle(&id, "v1", LifecycleState::Revoked)
        .unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let result = resolve(&reference, &store);

    assert_eq!(result, Err(ResolutionError::RevokedArtifact));
}

#[test]
fn stored_digest_verifies_original_content_and_detects_tampering() {
    let id = artifact_id("artifact.integrity");
    let original_content = b"trusted artifact content";
    let tampered_content = b"tampered artifact content";

    let artifact_version = version(&id, "v1", original_content);

    assert!(verify_integrity(original_content, artifact_version.digest()).is_ok());

    assert!(verify_integrity(tampered_content, artifact_version.digest()).is_err());
}

#[test]
fn artifact_reference_distinguishes_exact_versions_from_aliases() {
    let id = artifact_id("artifact.references");

    let exact = ArtifactReference::new(id.clone(), "v1");
    let alias = ArtifactReference::with_selector(id, VersionSelector::alias("latest"));

    assert!(exact.is_exact());
    assert!(!exact.is_alias());
    assert!(exact.is_valid());

    assert!(alias.is_alias());
    assert!(!alias.is_exact());
    assert!(alias.is_valid());

    assert_eq!(exact.version().as_str(), "v1");
    assert_eq!(alias.version().as_str(), "latest");
}

#[test]
fn removing_alias_does_not_remove_the_underlying_artifact_version() {
    let id = artifact_id("artifact.alias-removal");
    let store = InMemoryArtifactStore::new();

    store
        .store(validated_version(&id, "v1", b"persistent-version"))
        .unwrap();

    publish(&id, "v1", &store).unwrap();
    store.set_alias(&id, "latest", "v1").unwrap();

    assert_eq!(
        store.resolve_alias(&id, "latest").unwrap(),
        Some("v1".to_string())
    );

    assert!(store.remove_alias(&id, "latest").unwrap());

    assert_eq!(store.resolve_alias(&id, "latest").unwrap(), None);

    let exact = store.get(&id, "v1").unwrap().unwrap();

    assert_eq!(exact.version(), "v1");
    assert_eq!(exact.lifecycle(), &LifecycleState::Published);
}
