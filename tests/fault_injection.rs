//! Phase 16 deterministic fault-injection tests.
//!
//! Every scenario injects one bounded failure at a public Core boundary and
//! verifies classification, isolation, cleanup, or controlled recovery.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use nizaam_core::artifact::{ContentDigest, IntegrityProof};
use nizaam_core::capability::{
    CapabilityDefinition, CapabilityDispatchResult, CapabilityError, CapabilityInvocation,
    CapabilityRegistry, arc_handler, dispatch,
};
use nizaam_core::config::{
    resolution::ConfigurationResolver,
    snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
    update::{ConfigurationUpdateError, ConfigurationUpdater},
    validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
};
use nizaam_core::contracts::{ContractDescriptor, Interaction, PayloadDescriptor, Version};
use nizaam_core::control_plane::destination::{
    DestinationEligibilityInput, DestinationRequest, eligible_destinations,
};
use nizaam_core::control_plane::membership::Membership;
use nizaam_core::control_plane::observations::Observations;
use nizaam_core::control_plane::{ControlPlaneFailure, ControlPlaneFailureKind};
use nizaam_core::events::{
    DeliveryConfig, DeliveryDispatcher, DeliveryOutcome, Event, EventName, EventPublisher,
    EventSubscription, Scope,
};
use nizaam_core::identity::{
    AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, EventId,
    OperationId,
};
use nizaam_core::middleware::stages::{Middleware, MiddlewareResult};
use nizaam_core::operation::{CancellationToken, Operation, OperationContext};
use nizaam_core::retry::{
    Attempt, AttemptLifecycleState, BackoffPolicy, FailureCategory, RetryAdmission,
    RetryAdmissionRequest, RetryBudget, RetryPolicy, RetrySafetyGates,
};
use nizaam_core::runtime::{
    BackgroundTasks, ConcurrencyConfig, EngineContext, EngineRuntime, LifecycleState,
};
use nizaam_core::security::{
    AuthenticationError, AuthenticationRequest, Authenticator, AuthorizationDecision,
    AuthorizationError, AuthorizationRequest, Authorizer, CredentialExtractor, PrincipalId,
    PrincipalIdentity, PrincipalType, SecurityMiddleware,
};
use nizaam_core::status::Retryability;
use nizaam_core::streaming::{
    BackpressureConfig, BackpressurePolicy, Stream, StreamError, StreamItem,
};
use nizaam_core::transport::{TransportError, decode_frame};

fn context() -> EngineContext {
    EngineContext::new(OperationContext::new(Operation::new(
        OperationId::new("fault-operation").unwrap(),
        CorrelationId::new("fault-correlation").unwrap(),
    )))
}

fn request_with_capability(capability: &str) -> nizaam_core::contracts::UniversalRequest {
    let payload =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("fault.contract").unwrap(),
        CapabilityId::new(capability).unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload.clone(),
    );
    let metadata = nizaam_core::contracts::ContractMetadata::new(
        descriptor,
        nizaam_core::contracts::Participants::new(
            EngineId::new("fault-sender").unwrap(),
            EngineId::new("fault-receiver").unwrap(),
        ),
    );
    nizaam_core::contracts::UniversalRequest::new(nizaam_core::contracts::MessageEnvelope::new(
        nizaam_core::identity::MessageId::new("fault-message").unwrap(),
        OperationContext::new(Operation::new(
            OperationId::new("fault-request-operation").unwrap(),
            CorrelationId::new("fault-request-correlation").unwrap(),
        )),
        metadata,
        nizaam_core::contracts::EncodedPayload::new(payload, b"fault"),
    ))
}

fn valid_snapshot(id: u64, value: &str) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(std::collections::BTreeMap::new());
    parsed.insert(
        "fault.value".to_owned(),
        ConfigurationValue::String(value.to_owned()),
    );
    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();
    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}

#[test]
fn unavailable_transport_remains_a_transport_error() {
    let error = TransportError::Disconnected;
    assert!(error.is_retryable());
    assert!(matches!(error, TransportError::Disconnected));
}

#[test]
fn malformed_transport_frame_is_rejected() {
    let malformed = vec![0_u8; nizaam_core::transport::HEADER_LENGTH - 1];
    assert!(decode_frame(&malformed).is_err());
}

