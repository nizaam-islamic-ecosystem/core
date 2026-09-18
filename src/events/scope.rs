//! Generic scope representation for internal Nizaam Core events.
//!
//! A `Scope` describes the applicability boundary of an event. Scope
//! matching is distinct from security authorization and does not interpret
//! domain-specific semantics.
//!
//! Scope values use exact equality semantics. This module does not define
//! hierarchical scopes, wildcard scopes, routing policies, or authorization.

use std::fmt;

/// Error returned when an Event scope is empty or consists only of whitespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidScope;

impl fmt::Display for InvalidScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an event scope must not be empty")
    }
}

impl std::error::Error for InvalidScope {}

/// Identifies the generic applicability scope of an internal event.
///
/// Scope is an applicability mechanism, not an authorization mechanism.
/// Equality determines whether two scopes are the same. Core does not assign
/// domain-specific meaning to the value.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Scope(String);

impl Scope {
    /// Creates a scope from a non-empty value.
    ///
    /// Empty and whitespace-only values are rejected. No additional syntax or
    /// domain-specific interpretation is imposed by Core.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidScope> {
        let value = value.into();

        if value.trim().is_empty() {
            return Err(InvalidScope);
        }

        Ok(Self(value))
    }

    /// Returns the underlying scope value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Construction and validation
    // -------------------------------------------------------------------------

    #[test]
    fn scope_constructs_with_valid_value() {
        let scope = Scope::new("engine-a").unwrap();

        assert_eq!(scope.as_str(), "engine-a");
        assert_eq!(scope.to_string(), "engine-a");
    }

    #[test]
    fn scope_rejects_empty_value() {
        assert!(Scope::new("").is_err());
    }

    #[test]
    fn scope_rejects_whitespace_only_value() {
        assert!(Scope::new("   ").is_err());
        assert!(Scope::new("\t\n").is_err());
    }

    #[test]
    fn scope_preserves_value_without_interpretation() {
        let scope = Scope::new("tenant:example/engine-a").unwrap();

        assert_eq!(scope.as_str(), "tenant:example/engine-a");
    }

    // -------------------------------------------------------------------------
    // Equality and identity-independent value semantics
    // -------------------------------------------------------------------------

    #[test]
    fn equal_scope_values_are_equal() {
        let first = Scope::new("engine-a").unwrap();
        let second = Scope::new("engine-a").unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn different_scope_values_are_not_equal() {
        let first = Scope::new("engine-a").unwrap();
        let second = Scope::new("engine-b").unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn scope_uses_exact_equality_without_hierarchical_matching() {
        let parent = Scope::new("engine-a").unwrap();
        let child = Scope::new("engine-a/instance-1").unwrap();

        assert_ne!(parent, child);
    }

    #[test]
    fn scope_does_not_treat_special_values_as_wildcards() {
        let wildcard = Scope::new("*").unwrap();
        let concrete = Scope::new("engine-a").unwrap();

        assert_ne!(wildcard, concrete);
    }

    // -------------------------------------------------------------------------
    // Access and standard traits
    // -------------------------------------------------------------------------

    #[test]
    fn scope_implements_as_ref_str() {
        let scope = Scope::new("engine-a").unwrap();

        let value: &str = scope.as_ref();

        assert_eq!(value, "engine-a");
    }

    #[test]
    fn scope_clones_without_changing_value() {
        let original = Scope::new("engine-a").unwrap();
        let cloned = original.clone();

        assert_eq!(original, cloned);
        assert_eq!(cloned.as_str(), "engine-a");
    }

    #[test]
    fn scope_can_be_used_as_hash_map_key() {
        use std::collections::HashMap;

        let scope = Scope::new("engine-a").unwrap();
        let same_scope = Scope::new("engine-a").unwrap();

        let mut scopes = HashMap::new();
        scopes.insert(scope, "value");

        assert_eq!(scopes.get(&same_scope), Some(&"value"));
    }

    // -------------------------------------------------------------------------
    // Error behavior
    // -------------------------------------------------------------------------

    #[test]
    fn invalid_scope_error_has_expected_message() {
        let error = InvalidScope;

        assert_eq!(error.to_string(), "an event scope must not be empty");
        assert_eq!(error, InvalidScope);
        assert_eq!(error.clone(), InvalidScope);
    }
}
