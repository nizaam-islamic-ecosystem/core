//! Provider-neutral operational metrics for Core observability.
//!
//! This module owns metric definitions, bounded dimensions, and concurrent
//! in-process recording. It does not select or implement a metrics backend.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Maximum number of dimensions attached to one metric sample.
pub const MAX_METRIC_DIMENSIONS: usize = 8;

/// Maximum length of a metric name.
pub const MAX_METRIC_NAME_LENGTH: usize = 128;

/// Maximum length of a metric dimension key or value.
pub const MAX_METRIC_DIMENSION_LENGTH: usize = 256;

/// Errors produced by the Core metric mechanism.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetricError {
    /// The metric name is empty, whitespace-only, or too long.
    InvalidName,

    /// A metric dimension key or value is empty or too long.
    InvalidDimension,

    /// The metric contains more dimensions than the Core bound permits.
    DimensionLimitExceeded,

    /// The same metric name and dimensions were used with incompatible kinds.
    KindMismatch {
        expected: MetricKind,
        actual: MetricKind,
    },

    /// A counter update would exceed its supported range.
    CounterOverflow,

    /// A gauge or histogram observation is not finite.
    NonFiniteValue,

    /// The internal recorder lock could not be acquired.
    LockPoisoned,
}

impl std::fmt::Display for MetricError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => write!(formatter, "metric name is invalid"),
            Self::InvalidDimension => write!(formatter, "metric dimension is invalid"),
            Self::DimensionLimitExceeded => write!(
                formatter,
                "metric dimension limit of {MAX_METRIC_DIMENSIONS} was exceeded"
            ),
            Self::KindMismatch { expected, actual } => write!(
                formatter,
                "metric kind mismatch: expected {expected:?}, received {actual:?}"
            ),
            Self::CounterOverflow => write!(formatter, "metric counter overflowed"),
            Self::NonFiniteValue => write!(formatter, "metric value must be finite"),
            Self::LockPoisoned => write!(formatter, "metric recorder lock is poisoned"),
        }
    }
}

impl std::error::Error for MetricError {}

/// Stable identity for a metric.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MetricName(String);

impl MetricName {
    /// Creates a validated metric name without normalizing it.
    pub fn new(value: impl Into<String>) -> Result<Self, MetricError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > MAX_METRIC_NAME_LENGTH {
            return Err(MetricError::InvalidName);
        }
        Ok(Self(value))
    }

    /// Returns the metric name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MetricName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Semantic type of a metric series.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricKind {
    /// Monotonically increasing measurement.
    Counter,

    /// Current point-in-time measurement.
    Gauge,

    /// Aggregated observations such as latency or size.
    Histogram,
}

/// Controlled dimensions attached to one metric series.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct MetricDimensions(BTreeMap<String, String>);

impl MetricDimensions {
    /// Creates an empty dimension set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one dimension.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), MetricError> {
        let key = key.into();
        let value = value.into();

        if key.trim().is_empty()
            || value.trim().is_empty()
            || key.len() > MAX_METRIC_DIMENSION_LENGTH
            || value.len() > MAX_METRIC_DIMENSION_LENGTH
        {
            return Err(MetricError::InvalidDimension);
        }

        if !self.0.contains_key(&key) && self.0.len() >= MAX_METRIC_DIMENSIONS {
            return Err(MetricError::DimensionLimitExceeded);
        }

        self.0.insert(key, value);
        Ok(())
    }

    /// Returns the value associated with a dimension key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Returns the number of dimensions.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether there are no dimensions.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over dimension pairs in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// Identifies one metric series before its mutable value is recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetricDescriptor {
    name: MetricName,
    kind: MetricKind,
}

impl MetricDescriptor {
    /// Creates a descriptor from a validated name and metric kind.
    pub fn new(name: MetricName, kind: MetricKind) -> Self {
        Self { name, kind }
    }

    /// Returns the metric name.
    pub fn name(&self) -> &MetricName {
        &self.name
    }

    /// Returns the metric kind.
    pub fn kind(&self) -> MetricKind {
        self.kind
    }
}

/// The observed state of one metric series.
#[derive(Clone, Debug, PartialEq)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram { count: u64, sum: f64 },
}

