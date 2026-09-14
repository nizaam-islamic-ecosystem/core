//! Provider-neutral authentication primitives for Nizaam Core.
//!
//! This module defines the authentication boundary without selecting a
//! credential mechanism or authentication provider.
//!
//! Authentication establishes a trusted [`PrincipalIdentity`]. It does not
//! perform authorization and it does not construct or mutate request-scoped
//! [`SecurityContext`] values.

use crate::security::identity::PrincipalIdentity;
use core::fmt;

/// Authentication input supplied to an [`Authenticator`].
///
/// Core intentionally treats the input as opaque provider data. A concrete
/// authentication implementation may interpret it as a token, API key,
/// certificate-derived material, or another credential format without
/// requiring Core to understand that format.
///
/// The input is borrowed because the authenticator does not own the original
/// authentication material.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticationRequest<'a> {
    credentials: &'a [u8],
}

impl<'a> AuthenticationRequest<'a> {
    /// Creates an authentication request from opaque authentication material.
    pub fn new(credentials: &'a [u8]) -> Self {
        Self { credentials }
    }

    /// Returns the opaque authentication material.
    pub fn credentials(&self) -> &'a [u8] {
        self.credentials
    }

    /// Returns whether the authentication input is empty.
    pub fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }
}

/// Failure returned when authentication cannot establish a trusted identity.
///
/// These variants describe Core-level authentication outcomes rather than
/// provider-specific failures. Concrete authentication implementations are
/// responsible for mapping provider-specific errors into these categories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthenticationError {
    /// No authentication material was supplied.
    MissingCredentials,

    /// Authentication material was supplied but could not be validated.
    InvalidCredentials,

    /// Authentication could not be completed because of an authentication
    /// subsystem failure that is not represented by the other variants.
    Failed,
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingCredentials => "authentication credentials are required",
            Self::InvalidCredentials => "authentication credentials are invalid",
            Self::Failed => "authentication failed",
        };

        formatter.write_str(message)
    }
}

impl std::error::Error for AuthenticationError {}

