use std::fmt;

use super::resolution::ResolvedConfiguration;

/// Stable identity of an immutable runtime configuration snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConfigurationSnapshotId(u64);

impl ConfigurationSnapshotId {
    /// Creates a snapshot identifier from a monotonic generation value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the numeric generation value.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ConfigurationSnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An immutable runtime configuration snapshot.
///
/// A snapshot owns one fully resolved configuration. A new configuration must
/// produce a new snapshot rather than mutating an existing one.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigurationSnapshot {
    id: ConfigurationSnapshotId,
    configuration: ResolvedConfiguration,
}

impl ConfigurationSnapshot {
    /// Creates an immutable snapshot from a resolved configuration.
    #[must_use]
    pub fn new(id: ConfigurationSnapshotId, configuration: ResolvedConfiguration) -> Self {
        Self { id, configuration }
    }

    /// Returns the stable identity of this snapshot.
    #[must_use]
    pub const fn id(&self) -> ConfigurationSnapshotId {
        self.id
    }

    /// Returns the resolved configuration owned by this snapshot.
    #[must_use]
    pub fn configuration(&self) -> &ResolvedConfiguration {
        &self.configuration
    }

    /// Returns a resolved configuration value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&super::validation::ConfigurationValue> {
        self.configuration.get(key)
    }

    /// Returns whether the snapshot contains `key`.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.configuration.contains(key)
    }

    /// Returns the number of resolved configuration values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.configuration.len()
    }

    /// Returns whether the snapshot contains no configuration values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.configuration.is_empty()
    }

    /// Iterates over configuration values in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &super::validation::ConfigurationValue)> {
        self.configuration.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::resolution::ConfigurationResolver;
    use crate::config::validation::{
        ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
    };

    fn resolved_configuration(values: &[(&str, ConfigurationValue)]) -> ResolvedConfiguration {
        let mut parsed = ParsedConfiguration::empty();

        for (key, value) in values {
            parsed.insert((*key).to_owned(), value.clone());
        }

        let validated = ConfigurationValidator::new()
            .validate(parsed)
            .expect("test configuration should be valid");

        ConfigurationResolver::new()
            .resolve(&validated)
            .expect("test configuration should resolve")
    }

    #[test]
    fn snapshot_id_can_be_constructed() {
        let id = ConfigurationSnapshotId::new(42);

        assert_eq!(id.value(), 42);
        assert_eq!(id.to_string(), "42");
    }

    #[test]
    fn snapshot_can_be_constructed() {
        let configuration = resolved_configuration(&[("PORT", ConfigurationValue::Integer(8080))]);

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(1), configuration);

        assert_eq!(snapshot.id().value(), 1);
        assert_eq!(snapshot.len(), 1);
    }

    #[test]
    fn snapshot_exposes_read_only_configuration() {
        let configuration = resolved_configuration(&[
            ("HOST", ConfigurationValue::String("localhost".to_owned())),
            ("PORT", ConfigurationValue::Integer(8080)),
        ]);

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(2), configuration);

        assert_eq!(
            snapshot.get("HOST").and_then(ConfigurationValue::as_string),
            Some("localhost")
        );
        assert_eq!(
            snapshot
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
        assert!(snapshot.contains("HOST"));
        assert!(snapshot.contains("PORT"));
        assert!(!snapshot.contains("MISSING"));
        assert_eq!(snapshot.len(), 2);
        assert!(!snapshot.is_empty());
    }

    #[test]
    fn snapshot_iteration_is_deterministic() {
        let configuration = resolved_configuration(&[
            ("Z", ConfigurationValue::String("z".to_owned())),
            ("A", ConfigurationValue::String("a".to_owned())),
            ("M", ConfigurationValue::String("m".to_owned())),
        ]);

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(3), configuration);

        let keys: Vec<_> = snapshot.iter().map(|(key, _)| key).collect();

        assert_eq!(keys, vec!["A", "M", "Z"]);
    }

    #[test]
    fn snapshot_clone_preserves_identity_and_values() {
        let configuration =
            resolved_configuration(&[("VALUE", ConfigurationValue::String("value".to_owned()))]);

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(4), configuration);

        let cloned = snapshot.clone();

        assert_eq!(snapshot, cloned);
        assert_eq!(cloned.id(), snapshot.id());
        assert_eq!(cloned.get("VALUE"), snapshot.get("VALUE"));
    }

    #[test]
    fn different_snapshot_ids_are_distinguishable() {
        let configuration =
            resolved_configuration(&[("VALUE", ConfigurationValue::String("value".to_owned()))]);

        let first =
            ConfigurationSnapshot::new(ConfigurationSnapshotId::new(5), configuration.clone());
        let second = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(6), configuration);

        assert_ne!(first.id(), second.id());
        assert_ne!(first, second);
    }

    #[test]
    fn different_snapshots_do_not_mutate_each_other() {
        let first_configuration =
            resolved_configuration(&[("PORT", ConfigurationValue::Integer(8080))]);
        let second_configuration =
            resolved_configuration(&[("PORT", ConfigurationValue::Integer(9090))]);

        let first =
            ConfigurationSnapshot::new(ConfigurationSnapshotId::new(7), first_configuration);
        let second =
            ConfigurationSnapshot::new(ConfigurationSnapshotId::new(8), second_configuration);

        assert_eq!(
            first.get("PORT").and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
        assert_eq!(
            second.get("PORT").and_then(ConfigurationValue::as_integer),
            Some(9090)
        );
    }

    #[test]
    fn snapshot_owns_the_resolved_configuration() {
        let configuration = resolved_configuration(&[(
            "MODE",
            ConfigurationValue::String("production".to_owned()),
        )]);

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(9), configuration);

        assert_eq!(
            snapshot
                .configuration()
                .get("MODE")
                .and_then(ConfigurationValue::as_string),
            Some("production")
        );
    }

    #[test]
    fn empty_resolved_configuration_creates_empty_snapshot() {
        let configuration = resolved_configuration(&[]);
        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(10), configuration);

        assert_eq!(snapshot.len(), 0);
        assert!(snapshot.is_empty());
        assert!(snapshot.iter().next().is_none());
    }

    #[test]
    fn snapshot_id_order_is_stable() {
        let first = ConfigurationSnapshotId::new(1);
        let second = ConfigurationSnapshotId::new(2);

        assert!(first < second);
    }
}
