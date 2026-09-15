//! Integration tests for Phase 9 architectural conformance.
//!
//! These tests verify the security and middleware boundaries as an external
//! Core consumer would observe them. Individual primitive behavior belongs to
//! unit tests, public security API composition belongs to `tests/security.rs`,
//! and runtime-specific execution coverage belongs to `tests/runtime.rs`.

use std::sync::{Arc, Barrier, Mutex};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::{Status, Version};
use nizaam_core::runtime::pipeline::RequestPipelineError;
use nizaam_core::runtime::{EngineContext, ExecutionPipeline};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};

fn principal(principal_type: PrincipalType, id: &str) -> PrincipalIdentity {
    PrincipalIdentity::new(principal_type, PrincipalId::new(id).unwrap())
}

fn user_principal(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::User, id)
}

fn service_principal(id: &str) -> PrincipalIdentity {
    principal(PrincipalType::Service, id)
}

fn operation_context(operation_id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new(operation_id).unwrap(),
        CorrelationId::new(format!("{operation_id}-correlation")).unwrap(),
    ))
}

fn context(operation_id: &str) -> EngineContext {
    EngineContext::new(operation_context(operation_id))
}

fn request(
    message_id: &str,
    operation_id: &str,
    capability: &str,
    payload: &[u8],
) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

    let descriptor = ContractDescriptor::new(
        ContractId::new("conformance.contract").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );

    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("conformance-sender").unwrap(),
            EngineId::new("conformance-receiver").unwrap(),
        ),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(message_id).unwrap(),
        operation_context(operation_id),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn response(request: &UniversalRequest, payload: &[u8]) -> UniversalResponse {
    let mut envelope = request.envelope.clone();
    envelope.metadata.descriptor.interaction = Interaction::Response;
    envelope.payload = EncodedPayload::new(
        envelope.metadata.descriptor.payload.clone(),
        payload.to_vec(),
    );

    UniversalResponse::new(envelope, Status::Success)
}

#[derive(Clone)]
struct StaticAuthenticator {
    result: Result<PrincipalIdentity, AuthenticationError>,
}

impl Authenticator for StaticAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.result.clone()
    }
}

struct AllowingAuthorizer;

impl Authorizer for AllowingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}

struct StaticCredentialExtractor {
    credentials: Option<Vec<u8>>,
}

impl CredentialExtractor for StaticCredentialExtractor {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        self.credentials.clone()
    }
}

#[derive(Debug)]
struct RecordingMiddleware {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Middleware for RecordingMiddleware {
    fn on_request(
        &self,
        _context: &mut EngineContext,
        _request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        self.events.lock().unwrap().push("middleware");
        MiddlewareResult::Continue
    }
}

struct RecordingAuthenticator {
    events: Arc<Mutex<Vec<&'static str>>>,
    principal: PrincipalIdentity,
}

impl Authenticator for RecordingAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        self.events.lock().unwrap().push("authenticate");
        Ok(self.principal.clone())
    }
}

struct RecordingAuthorizer {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Authorizer for RecordingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        self.events.lock().unwrap().push("authorize");
        Ok(AuthorizationDecision::Allow)
    }
}

struct RejectingAuthenticator;

impl Authenticator for RejectingAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Err(AuthenticationError::InvalidCredentials)
    }
}

struct CredentialByOperation;

impl CredentialExtractor for CredentialByOperation {
    fn extract(&self, _context: &EngineContext, request: &UniversalRequest) -> Option<Vec<u8>> {
        Some(
            request
                .envelope
                .operation_context
                .operation
                .id
                .to_string()
                .into_bytes(),
        )
    }
}

struct PrincipalByCredential;

impl Authenticator for PrincipalByCredential {
    fn authenticate(
        &self,
        request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        match request.credentials() {
            b"concurrent-a" => Ok(user_principal("concurrent-user-a")),
            b"concurrent-b" => Ok(user_principal("concurrent-user-b")),
            _ => Err(AuthenticationError::InvalidCredentials),
        }
    }
}

struct InspectingAuthorizer {
    expected_capability: CapabilityId,
    observed: Arc<Mutex<Option<CapabilityId>>>,
}

impl Authorizer for InspectingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.capability(), &self.expected_capability);
        *self.observed.lock().unwrap() = Some(request.capability().clone());

        Ok(AuthorizationDecision::Allow)
    }
}

#[test]
fn request_must_pass_through_middleware_before_downstream_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let middleware_events = Arc::clone(&events);
    let downstream_events = Arc::clone(&events);

    let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
        events: middleware_events,
    });

    let mut context = context("middleware-order");
    let mut request = request(
        "middleware-order-message",
        "middleware-order",
        "conformance.test",
        b"payload",
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            downstream_events.lock().unwrap().push("downstream");
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["middleware", "downstream"],
    );
}

