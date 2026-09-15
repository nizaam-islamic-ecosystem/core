//! How physical content can be accessed.
//!
//! `ContentReference` is a provider-neutral abstraction describing where or
//! how the physical representation of an artifact version can be accessed.
//! Core must not become an object-storage implementation.
//!
//! A physical representation may be stored in local storage, a database,
//! object storage, distributed storage, a remote service, or another
//! provider implementation.

/// Provider-neutral reference to physical artifact content.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContentReference {
    provider: String,
    location: String,
}

impl ContentReference {
    /// Creates a new content reference.
    ///
    /// Construction remains infallible to preserve the lightweight value-object
    /// API. Callers that perform artifact operations must validate the reference
    /// before using it.
    pub fn new(provider: impl Into<String>, location: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            location: location.into(),
        }
    }

    /// Returns the provider identifier.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the provider-specific location or identifier.
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Returns whether the provider identifier is non-empty and non-whitespace.
    pub fn has_valid_provider(&self) -> bool {
        !self.provider.trim().is_empty()
    }

    /// Returns whether the location identifier is non-empty and non-whitespace.
    pub fn has_valid_location(&self) -> bool {
        !self.location.trim().is_empty()
    }

    /// Returns whether this content reference contains the minimum required
    /// information for artifact-level use.
    pub fn is_valid(&self) -> bool {
        self.has_valid_provider() && self.has_valid_location()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_reference_preserves_provider_and_location() {
        let reference = ContentReference::new("local", "/path/to/content");

        assert_eq!(reference.provider(), "local");
        assert_eq!(reference.location(), "/path/to/content");
        assert!(reference.is_valid());
    }

    #[test]
    fn content_reference_supports_different_providers() {
        let local = ContentReference::new("local", "/path/1");
        let object = ContentReference::new("object-storage", "bucket/key");

        assert_ne!(local, object);
        assert_eq!(local.provider(), "local");
        assert_eq!(object.provider(), "object-storage");
    }

    #[test]
    fn content_reference_rejects_empty_provider_semantically() {
        let reference = ContentReference::new("", "/content");

        assert!(!reference.has_valid_provider());
        assert!(!reference.is_valid());
    }

    #[test]
    fn content_reference_rejects_whitespace_provider_semantically() {
        let reference = ContentReference::new(" \t\n", "/content");

        assert!(!reference.has_valid_provider());
        assert!(!reference.is_valid());
    }

    #[test]
    fn content_reference_rejects_empty_location_semantically() {
        let reference = ContentReference::new("local", "");

        assert!(!reference.has_valid_location());
        assert!(!reference.is_valid());
    }

    #[test]
    fn content_reference_rejects_whitespace_location_semantically() {
        let reference = ContentReference::new("local", " \t\n");

        assert!(!reference.has_valid_location());
        assert!(!reference.is_valid());
    }

    #[test]
    fn content_reference_preserves_original_strings() {
        let reference = ContentReference::new("  local  ", "  /content  ");

        assert_eq!(reference.provider(), "  local  ");
        assert_eq!(reference.location(), "  /content  ");
        assert!(reference.is_valid());
    }

    #[test]
    fn content_reference_supports_clone_and_eq() {
        let first = ContentReference::new("provider", "loc");
        let second = first.clone();

        assert_eq!(first, second);
    }

    #[test]
    fn content_reference_can_be_distinct_by_location() {
        let first = ContentReference::new("provider", "loc-1");
        let second = ContentReference::new("provider", "loc-2");

        assert_ne!(first, second);
    }
}
