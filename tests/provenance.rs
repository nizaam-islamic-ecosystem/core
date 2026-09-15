#![allow(clippy::needless_borrow)]

//! Integration tests for the public Phase 10 provenance boundary.
//!
//! These tests exercise provenance as an external Core consumer would use it.
//! They focus on integration between provenance records, artifact references,
//! artifact lifecycle state, and the existing runtime provenance context.
//!
//! Individual implementation details are covered by the unit tests inside
//! `src/provenance/*`; this file verifies behavior across public Core APIs.

use nizaam_core::artifact::{
    ArtifactReference, ArtifactStore, InMemoryArtifactStore, LifecycleState, VersionSelector,
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

#[test]
fn provenance_record_preserves_source_relation_and_target() {
    let source_id = artifact_id("dataset");
    let target_id = artifact_id("model");

    let source = exact_reference(&source_id, "v4");
    let target = exact_reference(&target_id, "v2");

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
    let source_id = artifact_id("source-artifact");
    let target_id = artifact_id("target-artifact");

    let source = exact_reference(&source_id, "v1");
    let target = exact_reference(&target_id, "v7");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::TransformedFrom,
        target.clone(),
    );

    assert_eq!(record.source().artifact_id(), &source_id);
    assert_eq!(record.source().version().as_str(), "v1");

    assert_eq!(record.target().artifact_id(), &target_id);
    assert_eq!(record.target().version().as_str(), "v7");

    assert_eq!(record.source(), &source);
    assert_eq!(record.target(), &target);
}

#[test]
fn all_supported_provenance_relations_are_preserved() {
    let source_id = artifact_id("source");
    let target_id = artifact_id("target");

    let source = exact_reference(&source_id, "v1");
    let target = exact_reference(&target_id, "v2");

    let relations = [
        ProvenanceRelation::ProducedFrom,
        ProvenanceRelation::DerivedFrom,
        ProvenanceRelation::TransformedFrom,
        ProvenanceRelation::Referenced,
        ProvenanceRelation::Supersedes,
    ];

    for relation in relations {
        let record = ProvenanceRecord::new(source.clone(), relation, target.clone());

        assert!(record.is_valid());
        assert_eq!(record.source(), &source);
        assert_eq!(record.target(), &target);
        assert_eq!(record.relation(), relation);
    }
}

#[test]
fn provenance_record_round_trips_through_serde() {
    let source_id = artifact_id("serialization-source");
    let target_id = artifact_id("serialization-target");

    let record = ProvenanceRecord::new(
        exact_reference(&source_id, "v3"),
        ProvenanceRelation::DerivedFrom,
        exact_reference(&target_id, "v8"),
    );

    let serialized = serde_json::to_string(&record).unwrap();
    let restored: ProvenanceRecord = serde_json::from_str(&serialized).unwrap();

    assert_eq!(restored, record);
}

#[test]
fn invalid_source_reference_makes_provenance_record_invalid() {
    let source_id = artifact_id("invalid-source");
    let target_id = artifact_id("valid-target");

    let source = ArtifactReference::new(source_id, "");
    let target = exact_reference(&target_id, "v1");

    let record = ProvenanceRecord::new(source, ProvenanceRelation::DerivedFrom, target);

    assert!(!record.is_valid());
}

#[test]
fn invalid_target_reference_makes_provenance_record_invalid() {
    let source_id = artifact_id("valid-source");
    let target_id = artifact_id("invalid-target");

    let source = exact_reference(&source_id, "v1");
    let target = ArtifactReference::new(target_id, "   ");

    let record = ProvenanceRecord::new(source, ProvenanceRelation::DerivedFrom, target);

    assert!(!record.is_valid());
}

#[test]
fn alias_reference_can_be_preserved_in_historical_provenance() {
    let source_id = artifact_id("dataset");
    let target_id = artifact_id("result");

    let source = alias_reference(&source_id, "latest");
    let target = exact_reference(&target_id, "v7");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::Referenced,
        target.clone(),
    );

    assert!(record.is_valid());
    assert!(record.source().is_alias());
    assert_eq!(record.source().version().as_str(), "latest");

    assert!(record.target().is_exact());
    assert_eq!(record.target().version().as_str(), "v7");
}

#[test]
fn exact_version_reference_preserves_reproducible_provenance() {
    let input_id = artifact_id("quran-dataset");
    let output_id = artifact_id("embedding-index");

    let input = exact_reference(&input_id, "v14");
    let output = exact_reference(&output_id, "v7");

    let record = ProvenanceRecord::new(
        input.clone(),
        ProvenanceRelation::ProducedFrom,
        output.clone(),
    );

    assert!(record.is_valid());
    assert!(record.source().is_exact());
    assert!(record.target().is_exact());

    assert_eq!(record.source().version().as_str(), "v14");
    assert_eq!(record.target().version().as_str(), "v7");
}

