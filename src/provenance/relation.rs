//! Historical relationship types between provenance entities.
//!
//! `ProvenanceRelation` describes the semantic relationship recorded by a
//! `ProvenanceRecord`. It does not contain the participating entities,
//! timestamps, execution context, storage information, or retry state.
//!
//! The relation vocabulary is intentionally small and provider-neutral.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A historical relationship between two provenance entities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceRelation {
    /// The target was directly produced from the source.
    ProducedFrom,

    /// The target was derived from the source.
    DerivedFrom,

    /// The target was created by transforming the source.
    TransformedFrom,

    /// The source was referenced by the target.
    Referenced,

    /// The target supersedes the source.
    Supersedes,
}

impl ProvenanceRelation {
    /// Returns the stable textual representation of the relationship.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ProducedFrom => "produced_from",
            Self::DerivedFrom => "derived_from",
            Self::TransformedFrom => "transformed_from",
            Self::Referenced => "referenced",
            Self::Supersedes => "supersedes",
        }
    }
}

impl Serialize for ProvenanceRelation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ProvenanceRelation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;

        match value.as_str() {
            "produced_from" => Ok(Self::ProducedFrom),
            "derived_from" => Ok(Self::DerivedFrom),
            "transformed_from" => Ok(Self::TransformedFrom),
            "referenced" => Ok(Self::Referenced),
            "supersedes" => Ok(Self::Supersedes),
            other => Err(D::Error::unknown_variant(
                other,
                &[
                    "produced_from",
                    "derived_from",
                    "transformed_from",
                    "referenced",
                    "supersedes",
                ],
            )),
        }
    }
}

impl std::fmt::Display for ProvenanceRelation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_names_are_stable() {
        assert_eq!(ProvenanceRelation::ProducedFrom.as_str(), "produced_from");
        assert_eq!(ProvenanceRelation::DerivedFrom.as_str(), "derived_from");
        assert_eq!(
            ProvenanceRelation::TransformedFrom.as_str(),
            "transformed_from"
        );
        assert_eq!(ProvenanceRelation::Referenced.as_str(), "referenced");
        assert_eq!(ProvenanceRelation::Supersedes.as_str(), "supersedes");
    }

    #[test]
    fn different_relations_are_distinct() {
        assert_ne!(
            ProvenanceRelation::ProducedFrom,
            ProvenanceRelation::DerivedFrom
        );
        assert_ne!(
            ProvenanceRelation::DerivedFrom,
            ProvenanceRelation::TransformedFrom
        );
        assert_ne!(
            ProvenanceRelation::TransformedFrom,
            ProvenanceRelation::Referenced
        );
        assert_ne!(
            ProvenanceRelation::Referenced,
            ProvenanceRelation::Supersedes
        );
    }

    #[test]
    fn relation_display_uses_stable_name() {
        assert_eq!(
            ProvenanceRelation::ProducedFrom.to_string(),
            "produced_from"
        );
        assert_eq!(ProvenanceRelation::Supersedes.to_string(), "supersedes");
    }

    #[test]
    fn relation_supports_clone_and_eq() {
        let original = ProvenanceRelation::DerivedFrom;
        let cloned = original;

        assert_eq!(original, cloned);
    }

    #[test]
    fn relation_serializes_using_stable_wire_value() {
        let serialized = serde_json::to_string(&ProvenanceRelation::ProducedFrom).unwrap();

        assert_eq!(serialized, "\"produced_from\"");
    }

    #[test]
    fn relation_serialization_round_trips() {
        let relation = ProvenanceRelation::TransformedFrom;

        let serialized = serde_json::to_string(&relation).unwrap();
        let deserialized = serde_json::from_str::<ProvenanceRelation>(&serialized).unwrap();

        assert_eq!(deserialized, relation);
    }

    #[test]
    fn relation_deserialization_rejects_unknown_wire_value() {
        let result = serde_json::from_str::<ProvenanceRelation>("\"ProducedFrom\"");

        assert!(result.is_err());
    }
}
