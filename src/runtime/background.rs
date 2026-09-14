use std::{sync::Mutex, thread::JoinHandle};

use super::CancellationToken;

/// Owns runtime background workers and stops them through one shared
/// cancellation scope.
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

    /// Spawns and takes ownership of one runtime background task.
    ///
    /// Each task receives a child cancellation token derived from the shared
    /// runtime cancellation scope.
    ///
    /// Tasks cannot be spawned after shutdown begins.
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

    /// Signals all owned background tasks and waits for them to finish.
    ///
    /// Shutdown is idempotent. The first call closes the task collection,
    /// cancels the shared scope, and joins every owned task. Later calls do
    /// nothing.
    ///
    /// If one or more tasks panic, all owned tasks are still joined before
    /// the first panic payload is resumed.
    pub fn shutdown(&self) {
        let handles = {
            let mut state = self.state.lock().expect("background task lock poisoned");

            if state.closed {
                return;
            }

            state.closed = true;

            // Signal cancellation before taking the handles so every owned
            // task observes shutdown before we wait for it.
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

    #[cfg(test)]
    fn task_count(&self) -> usize {
        self.state
            .lock()
            .expect("background task lock poisoned")
            .handles
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::BackgroundTasks;
    use crate::runtime::CancellationToken;
    use std::{
        sync::{Arc, Barrier, Mutex},
        thread,
        time::Duration,
    };

    #[test]
    fn shutdown_cancels_and_joins_background_tasks() {
        let tasks = BackgroundTasks::new(CancellationToken::new());

        let stopped = Arc::new(Mutex::new(false));
        let stopped_by_task = Arc::clone(&stopped);

        tasks
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    thread::yield_now();
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
        assert_eq!(tasks.task_count(), 0);
    }

    #[test]
    fn shutdown_is_idempotent() {
        let tasks = BackgroundTasks::new(CancellationToken::new());

        let completed = Arc::new(Mutex::new(false));
        let completed_by_task = Arc::clone(&completed);

        tasks
            .spawn(move |cancellation| {
                while !cancellation.is_cancelled() {
                    thread::yield_now();
                }

                *completed_by_task.lock().unwrap() = true;
            })
            .unwrap();

        tasks.shutdown();
        tasks.shutdown();

        assert!(*completed.lock().unwrap());
        assert!(tasks.is_closed());
        assert_eq!(tasks.task_count(), 0);
    }

    #[test]
    fn shutdown_joins_all_background_tasks() {
        let tasks = BackgroundTasks::new(CancellationToken::new());

        let completed = Arc::new(Mutex::new(Vec::new()));
        let barrier = Arc::new(Barrier::new(3));

        for id in 0..2 {
            let completed = Arc::clone(&completed);
            let barrier = Arc::clone(&barrier);

            tasks
                .spawn(move |cancellation| {
                    barrier.wait();

                    while !cancellation.is_cancelled() {
                        thread::yield_now();
                    }

                    thread::sleep(Duration::from_millis(10));

                    completed.lock().unwrap().push(id);
                })
                .unwrap();
        }

        assert_eq!(tasks.task_count(), 2);

        // Release both workers.
        barrier.wait();

        tasks.shutdown();

        let completed = completed.lock().unwrap();

        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&0));
        assert!(completed.contains(&1));
        assert_eq!(tasks.task_count(), 0);
    }

    #[test]
    fn each_background_task_receives_an_independent_child_scope() {
        let parent = CancellationToken::new();
        let tasks = BackgroundTasks::new(parent.clone());

        let child_a = Arc::new(Mutex::new(None));
        let child_b = Arc::new(Mutex::new(None));

        let child_a_for_task = Arc::clone(&child_a);
        tasks
            .spawn(move |cancellation| {
                *child_a_for_task.lock().unwrap() = Some(cancellation);
            })
            .unwrap();

        let child_b_for_task = Arc::clone(&child_b);
        tasks
            .spawn(move |cancellation| {
                *child_b_for_task.lock().unwrap() = Some(cancellation);
            })
            .unwrap();

        tasks.shutdown();

        assert!(parent.is_cancelled());
        assert!(child_a.lock().unwrap().as_ref().unwrap().is_cancelled());
        assert!(child_b.lock().unwrap().as_ref().unwrap().is_cancelled());
    }

    #[test]
    fn shutdown_cancels_shared_scope() {
        let cancellation = CancellationToken::new();
        let tasks = BackgroundTasks::new(cancellation.clone());

        assert!(!cancellation.is_cancelled());

        tasks.shutdown();

        assert!(cancellation.is_cancelled());
        assert!(tasks.cancellation().is_cancelled());
    }

    #[test]
    fn no_tasks_can_remain_owned_after_shutdown() {
        let tasks = BackgroundTasks::new(CancellationToken::new());

        tasks.spawn(|_| {}).unwrap();
        tasks.spawn(|_| {}).unwrap();

        assert_eq!(tasks.task_count(), 2);

        tasks.shutdown();

        assert_eq!(tasks.task_count(), 0);
        assert!(tasks.is_closed());
    }
}