#[test]
fn historical_provenance_is_unchanged_after_artifact_supersession() {
    let id = artifact_id("model");
    let store = InMemoryArtifactStore::new();

    let source = exact_reference(&id, "v2");
    let target_id = artifact_id("output");
    let target = exact_reference(&target_id, "v7");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::DerivedFrom,
        target.clone(),
    );

    // The store needs a concrete artifact version so its lifecycle can change.
    let version = nizaam_core::artifact::ArtifactVersion::new(
        id.clone(),
        "v2",
        nizaam_core::artifact::ContentReference::new("test-provider", "model/v2"),
        nizaam_core::artifact::ContentDigest::new(b"model-v2"),
        7,
    );

    store.store(version).unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Validating)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Validated)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Published)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Superseded)
        .unwrap();

    assert_eq!(record.source(), &source);
    assert_eq!(record.target(), &target);
    assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
    assert!(record.is_valid());

    let stored = store.get(&id, "v2").unwrap().unwrap();

    assert_eq!(stored.lifecycle(), &LifecycleState::Superseded);
}

#[test]
fn historical_provenance_is_unchanged_after_artifact_revocation() {
    let id = artifact_id("revoked-model");
    let target_id = artifact_id("generated-output");

    let source = exact_reference(&id, "v2");
    let target = exact_reference(&target_id, "v7");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::DerivedFrom,
        target.clone(),
    );

    let store = InMemoryArtifactStore::new();

    let version = nizaam_core::artifact::ArtifactVersion::new(
        id.clone(),
        "v2",
        nizaam_core::artifact::ContentReference::new("test-provider", "model/v2"),
        nizaam_core::artifact::ContentDigest::new(b"revoked-model-v2"),
        15,
    );

    store.store(version).unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Validating)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Validated)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Published)
        .unwrap();

    store
        .transition_lifecycle(&id, "v2", LifecycleState::Revoked)
        .unwrap();

    assert_eq!(record.source(), &source);
    assert_eq!(record.target(), &target);
    assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
    assert!(record.is_valid());

    let stored = store.get(&id, "v2").unwrap().unwrap();

    assert_eq!(stored.lifecycle(), &LifecycleState::Revoked);
}

#[test]
fn provenance_record_is_independent_from_artifact_store_state() {
    let source_id = artifact_id("stored-source");
    let target_id = artifact_id("stored-target");

    let source = exact_reference(&source_id, "v1");
    let target = exact_reference(&target_id, "v2");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::Referenced,
        target.clone(),
    );

    let store = InMemoryArtifactStore::new();

    let version = nizaam_core::artifact::ArtifactVersion::new(
        source_id.clone(),
        "v1",
        nizaam_core::artifact::ContentReference::new("test-provider", "source/v1"),
        nizaam_core::artifact::ContentDigest::new(b"source-content"),
        14,
    );

    store.store(version).unwrap();

    store
        .transition_lifecycle(&source_id, "v1", LifecycleState::Validating)
        .unwrap();

    store
        .transition_lifecycle(&source_id, "v1", LifecycleState::Validated)
        .unwrap();

    assert_eq!(record.source(), &source);
    assert_eq!(record.target(), &target);
    assert_eq!(record.relation(), ProvenanceRelation::Referenced);
}

#[test]
fn provenance_context_remains_separate_from_historical_provenance_record() {
    let source_id = artifact_id("context-source");
    let target_id = artifact_id("context-target");

    let source = exact_reference(&source_id, "v1");
    let target = exact_reference(&target_id, "v2");

    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::TransformedFrom,
        target.clone(),
    );

    let context = ProvenanceContext::new()
        .with_attribute("engine", "test-engine")
        .with_attribute("purpose", "artifact-transformation");

    assert!(record.is_valid());

    assert_eq!(record.source(), &source);
    assert_eq!(record.target(), &target);

    assert_eq!(context.attribute("engine"), Some("test-engine"));
    assert_eq!(
        context.attribute("purpose"),
        Some("artifact-transformation")
    );
}

#[test]
fn provenance_context_attributes_are_preserved_across_cloning() {
    let context = ProvenanceContext::new()
        .with_attribute("engine", "test-engine")
        .with_attribute("operation", "transform");

    let cloned = context.clone();

    assert_eq!(cloned, context);
    assert_eq!(cloned.attribute("engine"), Some("test-engine"));
    assert_eq!(cloned.attribute("operation"), Some("transform"));
}

#[test]
fn multiple_provenance_records_can_describe_one_transformation() {
    let dataset_id = artifact_id("dataset");
    let model_id = artifact_id("model");
    let output_id = artifact_id("embedding-index");

    let dataset = exact_reference(&dataset_id, "v4");
    let model = exact_reference(&model_id, "v2");
    let output = exact_reference(&output_id, "v7");

    let dataset_record = ProvenanceRecord::new(
        dataset.clone(),
        ProvenanceRelation::TransformedFrom,
        output.clone(),
    );

    let model_record = ProvenanceRecord::new(
        model.clone(),
        ProvenanceRelation::DerivedFrom,
        output.clone(),
    );

    assert!(dataset_record.is_valid());
    assert!(model_record.is_valid());

    assert_eq!(dataset_record.source(), &dataset);
    assert_eq!(dataset_record.target(), &output);

    assert_eq!(model_record.source(), &model);
    assert_eq!(model_record.target(), &output);
}
