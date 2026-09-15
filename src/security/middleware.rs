//! Security middleware for Nizaam Core.
//!
//! This module connects the provider-neutral authentication and authorization
//! boundaries to the generic middleware system.
//!
//! The middleware:
//!
//! 1. obtains opaque authentication material through a caller-supplied
//!    credential extractor;
//! 2. authenticates that material into a trusted `PrincipalIdentity`;
//! 3. establishes a request-scoped `SecurityContext` on the existing
//!    `EngineContext`;
//! 4. identifies the requested capability from the existing Core request
//!    descriptor;
//! 5. performs generic Core authorization;
//! 6. continues only when authentication and authorization succeed.
//!
//! Provider-specific credential formats and engine/domain-specific
//! authorization remain outside this module.

use crate::{contracts::UniversalRequest, runtime::EngineContext};

use super::{
    authentication::{AuthenticationError, AuthenticationRequest, Authenticator},
    authorization::{AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer},
    context::SecurityContext,
};

use crate::middleware::stages::{
    Middleware, MiddlewareError, MiddlewareRejection, MiddlewareResult,
};

/// Extracts opaque authentication material from an incoming request.
///
/// The extractor is intentionally separate from `Authenticator`.
///
/// The extractor is responsible only for obtaining authentication material.
/// The authenticator is responsible for interpreting and validating that
/// material.
///
/// This separation keeps transport-specific credential extraction outside
/// Core security policy.
pub trait CredentialExtractor: Send + Sync {
    /// Returns authentication material for the supplied request.
    ///
    /// `None` means that no authentication material was available.
    fn extract(&self, context: &EngineContext, request: &UniversalRequest) -> Option<Vec<u8>>;
}

/// Security middleware that performs authentication and generic authorization.
///
/// The middleware is provider-neutral:
///
/// ```text
/// UniversalRequest
///        ↓
/// CredentialExtractor
///        ↓
/// opaque credentials
///        ↓
/// Authenticator
///        ↓
/// PrincipalIdentity
///        ↓
/// SecurityContext
///        ↓
/// AuthorizationRequest
///        ↓
/// Authorizer
///        ↓
/// Continue / Reject / Fail
/// ```
///
/// The middleware does not perform capability resolution or dispatch.
pub struct SecurityMiddleware<A, Z, C> {
    authenticator: A,
    authorizer: Z,
    credential_extractor: C,
}

impl<A, Z, C> SecurityMiddleware<A, Z, C>
where
    A: Authenticator,
    Z: Authorizer,
    C: CredentialExtractor,
{
    /// Creates security middleware from its authentication, authorization,
    /// and credential-extraction components.
    pub fn new(authenticator: A, authorizer: Z, credential_extractor: C) -> Self {
        Self {
            authenticator,
            authorizer,
            credential_extractor,
        }
    }

    /// Returns a reference to the configured authenticator.
    pub fn authenticator(&self) -> &A {
        &self.authenticator
    }

    /// Returns a reference to the configured authorizer.
    pub fn authorizer(&self) -> &Z {
        &self.authorizer
    }

    /// Returns a reference to the configured credential extractor.
    pub fn credential_extractor(&self) -> &C {
        &self.credential_extractor
    }
}

impl<A, Z, C> std::fmt::Debug for SecurityMiddleware<A, Z, C>
where
    A: Authenticator,
    Z: Authorizer,
    C: CredentialExtractor,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecurityMiddleware")
            .finish_non_exhaustive()
    }
}

