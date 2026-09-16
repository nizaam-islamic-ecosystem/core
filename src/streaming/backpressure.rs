//! Bounded stream-buffer capacity and backpressure policy.
//!
//! This module models bounded resource pressure for an application-level
//! stream without owning the stream queue, lifecycle, cancellation, or
//! scheduling behavior. The higher-level stream implementation decides how a
//! full buffer is handled according to the configured policy.

/// Defines the action permitted when a bounded stream buffer is full.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackpressurePolicy {
    /// The producer waits until capacity becomes available.
    Wait,
    /// The publication attempt is rejected while the buffer is full.
    Reject,
}

/// Configuration for a bounded stream buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackpressureConfig {
    capacity: usize,
    policy: BackpressurePolicy,
}

impl BackpressureConfig {
    /// Creates a bounded backpressure configuration.
    ///
    /// A stream buffer must have at least one slot. Zero-capacity rendezvous
    /// semantics are intentionally outside this abstraction.
    pub fn new(capacity: usize, policy: BackpressurePolicy) -> Result<Self, BackpressureError> {
        if capacity == 0 {
            return Err(BackpressureError::ZeroCapacity);
        }

        Ok(Self { capacity, policy })
    }

    /// Returns the configured maximum number of buffered logical items.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the behavior to use when the buffer is full.
    pub const fn policy(&self) -> BackpressurePolicy {
        self.policy
    }
}

/// Reports the bounded state of a stream buffer.
///
/// This type tracks occupancy only. It deliberately does not store the
/// buffered items themselves; queue storage belongs to the stream
/// implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackpressureState {
    capacity: usize,
    len: usize,
}

impl BackpressureState {
    /// Creates an empty state with the supplied positive capacity.
    pub fn new(capacity: usize) -> Result<Self, BackpressureError> {
        if capacity == 0 {
            return Err(BackpressureError::ZeroCapacity);
        }

        Ok(Self { capacity, len: 0 })
    }

    /// Returns the maximum number of buffered logical items.
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the current number of buffered logical items.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the bounded buffer contains no buffered logical items.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the number of additional logical items that can be accepted.
    pub const fn available(&self) -> usize {
        self.capacity - self.len
    }

    /// Returns whether the bounded buffer has reached capacity.
    pub const fn is_full(&self) -> bool {
        self.len == self.capacity
    }

    /// Attempts to account for one newly accepted buffered item.
    ///
    /// The actual item must only be inserted by the caller after this
    /// operation succeeds.
    pub fn push(&mut self) -> Result<(), BackpressureError> {
        if self.is_full() {
            return Err(BackpressureError::Full);
        }

        self.len += 1;
        Ok(())
    }

    /// Releases one buffered-item slot.
    ///
    /// The actual item must be removed by the caller before or together with
    /// this operation.
    pub fn pop(&mut self) -> Result<(), BackpressureError> {
        if self.len == 0 {
            return Err(BackpressureError::Underflow);
        }

        self.len -= 1;
        Ok(())
    }
}

/// Errors produced while creating or accounting for bounded capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackpressureError {
    /// A bounded buffer cannot be created with zero capacity.
    ZeroCapacity,
    /// A publication attempt was made while the buffer was already full.
    Full,
    /// Capacity was released while the buffer contained no items.
    Underflow,
}

impl std::fmt::Display for BackpressureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => {
                formatter.write_str("backpressure capacity must be greater than zero")
            }
            Self::Full => formatter.write_str("backpressure capacity is full"),
            Self::Underflow => formatter.write_str("backpressure occupancy cannot become negative"),
        }
    }
}

impl std::error::Error for BackpressureError {}

#[cfg(test)]
mod tests {
    use super::{BackpressureConfig, BackpressureError, BackpressurePolicy, BackpressureState};

    #[test]
    fn configuration_rejects_zero_capacity() {
        assert_eq!(
            BackpressureConfig::new(0, BackpressurePolicy::Wait),
            Err(BackpressureError::ZeroCapacity)
        );
    }

    #[test]
    fn state_rejects_zero_capacity() {
        assert_eq!(
            BackpressureState::new(0),
            Err(BackpressureError::ZeroCapacity)
        );
    }

