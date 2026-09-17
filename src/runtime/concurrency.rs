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

/// Configuration for bounded concurrent work admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcurrencyConfig {
    max_active: usize,
    max_queued: usize,
}

impl ConcurrencyConfig {
    /// Creates a bounded concurrency configuration.
    ///
    /// Both active and queued work limits must be greater than zero. Zero
    /// limits would prevent the corresponding class of work from ever being
    /// admitted and are therefore rejected explicitly.
    pub fn new(max_active: usize, max_queued: usize) -> Result<Self, ConcurrencyError> {
        if max_active == 0 {
            return Err(ConcurrencyError::ZeroActiveLimit);
        }

        if max_queued == 0 {
            return Err(ConcurrencyError::ZeroQueueLimit);
        }

        Ok(Self {
            max_active,
            max_queued,
        })
    }

    /// Returns the maximum number of concurrently active units of work.
    pub const fn max_active(&self) -> usize {
        self.max_active
    }

    /// Returns the maximum number of queued units of work awaiting execution.
    pub const fn max_queued(&self) -> usize {
        self.max_queued
    }
}

/// Current bounded concurrency occupancy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConcurrencyState {
    active: usize,
    queued: usize,
}

impl ConcurrencyState {
    /// Creates an empty occupancy state.
    pub const fn new() -> Self {
        Self {
            active: 0,
            queued: 0,
        }
    }

    /// Returns the number of active work units.
    pub const fn active(&self) -> usize {
        self.active
    }

    /// Returns the number of queued work units.
    pub const fn queued(&self) -> usize {
        self.queued
    }

    /// Attempts to reserve one active-work slot.
    pub fn try_acquire_active(
        &mut self,
        config: &ConcurrencyConfig,
    ) -> Result<(), ConcurrencyError> {
        if self.active >= config.max_active() {
            return Err(ConcurrencyError::ActiveLimitReached);
        }

        self.active += 1;
        Ok(())
    }

    /// Releases one active-work slot.
    pub fn release_active(&mut self) -> Result<(), ConcurrencyError> {
        if self.active == 0 {
            return Err(ConcurrencyError::ActiveUnderflow);
        }

        self.active -= 1;
        Ok(())
    }

    /// Attempts to reserve one queued-work slot.
    pub fn try_enqueue(&mut self, config: &ConcurrencyConfig) -> Result<(), ConcurrencyError> {
        if self.queued >= config.max_queued() {
            return Err(ConcurrencyError::QueueLimitReached);
        }

        self.queued += 1;
        Ok(())
    }

    /// Removes one unit of queued work and releases its queue slot.
    pub fn dequeue(&mut self) -> Result<(), ConcurrencyError> {
        if self.queued == 0 {
            return Err(ConcurrencyError::QueueUnderflow);
        }

        self.queued -= 1;
        Ok(())
    }

    /// Returns whether both occupancy counters are empty.
    pub const fn is_empty(&self) -> bool {
        self.active == 0 && self.queued == 0
    }
}

impl Default for ConcurrencyState {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors produced by bounded concurrent-work admission and accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConcurrencyError {
    /// The active-work limit cannot be zero.
    ZeroActiveLimit,
    /// The queued-work limit cannot be zero.
    ZeroQueueLimit,
    /// No additional active-work slot is available.
    ActiveLimitReached,
    /// An active-work slot was released when none was held.
    ActiveUnderflow,
    /// No additional queued-work slot is available.
    QueueLimitReached,
    /// Queued work was removed when the queue was empty.
    QueueUnderflow,
}

impl std::fmt::Display for ConcurrencyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroActiveLimit => {
                formatter.write_str("concurrency active limit must be greater than zero")
            }
            Self::ZeroQueueLimit => {
                formatter.write_str("concurrency queue limit must be greater than zero")
            }
            Self::ActiveLimitReached => {
                formatter.write_str("concurrency active-work limit is full")
            }
            Self::ActiveUnderflow => {
                formatter.write_str("concurrency active-work occupancy cannot become negative")
            }
            Self::QueueLimitReached => formatter.write_str("concurrency queued-work limit is full"),
            Self::QueueUnderflow => {
                formatter.write_str("concurrency queued-work occupancy cannot become negative")
            }
        }
    }
}

impl std::error::Error for ConcurrencyError {}

#[cfg(test)]
mod tests {
    use super::{ConcurrencyConfig, ConcurrencyError, ConcurrencyState, TaskScope};
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

        barrier.wait();

        handle_a.join().unwrap();
        handle_b.join().unwrap();

