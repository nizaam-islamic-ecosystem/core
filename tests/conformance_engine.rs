//! Phase 16 reference/conformance engine.
//!
//! This integration-test-only engine composes real `nizaam_core` APIs. It is
//! intentionally small and deterministic so later Phase 16 tests can exercise
//! lifecycle, capability dispatch, cancellation, deadlines, streaming, large
//! payloads, and observable side effects without introducing a second runtime.

use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use nizaam_core::capability::{
    CapabilityDefinition, CapabilityError, CapabilityInvocation, CapabilityOutcome,
    CapabilityRegistry, arc_handler, dispatch,
};
use nizaam_core::contracts::Version;
use nizaam_core::identity::{CapabilityId, ContractId, EngineId, EngineInstanceId};
use nizaam_core::runtime::{EngineContext, EngineRuntime, LifecycleState, RequestAdmissionError};
use nizaam_core::streaming::{BackpressureConfig, BackpressurePolicy, Stream};

/// Controlled behavior exposed by a reference capability.
#[derive(Clone, Debug)]
pub enum ReferenceBehavior {
    /// Return the request payload unchanged.
    Echo,
    /// Always fail from inside the capability handler.
    Fail,
    /// Fail on the first handler invocation and succeed thereafter.
    FailOnce,
    /// Delay execution while continuing to observe Core cancellation/deadline state.
    Delay(Duration),
    /// Return a deterministic payload supplied by the test.
    LargePayload(Vec<u8>),
    /// Record a side effect and return a deterministic payload.
    SideEffect(Vec<u8>),
}

impl ReferenceBehavior {
    fn output(&self, input: &[u8]) -> Result<Vec<u8>, CapabilityError> {
        match self {
            Self::Echo => Ok(input.to_vec()),
            Self::Fail | Self::FailOnce => Err(CapabilityError::HandlerFailed(
                "reference engine failure".into(),
            )),
            Self::Delay(_) => Ok(input.to_vec()),
            Self::LargePayload(payload) | Self::SideEffect(payload) => Ok(payload.clone()),
        }
    }
}

#[derive(Debug)]
struct HandlerState {
    behavior: Mutex<ReferenceBehavior>,
    fail_once_fired: AtomicBool,
    invocations: AtomicUsize,
    side_effects: AtomicUsize,
    last_context: Mutex<Option<EngineContext>>,
}

/// Error returned by the reference engine's public test-side dispatch helper.
#[derive(Debug)]
pub enum ReferenceDispatchError {
    /// The runtime rejected normal request admission.
    Admission(RequestAdmissionError),
    /// The capability registry or handler returned a Core capability error.
    Capability(CapabilityError),
}

/// Small engine wrapper used only by Phase 16 integration tests.
///
/// The wrapper owns explicit logical/concrete engine identities and delegates
/// lifecycle and capability execution to the corresponding production Core APIs.
pub struct ReferenceEngine {
    engine_id: EngineId,
    instance_id: EngineInstanceId,
    runtime: EngineRuntime,
    registry: CapabilityRegistry,
    handlers: Mutex<BTreeMap<CapabilityId, Arc<HandlerState>>>,
}