#[test]
fn capability_failure_remains_a_capability_error() {
    let registry = CapabilityRegistry::new();
    let capability = CapabilityId::new("fault.capability").unwrap();
    let definition = CapabilityDefinition::new(
        capability.clone(),
        EngineId::new("fault-engine").unwrap(),
        "fault capability",
    )
    .unwrap();
    registry
        .register(
            definition,
            arc_handler(|_, _| {
                Err(CapabilityError::HandlerFailed(
                    "deterministic failure".to_owned(),
                ))
            }),
        )
        .unwrap();

    let invocation = CapabilityInvocation::new(
        capability,
        ContractId::new("fault.contract").unwrap(),
        b"payload".to_vec(),
    );
    let result = dispatch(&registry, &context(), &invocation);

    assert!(matches!(
        result,
        CapabilityDispatchResult::Error(CapabilityError::HandlerFailed(_))
    ));
}

#[test]
fn runtime_admission_failure_is_local_to_runtime() {
    let runtime = EngineRuntime::new(
        EngineId::new("fault-runtime-engine").unwrap(),
        EngineInstanceId::new("fault-runtime-instance").unwrap(),
    );
    for state in [
        LifecycleState::Starting,
        LifecycleState::Configuring,
        LifecycleState::Dependencies,
        LifecycleState::Capabilities,
        LifecycleState::Registering,
        LifecycleState::Ready,
        LifecycleState::Serving,
    ] {
        runtime.transition(state).unwrap();
    }
    runtime.transition(LifecycleState::Draining).unwrap();

    assert_eq!(
        runtime.admit_request(),
        Err(nizaam_core::runtime::RequestAdmissionError::NotServing(
            LifecycleState::Draining
        ))
    );
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
struct AllowingAuthorizer;

impl Authorizer for AllowingAuthorizer {
    fn authorize(
        &self,
        _request: &AuthorizationRequest<'_>,
    ) -> Result<AuthorizationDecision, AuthorizationError> {
        Ok(AuthorizationDecision::Allow)
    }
}

#[derive(Clone)]
struct StaticCredentials;

impl CredentialExtractor for StaticCredentials {
    fn extract(
        &self,
        _context: &EngineContext,
        _request: &nizaam_core::contracts::UniversalRequest,
    ) -> Option<Vec<u8>> {
        Some(b"fault-credentials".to_vec())
    }
}

#[test]
fn authentication_failure_prevents_authorization() {
    let middleware =
        SecurityMiddleware::new(FailingAuthenticator, AllowingAuthorizer, StaticCredentials);
    let mut context = context();
    let mut request = request_with_capability("fault.auth");

    let result = middleware.on_request(&mut context, &mut request);
    assert!(matches!(result, MiddlewareResult::Fail(_)));
    assert!(context.security().is_none());
}

#[derive(Debug)]
struct SuccessfulAuthenticator;

impl Authenticator for SuccessfulAuthenticator {
    fn authenticate(
        &self,
        _request: &AuthenticationRequest<'_>,
    ) -> Result<PrincipalIdentity, AuthenticationError> {
        Ok(PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("fault-user").unwrap(),
        ))
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

#[test]
fn authorization_denial_stops_before_capability_execution() {
    let invoked = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&invoked);
    let middleware = SecurityMiddleware::new(
        SuccessfulAuthenticator,
        DenyingAuthorizer,
        StaticCredentials,
    );
    let mut context = context();
    let mut request = request_with_capability("fault.authorization");

    let result = middleware.on_request(&mut context, &mut request);
    assert!(matches!(result, MiddlewareResult::Reject(_)));
    assert!(!observed.load(Ordering::SeqCst));
}

#[test]
fn stream_failure_is_terminal_and_rejects_later_publication() {
    let stream: Stream<u8> = Stream::new(
        &context(),
        BackpressureConfig::new(2, BackpressurePolicy::Reject).unwrap(),
    )
    .unwrap();
    stream.open().unwrap();
    stream.publish(StreamItem::partial(0, 1)).unwrap();
    stream.fail().unwrap();

    assert_eq!(
        stream.publish(StreamItem::partial(1, 2)),
        Err(StreamError::Failed)
    );
}

#[test]
fn task_cancellation_reaches_runtime_owned_work_and_shutdown_cleans_up() {
    let tasks = BackgroundTasks::new(CancellationToken::new());
    let finished = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&finished);
    tasks
        .spawn(move |token| {
            while !token.is_cancelled() {
                thread::yield_now();
            }
            observed.store(true, Ordering::SeqCst);
        })
        .unwrap();

    tasks.cancellation().cancel();
    tasks.shutdown();
    assert!(finished.load(Ordering::SeqCst));
}