#[test]
fn security_rejection_must_stop_downstream_execution() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RejectingAuthenticator,
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"invalid".to_vec()),
        },
    ));

    let mut context = context("security-rejection");
    let mut request = request(
        "security-rejection-message",
        "security-rejection",
        "conformance.test",
        b"payload",
    );
    let downstream_called = Arc::new(Mutex::new(false));
    let downstream_called_by_handler = Arc::clone(&downstream_called);

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            *downstream_called_by_handler.lock().unwrap() = true;
            Ok(response(request, b"must-not-run"))
        });

    assert!(matches!(
        result,
        Err(RequestPipelineError::Middleware(
            nizaam_core::middleware::chain::MiddlewareChainError::Rejected(_)
        ))
    ));
    assert!(!*downstream_called.lock().unwrap());
    assert!(context.security().is_none());
}

#[test]
fn generic_authorization_must_precede_capability_execution() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let principal = user_principal("conformance-user");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RecordingAuthenticator {
            events: Arc::clone(&events),
            principal,
        },
        RecordingAuthorizer {
            events: Arc::clone(&events),
        },
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let mut context = context("authorization-order");
    let mut request = request(
        "authorization-order-message",
        "authorization-order",
        "conformance.capability",
        b"opaque payload",
    );

    let downstream_events = Arc::clone(&events);
    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |_context, request| {
            downstream_events.lock().unwrap().push("dispatch");
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        events.lock().unwrap().as_slice(),
        ["authenticate", "authorize", "dispatch"],
    );
}

#[test]
fn core_authorization_must_not_require_domain_payload_interpretation() {
    let expected_capability = CapabilityId::new("domain.operation").unwrap();
    let observed_capability = Arc::new(Mutex::new(None));

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("payload-opaque-user")),
        },
        InspectingAuthorizer {
            expected_capability: expected_capability.clone(),
            observed: Arc::clone(&observed_capability),
        },
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let original_payload = vec![0, 255, 17, 42, 128, 3, 99];

    let mut context = context("payload-opaque");
    let mut request = request(
        "payload-opaque-message",
        "payload-opaque",
        "domain.operation",
        &original_payload,
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, |_context, request| {
            assert_eq!(
                request.envelope.payload.bytes(),
                original_payload.as_slice()
            );
            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());
    assert_eq!(
        *observed_capability.lock().unwrap(),
        Some(expected_capability),
    );
    assert_eq!(
        request.envelope.payload.bytes(),
        original_payload.as_slice(),
    );
}

#[test]
fn trusted_security_context_must_preserve_principal_and_calling_service() {
    let authenticated_principal = user_principal("authenticated-user");
    let calling_service = service_principal("gateway-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(authenticated_principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"credentials".to_vec()),
        },
    ));

    let expected_principal = authenticated_principal.clone();
    let expected_calling_service = calling_service.clone();

    let mut context = context("identity-preservation").with_security(SecurityContext::new(
        user_principal("original-principal"),
        Some(calling_service.clone()),
    ));
    let mut request = request(
        "identity-preservation-message",
        "identity-preservation",
        "conformance.identity",
        b"payload",
    );

    let result: Result<UniversalResponse, RequestPipelineError<()>> =
        pipeline.run_request(&mut context, &mut request, move |context, request| {
            let security = context
                .security()
                .expect("successful authentication must establish security context");

            assert_eq!(security.principal(), &expected_principal);
            assert_eq!(security.calling_service(), Some(&expected_calling_service),);

            Ok(response(request, b"response"))
        });

    assert!(result.is_ok());

    let security = context
        .security()
        .expect("successful authentication must establish security context");
    assert_eq!(security.principal(), &authenticated_principal);
    assert_eq!(security.calling_service(), Some(&calling_service));
}

#[test]
fn concurrent_requests_must_isolate_security_context() {
    let pipeline = Arc::new(
        ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
            PrincipalByCredential,
            AllowingAuthorizer,
            CredentialByOperation,
        )),
    );

    let barrier = Arc::new(Barrier::new(3));
    let observed = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();

    for (message_id, operation_id, expected_principal) in [
        (
            "concurrent-message-a",
            "concurrent-a",
            user_principal("concurrent-user-a"),
        ),
        (
            "concurrent-message-b",
            "concurrent-b",
            user_principal("concurrent-user-b"),
        ),
    ] {
        let pipeline = Arc::clone(&pipeline);
        let barrier = Arc::clone(&barrier);
        let observed = Arc::clone(&observed);

        handles.push(std::thread::spawn(move || {
            let mut context = context(operation_id);
            let mut request = request(
                message_id,
                operation_id,
                "conformance.concurrent",
                b"payload",
            );

            barrier.wait();

            let result: Result<UniversalResponse, RequestPipelineError<()>> =
                pipeline.run_request(&mut context, &mut request, move |context, request| {
                    let security = context
                        .security()
                        .expect("authentication must establish security context");

                    assert_eq!(security.principal(), &expected_principal);
                    observed.lock().unwrap().push(expected_principal.clone());

                    Ok(response(request, b"response"))
                });

            assert!(result.is_ok());
        }));
    }

    barrier.wait();

    for handle in handles {
        handle.join().unwrap();
    }

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert!(observed.contains(&user_principal("concurrent-user-a")));
    assert!(observed.contains(&user_principal("concurrent-user-b")));
}
