//! Integration tests for the public Phase 10 provenance boundary.
//!
//! These tests exercise historical provenance as an external Core consumer
//! would use it. Provenance records are required to preserve exact artifact
//! versions so later alias changes cannot mutate historical meaning.

use nizaam_core::artifact::{
    ArtifactReference, ArtifactStore, ArtifactVersion, ContentDigest, ContentReference,
    InMemoryArtifactStore, IntegrityProof, LifecycleState, VersionSelector, publish,
};
use nizaam_core::identity::ArtifactId;
use nizaam_core::provenance::{ProvenanceContext, ProvenanceRecord, ProvenanceRelation};

fn artifact_id(value: &str) -> ArtifactId {
    ArtifactId::new(value).unwrap()
}

fn exact_reference(id: &ArtifactId, version: &str) -> ArtifactReference {
    ArtifactReference::new(id.clone(), version)
}

fn alias_reference(id: &ArtifactId, alias: &str) -> ArtifactReference {
    ArtifactReference::with_selector(id.clone(), VersionSelector::alias(alias))
}

fn artifact_version(id: &ArtifactId, version: &str) -> ArtifactVersion {
    let content = b"provenance artifact content";

    ArtifactVersion::new(
        id.clone(),
        version,
        ContentReference::new("test-provider", format!("content/{version}")),
        ContentDigest::new(content),
        content.len() as u64,
    )
}

fn verified_proof() -> IntegrityProof {
    let content = b"provenance artifact content";
    IntegrityProof::verify(content, &ContentDigest::new(content)).unwrap()
}

fn publish_fixture(store: &InMemoryArtifactStore, id: &ArtifactId, version: &str) {
    store.store(artifact_version(id, version)).unwrap();
    store
        .transition_lifecycle(id, version, LifecycleState::Validating)
        .unwrap();
    store
        .transition_lifecycle(id, version, LifecycleState::Validated)
        .unwrap();
    publish(id, version, store, &verified_proof()).unwrap();
}

#[test]
fn provenance_record_preserves_source_relation_and_target() {
    let source = exact_reference(&artifact_id("source"), "v1");
    let target = exact_reference(&artifact_id("target"), "v2");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::DerivedFrom,
        target.clone(),
    );

    assert_eq!(record.source(), &source);
    assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
    assert_eq!(record.target(), &target);
    assert!(record.is_valid());
}

#[test]
fn provenance_record_references_artifacts_without_duplicating_content() {
    let record = ProvenanceRecord::new(
        exact_reference(&artifact_id("source"), "v1"),
        ProvenanceRelation::ProducedFrom,
        exact_reference(&artifact_id("target"), "v2"),
    );

    assert!(record.is_valid());
    assert!(record.source().is_exact());
    assert!(record.target().is_exact());
}

#[test]
fn all_supported_provenance_relations_are_preserved() {
    let source = exact_reference(&artifact_id("source"), "v1");
    let target = exact_reference(&artifact_id("target"), "v2");

    for relation in [
        ProvenanceRelation::ProducedFrom,
        ProvenanceRelation::DerivedFrom,
        ProvenanceRelation::TransformedFrom,
        ProvenanceRelation::Referenced,
        ProvenanceRelation::Supersedes,
    ] {
        let record = ProvenanceRecord::new(source.clone(), relation, target.clone());
        assert_eq!(record.relation(), relation);
        assert!(record.is_valid());
    }
}

#[test]
fn provenance_record_round_trips_through_serde() {
    let record = ProvenanceRecord::new(
        exact_reference(&artifact_id("source"), "v1"),
        ProvenanceRelation::ProducedFrom,
        exact_reference(&artifact_id("target"), "v2"),
    );

    let serialized = serde_json::to_string(&record).unwrap();
    let deserialized = serde_json::from_str::<ProvenanceRecord>(&serialized).unwrap();

    assert_eq!(record, deserialized);
    assert!(serialized.contains("produced_from"));
}

#[test]
fn invalid_source_reference_makes_provenance_record_invalid() {
    let source = ArtifactReference::new(artifact_id("source"), "");
    let target = exact_reference(&artifact_id("target"), "v1");

    let record = ProvenanceRecord::new(source, ProvenanceRelation::DerivedFrom, target);

    assert!(!record.is_valid());
}

#[test]
fn invalid_target_reference_makes_provenance_record_invalid() {
    let source = exact_reference(&artifact_id("source"), "v1");
    let target = ArtifactReference::new(artifact_id("target"), "   ");

    let record = ProvenanceRecord::new(source, ProvenanceRelation::DerivedFrom, target);

    assert!(!record.is_valid());
}

