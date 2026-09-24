use crate::support::{section, show_arrow, step, success};
use nizaam_core::artifact::{
    Artifact, ArtifactReference, ArtifactStore, ArtifactVersion, ContentDigest, ContentReference,
    InMemoryArtifactStore, IntegrityProof, LifecycleState, VersionSelector, publish, resolve,
};
use nizaam_core::identity::ArtifactId;
use nizaam_core::provenance::{ProvenanceContext, ProvenanceRecord, ProvenanceRelation};

fn id(value: &str) -> ArtifactId {
    ArtifactId::new(value).unwrap()
}
fn version(id: &ArtifactId, version: &str, bytes: &[u8]) -> ArtifactVersion {
    ArtifactVersion::new(
        id.clone(),
        version,
        ContentReference::new("visual-provider", format!("content/{version}")),
        ContentDigest::new(bytes),
        bytes.len() as u64,
    )
}
fn publishable(store: &InMemoryArtifactStore, id: &ArtifactId, version: &str, bytes: &[u8]) {
    store.store(version_record(id, version, bytes)).unwrap();
    store
        .transition_lifecycle(id, version, LifecycleState::Validating)
        .unwrap();
    store
        .transition_lifecycle(id, version, LifecycleState::Validated)
        .unwrap();
    publish(
        id,
        version,
        store,
        &IntegrityProof::verify(bytes, &ContentDigest::new(bytes)).unwrap(),
    )
    .unwrap();
}
fn version_record(id: &ArtifactId, version_name: &str, bytes: &[u8]) -> ArtifactVersion {
    version(id, version_name, bytes)
}

#[test]
fn visual_artifact_identity_integrity_publication_and_provenance() {
    section("NIZAAM CORE — ARTIFACTS + PROVENANCE");
    let artifact_id = id("visual-artifact");
    let artifact = Artifact::new(artifact_id.clone());
    let v1 = version(&artifact_id, "v1", b"artifact-v1");
    let v2 = version(&artifact_id, "v2", b"artifact-v2");
    step(1, "logical artifact and versions");
    assert_eq!(artifact.id(), &artifact_id);
    assert_eq!(v1.artifact_id(), &artifact_id);
    assert_eq!(v2.artifact_id(), &artifact_id);
    assert_ne!(v1.version(), v2.version());
    println!("  ArtifactId       : {}", artifact.id());
    println!("  ArtifactVersion  : {} / {}", v1.version(), v2.version());
    show_arrow("Operation", "Artifact → Artifact Version");
    success("logical identity remains stable while version identity changes");

    step(2, "content reference and integrity");
    println!(
        "  provider/location: {} / {}",
        v1.content().provider(),
        v1.content().location()
    );
    let digest = ContentDigest::new(b"artifact-v1");
    assert!(IntegrityProof::verify(b"artifact-v1", &digest).is_ok());
    assert!(IntegrityProof::verify(b"tampered", &digest).is_err());
    success("content location is separate from artifact identity and tampering is rejected");

    step(3, "publication and exact versus alias resolution");
    let store = InMemoryArtifactStore::new();
    publishable(&store, &artifact_id, "v1", b"artifact-v1");
    publishable(&store, &artifact_id, "v2", b"artifact-v2");
    store.set_alias(&artifact_id, "latest", "v2").unwrap();
    let exact = resolve(&ArtifactReference::new(artifact_id.clone(), "v1"), &store).unwrap();
    let alias = resolve(
        &ArtifactReference::with_selector(artifact_id.clone(), VersionSelector::alias("latest")),
        &store,
    )
    .unwrap();
    assert_eq!(exact.version(), "v1");
    assert_eq!(alias.version(), "v2");
    println!("  exact reference : {}", exact.version());
    println!("  alias latest    : {}", alias.version());
    success("exact references and aliases remain distinct resolution semantics");

    step(4, "historical provenance versus execution context");
    let source = ArtifactReference::new(artifact_id.clone(), "v1");
    let target_id = id("visual-derived-artifact");
    let target = ArtifactReference::new(target_id, "v2");
    let record = ProvenanceRecord::new(
        source.clone(),
        ProvenanceRelation::DerivedFrom,
        target.clone(),
    );
    let snapshot = record.clone();
    let context = ProvenanceContext::new().with_attribute("stage", "visual-demo");
    assert!(record.is_valid());
    assert_eq!(context.attribute("stage"), Some("visual-demo"));
    assert_eq!(record, snapshot);
    println!("  Provenance relation : {:?}", record.relation());
    println!("  Context attribute   : stage=visual-demo");
    show_arrow("Content / integrity", "Historical ProvenanceRecord");
    success("provenance references exact versions and remains separate from ProvenanceContext");
}
