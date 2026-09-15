//! One exact immutable logical content state of an artifact.
//!
//! An `ArtifactVersion` represents one exact logical content state of an
//! artifact. It contains the artifact identity, version identity, content
//! reference, integrity information, content size, descriptive metadata,
//! and lifecycle state.
//!
//! Published versions are immutable with respect to their content,
//! content identity, version identity, and integrity information.

use crate::identity::ArtifactId;

use super::content::ContentReference;
use super::integrity::ContentDigest;
use super::lifecycle::LifecycleState;

/// One exact logical content state of an artifact.
///
/// The artifact identity remains stable across versions, while each
/// `ArtifactVersion` identifies one exact immutable logical state.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactVersion {
    artifact_id: ArtifactId,
    version: String,
    content: ContentReference,
    digest: ContentDigest,
    size: u64,
    metadata: Vec<(String, String)>,
    lifecycle: LifecycleState,
}

impl ArtifactVersion {
    /// Creates a new artifact version in the `Created` lifecycle state.
    ///
    /// The supplied `ArtifactId` must already be a valid Core identity.
    /// Version ordering is intentionally not interpreted by this type.
    pub fn new(
        artifact_id: ArtifactId,
        version: impl Into<String>,
        content: ContentReference,
        digest: ContentDigest,
        size: u64,
    ) -> Self {
        Self {
            artifact_id,
            version: version.into(),
            content,
            digest,
            size,
            metadata: Vec::new(),
            lifecycle: LifecycleState::Created,
        }
    }

    /// Returns the stable logical artifact identifier.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Returns the version identifier.
    ///
    /// The version identifier is an identity value only. `ArtifactVersion`
    /// does not assign ordering or semantic-version meaning to it.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the physical content reference.
    pub fn content(&self) -> &ContentReference {
        &self.content
    }

    /// Returns the recorded content digest.
    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    /// Returns the recorded content size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns descriptive artifact metadata.
    ///
    /// This metadata is not part of the content identity represented by the
    /// recorded digest.
    pub fn metadata(&self) -> &[(String, String)] {
        &self.metadata
    }

    /// Returns the current lifecycle state.
    pub fn lifecycle(&self) -> &LifecycleState {
        &self.lifecycle
    }

    /// Adds descriptive metadata without changing the content identity.
    ///
    /// Metadata mutation is only available before the version becomes
    /// published. Lifecycle-specific enforcement will be centralized in
    /// the artifact lifecycle/publication mechanisms.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((key.into(), value.into()));
        self
    }

    /// Updates the lifecycle state internally within the artifact module.
    ///
    /// Lifecycle validity must be enforced by the lifecycle mechanism rather
    /// than by arbitrary external callers. This method is therefore crate
    /// visible and is not part of the public artifact API.
    pub(crate) fn set_lifecycle(&mut self, state: LifecycleState) {
        self.lifecycle = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ArtifactId;

    fn sample_version() -> ArtifactVersion {
        ArtifactVersion::new(
            ArtifactId::new("test-artifact").unwrap(),
            "v1",
            ContentReference::new("provider", "location-1"),
            ContentDigest::new(b"digest-data"),
            1024,
        )
    }

    #[test]
    fn artifact_version_contains_identity_and_content() {
        let version = sample_version();

        assert_eq!(version.artifact_id().as_str(), "test-artifact");
        assert_eq!(version.version(), "v1");
        assert_eq!(version.size(), 1024);
    }

    #[test]
    fn artifact_version_starts_in_created_state() {
        let version = sample_version();

        assert_eq!(version.lifecycle(), &LifecycleState::Created);
    }

    #[test]
    fn artifact_version_metadata_is_descriptive_only() {
        let version = sample_version().with_metadata("author", "test");

        assert_eq!(version.metadata().len(), 1);
        assert_eq!(
            version.metadata()[0],
            ("author".to_string(), "test".to_string())
        );
    }

    #[test]
    fn artifact_version_lifecycle_can_be_updated_within_artifact_module() {
        let mut version = sample_version();

        version.set_lifecycle(LifecycleState::Validated);

        assert_eq!(version.lifecycle(), &LifecycleState::Validated);
    }

    #[test]
    fn artifact_version_supports_clone_and_eq() {
        let version = sample_version();
        let cloned = version.clone();

        assert_eq!(version, cloned);
    }
}
