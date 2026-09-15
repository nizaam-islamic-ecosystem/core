//! Phase 10 provenance boundary.
//!
//! Provenance provides provider-neutral mechanisms for carrying execution
//! context and recording historical relationships between artifact references.
//!
//! Core keeps execution context, relationship vocabulary, and historical
//! records as distinct concepts.

mod context;
mod record;
mod relation;

pub use context::ProvenanceContext;
pub use record::ProvenanceRecord;
pub use relation::ProvenanceRelation;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::artifact::{ArtifactReference, VersionSelector};
    use crate::identity::ArtifactId;

    fn source_reference() -> ArtifactReference {
        ArtifactReference::new(ArtifactId::new("source-artifact").unwrap(), "v1")
    }

    fn target_reference() -> ArtifactReference {
        ArtifactReference::new(ArtifactId::new("target-artifact").unwrap(), "v2")
    }

    #[test]
    fn provenance_context_record_and_relation_work_together() {
        let context = ProvenanceContext::new()
            .with_attribute("source", "fixture")
            .with_attribute("stage", "decode");

        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::DerivedFrom,
            target_reference(),
        );

        assert_eq!(context.attribute("source"), Some("fixture"));
        assert_eq!(context.attribute("stage"), Some("decode"));

        assert!(record.is_valid());
        assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
        assert_eq!(record.source().version().as_str(), "v1");
        assert_eq!(record.target().version().as_str(), "v2");
    }

    #[test]
    fn provenance_record_preserves_exact_references() {
        let source = ArtifactReference::new(ArtifactId::new("artifact-a").unwrap(), "v7");

        let target = ArtifactReference::new(ArtifactId::new("artifact-b").unwrap(), "v3");

        let record = ProvenanceRecord::new(
            source.clone(),
            ProvenanceRelation::TransformedFrom,
            target.clone(),
        );

        assert_eq!(record.source(), &source);
        assert_eq!(record.target(), &target);
        assert_eq!(record.relation(), ProvenanceRelation::TransformedFrom);
    }

    #[test]
    fn provenance_record_rejects_an_alias_reference_as_historical_state() {
        let source = ArtifactReference::with_selector(
            ArtifactId::new("source-artifact").unwrap(),
            VersionSelector::alias("latest"),
        );

        let record = ProvenanceRecord::new(
            source.clone(),
            ProvenanceRelation::Referenced,
            target_reference(),
        );

        assert!(!record.is_valid());
        assert_eq!(record.source(), &source);
        assert!(record.source().is_alias());
    }

    #[test]
    fn provenance_context_is_separate_from_historical_record() {
        let context = ProvenanceContext::new().with_attribute("stage", "decode");

        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::ProducedFrom,
            target_reference(),
        );

        assert_eq!(context.attribute("stage"), Some("decode"));
        assert!(record.is_valid());

        // The execution context does not become part of the historical record.
        assert_eq!(record.source().version().as_str(), "v1");
        assert_eq!(record.target().version().as_str(), "v2");
    }

    #[test]
    fn relation_identity_is_preserved_through_record() {
        let relation = ProvenanceRelation::Supersedes;

        let record = ProvenanceRecord::new(source_reference(), relation, target_reference());

        assert_eq!(record.relation(), relation);
        assert_eq!(record.relation().as_str(), "supersedes");
    }

    #[test]
    fn provenance_record_serialization_round_trips_through_public_api() {
        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::ProducedFrom,
            target_reference(),
        );

        let serialized = serde_json::to_string(&record).unwrap();

        let deserialized = serde_json::from_str::<ProvenanceRecord>(&serialized).unwrap();

        assert_eq!(deserialized, record);
    }
}