        let completed = completed.lock().unwrap();

        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&'a'));
        assert!(completed.contains(&'b'));
        assert!(!parent.is_cancelled());
    }

    #[test]
    fn concurrency_config_rejects_zero_active_limit() {
        assert_eq!(
            ConcurrencyConfig::new(0, 1),
            Err(ConcurrencyError::ZeroActiveLimit)
        );
    }

    #[test]
    fn concurrency_config_rejects_zero_queue_limit() {
        assert_eq!(
            ConcurrencyConfig::new(1, 0),
            Err(ConcurrencyError::ZeroQueueLimit)
        );
    }

    #[test]
    fn concurrency_config_preserves_limits() {
        let config = ConcurrencyConfig::new(4, 8).unwrap();

        assert_eq!(config.max_active(), 4);
        assert_eq!(config.max_queued(), 8);
    }

    #[test]
    fn concurrency_state_starts_empty() {
        let state = ConcurrencyState::new();

        assert_eq!(state.active(), 0);
        assert_eq!(state.queued(), 0);
        assert!(state.is_empty());
    }

    #[test]
    fn active_admission_is_bounded() {
        let config = ConcurrencyConfig::new(2, 2).unwrap();
        let mut state = ConcurrencyState::new();

        assert_eq!(state.try_acquire_active(&config), Ok(()));
        assert_eq!(state.try_acquire_active(&config), Ok(()));
        assert_eq!(state.active(), 2);
        assert_eq!(
            state.try_acquire_active(&config),
            Err(ConcurrencyError::ActiveLimitReached)
        );
        assert_eq!(state.active(), 2);
    }

    #[test]
    fn releasing_active_work_restores_capacity() {
        let config = ConcurrencyConfig::new(2, 2).unwrap();
        let mut state = ConcurrencyState::new();

        state.try_acquire_active(&config).unwrap();
        state.try_acquire_active(&config).unwrap();
        state.release_active().unwrap();
        state.try_acquire_active(&config).unwrap();

        assert_eq!(state.active(), 2);
    }

    #[test]
    fn active_release_from_empty_state_is_explicit() {
        let mut state = ConcurrencyState::new();

        assert_eq!(
            state.release_active(),
            Err(ConcurrencyError::ActiveUnderflow)
        );
        assert!(state.is_empty());
    }

    #[test]
    fn queue_admission_is_bounded() {
        let config = ConcurrencyConfig::new(2, 2).unwrap();
        let mut state = ConcurrencyState::new();

        assert_eq!(state.try_enqueue(&config), Ok(()));
        assert_eq!(state.try_enqueue(&config), Ok(()));
        assert_eq!(state.queued(), 2);
        assert_eq!(
            state.try_enqueue(&config),
            Err(ConcurrencyError::QueueLimitReached)
        );
        assert_eq!(state.queued(), 2);
    }

    #[test]
    fn dequeue_restores_queue_capacity() {
        let config = ConcurrencyConfig::new(2, 2).unwrap();
        let mut state = ConcurrencyState::new();

        state.try_enqueue(&config).unwrap();
        state.try_enqueue(&config).unwrap();
        state.dequeue().unwrap();
        state.try_enqueue(&config).unwrap();

        assert_eq!(state.queued(), 2);
    }

    #[test]
    fn dequeue_from_empty_queue_is_explicit() {
        let mut state = ConcurrencyState::new();

        assert_eq!(state.dequeue(), Err(ConcurrencyError::QueueUnderflow));
        assert!(state.is_empty());
    }

    #[test]
    fn occupancy_invariants_are_preserved() {
        let config = ConcurrencyConfig::new(2, 2).unwrap();
        let mut state = ConcurrencyState::new();

        state.try_acquire_active(&config).unwrap();
        state.try_enqueue(&config).unwrap();

        assert!(state.active() <= config.max_active());
        assert!(state.queued() <= config.max_queued());

        state.release_active().unwrap();
        state.dequeue().unwrap();

        assert!(state.is_empty());
    }

    #[test]
    fn errors_have_stable_display_messages() {
        assert_eq!(
            ConcurrencyError::ZeroActiveLimit.to_string(),
            "concurrency active limit must be greater than zero"
        );
        assert_eq!(
            ConcurrencyError::ZeroQueueLimit.to_string(),
            "concurrency queue limit must be greater than zero"
        );
        assert_eq!(
            ConcurrencyError::ActiveLimitReached.to_string(),
            "concurrency active-work limit is full"
        );
        assert_eq!(
            ConcurrencyError::ActiveUnderflow.to_string(),
            "concurrency active-work occupancy cannot become negative"
        );
        assert_eq!(
            ConcurrencyError::QueueLimitReached.to_string(),
            "concurrency queued-work limit is full"
        );
        assert_eq!(
            ConcurrencyError::QueueUnderflow.to_string(),
            "concurrency queued-work occupancy cannot become negative"
        );
    }
}