impl ReferenceEngine {
    /// Creates a reference engine with explicit `EngineId` and `EngineInstanceId`.
    pub fn new(engine_id: EngineId, instance_id: EngineInstanceId) -> Self {
        Self {
            runtime: EngineRuntime::new(engine_id.clone(), instance_id.clone()),
            engine_id,
            instance_id,
            registry: CapabilityRegistry::new(),
            handlers: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the logical engine identity supplied at construction.
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine-instance identity supplied at construction.
    pub fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }

    /// Returns the real Core runtime owned by this reference engine.
    pub fn runtime(&self) -> &EngineRuntime {
        &self.runtime
    }

    /// Returns the real Core capability registry used by this engine.
    pub fn registry(&self) -> &CapabilityRegistry {
        &self.registry
    }

    /// Registers a capability through the real Core capability registry.
    pub fn register_capability(
        &self,
        capability: &str,
        behavior: ReferenceBehavior,
    ) -> Result<(), String> {
        let capability_id = CapabilityId::new(capability).map_err(|error| error.to_string())?;
        let handler_state = Arc::new(HandlerState {
            behavior: Mutex::new(behavior),
            fail_once_fired: AtomicBool::new(false),
            invocations: AtomicUsize::new(0),
            side_effects: AtomicUsize::new(0),
            last_context: Mutex::new(None),
        });

        let state = Arc::clone(&handler_state);
        let registered_capability = capability_id.clone();

        let handler = arc_handler(
            move |context: &EngineContext, invocation: &CapabilityInvocation| {
                state.invocations.fetch_add(1, Ordering::SeqCst);
                *state
                    .last_context
                    .lock()
                    .expect("reference handler context lock should not be poisoned") =
                    Some(context.clone());

                let behavior = state
                    .behavior
                    .lock()
                    .expect("reference handler behavior lock should not be poisoned")
                    .clone();

                match behavior {
                    ReferenceBehavior::FailOnce => {
                        if !state.fail_once_fired.swap(true, Ordering::SeqCst) {
                            Err(CapabilityError::HandlerFailed(
                                "reference engine fail-once".into(),
                            ))
                        } else {
                            Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                        }
                    }
                    ReferenceBehavior::SideEffect(payload) => {
                        state.side_effects.fetch_add(1, Ordering::SeqCst);
                        Ok(CapabilityOutcome::new(payload))
                    }
                    ReferenceBehavior::Delay(duration) => {
                        let deadline = Instant::now()
                            .checked_add(duration)
                            .unwrap_or_else(Instant::now);

                        while Instant::now() < deadline {
                            if context.cancellation().is_cancelled() {
                                return Err(CapabilityError::Cancelled);
                            }
                            if context.is_expired() {
                                return Err(CapabilityError::DeadlineExpired);
                            }
                            thread::yield_now();
                        }

                        Ok(CapabilityOutcome::new(invocation.payload_bytes().to_vec()))
                    }
                    other => other
                        .output(invocation.payload_bytes())
                        .map(CapabilityOutcome::new),
                }
            },
        );

        let definition = CapabilityDefinition::new(
            registered_capability.clone(),
            self.engine_id.clone(),
            format!("Phase 16 reference capability: {capability}"),
        )
        .map_err(|error| error.to_string())?
        .with_version(Version::new(1, 0, 0));

        self.registry
            .register(definition, handler)
            .map_err(|error| error.to_string())?;

        self.handlers
            .lock()
            .expect("reference handler registry lock should not be poisoned")
            .insert(registered_capability, handler_state);

        Ok(())
    }

    /// Drives the real engine runtime through its lifecycle into `Serving`.
    pub fn serving(&self) -> Result<(), String> {
        for state in [
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
            LifecycleState::Registering,
            LifecycleState::Ready,
            LifecycleState::Serving,
        ] {
            self.runtime
                .transition(state)
                .map_err(|error| error.to_string())?;
        }

        Ok(())
    }

    /// Returns the current real Core runtime lifecycle state.
    pub fn state(&self) -> LifecycleState {
        self.runtime.state()
    }

    /// Dispatches through runtime admission and the real Core capability dispatcher.
    pub fn dispatch(
        &self,
        context: &EngineContext,
        capability: &str,
        contract: &str,
        payload: &[u8],
    ) -> Result<CapabilityOutcome, ReferenceDispatchError> {
        self.runtime
            .admit_request()
            .map_err(ReferenceDispatchError::Admission)?;

        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");
        let contract_id =
            ContractId::new(contract).expect("test contract identifiers must be valid");
        let invocation = CapabilityInvocation::new(capability_id, contract_id, payload.to_vec());

        match dispatch(&self.registry, context, &invocation) {
            nizaam_core::capability::CapabilityDispatchResult::Outcome(outcome) => Ok(outcome),
            nizaam_core::capability::CapabilityDispatchResult::Error(error) => {
                Err(ReferenceDispatchError::Capability(error))
            }
        }
    }

    /// Returns how many times the real registered handler has executed.
    pub fn invocation_count(&self, capability: &str) -> usize {
        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");

        self.handlers
            .lock()
            .expect("reference handler registry lock should not be poisoned")
            .get(&capability_id)
            .map_or(0, |state| state.invocations.load(Ordering::SeqCst))
    }

    /// Returns the number of observable side effects recorded by a side-effect capability.
    pub fn side_effect_count(&self, capability: &str) -> usize {
        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");

        self.handlers
            .lock()
            .expect("reference handler registry lock should not be poisoned")
            .get(&capability_id)
            .map_or(0, |state| state.side_effects.load(Ordering::SeqCst))
    }

    /// Returns the last execution context observed by the real registered handler.
    pub fn last_context(&self, capability: &str) -> Option<EngineContext> {
        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");

        self.handlers
            .lock()
            .expect("reference handler registry lock should not be poisoned")
            .get(&capability_id)
            .and_then(|state| {
                state
                    .last_context
                    .lock()
                    .expect("reference handler context lock should not be poisoned")
                    .clone()
            })
    }

    /// Opens an operation-owned stream using the real Core streaming implementation.
    pub fn open_stream<T>(
        &self,
        context: &EngineContext,
        capacity: usize,
        policy: BackpressurePolicy,
    ) -> Result<Stream<T>, nizaam_core::streaming::StreamError> {
        let config = BackpressureConfig::new(capacity, policy)
            .map_err(nizaam_core::streaming::StreamError::from)?;
        Stream::new(context, config)
    }

    /// Resets the fail-once state for a previously registered capability.
    pub fn reset_fail_once(&self, capability: &str) {
        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");

        if let Some(state) = self
            .handlers
            .lock()
            .expect("reference handler registry lock should not be poisoned")
            .get(&capability_id)
        {
            state.fail_once_fired.store(false, Ordering::SeqCst);
        }
    }

    /// Shuts down the real Core runtime owned by this engine.
    pub fn shutdown(&self) -> Result<bool, String> {
        self.runtime.shutdown().map_err(|error| error.to_string())
    }

    /// Returns whether a capability has been registered in the real Core registry.
    pub fn has_capability(&self, capability: &str) -> bool {
        let capability_id =
            CapabilityId::new(capability).expect("test capability identifiers must be valid");
        self.registry.contains(&capability_id)
    }
}

fn test_context() -> EngineContext {
    let operation = nizaam_core::operation::Operation::new(
        nizaam_core::identity::OperationId::new("phase16-reference-operation").unwrap(),
        nizaam_core::identity::CorrelationId::new("phase16-reference-correlation").unwrap(),
    );

    EngineContext::new(nizaam_core::operation::OperationContext::new(operation))
}

fn new_engine() -> ReferenceEngine {
    ReferenceEngine::new(
        EngineId::new("phase16-engine-a").unwrap(),
        EngineInstanceId::new("phase16-instance-a").unwrap(),
    )
}

#[test]
fn reference_engine_preserves_explicit_engine_identity() {
    let engine = new_engine();

    assert!(!engine.engine_id().as_str().is_empty());
    assert!(!engine.instance_id().as_str().is_empty());
    assert_eq!(engine.runtime().state(), LifecycleState::Created);
    assert_eq!(engine.state(), LifecycleState::Created);
}

#[test]
fn reference_engine_registers_capability_in_real_registry() {
    let engine = new_engine();

    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();

    assert!(engine.has_capability("conformance.echo"));
    assert_eq!(engine.registry().len(), 1);
}

#[test]
fn echo_behavior_preserves_opaque_payload() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let payload = [0_u8, 255, 17, 42, 128];
    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.echo",
            "conformance.contract",
            &payload,
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), payload);
    assert_eq!(engine.invocation_count("conformance.echo"), 1);
}

