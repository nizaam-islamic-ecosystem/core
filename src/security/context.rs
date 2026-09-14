//! Request-scoped trusted security context.
//!
//! `SecurityContext` contains trusted security identity associated with a
//! request after authentication has established that identity.
//!
//! Authentication mechanisms and authorization decisions are intentionally
//! outside this module. Likewise, credentials and provider-specific security
//! material must never be stored in this context.

use crate::security::identity::PrincipalIdentity;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Trusted request-scoped security information.
///
/// The context preserves both the principal represented by a request and,
/// when applicable, the trusted service that invoked the request on the
/// principal's behalf.
///
/// This type deliberately does not contain:
/// - authentication credentials;
/// - provider-specific authentication state;
/// - authorization decisions;
/// - roles or permissions;
/// - transport-specific security data.
///
/// Those concerns belong to their respective security layers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SecurityContext {
    /// The authenticated principal represented by this request.
    principal: PrincipalIdentity,

    /// The trusted service responsible for forwarding or invoking the request,
    /// when such a service exists.
    ///
    /// This remains optional because a request does not necessarily pass
    /// through an intermediate service.
    calling_service: Option<PrincipalIdentity>,
}

impl SecurityContext {
    /// Creates a security context for an authenticated principal.
    ///
    /// `calling_service` preserves an intermediary service identity when the
    /// request was made through a trusted service boundary.
    pub fn new(principal: PrincipalIdentity, calling_service: Option<PrincipalIdentity>) -> Self {
        Self {
            principal,
            calling_service,
        }
    }

    /// Returns the authenticated principal represented by this request.
    pub fn principal(&self) -> &PrincipalIdentity {
        &self.principal
    }

    /// Returns the trusted calling service, when one is present.
    pub fn calling_service(&self) -> Option<&PrincipalIdentity> {
        self.calling_service.as_ref()
    }
}

impl Hash for SecurityContext {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.principal.hash(state);
        self.calling_service.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn user_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(
            crate::security::identity::PrincipalType::User,
            crate::security::identity::PrincipalId::new(id).unwrap(),
        )
    }

    fn service_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(
            crate::security::identity::PrincipalType::Service,
            crate::security::identity::PrincipalId::new(id).unwrap(),
        )
    }

    fn engine_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(
            crate::security::identity::PrincipalType::Engine,
            crate::security::identity::PrincipalId::new(id).unwrap(),
        )
    }

    #[test]
    fn creates_context_without_calling_service() {
        let principal = user_identity("user-123");

        let context = SecurityContext::new(principal.clone(), None);

        assert_eq!(context.principal(), &principal);
        assert_eq!(context.calling_service(), None);
    }

    #[test]
    fn creates_context_with_calling_service() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        assert_eq!(context.principal(), &principal);
        assert_eq!(context.calling_service(), Some(&calling_service));
    }

    #[test]
    fn preserves_principal_and_calling_service_as_distinct_identities() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        assert_eq!(context.principal(), &principal);
        assert_eq!(context.calling_service(), Some(&calling_service));
        assert_ne!(context.principal(), context.calling_service().unwrap());
    }

    #[test]
    fn supports_service_as_authenticated_principal() {
        let principal = service_identity("service-a");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        assert_eq!(context.principal(), &principal);
        assert_eq!(context.calling_service(), Some(&calling_service));
    }

    #[test]
    fn supports_engine_as_calling_service_identity() {
        let principal = user_identity("user-123");
        let calling_engine = engine_identity("engine-a");

        let context = SecurityContext::new(principal.clone(), Some(calling_engine.clone()));

        assert_eq!(context.principal(), &principal);
        assert_eq!(context.calling_service(), Some(&calling_engine));
    }

    #[test]
    fn supports_engine_as_authenticated_principal() {
        let principal = engine_identity("engine-a");

        let context = SecurityContext::new(principal.clone(), None);

        assert_eq!(context.principal(), &principal);
        assert!(context.calling_service().is_none());
    }

    #[test]
    fn clone_preserves_security_context() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal, Some(calling_service));

        let cloned = context.clone();

        assert_eq!(context, cloned);
    }

    #[test]
    fn equality_depends_on_both_principal_and_calling_service() {
        let principal = user_identity("user-123");
        let gateway_one = service_identity("gateway-1");
        let gateway_two = service_identity("gateway-2");

        let first = SecurityContext::new(principal.clone(), Some(gateway_one.clone()));

        let same = SecurityContext::new(principal.clone(), Some(gateway_one));

        let different_service = SecurityContext::new(principal.clone(), Some(gateway_two));

        let no_service = SecurityContext::new(principal, None);

        assert_eq!(first, same);
        assert_ne!(first, different_service);
        assert_ne!(first, no_service);
    }

    #[test]
    fn hash_is_consistent_with_equality() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let first = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        let second = SecurityContext::new(principal, Some(calling_service));

        let mut contexts = HashSet::new();
        contexts.insert(first);

        assert!(contexts.contains(&second));
    }

    #[test]
    fn serializes_and_deserializes_without_calling_service() {
        let context = SecurityContext::new(user_identity("user-123"), None);

        let encoded = serde_json::to_string(&context).unwrap();
        let decoded: SecurityContext = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, context);
    }

    #[test]
    fn serializes_and_deserializes_with_calling_service() {
        let context = SecurityContext::new(
            user_identity("user-123"),
            Some(service_identity("gateway-1")),
        );

        let encoded = serde_json::to_string(&context).unwrap();
        let decoded: SecurityContext = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, context);
    }

    #[test]
    fn serialization_preserves_both_identities() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        let encoded = serde_json::to_string(&context).unwrap();
        let decoded: SecurityContext = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.principal(), &principal);
        assert_eq!(decoded.calling_service(), Some(&calling_service));
    }

    #[test]
    fn no_calling_service_means_no_intermediary_identity() {
        let context = SecurityContext::new(user_identity("user-123"), None);

        assert!(context.calling_service().is_none());
    }

    #[test]
    fn context_does_not_change_after_identity_values_are_cloned() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        let context_clone = context.clone();

        assert_eq!(context.principal(), &principal);
        assert_eq!(context_clone.principal(), &principal);

        assert_eq!(context.calling_service(), Some(&calling_service));
        assert_eq!(context_clone.calling_service(), Some(&calling_service));
    }
}
