//! Integration tests for the public Phase 9 security boundary.
//!
//! These tests exercise the provider-neutral security API as an external Core
//! consumer would use it. Runtime-specific execution coverage belongs to
//! `tests/runtime.rs`, while architectural conformance belongs to
//! `tests/conformance.rs`.

use std::sync::{Arc, Mutex};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::Version;
use nizaam_core::runtime::EngineContext;
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};

fn user_principal(id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(PrincipalType::User, PrincipalId::new(id).unwrap())
}

fn service_principal(id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new(id).unwrap())
}

fn engine_principal(id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(PrincipalType::Engine, PrincipalId::new(id).unwrap())
}

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new("security-integration-operation").unwrap(),
        CorrelationId::new("security-integration-correlation").unwrap(),
    ))
}

fn context() -> EngineContext {
    EngineContext::new(operation_context())
}

fn request_with_capability(capability: &str, payload: &[u8]) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("security-integration.contract").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );
    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("security-integration-sender").unwrap(),
            EngineId::new("security-integration-receiver").unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("security-integration-message").unwrap(),
        operation_context(),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

#[derive(Clone)]
struct TestAuthenticator {
    result: Result<PrincipalIdentity, AuthenticationError>,
}

impl Authenticator for TestAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.result.clone()
    }
}

struct RecordingAuthorizer {
    expected_principal: PrincipalIdentity,
    expected_calling_service: Option<PrincipalIdentity>,
    expected_capability: CapabilityId,
    observed: Arc<Mutex<bool>>,
    result: Result<AuthorizationDecision, AuthorizationError>,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.principal(), &self.expected_principal);
        assert_eq!(
            request.calling_service(),
            self.expected_calling_service.as_ref()
        );
        assert_eq!(request.capability(), &self.expected_capability);
        *self.observed.lock().unwrap() = true;
        self.result
    }
}

#[derive(Clone)]
struct TestCredentialExtractor {
    credentials: Option<Vec<u8>>,
}

impl CredentialExtractor for TestCredentialExtractor {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.credentials.clone()
    }
}

#[test]
fn principal_identity_supports_user_service_and_engine_principals() {
    let user = user_principal("user-1");
    let service = service_principal("service-1");
    let engine = engine_principal("engine-1");

    assert_eq!(user.principal_type(), PrincipalType::User);
    assert_eq!(service.principal_type(), PrincipalType::Service);
    assert_eq!(engine.principal_type(), PrincipalType::Engine);
    assert_ne!(user, service);
    assert_ne!(service, engine);
    assert_ne!(user, engine);
}

#[test]
fn security_context_preserves_principal_and_calling_service() {
    let principal = user_principal("user-1");
    let calling_service = service_principal("gateway-1");
    let context = SecurityContext::new(principal.clone(), Some(calling_service.clone()));

    assert_eq!(context.principal(), &principal);
    assert_eq!(context.calling_service(), Some(&calling_service));
}

#[test]
fn authenticator_establishes_trusted_principal_identity() {
    let expected = user_principal("authenticated-user");
    let authenticator = TestAuthenticator {
        result: Ok(expected.clone()),
    };
    let request = AuthenticationRequest::new(b"opaque credentials");

    assert_eq!(authenticator.authenticate(&request), Ok(expected));
}

#[test]
fn authorizer_receives_principal_calling_service_and_capability() {
    let principal = user_principal("authorized-user");
    let calling_service = service_principal("gateway-1");
    let capability = CapabilityId::new("quran.read").unwrap();
    let observed = Arc::new(Mutex::new(false));

    let authorizer = RecordingAuthorizer {
        expected_principal: principal.clone(),
        expected_calling_service: Some(calling_service.clone()),
        expected_capability: capability.clone(),
        observed: Arc::clone(&observed),
        result: Ok(AuthorizationDecision::Allow),
    };

    let request = AuthorizationRequest::new(&principal, Some(&calling_service), &capability);

    assert_eq!(
        authorizer.authorize(&request),
        Ok(AuthorizationDecision::Allow)
    );
    assert!(*observed.lock().unwrap());
}

