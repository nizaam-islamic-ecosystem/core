//! One exact immutable logical content state of an artifact.
//!
//! An `ArtifactVersion` represents one exact logical content state of an
//! artifact. It contains the artifact identity, version identity, content
//! reference, integrity information, content size, descriptive metadata,
//! and lifecycle state.
//!
//! Published versions are immutable with respect to their content,
//! content identity, version identity, and integrity information.
//! Persistence uses [`ArtifactVersionRecord`] so serialized state cannot
//! directly construct an executable `ArtifactVersion`.

use crate::identity::ArtifactId;

use super::content::ContentReference;
use super::integrity::ContentDigest;
use super::lifecycle::{LifecycleState, transition};

/// Persisted representation of an artifact version.
///
/// This type is intentionally separate from [`ArtifactVersion`]. It may be
/// serialized and deserialized, but deserialized data must pass through
/// [`ArtifactVersion::restore`] before becoming a live artifact version.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactVersionRecord {
    artifact_id: ArtifactId,
    version: String,
    content: ContentReference,
    digest: ContentDigest,
    size: u64,
    metadata: Vec<(String, String)>,
    lifecycle: LifecycleState,
}

/// Failure while restoring a persisted artifact version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactVersionRestoreError {
    /// The persisted content reference is structurally invalid.
    InvalidContentReference,

    /// The persisted version identifier is empty or whitespace-only.
    InvalidVersion,

    /// The persisted lifecycle state could not be reached through valid
    /// lifecycle transitions.
    InvalidLifecycleTransition {
        from: LifecycleState,
        to: LifecycleState,
    },
}

impl std::fmt::Display for ArtifactVersionRestoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidContentReference => {
                write!(formatter, "artifact content reference is invalid")
            }
            Self::InvalidVersion => write!(formatter, "artifact version is invalid"),
            Self::InvalidLifecycleTransition { from, to } => write!(
                formatter,
                "invalid artifact lifecycle transition from {:?} to {:?} during restoration",
                from, to
            ),
        }
    }
}

impl std::error::Error for ArtifactVersionRestoreError {}

impl ArtifactVersionRecord {
    /// Returns the persisted artifact identity.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Returns the persisted version identity.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the persisted content reference.
    pub fn content(&self) -> &ContentReference {
        &self.content
    }

    /// Returns the persisted content digest.
    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    /// Returns the persisted content size.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns persisted descriptive metadata.
    pub fn metadata(&self) -> &[(String, String)] {
        &self.metadata
    }

    /// Returns the persisted lifecycle state.
    pub fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }
}

/// One exact logical content state of an artifact.
///
/// The artifact identity remains stable across versions, while each
/// `ArtifactVersion` identifies one exact immutable logical state.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
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

    /// Restores a live artifact version from trusted persisted data.
    ///
    /// Restoration validates the content reference and reconstructs the
    /// persisted lifecycle only through the approved lifecycle transitions.
    /// In particular, entering `Published` always goes through
    /// `Validated → Published`; persisted data cannot assign that state
    /// directly through serde deserialization.
    pub fn restore(record: ArtifactVersionRecord) -> Result<Self, ArtifactVersionRestoreError> {
        if record.version.trim().is_empty() {
            return Err(ArtifactVersionRestoreError::InvalidVersion);
        }

        if !record.content.is_valid() {
            return Err(ArtifactVersionRestoreError::InvalidContentReference);
        }

        let mut version = Self::new(
            record.artifact_id,
            record.version,
            record.content,
            record.digest,
            record.size,
        );
        version.metadata = record.metadata;

        restore_lifecycle(&mut version, record.lifecycle)?;
        Ok(version)
    }

    /// Converts this version into its persistence representation.
    pub fn to_record(&self) -> ArtifactVersionRecord {
        ArtifactVersionRecord {
            artifact_id: self.artifact_id.clone(),
            version: self.version.clone(),
            content: self.content.clone(),
            digest: self.digest.clone(),
            size: self.size,
            metadata: self.metadata.clone(),
            lifecycle: self.lifecycle,
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
    /// Metadata can be changed only before publication. Once a version is
    /// `Published`, `Superseded`, `Archived`, or `Revoked`, the metadata is
    /// part of the preserved historical state and cannot be mutated.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, ArtifactVersionError> {
        match self.lifecycle {
            LifecycleState::Created | LifecycleState::Validating | LifecycleState::Validated => {
                self.metadata.push((key.into(), value.into()));
                Ok(self)
            }
            state => Err(ArtifactVersionError::MetadataMutationNotAllowed { state }),
        }
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

/// Errors produced by invalid artifact-version mutations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactVersionError {
    /// Metadata cannot be mutated after the version enters a preserved state.
    MetadataMutationNotAllowed { state: LifecycleState },
}

impl std::fmt::Display for ArtifactVersionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MetadataMutationNotAllowed { state } => write!(
                formatter,
                "artifact version metadata cannot be changed in {:?} state",
                state
            ),
        }
    }
}

