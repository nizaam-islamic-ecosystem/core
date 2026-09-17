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
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, MessageId, NodeId, OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::{Status, Version};
use nizaam_core::retry::{Attempt, AttemptLifecycleState};
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

fn operation_context_for_attempt(
    operation_id: &str,
    node_id: &str,
    attempt_id: &str,
) -> OperationContext {
    operation_context(operation_id).for_attempt(
        NodeId::new(node_id).unwrap(),
        AttemptId::new(attempt_id).unwrap(),
    )
}

fn context_for_attempt(operation_id: &str, node_id: &str, attempt_id: &str) -> EngineContext {
    EngineContext::new(operation_context_for_attempt(
        operation_id,
        node_id,
        attempt_id,
    ))
}

fn request_with_context(
    message_id: &str,
    capability: &str,
    payload: &[u8],
    operation_context: OperationContext,
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
        operation_context,
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

fn request(
    message_id: &str,
    operation_id: &str,
    capability: &str,
    payload: &[u8],
) -> UniversalRequest {
    request_with_context(
        message_id,
        capability,
        payload,
        operation_context(operation_id),
    )
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
    observed: Arc<Mutex<Vec<CapabilityId>>>,
}

impl Authorizer for InspectingAuthorizer {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        assert_eq!(request.capability(), &self.expected_capability);
        self.observed
            .lock()
            .unwrap()
            .push(request.capability().clone());

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

    let mut context = context_for_attempt(
        "security-rejection",
        "conformance-node-2",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "security-rejection-message",
        "conformance.test",
        b"payload",
        operation_context_for_attempt(
            "security-rejection",
            "conformance-node-2",
            "conformance-attempt-1",
        ),
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

    let mut context = context_for_attempt(
        "authorization-order",
        "conformance-node-3",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "authorization-order-message",
        "conformance.capability",
        b"opaque payload",
        operation_context_for_attempt(
            "authorization-order",
            "conformance-node-3",
            "conformance-attempt-1",
        ),
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
    let observed_capability = Arc::new(Mutex::new(Vec::new()));

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

    let mut context = context_for_attempt(
        "payload-opaque",
        "conformance-node-4",
        "conformance-attempt-1",
    );
    let mut request = request_with_context(
        "payload-opaque-message",
        "domain.operation",
        &original_payload,
        operation_context_for_attempt(
            "payload-opaque",
            "conformance-node-4",
            "conformance-attempt-1",
        ),
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
        vec![expected_capability],
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

    let mut context = context_for_attempt(
        "identity-preservation",
        "conformance-node-5",
        "conformance-attempt-1",
    )
    .with_security(SecurityContext::new(
        user_principal("original-principal"),
        Some(calling_service.clone()),
    ));
    let mut request = request_with_context(
        "identity-preservation-message",
        "conformance.identity",
        b"payload",
        operation_context_for_attempt(
            "identity-preservation",
            "conformance-node-5",
            "conformance-attempt-1",
        ),
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
            let attempt_id = format!("{operation_id}-attempt-1");
            let mut context =
                context_for_attempt(operation_id, "conformance-concurrent-node", &attempt_id);
            let mut request = request_with_context(
                message_id,
                "conformance.concurrent",
                b"payload",
                operation_context_for_attempt(
                    operation_id,
                    "conformance-concurrent-node",
                    &attempt_id,
                ),
            );

            barrier.wait();

            let result: Result<UniversalResponse, RequestPipelineError<()>> =
                pipeline.run_request(&mut context, &mut request, move |context, request| {
                    let security = context
                        .security()
                        .expect("authentication must establish security context");

                    assert_eq!(security.principal(), &expected_principal);
                    assert_eq!(
                        context.operation().operation.id.as_str(),
                        request.envelope.operation_context.operation.id.as_str(),
                    );
                    assert_eq!(
                        context.operation().attempt_id,
                        request.envelope.operation_context.attempt_id,
                    );
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

#[test]
fn retry_attempt_identity_survives_the_security_pipeline() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_downstream = Arc::clone(&observed);
    let principal = user_principal("retry-security-user");
    let calling_service = service_principal("retry-security-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"retry-credentials".to_vec()),
        },
    ));

    for (message_id, attempt_id) in [
        ("retry-security-message-1", "retry-security-attempt-1"),
        ("retry-security-message-2", "retry-security-attempt-2"),
    ] {
        let mut context = context_for_attempt(
            "retry-security-operation",
            "retry-security-node",
            attempt_id,
        )
        .with_security(SecurityContext::new(
            principal.clone(),
            Some(calling_service.clone()),
        ));

        let mut request = request_with_context(
            message_id,
            "conformance.retry-security",
            b"retry-payload",
            operation_context_for_attempt(
                "retry-security-operation",
                "retry-security-node",
                attempt_id,
            ),
        );

        let observed_downstream = Arc::clone(&observed_downstream);
        let principal_expected = principal.clone();
        let calling_service_expected = calling_service.clone();

        let result: Result<UniversalResponse, RequestPipelineError<()>> =
            pipeline.run_request(&mut context, &mut request, move |context, request| {
                let security = context
                    .security()
                    .expect("security context must survive into downstream execution");

                assert_eq!(security.principal(), &principal_expected);
                assert_eq!(security.calling_service(), Some(&calling_service_expected),);
                assert_eq!(
                    context.operation().operation.id,
                    request.envelope.operation_context.operation.id,
                );
                assert_eq!(
                    context.operation().attempt_id,
                    request.envelope.operation_context.attempt_id,
                );

                observed_downstream
                    .lock()
                    .unwrap()
                    .push(context.operation().attempt_id.clone());

                Ok(response(request, b"response"))
            });

        assert!(result.is_ok());
    }

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_ne!(observed[0], observed[1]);
    assert_eq!(
        observed[0].as_ref().unwrap().as_str(),
        "retry-security-attempt-1",
    );
    assert_eq!(
        observed[1].as_ref().unwrap().as_str(),
        "retry-security-attempt-2",
    );
}

#[test]
fn failed_attempt_can_be_followed_by_new_attempt_with_same_security_context() {
    let principal = user_principal("retry-preservation-user");
    let calling_service = service_principal("retry-preservation-service");

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(principal.clone()),
        },
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"retry-preservation-credentials".to_vec()),
        },
    ));

    let operation_id = OperationId::new("retry-preservation-operation").unwrap();

    let first_attempt = Attempt::new(
        operation_id.clone(),
        AttemptId::new("retry-preservation-attempt-1").unwrap(),
        1,
    )
    .unwrap();
    first_attempt.start().unwrap();

    let mut first_context = EngineContext::new(operation_context_for_attempt(
        "retry-preservation-operation",
        "retry-preservation-node",
        "retry-preservation-attempt-1",
    ))
    .with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));

    let mut first_request = request_with_context(
        "retry-preservation-message-1",
        "conformance.retry-preservation",
        b"payload",
        operation_context_for_attempt(
            "retry-preservation-operation",
            "retry-preservation-node",
            "retry-preservation-attempt-1",
        ),
    );

    let first_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut first_context,
        &mut first_request,
        |context, request| {
            let security = context
                .security()
                .expect("first attempt must have security context");
            assert_eq!(security.principal(), &principal);
            assert_eq!(security.calling_service(), Some(&calling_service));
            Ok(response(request, b"first response"))
        },
    );

    assert!(first_result.is_ok());
    first_attempt.fail().unwrap();
    assert_eq!(first_attempt.state(), AttemptLifecycleState::Failed);

    let second_attempt = Attempt::new(
        operation_id.clone(),
        AttemptId::new("retry-preservation-attempt-2").unwrap(),
        2,
    )
    .unwrap();
    second_attempt.start().unwrap();

    let mut second_context = EngineContext::new(operation_context_for_attempt(
        "retry-preservation-operation",
        "retry-preservation-node",
        "retry-preservation-attempt-2",
    ))
    .with_security(SecurityContext::new(
        principal.clone(),
        Some(calling_service.clone()),
    ));

    let mut second_request = request_with_context(
        "retry-preservation-message-2",
        "conformance.retry-preservation",
        b"payload",
        operation_context_for_attempt(
            "retry-preservation-operation",
            "retry-preservation-node",
            "retry-preservation-attempt-2",
        ),
    );

    let second_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut second_context,
        &mut second_request,
        |context, request| {
            let security = context
                .security()
                .expect("second attempt must have security context");
            assert_eq!(security.principal(), &principal);
            assert_eq!(security.calling_service(), Some(&calling_service));
            Ok(response(request, b"second response"))
        },
    );

    assert!(second_result.is_ok());
    second_attempt.succeed().unwrap();

    assert_eq!(first_attempt.operation_id(), second_attempt.operation_id());
    assert_ne!(first_attempt.attempt_id(), second_attempt.attempt_id());
    assert_eq!(first_attempt.state(), AttemptLifecycleState::Failed);
    assert_eq!(second_attempt.state(), AttemptLifecycleState::Succeeded);
    assert_eq!(first_context.security(), second_context.security(),);
}