#[test]
fn authorization_denial_and_failure_are_distinct() {
    let principal = user_principal("user-1");
    let capability = CapabilityId::new("quran.read").unwrap();
    let request = AuthorizationRequest::new(&principal, None, &capability);

    struct DenyingAuthorizer;
    impl Authorizer for DenyingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Ok(AuthorizationDecision::Deny)
        }
    }

    struct FailingAuthorizer;
    impl Authorizer for FailingAuthorizer {
        fn authorize(
            &self,
            _request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            Err(AuthorizationError::Failed)
        }
    }

    assert_eq!(
        DenyingAuthorizer.authorize(&request),
        Ok(AuthorizationDecision::Deny)
    );
    assert_eq!(
        FailingAuthorizer.authorize(&request),
        Err(AuthorizationError::Failed)
    );
}

#[test]
fn security_middleware_composes_authentication_and_authorization() {
    let principal = user_principal("middleware-user");
    let calling_service = service_principal("gateway-1");
    let capability = CapabilityId::new("quran.read").unwrap();
    let observed = Arc::new(Mutex::new(false));

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(principal.clone()),
        },
        RecordingAuthorizer {
            expected_principal: principal.clone(),
            expected_calling_service: Some(calling_service.clone()),
            expected_capability: capability,
            observed: Arc::clone(&observed),
            result: Ok(AuthorizationDecision::Allow),
        },
        TestCredentialExtractor {
            credentials: Some(b"opaque credentials".to_vec()),
        },
    );

    let mut context = context().with_security(SecurityContext::new(
        service_principal("original-principal"),
        Some(calling_service.clone()),
    ));
    let mut request = request_with_capability("quran.read", b"opaque payload");

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );
    assert!(*observed.lock().unwrap());

    let security = context
        .security()
        .expect("successful authentication must establish security context");
    assert_eq!(security.principal(), &principal);
    assert_eq!(security.calling_service(), Some(&calling_service));
}

#[test]
fn security_middleware_maps_security_outcomes_to_middleware_results() {
    let request = request_with_capability("quran.read", b"opaque payload");

    let denying = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(user_principal("user-1")),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("user-1"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("quran.read").unwrap(),
            observed: Arc::new(Mutex::new(false)),
            result: Ok(AuthorizationDecision::Deny),
        },
        TestCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    );

    let mut deny_context = context();
    let mut deny_request = request.clone();
    assert!(matches!(
        denying.on_request(&mut deny_context, &mut deny_request),
        MiddlewareResult::Reject(_)
    ));

    let failing = SecurityMiddleware::new(
        TestAuthenticator {
            result: Err(AuthenticationError::Failed),
        },
        RecordingAuthorizer {
            expected_principal: user_principal("unused"),
            expected_calling_service: None,
            expected_capability: CapabilityId::new("quran.read").unwrap(),
            observed: Arc::new(Mutex::new(false)),
            result: Ok(AuthorizationDecision::Allow),
        },
        TestCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    );

    let mut fail_context = context();
    let mut fail_request = request;
    assert!(matches!(
        failing.on_request(&mut fail_context, &mut fail_request),
        MiddlewareResult::Fail(_)
    ));
}

#[test]
fn authorization_uses_capability_identity_without_interpreting_payload() {
    let expected_capability = CapabilityId::new("quran.read").unwrap();
    let principal = user_principal("payload-opaque-user");
    let observed_capability = Arc::new(Mutex::new(None));
    let observed_capability_by_authorizer = Arc::clone(&observed_capability);

    struct PayloadOpaqueAuthorizer {
        expected_capability: CapabilityId,
        observed: Arc<Mutex<Option<CapabilityId>>>,
    }

    impl Authorizer for PayloadOpaqueAuthorizer {
        fn authorize(
            &self,
            request: &AuthorizationRequest<'_>,
        ) -> Result<AuthorizationDecision, AuthorizationError> {
            assert_eq!(request.capability(), &self.expected_capability);
            *self.observed.lock().unwrap() = Some(request.capability().clone());
            Ok(AuthorizationDecision::Allow)
        }
    }

    let middleware = SecurityMiddleware::new(
        TestAuthenticator {
            result: Ok(principal),
        },
        PayloadOpaqueAuthorizer {
            expected_capability: expected_capability.clone(),
            observed: observed_capability_by_authorizer,
        },
        TestCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    );

    let mut context = context();
    let mut request = request_with_capability(
        "quran.read",
        b"this payload is deliberately opaque to Core authorization",
    );

    assert_eq!(
        middleware.on_request(&mut context, &mut request),
        MiddlewareResult::Continue
    );
    assert_eq!(
        *observed_capability.lock().unwrap(),
        Some(expected_capability)
    );
}
