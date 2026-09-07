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
}
