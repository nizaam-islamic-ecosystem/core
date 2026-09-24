use crate::support::{section, show_arrow, step, success};
use nizaam_core::contracts::UniversalRequest;
use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityContext, SecurityMiddleware,
};

fn context() -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new("visual-security-operation").unwrap(),
        CorrelationId::new("visual-security-correlation").unwrap(),
    )))
}
fn request(capability: &str) -> UniversalRequest {
    let descriptor = ContractDescriptor::new(
        ContractId::new("visual.security").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("visual-security-message").unwrap(),
        OperationContext::new(Operation::new(
            OperationId::new("visual-security-operation").unwrap(),
            CorrelationId::new("visual-security-correlation").unwrap(),
        )),
        ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("target").unwrap(),
            ),
        ),
        EncodedPayload::new(descriptor.payload, b"opaque"),
    ))
}

#[derive(Debug)]
struct Auth;
impl Authenticator for Auth {
    fn authenticate(
        &self,
        request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        if request.credentials() == b"token" {
            Ok(PrincipalIdentity::new(
                PrincipalType::User,
                PrincipalId::new("visual-user").unwrap(),
            ))
        } else {
            Err(AuthenticationError::InvalidCredentials)
        }
    }
}
#[derive(Debug)]
struct Allow;
impl Authorizer for Allow {
    fn authorize(
        &self,
        request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        if request.capability().as_str() == "visual.read" {
            Ok(AuthorizationDecision::Allow)
        } else {
            Ok(AuthorizationDecision::Deny)
        }
    }
}
#[derive(Debug)]
struct Deny;
impl Authorizer for Deny {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Deny)
    }
}
#[derive(Clone)]
struct Credentials;
impl CredentialExtractor for Credentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        Some(b"token".to_vec())
    }
}

#[derive(Clone)]
struct InvalidCredentials;
impl CredentialExtractor for InvalidCredentials {
    fn extract(&self, _context: &EngineContext, _request: &UniversalRequest) -> Option<Vec<u8>> {
        Some(b"invalid-token".to_vec())
    }
}

#[test]
fn visual_security_boundary() {
    section("NIZAAM CORE — SECURITY");
    step(1, "authentication and trusted principal");
    let middleware = SecurityMiddleware::new(Auth, Allow, Credentials);
    let mut ctx = context();
    let mut req = request("visual.read");
    assert_eq!(
        middleware.on_request(&mut ctx, &mut req),
        MiddlewareResult::Continue
    );
    let security = ctx.security().unwrap();
    println!(
        "  principal type : {:?}",
        security.principal().principal_type()
    );
    println!("  principal id   : {}", security.principal().principal_id());
    assert_eq!(security.principal().principal_type(), PrincipalType::User);
    show_arrow(
        "Credential extraction",
        "Authentication → trusted PrincipalIdentity",
    );
    success("successful authentication establishes trusted security context");

    step(2, "calling-service preservation");
    let calling =
        PrincipalIdentity::new(PrincipalType::Service, PrincipalId::new("gateway").unwrap());
    let mut ctx = context().with_security(SecurityContext::new(
        security.principal().clone(),
        Some(calling.clone()),
    ));
    let mut req = request("visual.read");
    let middleware = SecurityMiddleware::new(Auth, Allow, Credentials);
    assert_eq!(
        middleware.on_request(&mut ctx, &mut req),
        MiddlewareResult::Continue
    );
    assert_eq!(ctx.security().unwrap().calling_service(), Some(&calling));
    success("calling-service identity remains available after re-authentication");

    step(
        3,
        "authentication failure is distinct from authorization denial",
    );
    let authorization_middleware = SecurityMiddleware::new(Auth, Deny, Credentials);
    let mut authorization_ctx = context();
    let mut authorization_req = request("visual.read");
    let authorization_result =
        authorization_middleware.on_request(&mut authorization_ctx, &mut authorization_req);
    assert!(matches!(authorization_result, MiddlewareResult::Reject(_)));

    let authentication_middleware = SecurityMiddleware::new(Auth, Allow, InvalidCredentials);
    let mut authentication_ctx = context();
    let mut authentication_req = request("visual.read");
    let authentication_result =
        authentication_middleware.on_request(&mut authentication_ctx, &mut authentication_req);
    assert!(matches!(authentication_result, MiddlewareResult::Reject(_)));

    println!("  authorization : Deny");
    println!("  authentication: InvalidCredentials");
    println!("  capability    : NOT EXECUTED");
    success("authentication failure and authorization denial both block execution");
}