    #[test]
    fn configuration_preserves_capacity_and_policy() {
        let config = BackpressureConfig::new(4, BackpressurePolicy::Reject).unwrap();

        assert_eq!(config.capacity(), 4);
        assert_eq!(config.policy(), BackpressurePolicy::Reject);
    }

    #[test]
    fn new_state_starts_empty() {
        let state = BackpressureState::new(3).unwrap();

        assert_eq!(state.capacity(), 3);
        assert_eq!(state.len(), 0);
        assert!(state.is_empty());
        assert_eq!(state.available(), 3);
        assert!(!state.is_full());
    }

    #[test]
    fn push_increases_occupancy_until_full() {
        let mut state = BackpressureState::new(3).unwrap();

        assert_eq!(state.push(), Ok(()));
        assert_eq!(state.len(), 1);
        assert!(!state.is_empty());
        assert_eq!(state.available(), 2);

        assert_eq!(state.push(), Ok(()));
        assert_eq!(state.len(), 2);
        assert_eq!(state.available(), 1);

        assert_eq!(state.push(), Ok(()));
        assert_eq!(state.len(), 3);
        assert_eq!(state.available(), 0);
        assert!(state.is_full());
    }

    #[test]
    fn push_when_full_is_explicit_and_does_not_change_state() {
        let mut state = BackpressureState::new(2).unwrap();

        state.push().unwrap();
        state.push().unwrap();

        assert_eq!(state.push(), Err(BackpressureError::Full));
        assert_eq!(state.len(), 2);
        assert_eq!(state.available(), 0);
        assert!(state.is_full());
    }

    #[test]
    fn pop_releases_capacity() {
        let mut state = BackpressureState::new(3).unwrap();

        state.push().unwrap();
        state.push().unwrap();
        state.push().unwrap();

        assert_eq!(state.pop(), Ok(()));
        assert_eq!(state.len(), 2);
        assert_eq!(state.available(), 1);
        assert!(!state.is_full());
    }

    #[test]
    fn released_capacity_can_be_reused() {
        let mut state = BackpressureState::new(2).unwrap();

        state.push().unwrap();
        state.push().unwrap();
        state.pop().unwrap();

        assert_eq!(state.push(), Ok(()));
        assert_eq!(state.len(), 2);
        assert!(state.is_full());
    }

    #[test]
    fn pop_from_empty_state_is_explicit_and_does_not_change_state() {
        let mut state = BackpressureState::new(2).unwrap();

        assert_eq!(state.pop(), Err(BackpressureError::Underflow));
        assert_eq!(state.len(), 0);
        assert_eq!(state.available(), 2);
        assert!(!state.is_full());
    }

    #[test]
    fn capacity_invariant_is_preserved_across_operations() {
        let mut state = BackpressureState::new(2).unwrap();

        assert_eq!(state.len(), 0);
        state.push().unwrap();
        assert!((0..=state.capacity()).contains(&state.len()));

        state.push().unwrap();
        assert!((0..=state.capacity()).contains(&state.len()));

        assert_eq!(state.push(), Err(BackpressureError::Full));
        assert!((0..=state.capacity()).contains(&state.len()));

        state.pop().unwrap();
        assert!((0..=state.capacity()).contains(&state.len()));

        state.pop().unwrap();
        assert!((0..=state.capacity()).contains(&state.len()));

        assert_eq!(state.pop(), Err(BackpressureError::Underflow));
        assert!((0..=state.capacity()).contains(&state.len()));
    }

    #[test]
    fn policies_are_distinct() {
        assert_ne!(BackpressurePolicy::Wait, BackpressurePolicy::Reject);
    }

    #[test]
    fn errors_have_stable_display_messages() {
        assert_eq!(
            BackpressureError::ZeroCapacity.to_string(),
            "backpressure capacity must be greater than zero"
        );
        assert_eq!(
            BackpressureError::Full.to_string(),
            "backpressure capacity is full"
        );
        assert_eq!(
            BackpressureError::Underflow.to_string(),
            "backpressure occupancy cannot become negative"
        );
    }
}