#[test]
fn security_rejection_does_not_create_an_automatic_retry_attempt() {
    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        RejectingAuthenticator,
        AllowingAuthorizer,
        StaticCredentialExtractor {
            credentials: Some(b"rejected-credentials".to_vec()),
        },
    ));

    let attempt = Attempt::new(
        OperationId::new("rejection-operation").unwrap(),
        AttemptId::new("rejection-attempt-1").unwrap(),
        1,
    )
    .unwrap();

    let attempt_id = attempt.attempt_id().clone();
    let mut context = context_for_attempt(
        "rejection-operation",
        "rejection-node",
        "rejection-attempt-1",
    );
    let mut request = request_with_context(
        "rejection-message",
        "conformance.rejection",
        b"payload",
        operation_context_for_attempt(
            "rejection-operation",
            "rejection-node",
            "rejection-attempt-1",
        ),
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
    assert_eq!(attempt.state(), AttemptLifecycleState::Created);
    assert_eq!(context.operation().attempt_id, Some(attempt_id));
}

#[test]
fn authorization_keeps_capability_identity_stable_across_attempts() {
    let expected_capability = CapabilityId::new("conformance.retry-capability").unwrap();
    let observed_capabilities = Arc::new(Mutex::new(Vec::new()));

    let pipeline = ExecutionPipeline::new().with_middleware(SecurityMiddleware::new(
        StaticAuthenticator {
            result: Ok(user_principal("retry-capability-user")),
        },
        InspectingAuthorizer {
            expected_capability: expected_capability.clone(),
            observed: Arc::clone(&observed_capabilities),
        },
        StaticCredentialExtractor {
            credentials: Some(b"retry-capability-credentials".to_vec()),
        },
    ));

    let mut first_context = context_for_attempt(
        "retry-capability-operation",
        "retry-capability-node",
        "retry-capability-attempt-1",
    );
    let mut first_request = request_with_context(
        "retry-capability-message-1",
        "conformance.retry-capability",
        b"first",
        operation_context_for_attempt(
            "retry-capability-operation",
            "retry-capability-node",
            "retry-capability-attempt-1",
        ),
    );

    let first_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut first_context,
        &mut first_request,
        |_context, request| Ok(response(request, b"response-1")),
    );
    assert!(first_result.is_ok());

    let mut second_context = context_for_attempt(
        "retry-capability-operation",
        "retry-capability-node",
        "retry-capability-attempt-2",
    );
    let mut second_request = request_with_context(
        "retry-capability-message-2",
        "conformance.retry-capability",
        b"second",
        operation_context_for_attempt(
            "retry-capability-operation",
            "retry-capability-node",
            "retry-capability-attempt-2",
        ),
    );

    let second_result: Result<UniversalResponse, RequestPipelineError<()>> = pipeline.run_request(
        &mut second_context,
        &mut second_request,
        |_context, request| Ok(response(request, b"response-2")),
    );
    assert!(second_result.is_ok());

    assert_eq!(
        first_context.operation().operation.id,
        second_context.operation().operation.id,
    );
    assert_ne!(
        first_context.operation().attempt_id,
        second_context.operation().attempt_id,
    );

    assert_eq!(
        *observed_capabilities.lock().unwrap(),
        vec![expected_capability.clone(), expected_capability],
    );
}
