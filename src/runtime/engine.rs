use std::sync::{Condvar, Mutex};

use super::{
    CancellationToken,
    background::BackgroundTasks,
    concurrency::ConcurrencyConfig,
    lifecycle::{Lifecycle, LifecycleState},
};
use crate::error::InvalidTransition;

/// Minimal lifecycle owner for an engine instance.
///
/// `EngineRuntime` owns the engine lifecycle, runtime shutdown signal, and
/// runtime-owned background tasks. Request execution, dependency semantics,
/// capability readiness, and engine registration are coordinated by later
/// runtime layers and are intentionally not introduced here.
#[derive(Debug)]
pub struct EngineRuntime {
    lifecycle: Mutex<Lifecycle>,
    shutdown: CancellationToken,
    background: BackgroundTasks,
    shutdown_state: Mutex<ShutdownState>,
    shutdown_complete: Condvar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownState {
    NotStarted,
    Initiated,
    Coordinating,
    Complete,
}

impl Default for EngineRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRuntime {
    /// Creates a new runtime in the `Created` lifecycle state.
    pub fn new() -> Self {
        let shutdown = CancellationToken::new();

        Self {
            lifecycle: Mutex::new(Lifecycle::new()),
            background: BackgroundTasks::new(shutdown.clone()),
            shutdown,
            shutdown_state: Mutex::new(ShutdownState::NotStarted),
            shutdown_complete: Condvar::new(),
        }
    }

    /// Creates a new runtime with bounded managed background-task admission.
    ///
    /// The existing [`Self::new`] constructor remains unchanged for callers
    /// that do not need bounded runtime-task admission.
    pub fn with_concurrency(config: ConcurrencyConfig) -> Self {
        let shutdown = CancellationToken::new();

        Self {
            lifecycle: Mutex::new(Lifecycle::new()),
            background: BackgroundTasks::with_concurrency(shutdown.clone(), config),
            shutdown,
            shutdown_state: Mutex::new(ShutdownState::NotStarted),
            shutdown_complete: Condvar::new(),
        }
    }

    /// Returns the current lifecycle state.
    pub fn state(&self) -> LifecycleState {
        self.lifecycle
            .lock()
            .expect("lifecycle lock poisoned")
            .state()
    }

    /// Transitions the runtime lifecycle to `next`.
    ///
    /// Lifecycle changes are coordinated with shutdown. Direct transitions to
    /// `Stopped` are reserved for the shutdown cleanup path.
    pub fn transition(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
        let shutdown_state = self
            .shutdown_state
            .lock()
            .expect("shutdown state lock poisoned");

        if *shutdown_state != ShutdownState::NotStarted {
            let current = self.state();
            return Err(InvalidTransition::new(
                format!("{current:?}"),
                format!("{next:?}"),
            ));
        }

        if next == LifecycleState::Stopped {
            let current = self.state();
            return Err(InvalidTransition::new(
                format!("{current:?}"),
                format!("{next:?}"),
            ));
        }

        self.transition_lifecycle(next)
    }

    fn transition_lifecycle(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
        self.lifecycle
            .lock()
            .expect("lifecycle lock poisoned")
            .transition(next)
    }

    /// Returns the runtime-wide shutdown cancellation token.
    pub fn shutdown_token(&self) -> &CancellationToken {
        &self.shutdown
    }

    /// Returns the runtime-owned background task manager.
    pub fn background_tasks(&self) -> &BackgroundTasks {
        &self.background
    }

