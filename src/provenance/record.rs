//! Historical provenance record describing a relationship between artifact
//! references.
//!
//! A `ProvenanceRecord` represents one historical provenance fact:
//!
//! ```text
//! source → relation → target
//! ```
//!
//! Historical provenance must preserve exact artifact versions. Mutable
//! aliases may be used before recording, but must be resolved to exact
//! version references before a record is considered valid.
//!
//! The record does not resolve references, access content, perform
//! authorization, or contain retry/attempt state.
//!
//! `ProvenanceContext` remains an execution-scoped concept and is intentionally
//! separate from this historical record.

use crate::artifact::ArtifactReference;

use super::relation::ProvenanceRelation;

/// One historical provenance relationship between two artifact references.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProvenanceRecord {
    source: ArtifactReference,
    relation: ProvenanceRelation,
    target: ArtifactReference,
}

impl ProvenanceRecord {
    /// Creates a provenance record from a source, relationship, and target.
    ///
    /// Construction is intentionally infallible. Callers performing
    /// provenance operations can use [`Self::is_valid`] before recording or
    /// persisting the relationship.
    pub fn new(
        source: ArtifactReference,
        relation: ProvenanceRelation,
        target: ArtifactReference,
    ) -> Self {
        Self {
            source,
            relation,
            target,
        }
    }

    /// Returns the source artifact reference.
    pub fn source(&self) -> &ArtifactReference {
        &self.source
    }

    /// Returns the provenance relationship.
    pub fn relation(&self) -> ProvenanceRelation {
        self.relation
    }

    /// Returns the target artifact reference.
    pub fn target(&self) -> &ArtifactReference {
        &self.target
    }

    /// Returns whether the record contains valid, exact artifact references.
    ///
    /// Mutable aliases are intentionally rejected because their targets can
    /// change after the historical fact is recorded. Callers should resolve
    /// aliases before constructing executable or historical provenance.
    pub fn is_valid(&self) -> bool {
        self.source.is_valid()
            && self.target.is_valid()
            && self.source.is_exact()
            && self.target.is_exact()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::identity::ArtifactId;

    fn source_reference() -> ArtifactReference {
        ArtifactReference::new(ArtifactId::new("source-artifact").unwrap(), "v1")
    }

    fn target_reference() -> ArtifactReference {
        ArtifactReference::new(ArtifactId::new("target-artifact").unwrap(), "v2")
    }

    #[test]
    fn record_preserves_source_relation_and_target() {
        let source = source_reference();
        let target = target_reference();

        let record = ProvenanceRecord::new(
            source.clone(),
            ProvenanceRelation::DerivedFrom,
            target.clone(),
        );

        assert_eq!(record.source(), &source);
        assert_eq!(record.relation(), ProvenanceRelation::DerivedFrom);
        assert_eq!(record.target(), &target);
    }

    #[test]
    fn valid_record_has_valid_artifact_references() {
        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::ProducedFrom,
            target_reference(),
        );

        assert!(record.is_valid());
    }

    #[test]
    fn invalid_source_reference_makes_record_invalid() {
        let source = ArtifactReference::new(ArtifactId::new("source-artifact").unwrap(), "");

        let record =
            ProvenanceRecord::new(source, ProvenanceRelation::DerivedFrom, target_reference());

        assert!(!record.is_valid());
    }

    #[test]
    fn invalid_target_reference_makes_record_invalid() {
        let target = ArtifactReference::new(ArtifactId::new("target-artifact").unwrap(), "   ");

        let record =
            ProvenanceRecord::new(source_reference(), ProvenanceRelation::DerivedFrom, target);

        assert!(!record.is_valid());
    }

    #[test]
    fn alias_references_are_not_valid_historical_provenance() {
        let source = ArtifactReference::with_selector(
            ArtifactId::new("source-artifact").unwrap(),
            crate::artifact::VersionSelector::alias("latest"),
        );

        let target = target_reference();

        let record = ProvenanceRecord::new(source.clone(), ProvenanceRelation::Referenced, target);

        assert_eq!(record.source(), &source);
        assert!(record.source().is_alias());
        assert!(!record.is_valid());
    }

    #[test]
    fn different_relationships_produce_distinct_records() {
        let source = source_reference();
        let target = target_reference();

        let derived = ProvenanceRecord::new(
            source.clone(),
            ProvenanceRelation::DerivedFrom,
            target.clone(),
        );

        let transformed =
            ProvenanceRecord::new(source, ProvenanceRelation::TransformedFrom, target);

        assert_ne!(derived, transformed);
    }

    #[test]
    fn record_supports_clone_and_eq() {
        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::Supersedes,
            target_reference(),
        );

        let cloned = record.clone();

        assert_eq!(record, cloned);
    }

    #[test]
    fn record_serialization_round_trips() {
        let record = ProvenanceRecord::new(
            source_reference(),
            ProvenanceRelation::ProducedFrom,
            target_reference(),
        );

        let serialized = serde_json::to_string(&record).unwrap();

        let deserialized = serde_json::from_str::<ProvenanceRecord>(&serialized).unwrap();

        assert_eq!(record, deserialized);
    }
}