/// Provider-neutral authentication boundary.
///
/// An authenticator establishes and validates the identity represented by an
/// authentication request. Successful authentication produces a trusted
/// [`PrincipalIdentity`].
///
/// The authenticator does not:
/// - make authorization decisions;
/// - resolve capabilities;
/// - invoke handlers;
/// - construct request execution contexts;
/// - interpret engine-specific payloads;
/// - depend on a specific authentication provider.
pub trait Authenticator: Send + Sync {
    /// Authenticates the supplied request and returns its trusted identity.
    fn authenticate(
        &self,
        request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::identity::{PrincipalId, PrincipalType};

    fn user_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::User, PrincipalId::new(id).unwrap())
    }

    fn service_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new(id).unwrap())
    }

    fn engine_identity(id: &str) -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new(id).unwrap())
    }

    #[derive(Debug)]
    struct MockUserAuthenticator;

    impl Authenticator for MockUserAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            if request.is_empty() {
                return Err(AuthenticationError::MissingCredentials);
            }

            if request.credentials() == b"valid-user" {
                return Ok(user_identity("user-123"));
            }

            Err(AuthenticationError::InvalidCredentials)
        }
    }

    #[derive(Debug)]
    struct MockServiceAuthenticator;

    impl Authenticator for MockServiceAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            if request.credentials() == b"valid-service" {
                return Ok(service_identity("gateway-1"));
            }

            Err(AuthenticationError::InvalidCredentials)
        }
    }

    #[derive(Debug)]
    struct MockEngineAuthenticator;

    impl Authenticator for MockEngineAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            if request.credentials() == b"valid-engine" {
                return Ok(engine_identity("quran-engine"));
            }

            Err(AuthenticationError::InvalidCredentials)
        }
    }

    #[derive(Debug)]
    struct FailingAuthenticator;

    impl Authenticator for FailingAuthenticator {
        fn authenticate(
            &self,
            _request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            Err(AuthenticationError::Failed)
        }
    }

    #[test]
    fn authentication_request_preserves_credentials() {
        let request = AuthenticationRequest::new(b"opaque-credential");

        assert_eq!(request.credentials(), b"opaque-credential");
        assert!(!request.is_empty());
    }

    #[test]
    fn authentication_request_detects_empty_credentials() {
        let request = AuthenticationRequest::new(b"");

        assert!(request.is_empty());
        assert_eq!(request.credentials(), b"");
    }

    #[test]
    fn mock_authenticator_authenticates_user() {
        let authenticator = MockUserAuthenticator;
        let request = AuthenticationRequest::new(b"valid-user");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity, user_identity("user-123"));
    }

    #[test]
    fn mock_authenticator_authenticates_service() {
        let authenticator = MockServiceAuthenticator;
        let request = AuthenticationRequest::new(b"valid-service");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity, service_identity("gateway-1"));
    }

    #[test]
    fn mock_authenticator_authenticates_engine() {
        let authenticator = MockEngineAuthenticator;
        let request = AuthenticationRequest::new(b"valid-engine");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity, engine_identity("quran-engine"));
    }

    #[test]
    fn missing_credentials_are_reported() {
        let authenticator = MockUserAuthenticator;
        let request = AuthenticationRequest::new(b"");

        let result = authenticator.authenticate(&request);

        assert_eq!(result, Err(AuthenticationError::MissingCredentials));
    }

    #[test]
    fn invalid_credentials_are_reported() {
        let authenticator = MockUserAuthenticator;
        let request = AuthenticationRequest::new(b"invalid-user");

        let result = authenticator.authenticate(&request);

        assert_eq!(result, Err(AuthenticationError::InvalidCredentials));
    }

    #[test]
    fn provider_failure_is_reported_as_failed() {
        let authenticator = FailingAuthenticator;
        let request = AuthenticationRequest::new(b"credential");

        let result = authenticator.authenticate(&request);

        assert_eq!(result, Err(AuthenticationError::Failed));
    }

    #[test]
    fn authentication_errors_have_distinct_variants() {
        assert_ne!(
            AuthenticationError::MissingCredentials,
            AuthenticationError::InvalidCredentials
        );
        assert_ne!(
            AuthenticationError::InvalidCredentials,
            AuthenticationError::Failed
        );
        assert_ne!(
            AuthenticationError::MissingCredentials,
            AuthenticationError::Failed
        );
    }

    #[test]
    fn authentication_errors_have_stable_messages() {
        assert_eq!(
            AuthenticationError::MissingCredentials.to_string(),
            "authentication credentials are required"
        );
        assert_eq!(
            AuthenticationError::InvalidCredentials.to_string(),
            "authentication credentials are invalid"
        );
        assert_eq!(
            AuthenticationError::Failed.to_string(),
            "authentication failed"
        );
    }

    #[test]
    fn authentication_error_implements_error_trait() {
        let error = AuthenticationError::Failed;

        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn authenticator_can_be_used_through_trait_object() {
        let authenticator: Box<dyn Authenticator> = Box::new(MockUserAuthenticator);
        let request = AuthenticationRequest::new(b"valid-user");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity, user_identity("user-123"));
    }

    #[test]
    fn authenticator_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<MockUserAuthenticator>();
        assert_send_sync::<MockServiceAuthenticator>();
        assert_send_sync::<MockEngineAuthenticator>();
        assert_send_sync::<FailingAuthenticator>();
    }

    #[test]
    fn authentication_input_remains_opaque_to_core() {
        let first = AuthenticationRequest::new(b"provider-a-format");
        let second = AuthenticationRequest::new(b"provider-b-format");

        assert_ne!(first, second);

        // Core only exposes the supplied bytes. Interpretation remains the
        // responsibility of the Authenticator implementation.
        assert_eq!(first.credentials(), b"provider-a-format");
        assert_eq!(second.credentials(), b"provider-b-format");
    }

    #[test]
    fn authentication_returns_identity_without_authorization_state() {
        let authenticator = MockUserAuthenticator;
        let request = AuthenticationRequest::new(b"valid-user");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity.principal_type(), PrincipalType::User);
        assert_eq!(identity.principal_id().as_str(), "user-123");
    }
}
