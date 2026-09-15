//! Integration tests for the Phase 10 Core artifact pipeline.
//!
//! These tests exercise Artifact behavior across the public Core boundaries:
//! identity, versioning, storage, lifecycle, publication, references,
//! resolution, execution context, and integrity.
//!
//! Individual implementation details belong to the unit tests inside
//! `src/artifact/*`; this file verifies that the pieces work together.

use nizaam_core::artifact::{
    Artifact, ArtifactReference, ArtifactStore, ArtifactVersion, ContentDigest, ContentReference,
    InMemoryArtifactStore, IntegrityProof, LifecycleState, PublicationError, ResolutionError,
    VersionSelector, publish, resolve, verify_integrity,
};
use nizaam_core::identity::{ArtifactId, CorrelationId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;

fn artifact_id(value: &str) -> ArtifactId {
    ArtifactId::new(value).unwrap()
}

fn content_reference(version_id: &str) -> ContentReference {
    ContentReference::new("test-provider", format!("artifacts/{version_id}"))
}

fn artifact_version(id: &ArtifactId, version_id: &str, content: &[u8]) -> ArtifactVersion {
    ArtifactVersion::new(
        id.clone(),
        version_id,
        content_reference(version_id),
        ContentDigest::new(content),
        content.len() as u64,
    )
}

fn integrity_proof(content: &[u8]) -> IntegrityProof {
    IntegrityProof::verify(content, &ContentDigest::new(content)).unwrap()
}

fn store_validated_version(
    store: &InMemoryArtifactStore,
    id: &ArtifactId,
    version_id: &str,
    content: &[u8],
) {
    store
        .store(artifact_version(id, version_id, content))
        .unwrap();

    store
        .transition_lifecycle(id, version_id, LifecycleState::Validating)
        .unwrap();

    store
        .transition_lifecycle(id, version_id, LifecycleState::Validated)
        .unwrap();
}

fn publish_version(
    store: &InMemoryArtifactStore,
    id: &ArtifactId,
    version_id: &str,
    content: &[u8],
) {
    store_validated_version(store, id, version_id, content);
    publish(id, version_id, store, &integrity_proof(content)).unwrap();
}

fn engine_context() -> EngineContext {
    let operation = Operation::new(
        OperationId::new("artifact-pipeline-operation").unwrap(),
        CorrelationId::new("artifact-pipeline-correlation").unwrap(),
    );

    let operation_context = OperationContext::new(operation);

    EngineContext::new(operation_context)
}

#[test]
fn artifact_lifecycle_works_across_identity_storage_publication_and_resolution() {
    let id = artifact_id("artifact.lifecycle");
    let store = InMemoryArtifactStore::new();
    let bytes = b"artifact lifecycle content";

    let artifact = Artifact::new(id.clone());

    store_validated_version(&store, &id, "v1", bytes);

    let validated = store.get(&id, "v1").unwrap().unwrap();

    assert_eq!(artifact.id(), &id);
    assert_eq!(validated.artifact_id(), &id);
    assert_eq!(validated.version(), "v1");
    assert_eq!(validated.lifecycle(), &LifecycleState::Validated);

    publish(&id, "v1", &store, &integrity_proof(bytes)).unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v1");
    assert_eq!(resolved.lifecycle(), &LifecycleState::Published);
}

#[test]
fn multiple_versions_share_identity_but_preserve_distinct_logical_state() {
    let id = artifact_id("artifact.versions");
    let store = InMemoryArtifactStore::new();

    let v1_content = b"version-one";
    let v2_content = b"version-two";

    publish_version(&store, &id, "v1", v1_content);
    publish_version(&store, &id, "v2", v2_content);

    let v1 = store.get(&id, "v1").unwrap().unwrap();
    let v2 = store.get(&id, "v2").unwrap().unwrap();

    assert_eq!(v1.artifact_id(), &id);
    assert_eq!(v2.artifact_id(), &id);

    assert_eq!(v1.version(), "v1");
    assert_eq!(v2.version(), "v2");

    assert_ne!(v1.digest(), v2.digest());
    assert_ne!(v1.content(), v2.content());
    assert_eq!(v1.lifecycle(), &LifecycleState::Published);
    assert_eq!(v2.lifecycle(), &LifecycleState::Published);
}

#[test]
fn exact_reference_remains_bound_to_one_version() {
    let id = artifact_id("artifact.exact-binding");
    let store = InMemoryArtifactStore::new();

    publish_version(&store, &id, "v1", b"first");
    publish_version(&store, &id, "v2", b"second");

    let reference = ArtifactReference::new(id.clone(), "v1");

    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v1");
}

#[test]
fn alias_resolution_can_be_converted_into_stable_exact_reference() {
    let id = artifact_id("artifact.alias-binding");
    let store = InMemoryArtifactStore::new();

    publish_version(&store, &id, "v1", b"first");
    publish_version(&store, &id, "v2", b"second");

    store.set_alias(&id, "latest", "v2").unwrap();

    let alias_reference =
        ArtifactReference::with_selector(id.clone(), VersionSelector::alias("latest"));

    let resolved = resolve(&alias_reference, &store).unwrap();

    assert_eq!(resolved.version(), "v2");

    let exact_reference = ArtifactReference::new(id.clone(), resolved.version().to_owned());

    store.set_alias(&id, "latest", "v1").unwrap();

    let exact_resolved = resolve(&exact_reference, &store).unwrap();
    let alias_resolved = resolve(&alias_reference, &store).unwrap();

    assert_eq!(exact_resolved.version(), "v2");
    assert_eq!(alias_resolved.version(), "v1");
}

#[test]
fn unpublished_version_is_not_available_through_normal_resolution() {
    let id = artifact_id("artifact.unpublished");
    let store = InMemoryArtifactStore::new();

    store
        .store(artifact_version(&id, "v1", b"unpublished content"))
        .unwrap();

    let reference = ArtifactReference::new(id, "v1");

    let result = resolve(&reference, &store);

    assert_eq!(result, Err(ResolutionError::VersionNotFound));
}

#[test]
fn publication_preserves_version_identity_content_and_integrity() {
    let id = artifact_id("artifact.publication");
    let store = InMemoryArtifactStore::new();
    let bytes = b"publication payload";

    store_validated_version(&store, &id, "v1", bytes);

    let before = store.get(&id, "v1").unwrap().unwrap();

    publish(&id, "v1", &store, &integrity_proof(bytes)).unwrap();

    let after = store.get(&id, "v1").unwrap().unwrap();

    assert_eq!(before.artifact_id(), after.artifact_id());
    assert_eq!(before.version(), after.version());
    assert_eq!(before.content(), after.content());
    assert_eq!(before.digest(), after.digest());
    assert_eq!(before.size(), after.size());
    assert_eq!(before.metadata(), after.metadata());

    assert_eq!(before.lifecycle(), &LifecycleState::Validated);
    assert_eq!(after.lifecycle(), &LifecycleState::Published);
}

#[test]
fn created_version_cannot_be_published() {
    let id = artifact_id("artifact.publication-state");
    let store = InMemoryArtifactStore::new();

    store
        .store(artifact_version(&id, "v1", b"created artifact"))
        .unwrap();

    let content = b"created artifact";
    let result = publish(&id, "v1", &store, &integrity_proof(content));

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
fn superseded_version_remains_resolvable_by_exact_reference() {
    let id = artifact_id("artifact.supersession");
    let store = InMemoryArtifactStore::new();

    publish_version(&store, &id, "v1", b"old content");
    publish_version(&store, &id, "v2", b"new content");

    store
        .transition_lifecycle(&id, "v1", LifecycleState::Superseded)
        .unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v1");
    assert_eq!(resolved.lifecycle(), &LifecycleState::Superseded);
}

#[test]
fn revoked_version_is_blocked_from_normal_resolution() {
    let id = artifact_id("artifact.revocation");
    let store = InMemoryArtifactStore::new();

    publish_version(&store, &id, "v1", b"revoked content");

    store
        .transition_lifecycle(&id, "v1", LifecycleState::Revoked)
        .unwrap();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let result = resolve(&reference, &store);

    assert_eq!(result, Err(ResolutionError::RevokedArtifact));
}

#[test]
fn artifact_integrity_remains_verifiable_after_storage_and_resolution() {
    let id = artifact_id("artifact.integrity-pipeline");
    let store = InMemoryArtifactStore::new();
    let content = b"integrity pipeline content";

    publish_version(&store, &id, "v1", content);

    let reference = ArtifactReference::new(id, "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert!(verify_integrity(content, resolved.digest()).is_ok());

    assert!(verify_integrity(b"tampered integrity pipeline content", resolved.digest(),).is_err());
}

#[test]
fn identical_content_does_not_collapse_distinct_artifact_identity() {
    let first_id = artifact_id("artifact.first");
    let second_id = artifact_id("artifact.second");
    let store = InMemoryArtifactStore::new();

    let shared_content = b"same physical content";

    publish_version(&store, &first_id, "v1", shared_content);
    publish_version(&store, &second_id, "v1", shared_content);

    let first = store.get(&first_id, "v1").unwrap().unwrap();
    let second = store.get(&second_id, "v1").unwrap().unwrap();

    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.content(), second.content());

    assert_ne!(first.artifact_id(), second.artifact_id());
}

#[test]
fn artifact_resolution_can_participate_in_an_existing_engine_context() {
    let id = artifact_id("artifact.engine-context");
    let store = InMemoryArtifactStore::new();

    publish_version(&store, &id, "v1", b"context-aware artifact");

    let context = engine_context();

    let reference = ArtifactReference::new(id.clone(), "v1");
    let resolved = resolve(&reference, &store).unwrap();

    assert_eq!(
        context.operation().operation.id.as_str(),
        "artifact-pipeline-operation"
    );
    assert_eq!(resolved.artifact_id(), &id);
    assert_eq!(resolved.version(), "v1");
}
