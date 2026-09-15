//! Stable logical identity for a managed artifact.
//!
//! An `Artifact` is a uniquely identifiable logical piece of content or
//! produced output that can be referenced, versioned, validated, accessed,
//! and associated with provenance.
//!
//! It does not own artifact content, versions, storage, or lifecycle state.
//! It represents only the stable logical identity of the artifact.

use crate::identity::ArtifactId;

/// Stable logical identity of a managed artifact.
///
/// `Artifact` deliberately contains only the stable [`ArtifactId`].
/// Artifact versions, content references, lifecycle state, and storage are
/// represented by separate Phase 10 mechanisms.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    id: ArtifactId,
}

impl Artifact {
    /// Creates a new artifact from an already validated [`ArtifactId`].
    pub fn new(id: ArtifactId) -> Self {
        Self { id }
    }

    /// Returns the stable artifact identifier.
    pub fn id(&self) -> &ArtifactId {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::ArtifactId;

    #[test]
    fn artifact_wraps_artifact_id() {
        let id = ArtifactId::new("test-artifact").unwrap();
        let artifact = Artifact::new(id.clone());

        assert_eq!(artifact.id(), &id);
    }

    #[test]
    fn artifact_preserves_stable_identity() {
        let id = ArtifactId::new("stable-id").unwrap();
        let first = Artifact::new(id.clone());
        let second = Artifact::new(id.clone());

        assert_eq!(first, second);
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn artifact_supports_clone_and_eq() {
        let id = ArtifactId::new("clone-test").unwrap();
        let artifact = Artifact::new(id);
        let cloned = artifact.clone();

        assert_eq!(artifact, cloned);
    }
}