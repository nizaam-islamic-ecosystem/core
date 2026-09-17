//! Validated idempotency-key and idempotency-identity primitives.
//!
//! An [`IdempotencyKey`] identifies a repeated submission of the same intended
//! logical action. An [`IdempotencyScope`] defines the contract-specific
//! namespace in which that key is meaningful. Together they form an
//! [`IdempotencyIdentity`].
//!
//! This module owns identity values only. Duplicate detection, stored outcomes,
//! retention, reconciliation, and state transitions remain outside this file.

use crate::identity::InvalidIdentity;

/// Identifies a repeated submission of the same intended logical action.
///
/// An idempotency key is distinct from [`crate::identity::OperationId`]. The
/// same key may be valid in different scopes and therefore is not required to
/// be globally unique across the Nizaam ecosystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Creates a validated idempotency key.
    ///
    /// Empty and whitespace-only values are rejected using the same Core
    /// identity validation contract as the existing identity types.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentity> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InvalidIdentity);
        }

        Ok(Self(value))
    }

    /// Returns the key value without allocating.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for IdempotencyKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for IdempotencyKey {
    type Err = InvalidIdentity;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for IdempotencyKey {
    type Error = InvalidIdentity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> serde::Deserialize<'de> for IdempotencyKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Defines the contract-specific namespace in which an idempotency key is
/// interpreted.
///
/// The scope is intentionally opaque. Operation, capability, service, tenant,
/// or other contract-specific semantics belong to the operation contract that
/// creates the scope rather than to this identity primitive.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct IdempotencyScope(String);

impl IdempotencyScope {
    /// Creates a validated idempotency scope.
    ///
    /// Empty and whitespace-only values are rejected using the same Core
    /// identity validation contract as the existing identity types.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentity> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InvalidIdentity);
        }

        Ok(Self(value))
    }

    /// Returns the scope value without allocating.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for IdempotencyScope {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for IdempotencyScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::str::FromStr for IdempotencyScope {
    type Err = InvalidIdentity;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for IdempotencyScope {
    type Error = InvalidIdentity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> serde::Deserialize<'de> for IdempotencyScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Composite identity used to recognize an idempotent logical submission.
///
/// Identity equality is structural:
///
/// ```text
/// IdempotencyIdentity = scope + key
/// ```
///
/// `OperationId`, capability identity, and other contract-specific discriminators
/// remain separate record-level information where the applicable contract
/// requires them for duplicate detection.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct IdempotencyIdentity {
    scope: IdempotencyScope,
    key: IdempotencyKey,
}

impl IdempotencyIdentity {
    /// Creates an idempotency identity from validated scope and key values.
    pub fn new(scope: IdempotencyScope, key: IdempotencyKey) -> Self {
        Self { scope, key }
    }

    /// Returns the scope component of this identity.
    pub fn scope(&self) -> &IdempotencyScope {
        &self.scope
    }

    /// Returns the key component of this identity.
    pub fn key(&self) -> &IdempotencyKey {
        &self.key
    }

