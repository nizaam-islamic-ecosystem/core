//! Fundamental result primitives shared by later Core systems.

use crate::identity::ArtifactId;

/// The high level technical outcome of a Core operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Success,
    Failure,
    Cancelled,
    TimedOut,
}

/// Whether a technical failure may be retried by a policy that permits it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retryability {
    Retryable,
    NonRetryable,
}

/// Compatibility result for a contract, schema, or version comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Compatibility {
    Compatible,
    Incompatible,
    Unknown,
}

/// Stable reference to an Error System event or definition.
///
/// The Error System owns the referenced error's definition and occurrence
/// semantics. This primitive deliberately does not duplicate them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ErrorReference(String);

impl ErrorReference {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reference to an artifact without assigning domain meaning to its content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReference {
    pub artifact_id: ArtifactId,
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_references_require_a_value() {
        assert!(ErrorReference::new("").is_none());
        assert!(ErrorReference::new("error-event-1").is_some());
    }
}