#[test]
fn unknown_capability_is_rejected_by_real_dispatch() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.missing",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(CapabilityError::Unknown))
    ));
    assert_eq!(engine.invocation_count("conformance.echo"), 0);
}

#[test]
fn handler_failure_remains_capability_failure() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail", ReferenceBehavior::Fail)
        .unwrap();
    engine.serving().unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.fail",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.fail"), 1);
}

#[test]
fn fail_once_behavior_is_deterministic() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail_once", ReferenceBehavior::FailOnce)
        .unwrap();
    engine.serving().unwrap();

    let first = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );
    assert!(matches!(
        first,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));

    let second = engine
        .dispatch(
            &test_context(),
            "conformance.fail_once",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(second.as_bytes(), b"payload");
    assert_eq!(engine.invocation_count("conformance.fail_once"), 2);
}

#[test]
fn delay_behavior_observes_an_expired_deadline_through_real_dispatch() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.delay",
            ReferenceBehavior::Delay(Duration::from_millis(5)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context()
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::ZERO).unwrap());

    let result = engine.dispatch(
        &context,
        "conformance.delay",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::DeadlineExpired
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.delay"), 0);
}

#[test]
fn cancelled_context_is_rejected_before_handler_execution() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.delay",
            ReferenceBehavior::Delay(Duration::from_millis(5)),
        )
        .unwrap();
    engine.serving().unwrap();

    let context = test_context();
    context.cancellation().cancel();

    let result = engine.dispatch(
        &context,
        "conformance.delay",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::Cancelled
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.delay"), 0);
}