/// Immutable metric snapshot suitable for inspection or later export.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricSnapshot {
    descriptor: MetricDescriptor,
    dimensions: MetricDimensions,
    value: MetricValue,
}

impl MetricSnapshot {
    /// Returns the metric descriptor.
    pub fn descriptor(&self) -> &MetricDescriptor {
        &self.descriptor
    }

    /// Returns the metric dimensions.
    pub fn dimensions(&self) -> &MetricDimensions {
        &self.dimensions
    }

    /// Returns the recorded value.
    pub fn value(&self) -> &MetricValue {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct MetricKey {
    name: MetricName,
    dimensions: MetricDimensions,
}

#[derive(Clone, Debug, PartialEq)]
enum RecordedMetric {
    Counter(u64),
    Gauge(f64),
    Histogram { count: u64, sum: f64 },
}

impl RecordedMetric {
    fn kind(&self) -> MetricKind {
        match self {
            Self::Counter(_) => MetricKind::Counter,
            Self::Gauge(_) => MetricKind::Gauge,
            Self::Histogram { .. } => MetricKind::Histogram,
        }
    }

    fn public_value(&self) -> MetricValue {
        match self {
            Self::Counter(value) => MetricValue::Counter(*value),
            Self::Gauge(value) => MetricValue::Gauge(*value),
            Self::Histogram { count, sum } => MetricValue::Histogram {
                count: *count,
                sum: *sum,
            },
        }
    }
}

/// Thread-safe in-process recorder for provider-neutral metrics.
#[derive(Clone, Default)]
pub struct MetricRecorder {
    series: Arc<Mutex<BTreeMap<MetricKey, RecordedMetric>>>,
}

impl MetricRecorder {
    /// Creates an empty metric recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments a counter by `amount` for the supplied metric series.
    pub fn increment_counter(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        amount: u64,
    ) -> Result<(), MetricError> {
        self.with_series(descriptor, dimensions, |series| match series {
            Some(RecordedMetric::Counter(current)) => {
                let value = current
                    .checked_add(amount)
                    .ok_or(MetricError::CounterOverflow)?;
                Ok(RecordedMetric::Counter(value))
            }
            Some(existing) => Err(MetricError::KindMismatch {
                expected: MetricKind::Counter,
                actual: existing.kind(),
            }),
            None => {
                let value = amount;
                Ok(RecordedMetric::Counter(value))
            }
        })
    }

    /// Sets the current gauge value for the supplied metric series.
    pub fn set_gauge(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        value: f64,
    ) -> Result<(), MetricError> {
        if !value.is_finite() {
            return Err(MetricError::NonFiniteValue);
        }

        self.with_series(descriptor, dimensions, |series| match series {
            Some(RecordedMetric::Gauge(_)) => Ok(RecordedMetric::Gauge(value)),
            Some(existing) => Err(MetricError::KindMismatch {
                expected: MetricKind::Gauge,
                actual: existing.kind(),
            }),
            None => Ok(RecordedMetric::Gauge(value)),
        })
    }

    /// Adds one observation to the supplied histogram series.
    pub fn observe(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        value: f64,
    ) -> Result<(), MetricError> {
        if !value.is_finite() {
            return Err(MetricError::NonFiniteValue);
        }

        self.with_series(descriptor, dimensions, |series| match series {
            Some(RecordedMetric::Histogram { count, sum }) => {
                let count = count.checked_add(1).ok_or(MetricError::CounterOverflow)?;
                let sum = *sum + value;
                if !sum.is_finite() {
                    return Err(MetricError::NonFiniteValue);
                }
                Ok(RecordedMetric::Histogram { count, sum })
            }
            Some(existing) => Err(MetricError::KindMismatch {
                expected: MetricKind::Histogram,
                actual: existing.kind(),
            }),
            None => Ok(RecordedMetric::Histogram {
                count: 1,
                sum: value,
            }),
        })
    }

    /// Returns an immutable snapshot of every currently recorded metric series.
    pub fn snapshot(&self) -> Result<Vec<MetricSnapshot>, MetricError> {
        let series = self.series.lock().map_err(|_| MetricError::LockPoisoned)?;
        Ok(series
            .iter()
            .map(|(key, value)| MetricSnapshot {
                descriptor: MetricDescriptor::new(key.name.clone(), value.kind()),
                dimensions: key.dimensions.clone(),
                value: value.public_value(),
            })
            .collect())
    }

    /// Removes all currently recorded metric series.
    pub fn clear(&self) -> Result<(), MetricError> {
        self.series
            .lock()
            .map_err(|_| MetricError::LockPoisoned)?
            .clear();
        Ok(())
    }

    fn with_series<F>(
        &self,
        descriptor: &MetricDescriptor,
        dimensions: MetricDimensions,
        update: F,
    ) -> Result<(), MetricError>
    where
        F: FnOnce(Option<&mut RecordedMetric>) -> Result<RecordedMetric, MetricError>,
    {
        let key = MetricKey {
            name: descriptor.name.clone(),
            dimensions,
        };
        let mut series = self.series.lock().map_err(|_| MetricError::LockPoisoned)?;

        if let Some(existing) = series.get_mut(&key) {
            let kind = existing.kind();
            if kind != descriptor.kind {
                return Err(MetricError::KindMismatch {
                    expected: descriptor.kind,
                    actual: kind,
                });
            }
            let replacement = update(Some(existing))?;
            *existing = replacement;
            return Ok(());
        }

        let replacement = match descriptor.kind {
            MetricKind::Counter => match update(None)? {
                RecordedMetric::Counter(value) => RecordedMetric::Counter(value),
                _ => unreachable!("counter updater must produce a counter"),
            },
            MetricKind::Gauge => match update(None)? {
                RecordedMetric::Gauge(value) => RecordedMetric::Gauge(value),
                _ => unreachable!("gauge updater must produce a gauge"),
            },
            MetricKind::Histogram => match update(None)? {
                RecordedMetric::Histogram { count, sum } => {
                    RecordedMetric::Histogram { count, sum }
                }
                _ => unreachable!("histogram updater must produce a histogram"),
            },
        };

        series.insert(key, replacement);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_descriptor(name: &str, kind: MetricKind) -> MetricDescriptor {
        MetricDescriptor::new(MetricName::new(name).unwrap(), kind)
    }

    #[test]
    fn metric_name_rejects_empty_and_overlong_values() {
        assert_eq!(MetricName::new(""), Err(MetricError::InvalidName));
        assert_eq!(MetricName::new("   "), Err(MetricError::InvalidName));
        assert_eq!(
            MetricName::new("x".repeat(MAX_METRIC_NAME_LENGTH + 1)),
            Err(MetricError::InvalidName)
        );
    }

    #[test]
    fn metric_name_preserves_valid_value() {
        let name = MetricName::new("runtime.requests.total").unwrap();
        assert_eq!(name.as_str(), "runtime.requests.total");
    }

    #[test]
    fn dimensions_are_bounded_and_replace_existing_keys() {
        let mut dimensions = MetricDimensions::new();
        for index in 0..MAX_METRIC_DIMENSIONS {
            dimensions.insert(format!("key-{index}"), "value").unwrap();
        }
        assert_eq!(dimensions.len(), MAX_METRIC_DIMENSIONS);
        assert_eq!(
            dimensions.insert("overflow", "value"),
            Err(MetricError::DimensionLimitExceeded)
        );

        dimensions.insert("key-0", "updated").unwrap();
        assert_eq!(dimensions.get("key-0"), Some("updated"));
        assert_eq!(dimensions.len(), MAX_METRIC_DIMENSIONS);
    }

    #[test]
    fn dimensions_reject_invalid_fields() {
        let mut dimensions = MetricDimensions::new();
        assert_eq!(
            dimensions.insert("", "value"),
            Err(MetricError::InvalidDimension)
        );
        assert_eq!(
            dimensions.insert("key", "   "),
            Err(MetricError::InvalidDimension)
        );
    }

    #[test]
    fn counter_increments_and_keeps_series_separate_by_dimensions() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("requests.total", MetricKind::Counter);
        let mut first = MetricDimensions::new();
        first.insert("engine", "a").unwrap();
        let mut second = MetricDimensions::new();
        second.insert("engine", "b").unwrap();

        recorder
            .increment_counter(&metric_descriptor, first.clone(), 2)
            .unwrap();
        recorder
            .increment_counter(&metric_descriptor, first, 3)
            .unwrap();
        recorder
            .increment_counter(&metric_descriptor, second, 7)
            .unwrap();

        let snapshot = recorder.snapshot().unwrap();
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.iter().any(|item| {
            item.dimensions().get("engine") == Some("a") && item.value() == &MetricValue::Counter(5)
        }));
        assert!(snapshot.iter().any(|item| {
            item.dimensions().get("engine") == Some("b") && item.value() == &MetricValue::Counter(7)
        }));
    }

    #[test]
    fn counter_rejects_kind_mismatch_and_overflow() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("requests.total", MetricKind::Counter);
        recorder
            .increment_counter(&metric_descriptor, MetricDimensions::new(), u64::MAX)
            .unwrap();
        assert_eq!(
            recorder.increment_counter(&metric_descriptor, MetricDimensions::new(), 1),
            Err(MetricError::CounterOverflow)
        );

        let gauge_descriptor = make_descriptor("requests.total", MetricKind::Gauge);
        assert_eq!(
            recorder.set_gauge(&gauge_descriptor, MetricDimensions::new(), 1.0),
            Err(MetricError::KindMismatch {
                expected: MetricKind::Gauge,
                actual: MetricKind::Counter,
            })
        );
    }