    /// Gracefully shuts down the runtime.
    ///
    /// Shutdown enters `Draining` first, signals runtime-owned background
    /// work, waits for that work to finish, and only then enters `Stopped`.
    ///
    /// `Stopped` is terminal and repeated shutdown calls are idempotent.
    ///
    /// Returns `true` when shutdown completed and `false` when a runtime-owned
    /// background task initiated shutdown and an external coordinator must
    /// finish cleanup.
    pub fn shutdown(&self) -> Result<bool, InvalidTransition> {
        if self.background.is_current_task() {
            let mut shutdown_state = self
                .shutdown_state
                .lock()
                .expect("shutdown state lock poisoned");

            if *shutdown_state == ShutdownState::Complete {
                return Ok(true);
            }

            if *shutdown_state == ShutdownState::NotStarted {
                if self.state() != LifecycleState::Draining {
                    self.transition_lifecycle(LifecycleState::Draining)?;
                }
                *shutdown_state = ShutdownState::Initiated;
            }

            drop(shutdown_state);
            self.background.begin_shutdown();
            return Ok(false);
        }

        let mut shutdown_state = self
            .shutdown_state
            .lock()
            .expect("shutdown state lock poisoned");

        loop {
            match *shutdown_state {
                ShutdownState::Complete => return Ok(true),
                ShutdownState::Coordinating => {
                    shutdown_state = self
                        .shutdown_complete
                        .wait(shutdown_state)
                        .expect("shutdown state lock poisoned");
                }
                ShutdownState::Initiated => {
                    *shutdown_state = ShutdownState::Coordinating;
                    break;
                }
                ShutdownState::NotStarted => {
                    if self.state() != LifecycleState::Draining {
                        self.transition_lifecycle(LifecycleState::Draining)?;
                    }
                    *shutdown_state = ShutdownState::Coordinating;
                    break;
                }
            }
        }

        drop(shutdown_state);

        let cleanup_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.background.shutdown();
        }));

        if let Err(payload) = cleanup_result {
            let mut shutdown_state = self
                .shutdown_state
                .lock()
                .expect("shutdown state lock poisoned");
            *shutdown_state = ShutdownState::NotStarted;
            self.shutdown_complete.notify_all();
            drop(shutdown_state);
            std::panic::resume_unwind(payload);
        }

        if self.state() != LifecycleState::Stopped {
            if let Err(error) = self.transition_lifecycle(LifecycleState::Stopped) {
                let mut shutdown_state = self
                    .shutdown_state
                    .lock()
                    .expect("shutdown state lock poisoned");
                *shutdown_state = ShutdownState::NotStarted;
                self.shutdown_complete.notify_all();
                return Err(error);
            }
        }

        let mut shutdown_state = self
            .shutdown_state
            .lock()
            .expect("shutdown state lock poisoned");
        *shutdown_state = ShutdownState::Complete;
        self.shutdown_complete.notify_all();

        Ok(true)
    }
}

