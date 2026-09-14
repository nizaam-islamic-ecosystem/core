//! Provider-neutral authorization primitives for Nizaam Core.
//!
//! This module defines the generic Core authorization boundary.
//!
//! Authorization consumes a trusted principal identity, the optional calling
//! service identity, and the requested capability identity. It produces an
//! explicit allow/deny decision or reports that authorization evaluation
//! itself failed.
//!
//! Domain-specific authorization remains owned by the engine. Core does not
//! interpret engine-specific payload semantics here.

use crate::identity::CapabilityId;
use crate::security::identity::PrincipalIdentity;
use core::fmt;

/// Generic request information supplied to an [`Authorizer`].
///
/// This type intentionally contains only Core-level security information.
/// It does not contain engine-specific payloads or an executable capability
/// handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizationRequest<'a> {
    /// The trusted authenticated principal represented by the request.
    principal: &'a PrincipalIdentity,

    /// The trusted service through which the request was made, when applicable.
    calling_service: Option<&'a PrincipalIdentity>,

    /// The capability the caller is requesting to invoke.
    capability: &'a CapabilityId,
}

impl<'a> AuthorizationRequest<'a> {
    /// Creates an authorization request from trusted Core-level information.
    pub fn new(
        principal: &'a PrincipalIdentity,
        calling_service: Option<&'a PrincipalIdentity>,
        capability: &'a CapabilityId,
    ) -> Self {
        Self {
            principal,
            calling_service,
            capability,
        }
    }

    /// Returns the authenticated principal.
    pub fn principal(&self) -> &'a PrincipalIdentity {
        self.principal
    }

    /// Returns the trusted calling service, when one is present.
    pub fn calling_service(&self) -> Option<&'a PrincipalIdentity> {
        self.calling_service
    }

    /// Returns the requested capability identifier.
    pub fn capability(&self) -> &'a CapabilityId {
        self.capability
    }
}

/// Result of a completed generic Core authorization evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthorizationDecision {
    /// The caller is permitted to invoke the requested capability.
    Allow,

    /// The caller is not permitted to invoke the requested capability.
    Deny,
}

impl fmt::Display for AuthorizationDecision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => formatter.write_str("allow"),
            Self::Deny => formatter.write_str("deny"),
        }
    }
}

/// Failure reported when the authorization subsystem cannot complete its
/// evaluation.
///
/// A normal authorization refusal is represented by
/// [`AuthorizationDecision::Deny`] rather than by this error.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthorizationError {
    /// The authorization mechanism itself failed while evaluating the request.
    Failed,
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed => formatter.write_str("authorization evaluation failed"),
        }
    }
}

impl std::error::Error for AuthorizationError {}

/// Provider-neutral generic Core authorization boundary.
///
/// An authorizer evaluates whether a trusted principal may invoke a requested
/// capability under generic Core security rules.
///
/// The implementation must not interpret engine-specific payload semantics.
pub trait Authorizer: Send + Sync {
    /// Evaluates whether the caller may invoke the requested capability.
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::CapabilityId;
    use crate::security::identity::{PrincipalId, PrincipalIdentity, PrincipalType};