impl std::error::Error for ArtifactVersionError {}

fn restore_lifecycle(
    version: &mut ArtifactVersion,
    target: LifecycleState,
) -> Result<(), ArtifactVersionRestoreError> {
    let transitions = match target {
        LifecycleState::Created => &[][..],
        LifecycleState::Validating => &[LifecycleState::Validating][..],
        LifecycleState::Validated => &[LifecycleState::Validating, LifecycleState::Validated][..],
        LifecycleState::Published => &[
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
        ][..],
        LifecycleState::Superseded => &[
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Superseded,
        ][..],
        LifecycleState::Archived => &[
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Superseded,
            LifecycleState::Archived,
        ][..],
        LifecycleState::Revoked => &[
            LifecycleState::Validating,
            LifecycleState::Validated,
            LifecycleState::Published,
            LifecycleState::Revoked,
        ][..],
    };

    for next in transitions {
        let current = *version.lifecycle();
        let state = transition(current, *next).map_err(|error| {
            ArtifactVersionRestoreError::InvalidLifecycleTransition {
                from: error.from(),
                to: error.to(),
            }
        })?;
        version.set_lifecycle(state);
    }

    Ok(())
}

impl From<&ArtifactVersion> for ArtifactVersionRecord {
    fn from(version: &ArtifactVersion) -> Self {
        version.to_record()
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
        let version = sample_version().with_metadata("author", "test").unwrap();

        assert_eq!(version.metadata().len(), 1);
        assert_eq!(
            version.metadata()[0],
            ("author".to_string(), "test".to_string())
        );
    }

    #[test]
    fn artifact_version_metadata_rejects_preserved_lifecycle_states() {
        for state in [
            LifecycleState::Published,
            LifecycleState::Superseded,
            LifecycleState::Archived,
            LifecycleState::Revoked,
        ] {
            let mut version = sample_version();
            version.set_lifecycle(state);

            assert_eq!(
                version.with_metadata("author", "test"),
                Err(ArtifactVersionError::MetadataMutationNotAllowed { state })
            );
        }
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

    #[test]
    fn persistence_record_round_trips_through_trusted_restoration() {
        let mut original = sample_version().with_metadata("author", "test").unwrap();
        original.set_lifecycle(LifecycleState::Validated);
        original.set_lifecycle(LifecycleState::Published);

        let record = original.to_record();
        let serialized = serde_json::to_string(&record).unwrap();
        let deserialized = serde_json::from_str::<ArtifactVersionRecord>(&serialized).unwrap();
        let restored = ArtifactVersion::restore(deserialized).unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn restoration_rejects_invalid_version() {
        let mut record = sample_version().to_record();
        record.version = "   ".to_string();

        assert_eq!(
            ArtifactVersion::restore(record),
            Err(ArtifactVersionRestoreError::InvalidVersion)
        );
    }

    #[test]
    fn restoration_rejects_invalid_content_reference() {
        let mut record = sample_version().to_record();
        record.content = ContentReference::new("", "");

        assert_eq!(
            ArtifactVersion::restore(record),
            Err(ArtifactVersionRestoreError::InvalidContentReference)
        );
    }
}
