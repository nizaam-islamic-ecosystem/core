use std::time::{Duration, Instant};

/// An absolute execution deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Deadline(Instant);

impl Deadline {
    /// Creates a deadline relative to the current instant.
    pub fn from_now(duration: Duration) -> Option<Self> {
        Instant::now().checked_add(duration).map(Self)
    }

    /// Creates a deadline at an absolute instant.
    pub fn at(instant: Instant) -> Self {
        Self(instant)
    }

    /// Returns whether the deadline has passed.
    pub fn is_expired(self) -> bool {
        Instant::now() >= self.0
    }

    /// Returns the time remaining, or zero when the deadline has passed.
    pub fn remaining(self) -> Duration {
        self.0.saturating_duration_since(Instant::now())
    }

    /// Keeps the earlier of two deadlines.
    pub fn min_with(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }
}

#[cfg(test)]
mod tests {
    use super::Deadline;
    use std::{thread, time::Duration};

    #[test]
    fn deadline_reports_remaining_time_and_expiration() {
        let deadline = Deadline::from_now(Duration::from_millis(20)).unwrap();

        assert!(!deadline.is_expired());
        assert!(deadline.remaining() <= Duration::from_millis(20));

        thread::sleep(Duration::from_millis(25));

        assert!(deadline.is_expired());
        assert_eq!(deadline.remaining(), Duration::ZERO);
    }

    #[test]
    fn deadline_rejects_unrepresentable_relative_duration() {
        assert!(Deadline::from_now(Duration::MAX).is_none());
    }

    #[test]
    fn earlier_deadline_limits_a_later_deadline() {
        let earlier = Deadline::from_now(Duration::from_millis(10)).unwrap();
        let later = Deadline::from_now(Duration::from_secs(1)).unwrap();

        assert_eq!(earlier.min_with(later), earlier);
    }
}
