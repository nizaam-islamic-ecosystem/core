use std::sync::Mutex;

use super::{
    CancellationToken,
    background::BackgroundTasks,
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
    pub fn transition(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
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
    pub fn shutdown(&self) -> Result<(), InvalidTransition> {
        let state = self.state();

        if state == LifecycleState::Stopped {
            return Ok(());
        }

        if state != LifecycleState::Draining {
            self.transition(LifecycleState::Draining)?;
        }

        // `BackgroundTasks::shutdown` cancels runtime-owned background work
        // through the shared shutdown token and joins all owned task handles.
        self.background.shutdown();

        // `STOPPED` is reached only after runtime-owned background work has
        // been signalled and joined.
        self.transition(LifecycleState::Stopped)?;

        Ok(())
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

        runtime.shutdown().unwrap();

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

        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_from_draining_goes_to_stopped_after_background_shutdown() {
        let runtime = serving_runtime();

        runtime.transition(LifecycleState::Draining).unwrap();

        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn shutdown_is_idempotent_after_stopped() {
        let runtime = serving_runtime();

        runtime.shutdown().unwrap();
        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
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

        runtime.shutdown().unwrap();

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
