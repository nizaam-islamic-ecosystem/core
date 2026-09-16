use std::collections::BTreeMap;

use super::environment::{Environment, EnvironmentSnapshot};

/// Loads raw configuration data from supported configuration sources.
///
/// The loader is responsible only for source acquisition. Parsing, validation,
/// resolution, secret handling, snapshot activation, and runtime updates belong
/// to later configuration layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigurationLoader;

impl ConfigurationLoader {
    /// Creates a configuration loader.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Loads the current process environment as raw configuration.
    pub fn load_environment(&self, environment: &Environment) -> LoadedConfiguration {
        let snapshot = environment.snapshot();
        LoadedConfiguration::from_environment(snapshot)
    }
}

/// Raw configuration loaded from one configuration source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedConfiguration {
    source: ConfigurationSource,
    values: BTreeMap<String, String>,
}

impl LoadedConfiguration {
    fn from_environment(snapshot: EnvironmentSnapshot) -> Self {
        let values = snapshot
            .iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();

        Self {
            source: ConfigurationSource::Environment,
            values,
        }
    }

    /// Returns the configuration source.
    #[must_use]
    pub const fn source(&self) -> ConfigurationSource {
        self.source
    }

    /// Returns a raw configuration value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Returns whether a key is present.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Returns the number of loaded values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether no values were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over raw key/value pairs in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    #[cfg(test)]
    pub(crate) fn from_test_values<I, K, V>(values: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            source: ConfigurationSource::Test,
            values: values
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }
}

/// Identifies the source represented by a loaded configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConfigurationSource {
    Environment,
    #[cfg(test)]
    Test,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_can_be_constructed() {
        assert_eq!(ConfigurationLoader::new(), ConfigurationLoader);
    }

    #[test]
    fn environment_loading_preserves_source_identity() {
        let loader = ConfigurationLoader::new();
        let environment = Environment::new();

        let loaded = loader.load_environment(&environment);

        assert_eq!(loaded.source(), ConfigurationSource::Environment);
    }

    #[test]
    fn environment_loading_produces_owned_raw_values() {
        let loader = ConfigurationLoader::new();
        let environment = Environment::new();

        let loaded = loader.load_environment(&environment);
        let entries: Vec<_> = loaded.iter().collect();

        assert_eq!(loaded.len(), entries.len());
        assert_eq!(loaded.is_empty(), entries.is_empty());

        for pair in entries.windows(2) {
            assert!(pair[0].0 <= pair[1].0);
        }

        if let Some((key, value)) = entries.first() {
            assert!(loaded.contains(key));
            assert_eq!(loaded.get(key), Some(*value));
        }
    }

    #[test]
    fn loader_preserves_environment_values_without_parsing_them() {
        let loader = ConfigurationLoader::new();
        let environment = Environment::new();

        let loaded = loader.load_environment(&environment);

        let key = if cfg!(windows) { "PATH" } else { "HOME" };

        if let Some(value) = loaded.get(key) {
            assert!(!value.contains('\0'));
        }
    }

    #[test]
    fn test_fixture_loader_preserves_values_and_marks_test_source() {
        let loaded = LoadedConfiguration::from_test_values([("A", "one"), ("B", "two")]);

        assert_eq!(loaded.source(), ConfigurationSource::Test);
        assert_eq!(loaded.get("A"), Some("one"));
        assert_eq!(loaded.get("B"), Some("two"));
    }

    #[test]
    fn missing_keys_remain_absent() {
        let loader = ConfigurationLoader::new();
        let environment = Environment::new();

        let loaded = loader.load_environment(&environment);

        let missing_key = format!("NIZAAM_LOADER_TEST_MISSING_{}", std::process::id());

        assert!(!loaded.contains(&missing_key));
        assert_eq!(loaded.get(&missing_key), None);
    }
}
