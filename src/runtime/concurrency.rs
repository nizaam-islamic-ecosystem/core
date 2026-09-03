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
    fn task_scope_cancellation_isolated_from_parent() {
        let parent = CancellationToken::new();
        let scope = TaskScope::new(&parent);

        scope.cancel();

        assert!(scope.is_cancelled());
        assert!(!parent.is_cancelled());
    }
}
