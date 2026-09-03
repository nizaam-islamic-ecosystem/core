use std::{sync::Mutex, thread::JoinHandle};

use super::CancellationToken;

/// Owns background workers and stops them through one shared cancellation scope.
#[derive(Debug)]
pub struct BackgroundTasks {
    cancellation: CancellationToken,
    state: Mutex<BackgroundState>,
}

#[derive(Debug, Default)]
struct BackgroundState {
    closed: bool,
    handles: Vec<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnError;

impl BackgroundTasks {
    pub fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            state: Mutex::new(BackgroundState::default()),
        }
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn spawn<F>(&self, task: F) -> Result<(), SpawnError>
    where
        F: FnOnce(CancellationToken) + Send + 'static,
    {
        let mut state = self.state.lock().expect("background task lock poisoned");
        if state.closed {
            return Err(SpawnError);
        }
        let token = self.cancellation.child_token();
        let handle = std::thread::spawn(move || task(token));
        state.handles.push(handle);
        Ok(())
    }

    pub fn shutdown(&self) {
        let handles = {
            let mut state = self.state.lock().expect("background task lock poisoned");
            if state.closed {
                return;
            }
            state.closed = true;
            self.cancellation.cancel();
            std::mem::take(&mut state.handles)
        };
        let mut panic_payload = None;
        for handle in handles {
            if let Err(payload) = handle.join() {
                panic_payload.get_or_insert(payload);
            }
        }
        if let Some(payload) = panic_payload {
            std::panic::resume_unwind(payload);
        }
    }

    #[cfg(test)]
    fn is_closed(&self) -> bool {
        self.state
            .lock()
            .expect("background task lock poisoned")
            .closed
    }
}

#[cfg(test)]
mod tests {
    use super::BackgroundTasks;
    use crate::runtime::CancellationToken;
    use std::sync::{Arc, Mutex};

    #[test]
    fn shutdown_cancels_and_joins_background_tasks() {
        let tasks = BackgroundTasks::new(CancellationToken::new());
        let stopped = Arc::new(Mutex::new(false));
        let stopped_by_task = stopped.clone();

        tasks
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    std::thread::yield_now();
                }
                *stopped_by_task.lock().unwrap() = true;
            })
            .unwrap();
        tasks.shutdown();

        assert!(*stopped.lock().unwrap());
    }

    #[test]
    fn shutdown_rejects_late_tasks() {
        let tasks = BackgroundTasks::new(CancellationToken::new());
        tasks.shutdown();

        assert_eq!(tasks.spawn(|_| {}), Err(super::SpawnError));
        assert!(tasks.is_closed());
    }
}