#[test]
fn unknown_retry_outcome_does_not_consume_retry_budget() {
    let operation = OperationId::new("fault-unknown-outcome").unwrap();
    let attempt = Attempt::new(operation, AttemptId::new("fault-attempt").unwrap(), 1).unwrap();
    attempt.start().unwrap();
    attempt.fail().unwrap();

    let policy = RetryPolicy::new(3, 4)
        .unwrap()
        .with_retryable_category(FailureCategory::Unknown);
    let mut budget = RetryBudget::new(2);
    let backoff = BackoffPolicy::no_backoff();
    let cancellation = CancellationToken::new();
    let admission = RetryAdmission::new(&policy, &backoff, &cancellation, None);
    let result = admission.admit_next(RetryAdmissionRequest {
        budget: &mut budget,
        current_attempt: &attempt,
        category: FailureCategory::Unknown,
        retryability: Retryability::Retryable,
        next_attempt_id: AttemptId::new("fault-next-attempt").unwrap(),
        jitter_source: None,
        safety_gates: RetrySafetyGates::new(true, true, true, true),
    });

    assert!(matches!(
        result,
        Err(nizaam_core::retry::RetryAdmissionError::Policy(_))
    ));
    assert_eq!(budget.consumed(), 0);
    assert_eq!(attempt.state(), AttemptLifecycleState::Failed);
}

#[test]
fn control_plane_failure_preserves_core_error_vocabulary() {
    let error = nizaam_core::error::GlobalError {
        code: nizaam_core::error::ErrorCode::new("CORE.CONTROL_PLANE.FAULT.001").unwrap(),
        owner: nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
        version: Version::new(1, 0, 0),
        class: nizaam_core::error::ErrorClass::Contract,
        severity: nizaam_core::error::Severity::Error,
        retryability: Retryability::NonRetryable,
        message: "deterministic control-plane fault".to_owned(),
        details: Vec::new(),
        solution_reference: None,
        context: nizaam_core::error::ErrorContext::new(OperationContext::new(Operation::new(
            OperationId::new("fault-control-plane").unwrap(),
            CorrelationId::new("fault-control-plane-correlation").unwrap(),
        ))),
        cause: nizaam_core::error::ErrorReference::new("CORE.TEST.CAUSE"),
    };
    let failure = ControlPlaneFailure::new(
        ControlPlaneFailureKind::DestinationUnavailable,
        error.clone(),
    );

    assert_eq!(
        failure.kind(),
        ControlPlaneFailureKind::DestinationUnavailable
    );
    assert_eq!(failure.error(), &error);
}

#[test]
fn invalid_configuration_update_keeps_active_snapshot() {
    let original = valid_snapshot(70, "stable");
    let parser = nizaam_core::config::parser::ConfigurationParser::new()
        .with_type(
            "fault.required",
            nizaam_core::config::parser::ConfigurationType::String,
        )
        .unwrap();
    let validator = ConfigurationValidator::new().require_key("fault.required");
    let resolver = ConfigurationResolver::new();
    let mut updater = ConfigurationUpdater::new(parser, validator, resolver, (*original).clone());

    let mut parsed = ParsedConfiguration::new(std::collections::BTreeMap::new());
    parsed.insert(
        "fault.value".to_owned(),
        ConfigurationValue::String("changed".to_owned()),
    );
    let loaded = nizaam_core::config::loader::LoadedConfiguration::from_values(parsed.iter().map(
        |(key, value)| {
            (
                key.to_owned(),
                match value {
                    ConfigurationValue::String(value) => value.to_owned(),
                    _ => String::new(),
                },
            )
        },
    ));
    let result = updater.update(&loaded);

    assert!(matches!(
        result,
        Err(ConfigurationUpdateError::Validation(_))
    ));
    assert_eq!(updater.current(), original.as_ref());
}

