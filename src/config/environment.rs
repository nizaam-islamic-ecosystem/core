use std::collections::BTreeMap;
use std::env;

const MAX_ENVIRONMENT_KEY_LENGTH: usize = 256;

/// Read-only access to environment-backed configuration.
///
/// This module only exposes raw environment values. Typed parsing,
/// validation, precedence, resolution, snapshots, secret handling, and
/// engine-specific semantics belong to the other configuration layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Environment;

impl Environment {
    /// Creates a process-environment source.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Returns the raw environment value for `key`, if it exists.
    pub fn get(&self, key: &str) -> Result<Option<String>, EnvironmentError> {
        validate_key(key)?;

        Ok(env::var_os(key).map(|value| value.to_string_lossy().into_owned()))
    }

    /// Returns the raw environment value for a required key.
    pub fn require(&self, key: &str) -> Result<String, EnvironmentError> {
        validate_key(key)?;

        match env::var_os(key) {
            Some(value) => Ok(value.to_string_lossy().into_owned()),
            None => Err(EnvironmentError::MissingKey {
                key: key.to_owned(),
            }),
        }
    }

    /// Returns whether `key` exists in the process environment.
    pub fn contains(&self, key: &str) -> Result<bool, EnvironmentError> {
        validate_key(key)?;
        Ok(env::var_os(key).is_some())
    }

    /// Captures the current process environment into a deterministic,
    /// read-only source snapshot.
    #[must_use]
    pub fn snapshot(&self) -> EnvironmentSnapshot {
        EnvironmentSnapshot::from_process()
    }
}

/// A deterministic, read-only snapshot of environment values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    values: BTreeMap<String, String>,
}

impl EnvironmentSnapshot {
    fn from_process() -> Self {
        Self {
            values: env::vars_os()
                .map(|(key, value)| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
        }
    }

    /// Returns a value from the snapshot.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Returns whether the snapshot contains `key`.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Returns the number of captured values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the snapshot is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over values in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// Errors produced by the environment configuration source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvironmentError {
    InvalidKey,
    KeyTooLong { max: usize },
    MissingKey { key: String },
}

impl std::fmt::Display for EnvironmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey => write!(f, "environment key must not be empty"),
            Self::KeyTooLong { max } => {
                write!(f, "environment key exceeds maximum length of {max}")
            }
            Self::MissingKey { key } => {
                write!(f, "required environment key is missing: {key}")
            }
        }
    }
}

impl std::error::Error for EnvironmentError {}

fn validate_key(key: &str) -> Result<(), EnvironmentError> {
    if key.is_empty() {
        return Err(EnvironmentError::InvalidKey);
    }

    if key.len() > MAX_ENVIRONMENT_KEY_LENGTH {
        return Err(EnvironmentError::KeyTooLong {
            max: MAX_ENVIRONMENT_KEY_LENGTH,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_can_be_constructed() {
        assert_eq!(Environment::new(), Environment);
    }

    #[test]
    fn empty_keys_are_rejected() {
        let environment = Environment::new();

        assert_eq!(
            environment.get("").expect_err("empty key must fail"),
            EnvironmentError::InvalidKey
        );
        assert_eq!(
            environment.require("").expect_err("empty key must fail"),
            EnvironmentError::InvalidKey
        );
        assert_eq!(
            environment.contains("").expect_err("empty key must fail"),
            EnvironmentError::InvalidKey
        );
    }

    #[test]
    fn overlong_keys_are_rejected() {
        let environment = Environment::new();
        let key = "K".repeat(MAX_ENVIRONMENT_KEY_LENGTH + 1);

        assert_eq!(
            environment.get(&key).expect_err("overlong key must fail"),
            EnvironmentError::KeyTooLong {
                max: MAX_ENVIRONMENT_KEY_LENGTH,
            }
        );
    }

    #[test]
    fn missing_required_key_is_reported() {
        let environment = Environment::new();
        let key = format!("NIZAAM_ENVIRONMENT_TEST_MISSING_{}", std::process::id());

        assert_eq!(environment.get(&key).expect("lookup should succeed"), None);
        assert_eq!(
            environment
                .require(&key)
                .expect_err("missing key must fail"),
            EnvironmentError::MissingKey { key }
        );
    }

    #[test]
    fn present_environment_key_is_read_as_a_raw_string() {
        let environment = Environment::new();

        let key = if cfg!(windows) { "PATH" } else { "HOME" };

        let value = environment
            .get(key)
            .expect("environment lookup should succeed");

        if let Some(value) = value {
            assert_eq!(
                value,
                environment
                    .require(key)
                    .expect("present environment key should be required successfully")
            );
            assert!(!value.contains('\0'));
        }
    }

    #[test]
    fn contains_matches_get_for_a_known_key() {
        let environment = Environment::new();
        let key = if cfg!(windows) { "PATH" } else { "HOME" };

        assert_eq!(
            environment.contains(key).expect("lookup should succeed"),
            environment
                .get(key)
                .expect("lookup should succeed")
                .is_some()
        );
    }

    #[test]
    fn snapshot_is_deterministically_ordered_and_read_only() {
        let environment = Environment::new();
        let snapshot = environment.snapshot();
        let entries: Vec<_> = snapshot.iter().collect();

        assert_eq!(snapshot.len(), entries.len());
        assert_eq!(snapshot.is_empty(), entries.is_empty());

        for pair in entries.windows(2) {
            assert!(pair[0].0 <= pair[1].0);
        }

        if let Some((key, value)) = entries.first() {
            assert!(snapshot.contains(key));
            assert_eq!(snapshot.get(key), Some(*value));
        }
    }
}
