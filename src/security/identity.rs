//! Trusted principal identity primitives for the Core security boundary.
//!
//! This module defines who a request represents. It deliberately does not
//! perform authentication or authorization and does not contain any
//! provider-specific credential handling.

use crate::identity::InvalidIdentity;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The kind of principal represented by a trusted security identity.
///
/// Principal type describes what kind of actor the identity represents. It
/// does not describe permissions, roles, authentication state, or
/// authorization results.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PrincipalType {
    /// A human user or other end-user principal.
    User,

    /// A service acting on behalf of itself or another principal.
    Service,

    /// A Nizaam engine principal.
    Engine,
}

impl fmt::Display for PrincipalType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::User => "user",
            Self::Service => "service",
            Self::Engine => "engine",
        };

        formatter.write_str(value)
    }
}

/// A validated identifier for a security principal.
///
/// A principal identifier is intentionally independent from other Core
/// identifiers such as `EngineId` or `OperationId` because a principal does
/// not have to be an engine or an operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PrincipalId(String);

impl<'de> Deserialize<'de> for PrincipalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl PrincipalId {
    /// Creates a principal identifier.
    ///
    /// Empty and whitespace-only values are rejected using the existing Core
    /// identity validation error.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentity> {
        let value = value.into();

        if value.trim().is_empty() {
            return Err(InvalidIdentity);
        }

        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PrincipalId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PrincipalId {
    type Err = InvalidIdentity;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for PrincipalId {
    type Error = InvalidIdentity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// A trusted identity describing the principal represented by a request.
///
/// `PrincipalIdentity` answers the identity question: *who is this
/// principal?*
///
/// Authentication state, authorization state, scopes, credentials, and
/// request-scoped security metadata belong to higher-level security
/// abstractions and are intentionally not stored here.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PrincipalIdentity {
    principal_type: PrincipalType,
    principal_id: PrincipalId,
}

impl PrincipalIdentity {
    /// Creates a trusted principal identity from its type and identifier.
    pub fn new(principal_type: PrincipalType, principal_id: PrincipalId) -> Self {
        Self {
            principal_type,
            principal_id,
        }
    }

    /// Returns the type of the principal.
    pub fn principal_type(&self) -> PrincipalType {
        self.principal_type
    }

    /// Returns the principal identifier.
    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_type_has_expected_variants() {
        assert_eq!(PrincipalType::User.to_string(), "user");
        assert_eq!(PrincipalType::Service.to_string(), "service");
        assert_eq!(PrincipalType::Engine.to_string(), "engine");
    }

    #[test]
    fn principal_type_serializes_and_deserializes() {
        let principal_type = PrincipalType::Service;

        let encoded = serde_json::to_string(&principal_type).unwrap();
        let decoded: PrincipalType = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, principal_type);
    }

    #[test]
    fn principal_id_constructs_and_preserves_value() {
        let id = PrincipalId::new("user-123").unwrap();

        assert_eq!(id.as_str(), "user-123");
        assert_eq!(id.to_string(), "user-123");
        assert_eq!(id.as_ref(), "user-123");
    }

    #[test]
    fn principal_id_rejects_empty_value() {
        assert_eq!(PrincipalId::new("").unwrap_err(), InvalidIdentity);
    }

    #[test]
    fn principal_id_rejects_whitespace_only_value() {
        assert_eq!(PrincipalId::new("   ").unwrap_err(), InvalidIdentity);
    }

    #[test]
    fn principal_id_accepts_non_empty_value_with_surrounding_whitespace() {
        let id = PrincipalId::new("  user-123  ").unwrap();

        assert_eq!(id.as_str(), "  user-123  ");
    }

    #[test]
    fn principal_id_parses_from_str() {
        let id: PrincipalId = "service-123".parse().unwrap();

        assert_eq!(id.as_str(), "service-123");
    }

    #[test]
    fn principal_id_try_from_string_works() {
        let id = PrincipalId::try_from(String::from("engine-123")).unwrap();

        assert_eq!(id.as_str(), "engine-123");
    }

    #[test]
    fn principal_id_rejects_invalid_from_str_value() {
        let result = "   ".parse::<PrincipalId>();

        assert_eq!(result.unwrap_err(), InvalidIdentity);
    }

    #[test]
    fn principal_identity_constructs_and_exposes_components() {
        let principal_id = PrincipalId::new("user-123").unwrap();
        let identity = PrincipalIdentity::new(PrincipalType::User, principal_id.clone());

        assert_eq!(identity.principal_type(), PrincipalType::User);
        assert_eq!(identity.principal_id(), &principal_id);
    }

    #[test]
    fn principal_identity_preserves_equality_and_hash_semantics() {
        use std::collections::HashSet;

        let id1 = PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("gateway-1").unwrap(),
        );
        let id2 = PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("gateway-1").unwrap(),
        );
        let id3 = PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("gateway-2").unwrap(),
        );

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        let mut identities = HashSet::new();
        identities.insert(id1);

        assert!(identities.contains(&id2));
        assert!(!identities.contains(&id3));
    }

    #[test]
    fn principal_identity_distinguishes_principal_types() {
        let user = PrincipalIdentity::new(PrincipalType::User, PrincipalId::new("123").unwrap());

        let service =
            PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new("123").unwrap());

        let engine =
            PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new("123").unwrap());

        assert_ne!(user, service);
        assert_ne!(service, engine);
        assert_ne!(user, engine);
    }

    #[test]
    fn principal_id_serializes_and_deserializes() {
        let principal_id = PrincipalId::new("user-123").unwrap();

        let encoded = serde_json::to_string(&principal_id).unwrap();
        let decoded: PrincipalId = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, principal_id);
    }

    #[test]
    fn principal_id_serde_rejects_empty_string() {
        let result: Result<PrincipalId, _> = serde_json::from_str("\"\"");

        assert!(result.is_err());
    }

    #[test]
    fn principal_id_serde_rejects_whitespace_only_string() {
        let result: Result<PrincipalId, _> = serde_json::from_str("\"   \"");

        assert!(result.is_err());
    }

    #[test]
    fn principal_identity_serializes_and_deserializes() {
        let identity = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("quran-engine").unwrap(),
        );

        let encoded = serde_json::to_string(&identity).unwrap();
        let decoded: PrincipalIdentity = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, identity);
    }
}