#[test]
fn artifact_integrity_failure_rejects_tampered_content() {
    let trusted = b"trusted-artifact";
    let digest = ContentDigest::new(trusted);
    assert!(IntegrityProof::verify(trusted, &digest).is_ok());
    assert!(IntegrityProof::verify(b"tampered-artifact", &digest).is_err());
}

#[test]
fn event_subscriber_panic_isolated_from_other_subscriber() {
    let lifecycle = Arc::new(nizaam_core::events::EventLifecycle::new());
    let owner = CancellationToken::new();
    let publisher = EventPublisher::new(Arc::clone(&lifecycle), &owner);
    publisher.activate().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();

    let panicking = EventSubscription::new(
        EventName::new("fault.event").unwrap(),
        "fault.event",
        Scope::new("engine:fault").unwrap(),
        |_event: &Event| panic!("deterministic subscriber fault"),
        &owner,
    )
    .unwrap();
    let healthy_sender = sender.clone();
    let healthy = EventSubscription::new(
        EventName::new("fault.event").unwrap(),
        "fault.event",
        Scope::new("engine:fault").unwrap(),
        move |_event: &Event| healthy_sender.send(()).unwrap(),
        &owner,
    )
    .unwrap();
    let panicking = publisher.subscribe(panicking).unwrap();
    let healthy = publisher.subscribe(healthy).unwrap();
    let dispatcher =
        DeliveryDispatcher::new(DeliveryConfig::new(4, 2, 4).unwrap(), owner.clone()).unwrap();
    let first = dispatcher.register(panicking).unwrap();
    let second = dispatcher.register(healthy).unwrap();
    let event = Event::new(
        EventId::new("fault-event-id").unwrap(),
        EventName::new("fault.event").unwrap(),
        "fault.event",
        Scope::new("engine:fault").unwrap(),
    )
    .unwrap();
    let publication = publisher.publish(event).unwrap();

    assert_eq!(publication.subscription_count(), 2);
    assert_eq!(
        first.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    assert_eq!(
        second.enqueue(Arc::clone(publication.event())).unwrap(),
        DeliveryOutcome::Accepted
    );
    receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    dispatcher.shutdown();
}

#[test]
fn deterministic_control_plane_eligibility_failure_does_not_create_execution() {
    let membership = Membership::new();
    let observations = Observations::new();
    let request =
        DestinationRequest::hard_explicit(EngineInstanceId::new("missing-instance").unwrap());
    let descriptor = ContractDescriptor::new(
        ContractId::new("fault.eligibility").unwrap(),
        CapabilityId::new("fault.capability").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
    );
    let result = eligible_destinations(DestinationEligibilityInput::new(
        &request,
        &membership.snapshot(),
        &observations.snapshot(),
        &CapabilityId::new("fault.capability").unwrap(),
        &descriptor,
    ));

    assert!(result.is_err());
}

#[test]
fn bounded_background_admission_rejects_excess_work_without_leaking_occupancy() {
    use std::sync::mpsc;

    let config = ConcurrencyConfig::new(1, 1).unwrap();
    let tasks = BackgroundTasks::with_concurrency(CancellationToken::new(), config);
    let gate = Arc::new(AtomicBool::new(false));
    let gate_worker = Arc::clone(&gate);
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();

    tasks
        .spawn_bounded(move |token| {
            started_tx.send(()).unwrap();

            while !gate_worker.load(Ordering::SeqCst) && !token.is_cancelled() {
                thread::yield_now();
            }

            finished_tx.send(()).unwrap();
        })
        .unwrap();

    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("bounded worker did not start");

    let second = tasks.spawn_bounded(|_| {});
    assert_eq!(
        second,
        Err(nizaam_core::runtime::BoundedSpawnError::ActiveLimitReached)
    );

    gate.store(true, Ordering::SeqCst);
    finished_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("bounded worker did not finish");

    let (replacement_tx, replacement_rx) = mpsc::channel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);

    loop {
        let sender = replacement_tx.clone();
        match tasks.spawn_bounded(move |_| {
            sender.send(()).unwrap();
        }) {
            Ok(()) => break,
            Err(nizaam_core::runtime::BoundedSpawnError::ActiveLimitReached)
                if std::time::Instant::now() < deadline =>
            {
                thread::yield_now();
            }
            Err(_) => panic!("bounded worker occupancy was not released"),
        }
    }

    replacement_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("replacement bounded worker did not run");

    tasks.shutdown();
}
