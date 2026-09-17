//! Bounded retry backoff and randomized jitter calculation.
//!
//! This module computes how long execution should wait before a subsequent
//! retry. It does not decide whether a retry is permitted, create attempts,
//! perform asynchronous sleeping, or own cancellation, deadlines, resource
//! admission, or idempotency semantics.

use std::time::Duration;

/// Jitter strategy applied after the exponential backoff has been bounded by
/// the configured maximum delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitterPolicy {
    /// Do not randomize the calculated delay.
    None,
    /// Choose a delay uniformly across the range from zero to the bounded
    /// delay, inclusive at the conceptual boundary.
    Full,
    /// Keep at least half of the bounded delay and randomize the remaining
    /// half.
    Equal,
}

/// Supplies randomness to a [`BackoffPolicy`] when jitter is enabled.
///
/// The retry subsystem does not own a global random-number generator. Runtime
/// or test code supplies a source explicitly, allowing production randomness
/// and deterministic tests to use the same backoff implementation.
pub trait JitterSource {
    /// Returns the next uniformly distributed 64-bit sample from the source.
    fn next_u64(&mut self) -> u64;
}

/// Error returned when a backoff policy cannot be constructed safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackoffPolicyError {
    /// The configured base delay cannot exceed the configured maximum delay.
    BaseDelayExceedsMaximum,
}

impl std::fmt::Display for BackoffPolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseDelayExceedsMaximum => {
                formatter.write_str("base delay must not exceed maximum delay")
            }
        }
    }
}

impl std::error::Error for BackoffPolicyError {}

/// Error returned while calculating a retry delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackoffError {
    /// Retry numbering starts at one because the first retry follows the
    /// original attempt.
    InvalidRetryNumber,
    /// A jitter strategy requiring randomness was selected without supplying
    /// a randomness source.
    JitterSourceRequired,
}

impl std::fmt::Display for BackoffError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRetryNumber => {
                formatter.write_str("retry number must be greater than zero")
            }
            Self::JitterSourceRequired => {
                formatter.write_str("a jitter source is required when jitter is enabled")
            }
        }
    }
}

impl std::error::Error for BackoffError {}

/// Immutable retry timing configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackoffPolicy {
    base_delay: Duration,
    max_delay: Duration,
    jitter: JitterPolicy,
}

impl BackoffPolicy {
    /// Creates a bounded exponential backoff policy.
    ///
    /// The first retry uses the configured base delay. Each subsequent retry
    /// doubles the delay until `max_delay` is reached. Jitter is applied only
    /// after that maximum-delay bound has been enforced.
    pub fn new(
        base_delay: Duration,
        max_delay: Duration,
        jitter: JitterPolicy,
    ) -> Result<Self, BackoffPolicyError> {
        if base_delay > max_delay {
            return Err(BackoffPolicyError::BaseDelayExceedsMaximum);
        }

        Ok(Self {
            base_delay,
            max_delay,
            jitter,
        })
    }

    /// Creates an explicit no-backoff, no-jitter policy.
    pub const fn no_backoff() -> Self {
        Self {
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            jitter: JitterPolicy::None,
        }
    }

    /// Returns the configured base delay.
    pub const fn base_delay(&self) -> Duration {
        self.base_delay
    }

    /// Returns the configured maximum delay.
    pub const fn max_delay(&self) -> Duration {
        self.max_delay
    }

    /// Returns the configured jitter strategy.
    pub const fn jitter(&self) -> JitterPolicy {
        self.jitter
    }

    /// Calculates the delay before the specified one-based retry.
    ///
    /// `retry_number = 1` refers to the delay before creating the second
    /// attempt, `retry_number = 2` to the delay before the third attempt, and
    /// so on.
    pub fn delay(
        &self,
        retry_number: u32,
        mut jitter_source: Option<&mut dyn JitterSource>,
    ) -> Result<Duration, BackoffError> {
        if retry_number == 0 {
            return Err(BackoffError::InvalidRetryNumber);
        }

        let bounded_delay = self.exponential_delay(retry_number);

        match self.jitter {
            JitterPolicy::None => Ok(bounded_delay),
            JitterPolicy::Full => {
                let source = jitter_source
                    .as_deref_mut()
                    .ok_or(BackoffError::JitterSourceRequired)?;
                Ok(jitter_full(bounded_delay, source.next_u64()))
            }
            JitterPolicy::Equal => {
                let source = jitter_source.ok_or(BackoffError::JitterSourceRequired)?;
                Ok(jitter_equal(bounded_delay, source.next_u64()))
            }
        }
    }

