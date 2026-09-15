//! Lightweight reference or selector for an artifact.
//!
//! An `ArtifactReference` may identify an exact version or a mutable
//! selector/alias. Resolution is responsible for turning the reference into
//! exactly one concrete `ArtifactVersion`.

use crate::identity::ArtifactId;

/// Version selector for an artifact reference.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum VersionSelector {
    /// An exact version identifier.
    Exact(String),

    /// A mutable alias or selector such as `latest`.
    Alias(String),
}

impl VersionSelector {
    /// Creates an exact version selector.
    ///
    /// Validation is intentionally exposed separately so the selector can
    /// retain the existing infallible construction API. Consumers that
    /// resolve or otherwise act on the selector must validate it first.
    pub fn exact(version: impl Into<String>) -> Self {
        Self::Exact(version.into())
    }

    /// Creates an alias selector.
    ///
    /// Alias resolution semantics are intentionally not defined here.
    pub fn alias(alias: impl Into<String>) -> Self {
        Self::Alias(alias.into())
    }

    /// Returns the selector string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Exact(version) | Self::Alias(version) => version,
        }
    }

    /// Returns `true` when this selector is an alias.
    pub fn is_alias(&self) -> bool {
        matches!(self, Self::Alias(_))
    }

    /// Returns `true` when this selector identifies an exact version.
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Returns whether the selector contains a non-empty, non-whitespace value.
    pub fn is_valid(&self) -> bool {
        !self.as_str().trim().is_empty()
    }
}

impl<'de> serde::Deserialize<'de> for VersionSelector {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        enum RawVersionSelector {
            Exact(String),
            Alias(String),
        }

        let selector = match RawVersionSelector::deserialize(deserializer)? {
            RawVersionSelector::Exact(version) => Self::Exact(version),
            RawVersionSelector::Alias(alias) => Self::Alias(alias),
        };

        if selector.is_valid() {
            Ok(selector)
        } else {
            Err(serde::de::Error::custom(
                "artifact version selector must not be empty or whitespace-only",
            ))
        }
    }
}

/// Lightweight reference or selector for an artifact.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactReference {
    artifact_id: ArtifactId,
    version: VersionSelector,
}

impl ArtifactReference {
    /// Creates a new artifact reference for an exact version.
    pub fn new(artifact_id: ArtifactId, version: impl Into<String>) -> Self {
        Self {
            artifact_id,
            version: VersionSelector::exact(version),
        }
    }

    /// Creates a new artifact reference with the supplied selector.
    pub fn with_selector(artifact_id: ArtifactId, version: VersionSelector) -> Self {
        Self {
            artifact_id,
            version,
        }
    }

    /// Returns the artifact identifier.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Returns the version selector.
    pub fn version(&self) -> &VersionSelector {
        &self.version
    }

    /// Returns `true` when the reference uses an alias selector.
    pub fn is_alias(&self) -> bool {
        self.version.is_alias()
    }

    /// Returns `true` when the reference identifies an exact version.
    pub fn is_exact(&self) -> bool {
        self.version.is_exact()
    }

    /// Returns whether the reference contains a valid version selector.
    pub fn is_valid(&self) -> bool {
        self.version.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ArtifactId;

    #[test]
    fn exact_reference_identifies_specific_version() {
        let id = ArtifactId::new("artifact-a").unwrap();
        let reference = ArtifactReference::new(id.clone(), "v7");

        assert_eq!(reference.artifact_id(), &id);
        assert!(reference.is_exact());
        assert!(!reference.is_alias());
        assert!(reference.is_valid());
        assert_eq!(reference.version().as_str(), "v7");
    }

    #[test]
    fn alias_reference_uses_mutable_selector() {
        let id = ArtifactId::new("artifact-a").unwrap();
        let reference =
            ArtifactReference::with_selector(id.clone(), VersionSelector::alias("latest"));

        assert_eq!(reference.artifact_id(), &id);
        assert!(reference.is_alias());
        assert!(!reference.is_exact());
        assert!(reference.is_valid());
        assert_eq!(reference.version().as_str(), "latest");
    }

    #[test]
    fn selector_rejects_empty_values() {
        assert!(!VersionSelector::exact("").is_valid());
        assert!(!VersionSelector::alias("").is_valid());
    }

    #[test]
    fn selector_rejects_whitespace_only_values() {
        assert!(!VersionSelector::exact("   ").is_valid());
        assert!(!VersionSelector::alias("\t\n").is_valid());
    }

    #[test]
    fn selector_preserves_non_whitespace_values_without_normalizing() {
        let selector = VersionSelector::exact("  v1  ");

        assert!(selector.is_valid());
        assert_eq!(selector.as_str(), "  v1  ");
    }

    #[test]
    fn reference_can_be_detected_as_invalid_without_mutation() {
        let id = ArtifactId::new("artifact-a").unwrap();
        let reference = ArtifactReference::new(id, "");

        assert!(!reference.is_valid());
        assert_eq!(reference.version().as_str(), "");
    }

    #[test]
    fn reference_supports_clone_and_eq() {
        let id = ArtifactId::new("artifact-b").unwrap();
        let first = ArtifactReference::new(id.clone(), "v1");
        let second = ArtifactReference::new(id, "v1");

        assert_eq!(first, second);
    }

    #[test]
    fn invalid_selector_deserialization_is_rejected() {
        let result = serde_json::from_str::<VersionSelector>(r#"{"Exact":""}"#);

        assert!(result.is_err());
    }

    #[test]
    fn valid_selector_deserialization_is_preserved() {
        let selector = serde_json::from_str::<VersionSelector>(r#"{"Alias":"latest"}"#).unwrap();

        assert_eq!(selector, VersionSelector::Alias("latest".to_string()));
    }
}