#[test]
fn alias_reference_is_not_valid_historical_provenance() {
    let source = alias_reference(&artifact_id("source"), "latest");
    let target = exact_reference(&artifact_id("target"), "v2");

    let record = ProvenanceRecord::new(source.clone(), ProvenanceRelation::Referenced, target);

    assert!(record.source().is_alias());
    assert!(!record.is_valid());
}

#[test]
fn exact_version_reference_preserves_reproducible_provenance() {
    let source = exact_reference(&artifact_id("source"), "v7");
    let target = exact_reference(&artifact_id("target"), "v3");

    let record = ProvenanceRecord::new(source.clone(), ProvenanceRelation::TransformedFrom, target);

    assert!(record.is_valid());
    assert!(record.source().is_exact());
    assert_eq!(record.source(), &source);
}

#[test]
fn provenance_record_is_independent_from_artifact_store_state() {
    let record = ProvenanceRecord::new(
        exact_reference(&artifact_id("source"), "v1"),
        ProvenanceRelation::ProducedFrom,
        exact_reference(&artifact_id("target"), "v2"),
    );
    let snapshot = record.clone();

    assert_eq!(record, snapshot);
    assert!(record.is_valid());
}

#[test]
fn provenance_context_remains_separate_from_historical_provenance_record() {
    let context = ProvenanceContext::new().with_attribute("stage", "decode");
    let record = ProvenanceRecord::new(
        exact_reference(&artifact_id("source"), "v1"),
        ProvenanceRelation::DerivedFrom,
        exact_reference(&artifact_id("target"), "v2"),
    );

    assert_eq!(context.attribute("stage"), Some("decode"));
    assert!(record.is_valid());
}

#[test]
fn provenance_context_attributes_are_preserved_across_cloning() {
    let context = ProvenanceContext::new()
        .with_attribute("engine", "decoder")
        .with_attribute("stage", "normalize");
    let cloned = context.clone();

    assert_eq!(cloned.attribute("engine"), Some("decoder"));
    assert_eq!(cloned.attribute("stage"), Some("normalize"));
}

#[test]
fn historical_provenance_is_unchanged_after_artifact_supersession() {
    let source_id = artifact_id("source");
    let target_id = artifact_id("target");
    let store = InMemoryArtifactStore::new();

    publish_fixture(&store, &source_id, "v1");
    publish_fixture(&store, &target_id, "v1");
    store
        .transition_lifecycle(&target_id, "v1", LifecycleState::Superseded)
        .unwrap();

    let record = ProvenanceRecord::new(
        exact_reference(&source_id, "v1"),
        ProvenanceRelation::ProducedFrom,
        exact_reference(&target_id, "v1"),
    );
    let snapshot = record.clone();

    assert_eq!(
        store.get(&target_id, "v1").unwrap().unwrap().lifecycle(),
        &LifecycleState::Superseded
    );
    assert_eq!(record, snapshot);
    assert!(record.is_valid());
}

#[test]
fn historical_provenance_is_unchanged_after_artifact_revocation() {
    let source_id = artifact_id("source");
    let target_id = artifact_id("target");
    let store = InMemoryArtifactStore::new();

    publish_fixture(&store, &source_id, "v1");
    publish_fixture(&store, &target_id, "v1");
    store
        .transition_lifecycle(&target_id, "v1", LifecycleState::Revoked)
        .unwrap();

    let record = ProvenanceRecord::new(
        exact_reference(&source_id, "v1"),
        ProvenanceRelation::DerivedFrom,
        exact_reference(&target_id, "v1"),
    );
    let snapshot = record.clone();

    assert_eq!(
        store.get(&target_id, "v1").unwrap().unwrap().lifecycle(),
        &LifecycleState::Revoked
    );
    assert_eq!(record, snapshot);
    assert!(record.is_valid());
}

#[test]
fn multiple_provenance_records_can_describe_one_transformation() {
    let source = exact_reference(&artifact_id("source"), "v1");
    let target = exact_reference(&artifact_id("target"), "v2");

    let produced = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::ProducedFrom,
        target.clone(),
    );
    let transformed = ProvenanceRecord::new(source, ProvenanceRelation::TransformedFrom, target);

    assert!(produced.is_valid());
    assert!(transformed.is_valid());
    assert_ne!(produced.relation(), transformed.relation());
}