impl Drop for EngineRuntime {
    fn drop(&mut self) {
        self.background.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::EngineRuntime;
    use crate::error::InvalidTransition;
    use crate::runtime::concurrency::ConcurrencyConfig;
    use crate::runtime::lifecycle::LifecycleState;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn serving_runtime() -> EngineRuntime {
        let runtime = EngineRuntime::new();

        runtime.transition(LifecycleState::Starting).unwrap();
        runtime.transition(LifecycleState::Configuring).unwrap();
        runtime.transition(LifecycleState::Dependencies).unwrap();
        runtime.transition(LifecycleState::Capabilities).unwrap();
        runtime.transition(LifecycleState::Registering).unwrap();
        runtime.transition(LifecycleState::Ready).unwrap();
        runtime.transition(LifecycleState::Serving).unwrap();

        runtime
    }

    #[test]
    fn engine_runtime_shutdown_stops_background_work() {
        let runtime = serving_runtime();

        assert!(runtime.shutdown().unwrap());

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn configured_runtime_preserves_lifecycle_and_shutdown_behavior() {
        let runtime = EngineRuntime::with_concurrency(ConcurrencyConfig::new(1, 1).unwrap());

        assert_eq!(runtime.state(), LifecycleState::Created);
        assert!(!runtime.shutdown_token().is_cancelled());

        runtime.transition(LifecycleState::Starting).unwrap();
        runtime.transition(LifecycleState::Configuring).unwrap();
        runtime.transition(LifecycleState::Dependencies).unwrap();
        runtime.transition(LifecycleState::Capabilities).unwrap();
        runtime.transition(LifecycleState::Registering).unwrap();
        runtime.transition(LifecycleState::Ready).unwrap();
        runtime.transition(LifecycleState::Serving).unwrap();

        assert!(runtime.shutdown().unwrap());
        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn dropping_engine_runtime_cancels_and_joins_background_work() {
        let stopped = Arc::new(Mutex::new(false));
        let stopped_by_task = Arc::clone(&stopped);

        {
            let runtime = EngineRuntime::new();

            runtime
                .background_tasks()
                .spawn(move |cancellation| {
                    while !cancellation.is_cancelled() {
                        thread::yield_now();
                    }

                    *stopped_by_task.lock().unwrap() = true;
                })
                .unwrap();
        }

        assert!(*stopped.lock().unwrap());
    }

    #[test]
    fn shutdown_transitions_through_draining_when_active() {
        let runtime = serving_runtime();

        assert!(runtime.shutdown().unwrap());

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_from_draining_goes_to_stopped_after_background_shutdown() {
        let runtime = serving_runtime();

        runtime.transition(LifecycleState::Draining).unwrap();

        assert!(runtime.shutdown().unwrap());

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_is_idempotent_after_stopped() {
        let runtime = serving_runtime();

        assert!(runtime.shutdown().unwrap());
        assert!(runtime.shutdown().unwrap());

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_cleans_up_background_work_when_already_stopped() {
        let stopped = Arc::new(Mutex::new(false));
        let stopped_by_task = Arc::clone(&stopped);
        let runtime = serving_runtime();

        runtime
            .background_tasks()
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    thread::yield_now();
                }

                *stopped_by_task.lock().unwrap() = true;
            })
            .unwrap();

        runtime.transition(LifecycleState::Draining).unwrap();

        assert_eq!(
            runtime.transition(LifecycleState::Stopped),
            Err(InvalidTransition::new("Draining", "Stopped"))
        );
        assert!(!*stopped.lock().unwrap());

        assert!(runtime.shutdown().unwrap());

        assert!(*stopped.lock().unwrap());
        assert!(runtime.shutdown_token().is_cancelled());
        assert_eq!(runtime.state(), LifecycleState::Stopped);
    }

    #[test]
    fn concurrent_shutdown_callers_serialize_and_wait_for_cleanup() {
        use std::sync::mpsc;

        let runtime = Arc::new(serving_runtime());
        let (cancellation_seen_tx, cancellation_seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (first_result_tx, first_result_rx) = mpsc::channel();
        let (second_result_tx, second_result_rx) = mpsc::channel();

        runtime
            .background_tasks()
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    thread::yield_now();
                }

                cancellation_seen_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();

        let first_runtime = Arc::clone(&runtime);
        let first = thread::spawn(move || {
            first_result_tx.send(first_runtime.shutdown()).unwrap();
        });

        cancellation_seen_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first shutdown did not cancel background work");

        let second_runtime = Arc::clone(&runtime);
        let second = thread::spawn(move || {
            second_result_tx.send(second_runtime.shutdown()).unwrap();
        });

        assert!(
            second_result_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "second shutdown returned before the first shutdown completed"
        );
        assert_eq!(runtime.state(), LifecycleState::Draining);

        release_tx.send(()).unwrap();

        assert!(
            first_result_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
        );
        assert!(
            second_result_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
        );
        assert_eq!(runtime.state(), LifecycleState::Stopped);

        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    fn reentrant_shutdown_from_background_task_does_not_deadlock() {
        let runtime = Arc::new(serving_runtime());
        let (status_tx, status_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();

        let task_runtime = Arc::clone(&runtime);
        runtime
            .background_tasks()
            .spawn(move |_| {
                status_tx.send(task_runtime.shutdown()).unwrap();
                done_tx.send(()).unwrap();
            })
            .unwrap();

        assert!(
            !status_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
        );
        done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("background task deadlocked during reentrant shutdown");

        assert!(runtime.shutdown().unwrap());
        assert_eq!(runtime.state(), LifecycleState::Stopped);
    }

    #[test]
    fn shutdown_rejects_direct_stopped_transition() {
        let runtime = serving_runtime();

        assert_eq!(
            runtime.transition(LifecycleState::Stopped),
            Err(InvalidTransition::new("Serving", "Stopped"))
        );
        assert_eq!(runtime.state(), LifecycleState::Serving);
    }

    #[test]
    fn shutdown_joins_background_work_before_returning() {
        let finished = Arc::new(Mutex::new(false));
        let finished_by_task = Arc::clone(&finished);

        let runtime = serving_runtime();

        runtime
            .background_tasks()
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    thread::yield_now();
                }

                thread::sleep(Duration::from_millis(10));
                *finished_by_task.lock().unwrap() = true;
            })
            .unwrap();

        assert!(runtime.shutdown().unwrap());

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(*finished.lock().unwrap());
    }

    #[test]
    fn shutdown_from_created_rejects_invalid_draining_transition() {
        let runtime = EngineRuntime::new();

        let result = runtime.shutdown();

        assert_eq!(result, Err(InvalidTransition::new("Created", "Draining")));
        assert_eq!(runtime.state(), LifecycleState::Created);
        assert!(!runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_from_ready_rejects_invalid_draining_transition() {
        let runtime = EngineRuntime::new();

        runtime.transition(LifecycleState::Starting).unwrap();
        runtime.transition(LifecycleState::Configuring).unwrap();
        runtime.transition(LifecycleState::Dependencies).unwrap();
        runtime.transition(LifecycleState::Capabilities).unwrap();
        runtime.transition(LifecycleState::Registering).unwrap();
        runtime.transition(LifecycleState::Ready).unwrap();

        let result = runtime.shutdown();

        assert_eq!(result, Err(InvalidTransition::new("Ready", "Draining")));
        assert_eq!(runtime.state(), LifecycleState::Ready);
        assert!(!runtime.shutdown_token().is_cancelled());
    }
}
