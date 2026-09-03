use std::{sync::Mutex, thread::JoinHandle};

use super::CancellationToken;

/// Owns background workers and stops them through one shared cancellation scope.
#[derive(Debug)]
pub struct BackgroundTasks {
    cancellation: CancellationToken,
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl BackgroundTasks {
    pub fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            handles: Mutex::new(Vec::new()),
        }
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn spawn<F>(&self, task: F)
    where
        F: FnOnce(CancellationToken) + Send + 'static,
    {
        let token = self.cancellation.child_token();
        let handle = std::thread::spawn(move || task(token));
        self.handles
            .lock()
            .expect("background task lock poisoned")
            .push(handle);
    }

    pub fn shutdown(&self) {
        self.cancellation.cancel();
        let handles =
            std::mem::take(&mut *self.handles.lock().expect("background task lock poisoned"));
        for handle in handles {
            handle.join().expect("background task panicked");
        }
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

        tasks.spawn(move |cancellation| {
            while !cancellation.is_cancelled() {
                std::thread::yield_now();
            }
            *stopped_by_task.lock().unwrap() = true;
        });
        tasks.shutdown();

        assert!(*stopped.lock().unwrap());
    }
}
