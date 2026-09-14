use super::CancellationToken;

/// Cancellation scope used by one unit of concurrent work.
#[derive(Clone, Debug)]
pub struct TaskScope {
    cancellation: CancellationToken,
}

impl TaskScope {
    pub fn new(parent: &CancellationToken) -> Self {
        Self {
            cancellation: parent.child_token(),
        }
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::TaskScope;
    use crate::runtime::CancellationToken;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn task_scope_starts_uncancelled() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);

        assert!(!scope.is_cancelled());
    }

    #[test]
    fn task_scope_cancellation_isolated_from_parent() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);

        scope.cancel();

        assert!(scope.is_cancelled());
        assert!(!parent.is_cancelled());
    }

    #[test]
    fn parent_cancellation_reaches_task_scope() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);

        parent.cancel();

        assert!(scope.is_cancelled());
    }

    #[test]
    fn task_scope_supports_clone_and_distinct_cancellation_views() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);
        let clone = scope.clone();

        scope.cancel();

        assert!(scope.is_cancelled());
        assert!(clone.is_cancelled());
    }

    #[test]
    fn task_scope_exposes_child_cancellation_token() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);

        scope.cancellation().cancel();

        assert!(scope.is_cancelled());
    }

    #[test]
    fn sibling_task_scopes_remain_independent() {
        let parent = CancellationToken::new();
        let scope_a = TaskScope::new(&parent);
        let scope_b = TaskScope::new(&parent);

        scope_a.cancel();

        assert!(scope_a.is_cancelled());
        assert!(!scope_b.is_cancelled());
        assert!(!parent.is_cancelled());
    }

    #[test]
    fn task_scope_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<TaskScope>();
    }

    #[test]
    fn independent_task_scopes_can_execute_concurrently() {
        let parent = CancellationToken::new();

        let scope_a = TaskScope::new(&parent);
        let scope_b = TaskScope::new(&parent);

        let barrier = Arc::new(Barrier::new(3));
        let completed = Arc::new(Mutex::new(Vec::new()));

        let barrier_a = Arc::clone(&barrier);
        let completed_a = Arc::clone(&completed);
        let cancellation_a = scope_a.cancellation().clone();

        let handle_a = thread::spawn(move || {
            barrier_a.wait();

            assert!(!cancellation_a.is_cancelled());

            thread::sleep(Duration::from_millis(20));

            completed_a.lock().unwrap().push('a');
        });

        let barrier_b = Arc::clone(&barrier);
        let completed_b = Arc::clone(&completed);
        let cancellation_b = scope_b.cancellation().clone();

        let handle_b = thread::spawn(move || {
            barrier_b.wait();

            assert!(!cancellation_b.is_cancelled());

            thread::sleep(Duration::from_millis(20));

            completed_b.lock().unwrap().push('b');
        });

        // Release both workers at the same time.
        barrier.wait();

        handle_a.join().unwrap();
        handle_b.join().unwrap();

        let completed = completed.lock().unwrap();

        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&'a'));
        assert!(completed.contains(&'b'));
        assert!(!parent.is_cancelled());
    }
}
