use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use super::loader::LoadedConfiguration;
use super::parser::{ConfigurationParser, ParseErrors};
use super::resolution::{ConfigurationResolver, ResolutionErrors};
use super::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use super::validation::{
    ConfigurationValidator, SemanticValidator, ValidatedConfiguration, ValidationErrors,
};

trait ErasedSemanticValidator: fmt::Debug + Send + Sync {
    fn validate(&self, configuration: &ValidatedConfiguration) -> Result<(), String>;
}

impl<V> ErasedSemanticValidator for V
where
    V: SemanticValidator + fmt::Debug + Send + Sync + 'static,
    V::Error: fmt::Display,
{
    fn validate(&self, configuration: &ValidatedConfiguration) -> Result<(), String> {
        SemanticValidator::validate(self, configuration).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ConfigurationUpdaterId(u64);

impl ConfigurationUpdaterId {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Orchestrates configuration proposals through parsing, validation,
/// resolution, snapshot creation, and atomic activation.
///
/// The updater never mutates an existing `ConfigurationSnapshot`. A successful
/// update publishes a new snapshot generation, while a failed update leaves
/// the currently active snapshot unchanged.
#[derive(Clone, Debug)]
pub struct ConfigurationUpdater {
    parser: ConfigurationParser,
    validator: ConfigurationValidator,
    resolver: ConfigurationResolver,
    semantic_validator: Option<Arc<dyn ErasedSemanticValidator>>,
    current: ConfigurationSnapshot,
    lineage: ConfigurationUpdaterId,
}

impl ConfigurationUpdater {
    /// Creates an updater with an already resolved initial snapshot.
    #[must_use]
    pub fn new(
        parser: ConfigurationParser,
        validator: ConfigurationValidator,
        resolver: ConfigurationResolver,
        initial_snapshot: ConfigurationSnapshot,
    ) -> Self {
        Self {
            parser,
            validator,
            resolver,
            semantic_validator: None,
            current: initial_snapshot,
            lineage: ConfigurationUpdaterId::new(),
        }
    }

    /// Attaches an engine-specific semantic validation boundary to updates.
    pub fn with_semantic_validator<V>(mut self, validator: V) -> Self
    where
        V: SemanticValidator + fmt::Debug + Send + Sync + 'static,
        V::Error: fmt::Display,
    {
        self.semantic_validator = Some(Arc::new(validator));
        self
    }

    /// Returns the currently active immutable configuration snapshot.
    #[must_use]
    pub fn current(&self) -> &ConfigurationSnapshot {
        &self.current
    }

    /// Prepares a complete configuration proposal without activating it.
    ///
    /// Parsing, structural validation, semantic validation when configured, and
    /// resolution happen before any active state is changed. A successful
    /// preparation produces the next snapshot generation and can later be
    /// activated atomically.
    pub fn prepare(
        &self,
        proposal: &LoadedConfiguration,
    ) -> Result<PreparedConfigurationUpdate, ConfigurationUpdateError> {
        let parsed = self
            .parser
            .parse(proposal)
            .map_err(ConfigurationUpdateError::Parse)?;

        let validated = self
            .validator
            .validate(parsed)
            .map_err(ConfigurationUpdateError::Validation)?;

        if let Some(validator) = &self.semantic_validator {
            validator
                .validate(&validated)
                .map_err(|message| ConfigurationUpdateError::SemanticValidation { message })?;
        }

        let resolved = self
            .resolver
            .resolve(&validated)
            .map_err(ConfigurationUpdateError::Resolution)?;

        let next_id = next_snapshot_id(self.current.id())
            .ok_or(ConfigurationUpdateError::SnapshotIdExhausted)?;

        let snapshot = ConfigurationSnapshot::new(next_id, resolved);

        Ok(PreparedConfigurationUpdate {
            expected_current: self.current.id(),
            snapshot,
            lineage: self.lineage,
        })
    }

    /// Activates a previously prepared update only if it belongs to this updater
    /// and its expected current snapshot is still active.
    ///
    /// This provides optimistic conflict detection: a prepared proposal cannot
    /// overwrite a configuration snapshot that became active after it was
    /// prepared.
    pub fn activate(
        &mut self,
        prepared: PreparedConfigurationUpdate,
    ) -> Result<ConfigurationUpdateResult, ConfigurationUpdateError> {
        let actual = self.current.id();

        if self.lineage != prepared.lineage {
            return Err(ConfigurationUpdateError::LineageMismatch);
        }

        if actual != prepared.expected_current {
            return Err(ConfigurationUpdateError::Conflict {
                expected: prepared.expected_current,
                actual,
            });
        }

        let previous = self.current.id();
        let next = prepared.snapshot.id();

        self.current = prepared.snapshot;

        Ok(ConfigurationUpdateResult {
            previous,
            current: next,
        })
    }

    /// Prepares and activates a complete configuration proposal.
    ///
    /// The active snapshot is changed only after parsing, structural validation,
    /// configured semantic validation, resolution, and snapshot construction
    /// have all succeeded.
    pub fn update(
        &mut self,
        proposal: &LoadedConfiguration,
    ) -> Result<ConfigurationUpdateResult, ConfigurationUpdateError> {
        let prepared = self.prepare(proposal)?;
        self.activate(prepared)
    }
}

/// A configuration update that has completed parsing, validation, resolution,
/// and snapshot construction but has not yet been activated.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedConfigurationUpdate {
    expected_current: ConfigurationSnapshotId,
    snapshot: ConfigurationSnapshot,
    lineage: ConfigurationUpdaterId,
}

impl PreparedConfigurationUpdate {
    /// Returns the snapshot generation that was current during preparation.
    #[must_use]
    pub const fn expected_current(&self) -> ConfigurationSnapshotId {
        self.expected_current
    }

    /// Returns the candidate snapshot that will be activated.
    #[must_use]
    pub fn snapshot(&self) -> &ConfigurationSnapshot {
        &self.snapshot
    }
}

/// Result of a successfully activated configuration update.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConfigurationUpdateResult {
    previous: ConfigurationSnapshotId,
    current: ConfigurationSnapshotId,
}

impl ConfigurationUpdateResult {
    #[must_use]
    pub const fn previous(&self) -> ConfigurationSnapshotId {
        self.previous
    }

    #[must_use]
    pub const fn current(&self) -> ConfigurationSnapshotId {
        self.current
    }
}

/// Failure encountered while preparing or activating a configuration update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigurationUpdateError {
    Parse(ParseErrors),
    Validation(ValidationErrors),
    Resolution(ResolutionErrors),
    SemanticValidation {
        message: String,
    },
    LineageMismatch,
    Conflict {
        expected: ConfigurationSnapshotId,
        actual: ConfigurationSnapshotId,
    },
    SnapshotIdExhausted,
}

