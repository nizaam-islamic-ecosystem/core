//! Phase 16 reference engine shared by integration-test crates.
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

#[cfg(test)]
mod tests {
    use super::*;
    use nizaam_core::identity::{AttemptId, CorrelationId, NodeId, OperationId};
    use nizaam_core::operation::{Operation, OperationContext};
    use nizaam_core::streaming::StreamItem;

    fn context(id: &str) -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new(id).expect("test operation id must be valid"),
            CorrelationId::new(format!("{id}-correlation"))
                .expect("test correlation id must be valid"),
        )))
    }

    #[test]
    fn reference_engine_exercises_all_behaviors_and_boundaries() {
        let engine = ReferenceEngine::new(
            EngineId::new("reference-test-engine").expect("test engine id must be valid"),
            EngineInstanceId::new("reference-test-instance")
                .expect("test instance id must be valid"),
        );

        assert_eq!(engine.engine_id().as_str(), "reference-test-engine");
        assert_eq!(engine.instance_id().as_str(), "reference-test-instance");
        assert_eq!(engine.state(), LifecycleState::Created);
        assert_eq!(engine.runtime().state(), LifecycleState::Created);
        assert!(!engine.has_capability("missing.capability"));
        assert!(!engine.registry().contains(
            &CapabilityId::new("missing.capability").expect("test capability id must be valid")
        ));

        engine
            .register_capability("reference.echo", ReferenceBehavior::Echo)
            .unwrap();
        engine
            .register_capability("reference.fail", ReferenceBehavior::Fail)
            .unwrap();
        engine
            .register_capability("reference.fail_once", ReferenceBehavior::FailOnce)
            .unwrap();
        engine
            .register_capability("reference.delay", ReferenceBehavior::Delay(Duration::ZERO))
            .unwrap();
        engine
            .register_capability(
                "reference.large",
                ReferenceBehavior::LargePayload(b"large-output".to_vec()),
            )
            .unwrap();
        engine
            .register_capability(
                "reference.side_effect",
                ReferenceBehavior::SideEffect(b"side-effect-output".to_vec()),
            )
            .unwrap();

        assert!(engine.has_capability("reference.echo"));
        assert!(engine.has_capability("reference.fail"));
        assert!(engine.has_capability("reference.fail_once"));
        assert!(engine.has_capability("reference.delay"));
        assert!(engine.has_capability("reference.large"));
        assert!(engine.has_capability("reference.side_effect"));

        engine.serving().unwrap();
        assert_eq!(engine.state(), LifecycleState::Serving);
        assert_eq!(engine.runtime().state(), LifecycleState::Serving);

        let echo_context = context("reference-echo");
        let echo = engine
            .dispatch(
                &echo_context,
                "reference.echo",
                "reference.contract",
                b"echo-payload",
            )
            .unwrap();
        assert_eq!(echo.as_bytes(), b"echo-payload");
        assert_eq!(engine.invocation_count("reference.echo"), 1);
        assert_eq!(
            engine
                .last_context("reference.echo")
                .expect("echo context must be recorded")
                .operation()
                .operation
                .id
                .as_str(),
            "reference-echo"
        );

        let fail = engine.dispatch(
            &context("reference-fail"),
            "reference.fail",
            "reference.contract",
            b"fail-payload",
        );
        match fail {
            Err(ReferenceDispatchError::Capability(CapabilityError::HandlerFailed(message))) => {
                assert_eq!(message, "reference engine failure");
            }
            other => panic!("expected reference capability failure, got {other:?}"),
        }
        assert_eq!(engine.invocation_count("reference.fail"), 1);

        let first_fail_once = engine.dispatch(
            &context("reference-fail-once"),
            "reference.fail_once",
            "reference.contract",
            b"first",
        );
        match first_fail_once {
            Err(ReferenceDispatchError::Capability(CapabilityError::HandlerFailed(message))) => {
                assert_eq!(message, "reference engine fail-once");
            }
            other => panic!("expected first fail-once error, got {other:?}"),
        }

        let second_fail_once = engine
            .dispatch(
                &context("reference-fail-once"),
                "reference.fail_once",
                "reference.contract",
                b"second",
            )
            .unwrap();
        assert_eq!(second_fail_once.as_bytes(), b"second");
        assert_eq!(engine.invocation_count("reference.fail_once"), 2);

        engine.reset_fail_once("reference.fail_once");
        let reset_fail_once = engine.dispatch(
            &context("reference-fail-once"),
            "reference.fail_once",
            "reference.contract",
            b"after-reset",
        );
        assert!(matches!(
            reset_fail_once,
            Err(ReferenceDispatchError::Capability(
                CapabilityError::HandlerFailed(_)
            ))
        ));

        let delayed = engine
            .dispatch(
                &context("reference-delay"),
                "reference.delay",
                "reference.contract",
                b"delay-payload",
            )
            .unwrap();
        assert_eq!(delayed.as_bytes(), b"delay-payload");

        let large = engine
            .dispatch(
                &context("reference-large"),
                "reference.large",
                "reference.contract",
                b"ignored-input",
            )
            .unwrap();
        assert_eq!(large.as_bytes(), b"large-output");

        let side_effect = engine
            .dispatch(
                &context("reference-side-effect"),
                "reference.side_effect",
                "reference.contract",
                b"ignored-input",
            )
            .unwrap();
        assert_eq!(side_effect.as_bytes(), b"side-effect-output");
        assert_eq!(engine.side_effect_count("reference.side_effect"), 1);

        let stream_context = context("reference-stream");
        let stream = engine
            .open_stream::<u32>(&stream_context, 2, BackpressurePolicy::Reject)
            .unwrap();
        stream.open().unwrap();
        let consumer = stream.consumer().unwrap();

        stream.publish(StreamItem::partial(0, 1)).unwrap();
        stream.publish(StreamItem::final_item(1, 1)).unwrap();
        assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 0);
        assert_eq!(consumer.next_item().unwrap().unwrap().sequence(), 1);

        assert!(engine.shutdown().unwrap());
        assert_eq!(engine.state(), LifecycleState::Stopped);

        let admission = engine.dispatch(
            &context("reference-after-shutdown"),
            "reference.echo",
            "reference.contract",
            b"rejected",
        );
        match admission {
            Err(ReferenceDispatchError::Admission(error)) => {
                let _ = format!("{error:?}");
            }
            other => panic!("expected admission rejection after shutdown, got {other:?}"),
        }

        let _attempt =
            AttemptId::new("reference-test-attempt").expect("test attempt id must be valid");
        let _node = NodeId::new("reference-test-node").expect("test node id must be valid");
    }
}