#[test]
fn large_payload_behavior_returns_test_supplied_bytes_unchanged() {
    let payload = vec![42_u8; 4096];
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.large",
            ReferenceBehavior::LargePayload(payload.clone()),
        )
        .unwrap();
    engine.serving().unwrap();

    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.large",
            "conformance.contract",
            b"ignored",
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), payload.as_slice());
}

#[test]
fn side_effect_behavior_records_actual_handler_execution() {
    let engine = new_engine();
    engine
        .register_capability(
            "conformance.side_effect",
            ReferenceBehavior::SideEffect(b"committed".to_vec()),
        )
        .unwrap();
    engine.serving().unwrap();

    let outcome = engine
        .dispatch(
            &test_context(),
            "conformance.side_effect",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(outcome.as_bytes(), b"committed");
    assert_eq!(engine.invocation_count("conformance.side_effect"), 1);
    assert_eq!(engine.side_effect_count("conformance.side_effect"), 1);
}

#[test]
fn runtime_admission_prevents_dispatch_before_serving() {
    let engine = new_engine();
    engine
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();

    let result = engine.dispatch(
        &test_context(),
        "conformance.echo",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Admission(
            RequestAdmissionError::NotServing(LifecycleState::Created)
        ))
    ));
    assert_eq!(engine.invocation_count("conformance.echo"), 0);
}

#[test]
fn shutdown_is_delegated_to_real_runtime_and_is_terminal() {
    let engine = new_engine();
    engine.serving().unwrap();

    assert!(engine.shutdown().unwrap());
    assert_eq!(engine.state(), LifecycleState::Stopped);
    assert!(engine.shutdown().unwrap());
}

#[test]
fn separate_reference_engines_keep_handler_state_independent() {
    let first = new_engine();
    let second = ReferenceEngine::new(
        EngineId::new("phase16-engine-b").unwrap(),
        EngineInstanceId::new("phase16-instance-b").unwrap(),
    );

    first
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    second
        .register_capability("conformance.echo", ReferenceBehavior::Echo)
        .unwrap();
    first.serving().unwrap();
    second.serving().unwrap();

    first
        .dispatch(
            &test_context(),
            "conformance.echo",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    assert_eq!(first.invocation_count("conformance.echo"), 1);
    assert_eq!(second.invocation_count("conformance.echo"), 0);
}

#[test]
fn dispatched_context_is_observable_at_the_real_handler_boundary() {
    let engine = new_engine();
    engine
        .register_capability("conformance.context", ReferenceBehavior::Echo)
        .unwrap();
    engine.serving().unwrap();

    let context = test_context()
        .with_deadline(nizaam_core::runtime::Deadline::from_now(Duration::from_secs(1)).unwrap());

    engine
        .dispatch(
            &context,
            "conformance.context",
            "conformance.contract",
            b"payload",
        )
        .unwrap();

    let observed = engine.last_context("conformance.context").unwrap();
    assert!(observed.deadline().is_some());
    assert!(!observed.is_expired());
}

#[test]
fn reference_stream_uses_real_stream_api_and_operation_ownership() {
    let engine = new_engine();
    let context = test_context();

    let stream = engine
        .open_stream::<Vec<u8>>(&context, 2, BackpressurePolicy::Reject)
        .unwrap();

    assert_eq!(stream.capacity(), 2);
    assert_eq!(stream.backpressure_policy(), BackpressurePolicy::Reject);
    assert!(!stream.has_observable_output());
    assert!(!stream.owner().operation_id().as_str().is_empty());

    stream.open().unwrap();
    assert_eq!(
        stream.state(),
        nizaam_core::streaming::StreamLifecycleState::Open
    );
}

#[test]
fn fail_once_state_can_be_explicitly_reset() {
    let engine = new_engine();
    engine
        .register_capability("conformance.fail_once", ReferenceBehavior::FailOnce)
        .unwrap();
    engine.serving().unwrap();

    let _ = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );
    engine.reset_fail_once("conformance.fail_once");

    let result = engine.dispatch(
        &test_context(),
        "conformance.fail_once",
        "conformance.contract",
        b"payload",
    );

    assert!(matches!(
        result,
        Err(ReferenceDispatchError::Capability(
            CapabilityError::HandlerFailed(_)
        ))
    ));
}