    /// Calculates the exponentially increasing delay and caps it at the
    /// configured maximum without overflowing or iterating through an
    /// unbounded retry count.
    fn exponential_delay(&self, retry_number: u32) -> Duration {
        if self.base_delay.is_zero() || self.base_delay >= self.max_delay {
            return self.base_delay;
        }

        let mut delay = self.base_delay;
        let mut remaining_doublings = retry_number.saturating_sub(1);

        while remaining_doublings > 0 {
            if delay >= self.max_delay {
                break;
            }

            let doubled = delay.saturating_add(delay);
            delay = doubled.min(self.max_delay);
            remaining_doublings -= 1;
        }

        delay.min(self.max_delay)
    }
}

fn jitter_full(delay: Duration, sample: u64) -> Duration {
    duration_from_nanos(random_inclusive(delay.as_nanos(), sample))
}

fn jitter_equal(delay: Duration, sample: u64) -> Duration {
    let delay_nanos = delay.as_nanos();
    let lower_bound = delay_nanos / 2;
    let span = delay_nanos.saturating_sub(lower_bound);
    let offset = random_inclusive(span, sample);

    duration_from_nanos(lower_bound.saturating_add(offset))
}

/// Maps a 64-bit random sample into the inclusive range `0..=upper_bound`.
///
/// The calculation avoids multiplying the full `u128` upper bound by the
/// `u64` sample directly, because a maximum [`Duration`] can require more
/// than 128 bits for that intermediate product.
fn random_inclusive(upper_bound: u128, sample: u64) -> u128 {
    if upper_bound == 0 || sample == 0 {
        return 0;
    }

    let denominator = u64::MAX as u128;
    let sample = sample as u128;
    let quotient = upper_bound / denominator;
    let remainder = upper_bound % denominator;

    quotient * sample + (remainder * sample) / denominator
}

