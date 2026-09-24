use crate::support::{failure, section, show_arrow, step, success};
use nizaam_core::capability::{
    CapabilityDefinition, CapabilityDispatchResult, CapabilityError, CapabilityInvocation,
    CapabilityRegistry, arc_handler, dispatch,
};
use nizaam_core::config::parser::ConfigurationParser;
use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::update::{ConfigurationUpdateError, ConfigurationUpdater};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::contracts::UniversalRequest;
use nizaam_core::contracts::descriptor::{
    ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
};
use nizaam_core::contracts::envelope::MessageEnvelope;
use nizaam_core::contracts::metadata::{ContractMetadata, Participants};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventName, EventPublisher,
    EventSubscription, Scope,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, EventId,
    MessageId, OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::retry::{
    Attempt, BackoffPolicy, FailureCategory, RetryAdmission, RetryAdmissionError,
    RetryAdmissionRequest, RetryBudget, RetryDenialReason, RetryPolicy, RetrySafetyGates,
};
use nizaam_core::runtime::{EngineRuntime, LifecycleState};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityMiddleware,
};
use nizaam_core::status::Retryability;
use nizaam_core::transport::{TransportError, decode_frame};
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

fn context() -> nizaam_core::runtime::EngineContext {
    nizaam_core::runtime::EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new("visual-fault-operation").unwrap(),
        CorrelationId::new("visual-fault-correlation").unwrap(),
    )))
}
fn request(capability: &str) -> UniversalRequest {
    let descriptor = ContractDescriptor::new(
        ContractId::new("visual.fault").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new("visual-fault-message").unwrap(),
        OperationContext::new(Operation::new(
            OperationId::new("visual-fault-operation").unwrap(),
            CorrelationId::new("visual-fault-correlation").unwrap(),
        )),
        ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("target").unwrap(),
            ),
        ),
        EncodedPayload::new(descriptor.payload, b"fault"),
    ))
}

#[derive(Debug)]
struct GoodAuth;
impl Authenticator for GoodAuth {
    fn authenticate(
        &self,
        _: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Ok(PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("fault-user").unwrap(),
        ))
    }
}
#[derive(Debug)]
struct DenyAuthz;
impl Authorizer for DenyAuthz {
    fn authorize(
        &self,
        _: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Deny)
    }
}
#[derive(Clone)]
struct Creds;
impl CredentialExtractor for Creds {
    fn extract(
        &self,
        _: &nizaam_core::runtime::EngineContext,
        _: &UniversalRequest,
    ) -> Option<Vec<u8>> {
        Some(b"opaque".to_vec())
    }
}

