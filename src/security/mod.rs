//! Core security primitives for Nizaam.
//!
//! This module provides the provider-neutral security boundary used by the
//! runtime and middleware layers.
//!
//! Security responsibilities are intentionally separated:
//!
//! - [`identity`] represents trusted principals.
//! - [`context`] carries trusted request-scoped security identity.
//! - [`authentication`] establishes trusted identity from authentication input.
//! - [`authorization`] evaluates generic Core authorization decisions.
//! - [`middleware`] integrates authentication, security context establishment,
//!   and authorization with the Core middleware pipeline.
//!
//! Provider-specific credentials, authentication mechanisms, and
//! engine/domain-specific authorization remain outside this module.

pub mod authentication;
pub mod authorization;
pub mod context;
pub mod identity;
pub mod middleware;

pub use authentication::{AuthenticationError, AuthenticationRequest, Authenticator};

pub use authorization::{
    AuthorizationDecision, AuthorizationError, AuthorizationRequest, Authorizer,
};

pub use context::SecurityContext;

pub use identity::{PrincipalId, PrincipalIdentity, PrincipalType};

pub use middleware::{CredentialExtractor, SecurityMiddleware};

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
    };

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
        CapabilityId::new(id).unwrap()
    }

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("security-module-operation").unwrap(),
            CorrelationId::new("security-module-correlation").unwrap(),
        )))
    }

    fn request_with_capability(capability: &str) -> UniversalRequest {
        let descriptor = ContractDescriptor::new(
            ContractId::new("security.module.contract").unwrap(),
            capability_id(capability),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("security-module-sender").unwrap(),
                EngineId::new("security-module-receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("security-module-operation").unwrap(),
            CorrelationId::new("security-module-correlation").unwrap(),
        ));

        UniversalRequest::new(MessageEnvelope::new(
            MessageId::new("security-module-request").unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"opaque security test payload"),
        ))
    }

    #[derive(Debug)]
    struct UserAuthenticator;

    impl Authenticator for UserAuthenticator {
        fn authenticate(
            &self,
            request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            match request.credentials() {
                b"user-token" => Ok(user_identity("user-123")),

                b"service-token" => Ok(service_identity("service-123")),

                b"engine-token" => Ok(engine_identity("engine-123")),

                b"" => Err(AuthenticationError::MissingCredentials),

                _ => Err(AuthenticationError::InvalidCredentials),
            }
        }
    }

    #[derive(Debug)]
    struct AllowingAuthorizer;

    impl Authorizer for AllowingAuthorizer {
        fn authorize(
            &self,
            request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            if request.capability().as_str() == "quran.read" {
                Ok(AuthorizationDecision::Allow)
            } else {
                Ok(AuthorizationDecision::Deny)
            }
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
    struct FailingAuthenticator;

    impl Authenticator for FailingAuthenticator {
        fn authenticate(
            &self,
            _request: &AuthenticationRequest<'_>,
        ) -> Result<PrincipalIdentity, AuthenticationError> {
            Err(AuthenticationError::Failed)
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

    #[test]
    fn security_module_reexports_identity_types() {
        let principal = user_identity("user-123");

        assert_eq!(principal.principal_type(), PrincipalType::User);

        assert_eq!(principal.principal_id().as_str(), "user-123");
    }

    #[test]
    fn security_module_reexports_context_type() {
        let principal = user_identity("user-123");

        let calling_service = service_identity("gateway-1");

        let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        assert_eq!(context.principal(), &principal);

        assert_eq!(context.calling_service(), Some(&calling_service));
    }

    #[test]
    fn security_module_reexports_authentication_types() {
        let authenticator = UserAuthenticator;

        let request = AuthenticationRequest::new(b"user-token");

        let identity = authenticator.authenticate(&request).unwrap();

        assert_eq!(identity, user_identity("user-123"));
    }

    #[test]
    fn security_module_reexports_authorization_types() {
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
    fn security_module_reexports_middleware_types() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowingAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let debug = format!("{middleware:?}");

        assert!(debug.contains("SecurityMiddleware"));
    }

    #[test]
    fn authenticated_identity_flows_into_security_context() {
        let authenticator = UserAuthenticator;

        let request = AuthenticationRequest::new(b"user-token");

        let principal = authenticator.authenticate(&request).unwrap();

        let security_context = SecurityContext::new(principal.clone(), None);

        assert_eq!(security_context.principal(), &principal);

        assert!(security_context.calling_service().is_none());
    }

    #[test]
    fn authenticated_identity_and_calling_service_remain_distinct() {
        let authenticator = UserAuthenticator;

        let authentication_request = AuthenticationRequest::new(b"user-token");

        let principal = authenticator.authenticate(&authentication_request).unwrap();

        let calling_service = service_identity("gateway-1");

        let security_context =
            SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        assert_eq!(security_context.principal(), &principal);

        assert_eq!(security_context.calling_service(), Some(&calling_service));

        assert_ne!(
            security_context.principal(),
            security_context.calling_service().unwrap()
        );
    }

    #[test]
    fn security_context_information_can_be_used_for_authorization() {
        let principal = user_identity("user-123");

        let calling_service = service_identity("gateway-1");

        let security_context =
            SecurityContext::new(principal.clone(), Some(calling_service.clone()));

        let capability = capability_id("quran.read");

        let authorization_request = AuthorizationRequest::new(
            security_context.principal(),
            security_context.calling_service(),
            &capability,
        );

        let authorizer = AllowingAuthorizer;

        assert_eq!(
            authorizer.authorize(&authorization_request,),
            Ok(AuthorizationDecision::Allow)
        );
    }

    #[test]
    fn authorization_can_distinguish_capabilities() {
        let principal = user_identity("user-123");

        let authorizer = AllowingAuthorizer;

        let read_capability = capability_id("quran.read");

        let write_capability = capability_id("quran.write");

        let read_request = AuthorizationRequest::new(&principal, None, &read_capability);

        let write_request = AuthorizationRequest::new(&principal, None, &write_capability);

        assert_eq!(
            authorizer.authorize(&read_request,),
            Ok(AuthorizationDecision::Allow)
        );

        assert_eq!(
            authorizer.authorize(&write_request,),
            Ok(AuthorizationDecision::Deny)
        );
    }

    #[test]
    fn service_principal_can_be_authenticated_and_authorized() {
        let authenticator = UserAuthenticator;

        let authentication_request = AuthenticationRequest::new(b"service-token");

        let principal = authenticator.authenticate(&authentication_request).unwrap();

        assert_eq!(principal, service_identity("service-123"));

        let capability = capability_id("quran.read");

        let authorization_request = AuthorizationRequest::new(&principal, None, &capability);

        let authorizer = AllowingAuthorizer;

        assert_eq!(
            authorizer.authorize(&authorization_request,),
            Ok(AuthorizationDecision::Allow)
        );
    }

    #[test]
    fn engine_principal_can_be_used_in_security_context() {
        let principal = engine_identity("quran-engine");

        let context = SecurityContext::new(principal.clone(), None);

        assert_eq!(context.principal(), &principal);

        assert_eq!(context.principal().principal_type(), PrincipalType::Engine);
    }

    #[test]
    fn authentication_failure_does_not_create_security_context() {
        let authenticator = FailingAuthenticator;

        let request = AuthenticationRequest::new(b"credential");

        let result = authenticator.authenticate(&request);

        assert_eq!(result, Err(AuthenticationError::Failed));
    }

    #[test]
    fn authorization_failure_is_distinct_from_denial() {
        let principal = user_identity("user-123");

        let capability = capability_id("quran.read");

        let request = AuthorizationRequest::new(&principal, None, &capability);

        let denying_authorizer = DenyingAuthorizer;

        let failing_authorizer = FailingAuthorizer;

        assert_eq!(
            denying_authorizer.authorize(&request),
            Ok(AuthorizationDecision::Deny)
        );

        assert_eq!(
            failing_authorizer.authorize(&request),
            Err(AuthorizationError::Failed)
        );
    }

    #[test]
    fn security_middleware_authenticates_and_authorizes() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowingAuthorizer,
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
            &user_identity("user-123")
        );
    }

    #[test]
    fn security_middleware_rejects_invalid_credentials() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowingAuthorizer,
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
    fn security_middleware_rejects_denied_authorization() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            DenyingAuthorizer,
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
    fn security_middleware_produces_failure_for_authentication_subsystem_error() {
        let middleware = SecurityMiddleware::new(
            FailingAuthenticator,
            AllowingAuthorizer,
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
    fn security_middleware_produces_failure_for_authorization_subsystem_error() {
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
    fn security_middleware_uses_capability_from_core_request_descriptor() {
        #[derive(Debug)]
        struct CapabilityAuthorizer;

        impl Authorizer for CapabilityAuthorizer {
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
            CapabilityAuthorizer,
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
    fn security_middleware_preserves_existing_calling_service_identity() {
        let middleware = SecurityMiddleware::new(
            UserAuthenticator,
            AllowingAuthorizer,
            StaticCredentials {
                credentials: Some(b"user-token"),
            },
        );

        let calling_service = service_identity("gateway-1");

        let initial_security_context = SecurityContext::new(
            user_identity("original-user"),
            Some(calling_service.clone()),
        );

        let mut context = context().with_security(initial_security_context);

        let mut request = request_with_capability("quran.read");

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
            &user_identity("user-123")
        );
    }

    #[test]
    fn security_middleware_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<
            SecurityMiddleware<UserAuthenticator, AllowingAuthorizer, StaticCredentials>,
        >();
    }
}