fn duration_from_nanos(nanos: u128) -> Duration {
    let seconds = nanos / 1_000_000_000;
    let subsec_nanos = (nanos % 1_000_000_000) as u32;

    if seconds > u64::MAX as u128 {
        Duration::MAX
    } else {
        Duration::new(seconds as u64, subsec_nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct FixedJitterSource {
        sample: u64,
    }

    impl JitterSource for FixedJitterSource {
        fn next_u64(&mut self) -> u64 {
            self.sample
        }
    }

    fn milliseconds(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn policy() -> BackoffPolicy {
        BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::None).unwrap()
    }

    #[test]
    fn creates_valid_policy() {
        let backoff = policy();

        assert_eq!(backoff.base_delay(), milliseconds(100));
        assert_eq!(backoff.max_delay(), milliseconds(1_000));
        assert_eq!(backoff.jitter(), JitterPolicy::None);
    }

    #[test]
    fn rejects_base_delay_above_maximum() {
        let result =
            BackoffPolicy::new(milliseconds(1_001), milliseconds(1_000), JitterPolicy::None);

        assert_eq!(result, Err(BackoffPolicyError::BaseDelayExceedsMaximum));
    }

    #[test]
    fn allows_zero_delays() {
        let result = BackoffPolicy::new(Duration::ZERO, Duration::ZERO, JitterPolicy::None);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().delay(1, None).unwrap(), Duration::ZERO);
    }

    #[test]
    fn no_backoff_is_explicit_and_zero() {
        let backoff = BackoffPolicy::no_backoff();

        assert_eq!(backoff.base_delay(), Duration::ZERO);
        assert_eq!(backoff.max_delay(), Duration::ZERO);
        assert_eq!(backoff.jitter(), JitterPolicy::None);
        assert_eq!(backoff.delay(1, None).unwrap(), Duration::ZERO);
    }

    #[test]
    fn rejects_zero_retry_number() {
        assert_eq!(
            policy().delay(0, None),
            Err(BackoffError::InvalidRetryNumber)
        );
    }

    #[test]
    fn first_retry_uses_base_delay() {
        assert_eq!(policy().delay(1, None).unwrap(), milliseconds(100));
    }

    #[test]
    fn subsequent_retries_double_exponentially() {
        let backoff = policy();

        assert_eq!(backoff.delay(1, None).unwrap(), milliseconds(100));
        assert_eq!(backoff.delay(2, None).unwrap(), milliseconds(200));
        assert_eq!(backoff.delay(3, None).unwrap(), milliseconds(400));
        assert_eq!(backoff.delay(4, None).unwrap(), milliseconds(800));
    }

    #[test]
    fn delay_is_capped_at_maximum() {
        let backoff = policy();

        assert_eq!(backoff.delay(5, None).unwrap(), milliseconds(1_000));
        assert_eq!(backoff.delay(6, None).unwrap(), milliseconds(1_000));
    }

    #[test]
    fn very_large_retry_number_remains_bounded() {
        let backoff = policy();

        assert_eq!(backoff.delay(u32::MAX, None).unwrap(), milliseconds(1_000));
    }

    #[test]
    fn none_jitter_does_not_require_source() {
        assert_eq!(policy().delay(3, None).unwrap(), milliseconds(400));
    }

    #[test]
    fn enabled_jitter_requires_source() {
        let full =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let equal = BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Equal)
            .unwrap();

        assert_eq!(full.delay(1, None), Err(BackoffError::JitterSourceRequired));
        assert_eq!(
            equal.delay(1, None),
            Err(BackoffError::JitterSourceRequired)
        );
    }

    #[test]
    fn full_jitter_zero_sample_reaches_lower_bound() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let mut source = FixedJitterSource { sample: 0 };

        assert_eq!(backoff.delay(3, Some(&mut source)).unwrap(), Duration::ZERO);
    }

    #[test]
    fn full_jitter_max_sample_reaches_upper_bound() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let mut source = FixedJitterSource { sample: u64::MAX };

        assert_eq!(
            backoff.delay(3, Some(&mut source)).unwrap(),
            milliseconds(400)
        );
    }

    #[test]
    fn full_jitter_stays_within_bounded_delay() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let samples = [0, 1, u64::MAX / 2, u64::MAX - 1, u64::MAX];

        for sample in samples {
            let mut source = FixedJitterSource { sample };
            let delay = backoff.delay(5, Some(&mut source)).unwrap();
            assert!(delay <= milliseconds(1_000));
        }
    }

    #[test]
    fn equal_jitter_preserves_half_of_delay() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Equal)
                .unwrap();

        let mut lower_source = FixedJitterSource { sample: 0 };
        let mut upper_source = FixedJitterSource { sample: u64::MAX };

        assert_eq!(
            backoff.delay(3, Some(&mut lower_source)).unwrap(),
            milliseconds(200)
        );
        assert_eq!(
            backoff.delay(3, Some(&mut upper_source)).unwrap(),
            milliseconds(400)
        );
    }

    #[test]
    fn equal_jitter_stays_between_half_and_full_delay() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Equal)
                .unwrap();
        let samples = [0, 1, u64::MAX / 2, u64::MAX - 1, u64::MAX];

        for sample in samples {
            let mut source = FixedJitterSource { sample };
            let delay = backoff.delay(4, Some(&mut source)).unwrap();
            assert!(delay >= milliseconds(400));
            assert!(delay <= milliseconds(800));
        }
    }

    #[test]
    fn jitter_is_applied_after_maximum_delay_is_enforced() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(500), JitterPolicy::Equal).unwrap();
        let mut source = FixedJitterSource { sample: u64::MAX };

        assert_eq!(
            backoff.delay(10, Some(&mut source)).unwrap(),
            milliseconds(500)
        );
    }

    #[test]
    fn zero_base_delay_remains_zero() {
        let backoff =
            BackoffPolicy::new(Duration::ZERO, milliseconds(1_000), JitterPolicy::None).unwrap();

        assert_eq!(backoff.delay(1, None).unwrap(), Duration::ZERO);
        assert_eq!(backoff.delay(100, None).unwrap(), Duration::ZERO);
    }

    #[test]
    fn deterministic_source_produces_repeatable_jitter() {
        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let mut first_source = FixedJitterSource { sample: 42 };
        let mut second_source = FixedJitterSource { sample: 42 };

        let first = backoff.delay(4, Some(&mut first_source)).unwrap();
        let second = backoff.delay(4, Some(&mut second_source)).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn source_is_consumed_once_per_calculation() {
        #[derive(Debug)]
        struct CountingSource {
            calls: u32,
        }

        impl JitterSource for CountingSource {
            fn next_u64(&mut self) -> u64 {
                self.calls += 1;
                0
            }
        }

        let backoff =
            BackoffPolicy::new(milliseconds(100), milliseconds(1_000), JitterPolicy::Full).unwrap();
        let mut source = CountingSource { calls: 0 };

        let _ = backoff.delay(1, Some(&mut source)).unwrap();
        assert_eq!(source.calls, 1);
    }

    #[test]
    fn duration_conversion_preserves_subsecond_precision() {
        let delay = Duration::new(2, 345_678_901);

        assert_eq!(duration_from_nanos(delay.as_nanos()), delay);
    }
}