#[test]
fn visual_fault_classification_recovery_and_cleanup() {
    section("NIZAAM CORE — FAULT + RECOVERY");
    step(1, "transport failure remains transport-owned");
    let error = TransportError::Disconnected;
    assert!(error.is_retryable());
    println!("  failure : {error:?}");
    failure("controlled transport failure observed");
    success("transport failure remains distinguishable from capability failure");

    step(2, "stale route meets local runtime admission");
    let runtime = EngineRuntime::new(
        EngineId::new("fault-engine").unwrap(),
        EngineInstanceId::new("fault-instance").unwrap(),
    );
    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
        LifecycleState::Draining,
    ] {
        runtime.transition(state).unwrap();
    }
    assert!(runtime.admit_request().is_err());
    println!("  routing decision exists → runtime is Draining → admission rejected");
    show_arrow("RoutingDecision", "local runtime admission");

    step(3, "capability failure remains capability-owned");
    let registry = CapabilityRegistry::new();
    let capability = CapabilityId::new("visual.failure").unwrap();
    registry
        .register(
            CapabilityDefinition::new(
                capability.clone(),
                EngineId::new("fault-engine").unwrap(),
                "failure capability",
            )
            .unwrap(),
            arc_handler(|_, _| Err(CapabilityError::HandlerFailed("controlled".to_owned()))),
        )
        .unwrap();
    let invocation = CapabilityInvocation::new(
        capability,
        ContractId::new("visual.fault").unwrap(),
        b"payload".to_vec(),
    );
    assert!(matches!(
        dispatch(&registry, &context(), &invocation),
        CapabilityDispatchResult::Error(CapabilityError::HandlerFailed(_))
    ));
    success("handler failure is not mislabeled as transport failure");

    step(4, "security denial before execution");
    let middleware = SecurityMiddleware::new(GoodAuth, DenyAuthz, Creds);
    let mut ctx = context();
    let mut req = request("visual.denied");
    assert!(matches!(
        middleware.on_request(&mut ctx, &mut req),
        MiddlewareResult::Reject(_)
    ));
    println!("  authorization = Deny → request rejected");

    step(5, "unknown outcome is rejected at retry boundary");
    let operation = OperationId::new("visual-unknown-operation").unwrap();
    let attempt = Attempt::new(
        operation.clone(),
        AttemptId::new("visual-unknown-a1").unwrap(),
        1,
    )
    .unwrap();
    attempt.start().unwrap();
    attempt.fail().unwrap();
    let policy = RetryPolicy::new(2, 2)
        .unwrap()
        .with_retryable_category(FailureCategory::Unknown);
    let backoff = BackoffPolicy::no_backoff();
    let token = CancellationToken::new();
    let mut budget = RetryBudget::new(1);
    let admission = RetryAdmission::new(&policy, &backoff, &token, None);
    let result = admission.admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &attempt,
        category: FailureCategory::Unknown,
        retryability: Retryability::Retryable,
        next_attempt_id: AttemptId::new("visual-unknown-a2").unwrap(),
        jitter_source: None,
        safety_gates: RetrySafetyGates::new(true, true, true, true),
    });
    assert!(matches!(
        result,
        Err(RetryAdmissionError::Policy(
            RetryDenialReason::UnknownOutcome
        ))
    ));
    assert_eq!(budget.consumed(), 0);
    println!("  unknown outcome → retry admission denied pending reconciliation");

    step(6, "configuration failure preserves active snapshot");
    let mut parsed = ParsedConfiguration::new(BTreeMap::new());
    parsed.insert(
        "visual.value".to_owned(),
        ConfigurationValue::String("stable".to_owned()),
    );
    let resolved = ConfigurationResolver::new()
        .resolve(&ConfigurationValidator::new().validate(parsed).unwrap())
        .unwrap();
    let snapshot = Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(1),
        resolved,
    ));
    let parser = ConfigurationParser::new()
        .with_type(
            "required",
            nizaam_core::config::parser::ConfigurationType::String,
        )
        .unwrap();
    let mut updater = ConfigurationUpdater::new(
        parser,
        ConfigurationValidator::new().require_key("required"),
        ConfigurationResolver::new(),
        (*snapshot).clone(),
    );
    assert!(matches!(
        updater.update(
            &nizaam_core::config::loader::LoadedConfiguration::from_values([(
                String::from("visual.value"),
                String::from("changed")
            )])
        ),
        Err(ConfigurationUpdateError::Validation(_))
    ));
    assert_eq!(updater.current(), snapshot.as_ref());
    success("failed configuration update leaves active snapshot unchanged");

    step(7, "subscriber failure isolation");
    let owner = CancellationToken::new();
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();
    let (sender, receiver) = mpsc::channel();
    let panicking = EventSubscription::new(
        EventName::new("visual.fault.event").unwrap(),
        "visual.fault.event",
        Scope::new("engine:fault").unwrap(),
        |_e: &Event| panic!("controlled"),
        &owner,
    )
    .unwrap();
    let healthy = EventSubscription::new(
        EventName::new("visual.fault.event").unwrap(),
        "visual.fault.event",
        Scope::new("engine:fault").unwrap(),
        move |_e: &Event| {
            sender.send(()).unwrap();
        },
        &owner,
    )
    .unwrap();
    let first = publisher.subscribe(panicking).unwrap();
    let second = publisher.subscribe(healthy).unwrap();
    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(4, 2, 4).unwrap(), owner.clone()).unwrap();
    let h1 = dispatcher.register(first).unwrap();
    let h2 = dispatcher.register(second).unwrap();
    let publication = publisher
        .publish(
            Event::new(
                EventId::new("visual-fault-event").unwrap(),
                EventName::new("visual.fault.event").unwrap(),
                "visual.fault.event",
                Scope::new("engine:fault").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        h1.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        h2.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    dispatcher.shutdown();
    success("one subscriber panic does not prevent unrelated matching delivery");

    step(8, "framing violation is rejected");
    assert!(decode_frame(&[0; 47]).is_err());
    success("malformed frame is rejected at the transport framing boundary");
}