    #[test]
    fn gauge_sets_and_rejects_non_finite_values() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("active.requests", MetricKind::Gauge);
        recorder
            .set_gauge(&metric_descriptor, MetricDimensions::new(), 4.5)
            .unwrap();
        recorder
            .set_gauge(&metric_descriptor, MetricDimensions::new(), 8.0)
            .unwrap();

        assert_eq!(
            recorder.set_gauge(&metric_descriptor, MetricDimensions::new(), f64::NAN),
            Err(MetricError::NonFiniteValue)
        );
        assert_eq!(
            recorder.set_gauge(&metric_descriptor, MetricDimensions::new(), f64::INFINITY),
            Err(MetricError::NonFiniteValue)
        );

        let snapshot = recorder.snapshot().unwrap();
        assert_eq!(snapshot[0].value(), &MetricValue::Gauge(8.0));
    }

    #[test]
    fn histogram_aggregates_count_and_sum() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("request.latency", MetricKind::Histogram);
        recorder
            .observe(&metric_descriptor, MetricDimensions::new(), 10.0)
            .unwrap();
        recorder
            .observe(&metric_descriptor, MetricDimensions::new(), 20.0)
            .unwrap();

        assert_eq!(
            recorder.snapshot().unwrap()[0].value(),
            &MetricValue::Histogram {
                count: 2,
                sum: 30.0
            }
        );
    }

    #[test]
    fn histogram_rejects_non_finite_observations() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("request.latency", MetricKind::Histogram);
        assert_eq!(
            recorder.observe(
                &metric_descriptor,
                MetricDimensions::new(),
                f64::NEG_INFINITY
            ),
            Err(MetricError::NonFiniteValue)
        );
    }

    #[test]
    fn snapshot_is_deterministic_and_clear_removes_series() {
        let recorder = MetricRecorder::new();
        recorder
            .set_gauge(
                &make_descriptor("b.metric", MetricKind::Gauge),
                MetricDimensions::new(),
                2.0,
            )
            .unwrap();
        recorder
            .set_gauge(
                &make_descriptor("a.metric", MetricKind::Gauge),
                MetricDimensions::new(),
                1.0,
            )
            .unwrap();

        let snapshot = recorder.snapshot().unwrap();
        assert_eq!(snapshot[0].descriptor().name().as_str(), "a.metric");
        assert_eq!(snapshot[1].descriptor().name().as_str(), "b.metric");

        recorder.clear().unwrap();
        assert!(recorder.snapshot().unwrap().is_empty());
    }

    #[test]
    fn recorder_supports_concurrent_updates_without_mixing_series() {
        let recorder = MetricRecorder::new();
        let metric_descriptor = make_descriptor("requests.total", MetricKind::Counter);
        let mut handles = Vec::new();

        for _ in 0..8 {
            let recorder = recorder.clone();
            let metric_descriptor = metric_descriptor.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..100 {
                    recorder
                        .increment_counter(&metric_descriptor, MetricDimensions::new(), 1)
                        .unwrap();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            recorder.snapshot().unwrap()[0].value(),
            &MetricValue::Counter(800)
        );
    }
}