impl fmt::Display for ConfigurationUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(errors) => write!(f, "configuration update parsing failed: {errors}"),
            Self::Validation(errors) => {
                write!(f, "configuration update validation failed: {errors}")
            }
            Self::Resolution(errors) => {
                write!(f, "configuration update resolution failed: {errors}")
            }
            Self::SemanticValidation { message } => {
                write!(
                    f,
                    "configuration update semantic validation failed: {message}"
                )
            }
            Self::LineageMismatch => {
                f.write_str("configuration update prepared by a different updater")
            }
            Self::Conflict { expected, actual } => write!(
                f,
                "configuration update conflict: expected snapshot {expected}, \
                 but current snapshot is {actual}"
            ),
            Self::SnapshotIdExhausted => {
                f.write_str("configuration snapshot identifier is exhausted")
            }
        }
    }
}

impl std::error::Error for ConfigurationUpdateError {}

fn next_snapshot_id(current: ConfigurationSnapshotId) -> Option<ConfigurationSnapshotId> {
    current
        .value()
        .checked_add(1)
        .map(ConfigurationSnapshotId::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::validation::{ConfigurationValue, ParsedConfiguration};

    fn loaded(values: &[(&str, &str)]) -> LoadedConfiguration {
        LoadedConfiguration::from_test_values(
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        )
    }

    fn updater(initial: &[(&str, ConfigurationValue)]) -> ConfigurationUpdater {
        let mut parsed = ParsedConfiguration::empty();

        for (key, value) in initial {
            parsed.insert((*key).to_owned(), value.clone());
        }

        let validated = ConfigurationValidator::new()
            .validate(parsed)
            .expect("test configuration should validate");

        let resolved = ConfigurationResolver::new()
            .resolve(&validated)
            .expect("test configuration should resolve");

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(0), resolved);

        let parser = ConfigurationParser::new()
            .with_type("PORT", crate::config::parser::ConfigurationType::Integer)
            .expect("test parser type declaration should succeed");

        ConfigurationUpdater::new(
            parser,
            ConfigurationValidator::new(),
            ConfigurationResolver::new(),
            snapshot,
        )
    }

    #[test]
    fn updater_exposes_initial_snapshot() {
        let updater = updater(&[("PORT", ConfigurationValue::Integer(8080))]);

        assert_eq!(updater.current().id().value(), 0);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn successful_update_creates_next_snapshot() {
        let mut updater = updater(&[]);

        let result = updater
            .update(&loaded(&[("PORT", "9090")]))
            .expect("update should succeed");

        assert_eq!(result.previous().value(), 0);
        assert_eq!(result.current().value(), 1);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(9090)
        );
    }

    #[test]
    fn successful_update_does_not_mutate_old_snapshot() {
        let mut updater = updater(&[("PORT", ConfigurationValue::Integer(8080))]);

        let old = updater.current().clone();

        updater
            .update(&loaded(&[("PORT", "9090")]))
            .expect("update should succeed");

        assert_eq!(
            old.get("PORT").and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
        assert_eq!(old.id().value(), 0);
    }

    #[test]
    fn parse_failure_preserves_current_snapshot() {
        let mut updater = updater(&[("PORT", ConfigurationValue::Integer(8080))]);

        let parser = ConfigurationParser::new()
            .with_type("PORT", crate::config::parser::ConfigurationType::Integer)
            .expect("type declaration should succeed");

        updater.parser = parser;

        let error = updater
            .update(&loaded(&[("PORT", "not-an-integer")]))
            .expect_err("update should fail");

        assert!(matches!(error, ConfigurationUpdateError::Parse(_)));
        assert_eq!(updater.current().id().value(), 0);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn validation_failure_preserves_current_snapshot() {
        let mut updater = updater(&[]);
        updater.validator = ConfigurationValidator::new().require_key("REQUIRED");

        let error = updater
            .update(&loaded(&[("OTHER", "value")]))
            .expect_err("validation should fail");

        assert!(matches!(error, ConfigurationUpdateError::Validation(_)));
        assert_eq!(updater.current().id().value(), 0);
    }

    #[test]
    fn resolution_failure_preserves_current_snapshot() {
        let mut updater = updater(&[]);

        let error = updater
            .update(&loaded(&[("VALUE", "${MISSING}")]))
            .expect_err("resolution should fail");

        assert!(matches!(error, ConfigurationUpdateError::Resolution(_)));
        assert_eq!(updater.current().id().value(), 0);
    }

    #[test]
    fn prepare_does_not_activate_configuration() {
        let updater = updater(&[]);

        let prepared = updater
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("preparation should succeed");

        assert_eq!(updater.current().id().value(), 0);
        assert_eq!(prepared.expected_current().value(), 0);
        assert_eq!(prepared.snapshot().id().value(), 1);
        assert_eq!(
            prepared
                .snapshot()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(9090)
        );
    }

    #[test]
    fn prepared_update_can_be_activated() {
        let mut updater = updater(&[]);

        let prepared = updater
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("preparation should succeed");

        let result = updater
            .activate(prepared)
            .expect("activation should succeed");

        assert_eq!(result.previous().value(), 0);
        assert_eq!(result.current().value(), 1);
    }

    #[test]
    fn stale_prepared_update_is_rejected() {
        let mut updater = updater(&[]);

        let first = updater
            .prepare(&loaded(&[("PORT", "8080")]))
            .expect("first preparation should succeed");

        let second = updater
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("second preparation should succeed");

        updater
            .activate(first)
            .expect("first activation should succeed");

        let error = updater
            .activate(second)
            .expect_err("stale activation should fail");

        assert_eq!(
            error,
            ConfigurationUpdateError::Conflict {
                expected: ConfigurationSnapshotId::new(0),
                actual: ConfigurationSnapshotId::new(1),
            }
        );

        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn failed_activation_does_not_change_current_snapshot() {
        let mut updater = updater(&[]);

        let first = updater
            .prepare(&loaded(&[("PORT", "8080")]))
            .expect("preparation should succeed");

        let second = updater
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("preparation should succeed");

        updater
            .activate(first)
            .expect("first activation should succeed");

        let _ = updater.activate(second);

        assert_eq!(updater.current().id().value(), 1);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn snapshot_ids_progress_monotonically() {
        let mut updater = updater(&[]);

        for expected in 1..=3 {
            let result = updater
                .update(&loaded(&[("PORT", "8080")]))
                .expect("update should succeed");

            assert_eq!(result.current().value(), expected);
        }
    }

    #[derive(Debug)]
    struct PortRangeValidator;

    #[derive(Debug)]
    struct PortRangeError;

    impl fmt::Display for PortRangeError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("PORT must be between 1 and 65535")
        }
    }

    impl SemanticValidator for PortRangeValidator {
        type Error = PortRangeError;

        fn validate(&self, configuration: &ValidatedConfiguration) -> Result<(), Self::Error> {
            match configuration.get("PORT") {
                Some(ConfigurationValue::Integer(port)) if (1..=65_535).contains(port) => Ok(()),
                _ => Err(PortRangeError),
            }
        }
    }

    #[test]
    fn semantic_validation_runs_before_candidate_activation() {
        let mut updater = updater(&[]).with_semantic_validator(PortRangeValidator);

        let error = updater
            .update(&loaded(&[("PORT", "70000")]))
            .expect_err("semantic validation must reject an invalid port");

        assert_eq!(
            error,
            ConfigurationUpdateError::SemanticValidation {
                message: "PORT must be between 1 and 65535".to_owned(),
            }
        );
        assert_eq!(updater.current().id().value(), 0);
    }

    #[test]
    fn prepared_update_cannot_be_activated_by_another_updater() {
        let first = updater(&[]);
        let mut second = updater(&[]);

        let prepared = first
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("preparation should succeed");

        assert_eq!(
            second
                .activate(prepared)
                .expect_err("cross-updater activation must fail"),
            ConfigurationUpdateError::LineageMismatch
        );
        assert_eq!(second.current().id().value(), 0);
    }

    #[test]
    fn snapshot_id_overflow_is_detected() {
        assert_eq!(
            next_snapshot_id(ConfigurationSnapshotId::new(u64::MAX)),
            None
        );
    }

    #[test]
    fn prepared_snapshot_remains_immutable_after_activation() {
        let mut updater = updater(&[]);

        let prepared = updater
            .prepare(&loaded(&[("PORT", "9090")]))
            .expect("preparation should succeed");

        let prepared_snapshot = prepared.snapshot().clone();

        updater
            .activate(prepared)
            .expect("activation should succeed");

        assert_eq!(
            prepared_snapshot
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(9090)
        );
        assert_eq!(prepared_snapshot.id().value(), 1);
    }

    #[test]
    fn update_errors_are_safe_to_render() {
        let error = ConfigurationUpdateError::Conflict {
            expected: ConfigurationSnapshotId::new(1),
            actual: ConfigurationSnapshotId::new(2),
        };

        assert_eq!(
            error.to_string(),
            "configuration update conflict: expected snapshot 1, \
             but current snapshot is 2"
        );
    }

    #[test]
    fn current_snapshot_is_not_replaced_during_failed_preparation() {
        let mut updater = updater(&[("PORT", ConfigurationValue::Integer(8080))]);

        updater.validator = ConfigurationValidator::new().require_key("REQUIRED");

        let _ = updater.update(&loaded(&[("PORT", "9090")]));

        assert_eq!(updater.current().id().value(), 0);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }
}