    fn user_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::User, PrincipalId::new(id).unwrap())
    }

    fn service_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new(id).unwrap())
    }

    fn engine_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new(id).unwrap())
    }

    fn capability_id(id: &str) -> CapabilityId {
        id.parse().unwrap()
    }

    #[derive(Debug)]
    struct AllowingAuthorizer;

    impl Authorizer for AllowingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Allow)
        }
    }

    #[derive(Debug)]
    struct DenyingAuthorizer;

    impl Authorizer for DenyingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Deny)
        }
    }

    #[derive(Debug)]
    struct FailingAuthorizer;

    impl Authorizer for FailingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Err(AuthorizationError::Failed)
        }
    }

    #[test]
    fn authorization_request_preserves_principal() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert_eq!(request.principal(), &principal);
    }

    #[test]
    fn authorization_request_supports_no_calling_service() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert!(request.calling_service().is_none());
    }

    #[test]
    fn authorization_request_preserves_calling_service() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        assert_eq!(request.calling_service(), Some(&calling_service));
    }

    #[test]
    fn authorization_request_preserves_capability() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert_eq!(request.capability(), &capability);
    }

    #[test]
    fn authorization_request_preserves_distinct_principal_and_calling_service() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        assert_eq!(request.principal(), &principal);
        assert_eq!(request.calling_service(), Some(&calling_service));
        assert_ne!(request.principal(), request.calling_service().unwrap());
    }

    #[test]
    fn authorization_request_supports_service_principal() {
        let principal = service_identity("service-a");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert_eq!(request.principal(), &principal);
    }

    #[test]
    fn authorization_request_supports_engine_principal() {
        let principal = engine_identity("quran-engine");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert_eq!(request.principal(), &principal);
    }

    #[test]
    fn authorization_request_supports_engine_calling_service() {
        let principal = user_identity("user-123");
        let calling_engine = engine_identity("gateway-engine");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, Some(&calling_engine), &capability);

        assert_eq!(request.calling_service(), Some(&calling_engine));
    }

    #[test]
    fn authorization_request_clone_preserves_values() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        let cloned = request;

        assert_eq!(cloned.principal(), &principal);
        assert_eq!(cloned.calling_service(), Some(&calling_service));
        assert_eq!(cloned.capability(), &capability);
    }

    #[test]
    fn authorization_request_equality_depends_on_all_fields() {
        let principal = user_identity("user-123");
        let other_principal = user_identity("user-456");
        let calling_service = service_identity("gateway-1");
        let other_service = service_identity("gateway-2");
        let capability = capability_id("quran.read");
        let other_capability = capability_id("quran.search");

        let first = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        let same = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        let different_principal =
            AuthorizationRequest::new(&other_principal, Some(&calling_service), &capability);

        let different_service =
            AuthorizationRequest::new(&principal, Some(&other_service), &capability);

        let different_capability =
            AuthorizationRequest::new(&principal, Some(&calling_service), &other_capability);

        assert_eq!(first, same);
        assert_ne!(first, different_principal);
        assert_ne!(first, different_service);
        assert_ne!(first, different_capability);
    }

    #[test]
    fn authorization_decision_has_explicit_semantics() {
        assert_eq!(AuthorizationDecision::Allow.to_string(), "allow");
        assert_eq!(AuthorizationDecision::Deny.to_string(), "deny");
        assert_ne!(AuthorizationDecision::Allow, AuthorizationDecision::Deny);
    }

    #[test]
    fn authorization_error_has_expected_message() {
        assert_eq!(
            AuthorizationError::Failed.to_string(),
            "authorization evaluation failed"
        );
    }

    #[test]
    fn authorization_error_implements_error_trait() {
        let error = AuthorizationError::Failed;

        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn allowing_authorizer_returns_allow() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");
        let request = AuthorizationRequest::new(&principal, None, &capability);

        let authorizer = AllowingAuthorizer;

        assert_eq!(
            authorizer.authorize(&request),
            Ok(AuthorizationDecision::Allow)
        );
    }

    #[test]
    fn denying_authorizer_returns_deny() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");
        let request = AuthorizationRequest::new(&principal, None, &capability);

        let authorizer = DenyingAuthorizer;

        assert_eq!(
            authorizer.authorize(&request),
            Ok(AuthorizationDecision::Deny)
        );
    }

    #[test]
    fn failing_authorizer_returns_error() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");
        let request = AuthorizationRequest::new(&principal, None, &capability);

        let authorizer = FailingAuthorizer;

        assert_eq!(
            authorizer.authorize(&request),
            Err(AuthorizationError::Failed)
        );
    }

    #[test]
    fn deny_is_distinct_from_authorization_failure() {
        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");
        let request = AuthorizationRequest::new(&principal, None, &capability);

        let denying = DenyingAuthorizer;
        let failing = FailingAuthorizer;

        assert_eq!(denying.authorize(&request), Ok(AuthorizationDecision::Deny));

        assert_eq!(failing.authorize(&request), Err(AuthorizationError::Failed));
    }

    #[test]
    fn authorizer_can_be_used_as_trait_object() {
        let authorizer: Box<dyn Authorizer> = Box::new(AllowingAuthorizer);

        let principal = user_identity("user-123");
        let capability = capability_id("quran.read");
        let request = AuthorizationRequest::new(&principal, None, &capability);

        assert_eq!(
            authorizer.authorize(&request),
            Ok(AuthorizationDecision::Allow)
        );
    }

    #[test]
    fn authorizer_trait_requires_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<AllowingAuthorizer>();
        assert_send_sync::<DenyingAuthorizer>();
        assert_send_sync::<FailingAuthorizer>();
    }

    #[test]
    fn authorization_request_contains_only_core_level_information() {
        let principal = user_identity("user-123");
        let calling_service = service_identity("gateway-1");
        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

        assert_eq!(request.principal(), &principal);
        assert_eq!(request.calling_service(), Some(&calling_service));
        assert_eq!(request.capability(), &capability);
    }
}