    /// Splits this identity into its validated scope and key components.
    pub fn into_parts(self) -> (IdempotencyScope, IdempotencyKey) {
        (self.scope, self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::str::FromStr;

    #[test]
    fn idempotency_key_constructs_and_preserves_value() {
        let key = IdempotencyKey::new("request-123").unwrap();

        assert_eq!(key.as_str(), "request-123");
        assert_eq!(key.to_string(), "request-123");
    }

    #[test]
    fn idempotency_key_rejects_empty_and_whitespace_only_values() {
        assert!(IdempotencyKey::new("").is_err());
        assert!(IdempotencyKey::new("   ").is_err());
    }

    #[test]
    fn idempotency_key_preserves_non_empty_whitespace() {
        let key = IdempotencyKey::new("  request-123  ").unwrap();

        assert_eq!(key.as_str(), "  request-123  ");
    }

    #[test]
    fn idempotency_key_supports_as_ref_display_and_from_str() {
        let key = IdempotencyKey::from_str("request-456").unwrap();

        assert_eq!(key.as_ref(), "request-456");
        assert_eq!(key.to_string(), "request-456");
    }

    #[test]
    fn idempotency_key_supports_try_from_string() {
        let key = IdempotencyKey::try_from(String::from("request-789")).unwrap();

        assert_eq!(key.as_str(), "request-789");
    }

    #[test]
    fn idempotency_scope_constructs_and_preserves_value() {
        let scope = IdempotencyScope::new("payment-service").unwrap();

        assert_eq!(scope.as_str(), "payment-service");
        assert_eq!(scope.to_string(), "payment-service");
    }

    #[test]
    fn idempotency_scope_rejects_empty_and_whitespace_only_values() {
        assert!(IdempotencyScope::new("").is_err());
        assert!(IdempotencyScope::new("   ").is_err());
    }

    #[test]
    fn idempotency_scope_preserves_non_empty_whitespace() {
        let scope = IdempotencyScope::new("  service-a  ").unwrap();

        assert_eq!(scope.as_str(), "  service-a  ");
    }

    #[test]
    fn idempotency_scope_supports_as_ref_display_and_from_str() {
        let scope = IdempotencyScope::from_str("quran-service").unwrap();

        assert_eq!(scope.as_ref(), "quran-service");
        assert_eq!(scope.to_string(), "quran-service");
    }

    #[test]
    fn idempotency_scope_supports_try_from_string() {
        let scope = IdempotencyScope::try_from(String::from("scope-1")).unwrap();

        assert_eq!(scope.as_str(), "scope-1");
    }

    #[test]
    fn identity_constructs_from_scope_and_key() {
        let scope = IdempotencyScope::new("service-a").unwrap();
        let key = IdempotencyKey::new("request-1").unwrap();
        let identity = IdempotencyIdentity::new(scope.clone(), key.clone());

        assert_eq!(identity.scope(), &scope);
        assert_eq!(identity.key(), &key);
    }

    #[test]
    fn identity_can_be_split_into_its_parts() {
        let scope = IdempotencyScope::new("service-a").unwrap();
        let key = IdempotencyKey::new("request-2").unwrap();
        let identity = IdempotencyIdentity::new(scope.clone(), key.clone());

        assert_eq!(identity.into_parts(), (scope, key));
    }

    #[test]
    fn same_scope_and_key_produce_equal_identity() {
        let first = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-3").unwrap(),
        );
        let second = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-3").unwrap(),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn same_key_in_different_scopes_produces_distinct_identity() {
        let first = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-4").unwrap(),
        );
        let second = IdempotencyIdentity::new(
            IdempotencyScope::new("service-b").unwrap(),
            IdempotencyKey::new("request-4").unwrap(),
        );

        assert_ne!(first, second);
    }

    #[test]
    fn different_keys_in_one_scope_produce_distinct_identity() {
        let first = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-5").unwrap(),
        );
        let second = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-6").unwrap(),
        );

        assert_ne!(first, second);
    }

    #[test]
    fn identity_hashing_distinguishes_scopes() {
        let first = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-7").unwrap(),
        );
        let second = IdempotencyIdentity::new(
            IdempotencyScope::new("service-b").unwrap(),
            IdempotencyKey::new("request-7").unwrap(),
        );

        let mut identities = HashSet::new();
        identities.insert(first);
        identities.insert(second);

        assert_eq!(identities.len(), 2);
    }

    #[test]
    fn idempotency_key_serialization_round_trips() {
        let key = IdempotencyKey::new("request-8").unwrap();
        let encoded = serde_json::to_string(&key).unwrap();
        let decoded: IdempotencyKey = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, key);
    }

    #[test]
    fn idempotency_scope_serialization_round_trips() {
        let scope = IdempotencyScope::new("service-a").unwrap();
        let encoded = serde_json::to_string(&scope).unwrap();
        let decoded: IdempotencyScope = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, scope);
    }

    #[test]
    fn idempotency_identity_serialization_round_trips() {
        let identity = IdempotencyIdentity::new(
            IdempotencyScope::new("service-a").unwrap(),
            IdempotencyKey::new("request-9").unwrap(),
        );
        let encoded = serde_json::to_string(&identity).unwrap();
        let decoded: IdempotencyIdentity = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, identity);
    }

    #[test]
    fn deserialization_rejects_invalid_key_values() {
        let result = serde_json::from_str::<IdempotencyKey>("\"   \"");

        assert!(result.is_err());
    }

    #[test]
    fn deserialization_rejects_invalid_scope_values() {
        let result = serde_json::from_str::<IdempotencyScope>("\"\"");

        assert!(result.is_err());
    }
}