impl<A, Z, C> Middleware for SecurityMiddleware<A, Z, C>
where
    A: Authenticator,
    Z: Authorizer,
    C: CredentialExtractor,
{
    fn on_request(
        &self,
        context: &mut EngineContext,
        request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        /*
         * Authentication material is deliberately extracted by a separate
         * provider-neutral component. Core does not interpret transport
         * headers, JWTs, API keys, certificates, or any other credential
         * format here.
         */
        let credentials = match self.credential_extractor.extract(context, request) {
            Some(credentials) if !credentials.is_empty() => credentials,

            None => {
                return MiddlewareResult::Reject(MiddlewareRejection::new(
                    "authentication credentials are required",
                ));
            }

            Some(_) => {
                return MiddlewareResult::Reject(MiddlewareRejection::new(
                    "authentication credentials are required",
                ));
            }
        };

        /*
         * The authentication input is borrowed only for the duration of the
         * authentication operation. It is never placed into SecurityContext.
         */
        let authentication_request = AuthenticationRequest::new(&credentials);

        let principal = match self.authenticator.authenticate(&authentication_request) {
            Ok(principal) => principal,

            Err(AuthenticationError::MissingCredentials) => {
                return MiddlewareResult::Reject(MiddlewareRejection::new(
                    "authentication credentials are required",
                ));
            }

            Err(AuthenticationError::InvalidCredentials) => {
                return MiddlewareResult::Reject(MiddlewareRejection::new(
                    "authentication credentials are invalid",
                ));
            }

            Err(AuthenticationError::Failed) => {
                return MiddlewareResult::Fail(MiddlewareError::new("authentication failed"));
            }
        };

        /*
         * Preserve an already-established trusted calling-service identity
         * when the current request context contains one.
         *
         * We intentionally do not infer calling-service identity from the
         * request payload or arbitrary transport metadata.
         */
        let calling_service = context
            .security()
            .and_then(SecurityContext::calling_service)
            .cloned();

        let security_context = SecurityContext::new(principal, calling_service);

        /*
         * EngineContext::with_security already provides the existing Core
         * context replacement mechanism. We replace the current request's
         * context with the enriched context rather than creating a second
         * competing request context.
         */
        *context = context.clone().with_security(security_context);

        /*
         * Capability identification uses the Core contract descriptor.
         * No engine-specific payload interpretation occurs here.
         */
        let capability_id = &request.envelope.metadata.descriptor.capability_id;

        let security = context
            .security()
            .expect("security context must be established after authentication");

        let authorization_request = AuthorizationRequest::new(
            security.principal(),
            security.calling_service(),
            capability_id,
        );

        match self.authorizer.authorize(&authorization_request) {
            Ok(AuthorizationDecision::Allow) => MiddlewareResult::Continue,

            Ok(AuthorizationDecision::Deny) => {
                MiddlewareResult::Reject(MiddlewareRejection::new("authorization denied"))
            }

            Err(AuthorizationError::Failed) => {
                MiddlewareResult::Fail(MiddlewareError::new("authorization failed"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        contracts::{
            UniversalRequest,
            descriptor::{
                ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
            },
            envelope::MessageEnvelope,
            metadata::{ContractMetadata, Participants},
        },
        identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId},
        middleware::stages::{Middleware, MiddlewareError, MiddlewareRejection, MiddlewareResult},
        operation::{Operation, OperationContext},
        runtime::EngineContext,
        security::{PrincipalId, PrincipalIdentity, PrincipalType},
    };

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("security-middleware-operation").unwrap(),
            CorrelationId::new("security-middleware-correlation").unwrap(),
        )))
    }

    fn request_with_capability(capability: &str) -> UniversalRequest {
        let descriptor = ContractDescriptor::new(
            ContractId::new("security.middleware.contract").unwrap(),
            CapabilityId::new(capability).unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("security-middleware-sender").unwrap(),
                EngineId::new("security-middleware-receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("security-middleware-operation").unwrap(),
            CorrelationId::new("security-middleware-correlation").unwrap(),
        ));

        UniversalRequest::new(MessageEnvelope::new(
            MessageId::new("security-middleware-request").unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"opaque domain payload"),
        ))
    }

    fn user_identity() -> PrincipalIdentity {
        PrincipalIdentity::new(PrincipalType::User, PrincipalId::new("user-123").unwrap())
    }

    fn service_identity() -> PrincipalIdentity {
        PrincipalIdentity::new(
            PrincipalType::Service,
            PrincipalId::new("gateway-1").unwrap(),
        )
    }

    #[derive(Clone, Copy, Debug)]
    struct StaticCredentials {
        credentials: Option<&'static [u8]>,
    }

    impl CredentialExtractor for StaticCredentials {
        fn extract(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
        ) -> Option<Vec<u8>> {
            self.credentials.map(ToOwned::to_owned)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct UserAuthenticator;

    impl Authenticator for UserAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            match request.credentials() {
                b"user-token" => Ok(user_identity()),

                b"" => Err(AuthenticationError::MissingCredentials),

                _ => Err(AuthenticationError::InvalidCredentials),
            }
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct AllowAllAuthorizer;

    impl Authorizer for AllowAllAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Allow)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct DenyAllAuthorizer;

    impl Authorizer for DenyAllAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Deny)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FailingAuthenticator;

    impl Authenticator for FailingAuthenticator {
        fn authenticate(
            &self,
            _request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            Err(AuthenticationError::Failed)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FailingAuthorizer;

    impl Authorizer for FailingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Err(AuthorizationError::Failed)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct ServiceAuthenticator;

    impl Authenticator for ServiceAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            if request.credentials() == b"service-token" {
                Ok(service_identity())
            } else {
                Err(AuthenticationError::InvalidCredentials)
            }
        }
    }

    #[test]
    fn security_middleware_authenticates_and_continues() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(result, MiddlewareResult::Continue);

        assert_eq!(
            context
                .security()
                .expect("security context must be established after authentication")
                .principal(),
            &user_identity()
        );
    }

    #[test]
    fn authentication_establishes_security_context_on_same_engine_context() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let original_operation = context.operation().operation.id.clone();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(result, MiddlewareResult::Continue);

        assert_eq!(context.operation().operation.id, original_operation);

        assert_eq!(
            context
                .security()
                .expect("security context must be established after authentication")
                .principal(),
            &user_identity()
        );
    }

    #[test]
    fn calling_service_identity_is_preserved() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let calling_service = service_identity();

        let security_context = SecurityContext::new(user_identity(), Some(calling_service.clone()));

        let mut context = context().with_security(security_context);

        let mut request = request_with_capability("quran.read");

        /*
         * Authentication replaces the principal while preserving the already
         * trusted calling service from the current Core context.
         */
        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(result, MiddlewareResult::Continue);

        assert_eq!(
            context
                .security()
                .expect("security context must be established after authentication")
                .calling_service(),
            Some(&calling_service)
        );

        assert_eq!(
            context
                .security()
                .expect("security context must be established after authentication")
                .principal(),
            &user_identity()
        );
    }

    #[test]
    fn missing_credentials_are_rejected() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials { credentials: None },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Reject(MiddlewareRejection::new(
                "authentication credentials are required",
            ),)
        );
    }

    #[test]
    fn invalid_credentials_are_rejected() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"invalid-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Reject(MiddlewareRejection::new(
                "authentication credentials are invalid",
            ),)
        );
    }

    #[test]
    fn empty_credentials_are_rejected() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b""),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Reject(MiddlewareRejection::new(
                "authentication credentials are required",
            ),)
        );
    }

    #[test]
    fn authentication_failure_is_middleware_failure() {
        let middleware = SecurityMiddleware::new(
            FailingAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Fail(MiddlewareError::new("authentication failed",),)
        );
    }

    #[test]
    fn authorization_allow_continues_processing() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        assert_eq!(
            middleware.on_request(&mut context, &mut request,),
            MiddlewareResult::Continue
        );
    }

    #[test]
    fn authorization_deny_rejects_processing() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            DenyAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Reject(MiddlewareRejection::new("authorization denied",),)
        );
    }

    #[test]
    fn authorization_failure_is_middleware_failure() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            FailingAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Fail(MiddlewareError::new("authorization failed",),)
        );
    }

    #[test]
    fn authorization_receives_capability_identifier_without_payload_interpretation() {
        #[derive(Clone, Copy, Debug)]
        struct CapabilityCheckingAuthorizer;

        impl Authorizer for CapabilityCheckingAuthorizer {
            fn authorize(
                &self,
                request: &AuthorizationRequest<'_>,
            ) -> Result<AuthorizationDecision, AuthorizationError> {
                assert_eq!(request.capability().as_str(), "quran.read");

                Ok(AuthorizationDecision::Allow)
            }
        }

        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            CapabilityCheckingAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        assert_eq!(
            middleware.on_request(&mut context, &mut request,),
            MiddlewareResult::Continue
        );
    }

    #[test]
    fn service_principal_can_be_authenticated() {
        let middleware = SecurityMiddleware::new(
            ServiceAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"service-token"),
            },
        );

        let mut context = context();

        let mut request = request_with_capability("quran.read");

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(result, MiddlewareResult::Continue);

        assert_eq!(
            context
                .security()
                .expect("security context must be established after authentication")
                .principal()
                .principal_type(),
            PrincipalType::Service
        );
    }

    #[test]
    fn security_middleware_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<
            SecurityMiddleware<UserAuthenticator, AllowAllAuthorizer, StaticCredentials>,
        >();
    }

    #[test]
    fn security_middleware_debug_does_not_require_debug_components() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowAllAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let debug = format!("{middleware:?}");

        assert!(debug.contains("SecurityMiddleware"));
    }
}
