use std::sync::Mutex;

use super::{
    CancellationToken,
    background::BackgroundTasks,
    lifecycle::{InvalidTransition, Lifecycle, LifecycleState},
};

/// Minimal lifecycle owner for an engine instance.
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
    pub fn new() -> Self {
        let shutdown = CancellationToken::new();
        Self {
            lifecycle: Mutex::new(Lifecycle::new()),
            background: BackgroundTasks::new(shutdown.clone()),
            shutdown,
        }
    }

    pub fn state(&self) -> LifecycleState {
        self.lifecycle
            .lock()
            .expect("lifecycle lock poisoned")
            .state()
    }

    pub fn transition(&self, next: LifecycleState) -> Result<(), InvalidTransition> {
        self.lifecycle
            .lock()
            .expect("lifecycle lock poisoned")
            .transition(next)
    }

    pub fn shutdown_token(&self) -> &CancellationToken {
        &self.shutdown
    }

    pub fn background_tasks(&self) -> &BackgroundTasks {
        &self.background
    }

    pub fn shutdown(&self) -> Result<(), InvalidTransition> {
        self.transition(LifecycleState::Stopped)?;
        self.background.shutdown();
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
    use crate::runtime::lifecycle::LifecycleState;

    #[test]
    fn engine_runtime_shutdown_stops_background_work() {
        let runtime = EngineRuntime::new();

        runtime.transition(LifecycleState::Starting).unwrap();
        runtime.transition(LifecycleState::Ready).unwrap();
        runtime.shutdown().unwrap();

        assert_eq!(runtime.state(), LifecycleState::Stopped);
        assert!(runtime.shutdown_token().is_cancelled());
    }

    #[test]
    fn dropping_engine_runtime_cancels_and_joins_background_work() {
        let stopped = std::sync::Arc::new(std::sync::Mutex::new(false));
        let stopped_by_task = stopped.clone();
        {
            let runtime = EngineRuntime::new();
            runtime
                .background_tasks()
                .spawn(move |cancellation| {
                    while !cancellation.is_cancelled() {
                        std::thread::yield_now();
                    }
                    *stopped_by_task.lock().unwrap() = true;
                })
                .unwrap();
        }

        assert!(*stopped.lock().unwrap());
    }
}
