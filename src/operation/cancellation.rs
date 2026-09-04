use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A thread safe cancellation signal that can be derived into child scopes.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    parent: Option<Arc<CancellationToken>>,
}

impl CancellationToken {
    /// Creates an active root cancellation token.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: None,
        }
    }

    /// Returns whether this token or any of its ancestors has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self
                .parent
                .as_ref()
                .is_some_and(|parent| parent.is_cancelled())
    }

    /// Cancels this token and all work observing this token.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Creates an active child token that observes this token's cancellation.
    pub fn child_token(&self) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: Some(Arc::new(self.clone())),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::CancellationToken;
    use std::thread;

    #[test]
    fn cancellation_is_idempotent() {
        let token = CancellationToken::new();

        token.cancel();
        token.cancel();

        assert!(token.is_cancelled());
    }

    #[test]
    fn parent_cancellation_reaches_children() {
        let parent = CancellationToken::new();
        let child = parent.child_token();

        parent.cancel();

        assert!(child.is_cancelled());
    }

    #[test]
    fn child_cancellation_does_not_affect_parent_or_siblings() {
        let parent = CancellationToken::new();
        let child = parent.child_token();
        let sibling = parent.child_token();

        child.cancel();

        assert!(!parent.is_cancelled());
        assert!(child.is_cancelled());
        assert!(!sibling.is_cancelled());
    }

    #[test]
    fn cancellation_is_visible_across_threads() {
        let token = CancellationToken::new();
        let worker_token = token.clone();
        let worker = thread::spawn(move || {
            while !worker_token.is_cancelled() {
                thread::yield_now();
            }
        });

        token.cancel();
        worker.join().unwrap();
    }
}
