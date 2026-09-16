use std::collections::BTreeMap;
use std::fmt;

use super::validation::{ConfigurationValue, ValidatedConfiguration};

/// Resolves validated configuration into the deterministic representation
/// consumed by later runtime configuration layers.
///
/// Resolution applies explicit values and registered defaults, then resolves
/// supported configuration references. It does not load sources, parse raw
/// input, perform semantic engine validation, manage secrets, or mutate
/// runtime state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigurationResolver {
    defaults: BTreeMap<String, ConfigurationValue>,
}

impl ConfigurationResolver {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            defaults: BTreeMap::new(),
        }
    }

    /// Registers a default value for a configuration key.
    ///
    /// Explicitly supplied configuration always takes precedence over a
    /// registered default.
    pub fn with_default(
        mut self,
        key: impl Into<String>,
        value: ConfigurationValue,
    ) -> Result<Self, ResolutionError> {
        let key = key.into();
        validate_key(&key)?;

        if self.defaults.insert(key.clone(), value).is_some() {
            return Err(ResolutionError::DuplicateDefault { key });
        }

        Ok(self)
    }

    /// Resolves a validated configuration into an immutable resolved result.
    ///
    /// Resolution is deterministic and produces no partial result. If any
    /// reference cannot be resolved, the complete operation fails.
    pub fn resolve(
        &self,
        configuration: &ValidatedConfiguration,
    ) -> Result<ResolvedConfiguration, ResolutionErrors> {
        let mut values = BTreeMap::new();

        for (key, value) in configuration.iter() {
            values.insert(key.to_owned(), value.clone());
        }

        for (key, value) in &self.defaults {
            values.entry(key.clone()).or_insert_with(|| value.clone());
        }

        let mut resolver = ReferenceResolution::new(&values);
        let keys: Vec<String> = values.keys().cloned().collect();

        for key in keys {
            if let Err(error) = resolver.resolve_key(&key) {
                resolver.errors.push(error);
            }
        }

        if resolver.errors.is_empty() {
            Ok(ResolvedConfiguration::new(resolver.values))
        } else {
            Err(resolver.errors)
        }
    }
}

/// The final configuration produced by deterministic resolution.
///
/// Values are privately owned and cannot be mutated through this type's public
/// API.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedConfiguration {
    values: BTreeMap<String, ConfigurationValue>,
}

impl ResolvedConfiguration {
    fn new(values: BTreeMap<String, ConfigurationValue>) -> Self {
        Self { values }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ConfigurationValue> {
        self.values.get(key)
    }

    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over resolved values in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ConfigurationValue)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }
}

/// A single failure encountered while applying resolution rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionError {
    InvalidKey { key: String },
    DuplicateDefault { key: String },
    UnsupportedReference { key: String, reference: String },
    UnresolvedReference { key: String, reference: String },
    ReferenceCycle { key: String },
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey { key } => {
                write!(f, "invalid configuration key: {key}")
            }
            Self::DuplicateDefault { key } => {
                write!(f, "duplicate configuration default: {key}")
            }
            Self::UnsupportedReference { key, reference } => write!(
                f,
                "unsupported configuration reference '{reference}' in key '{key}'"
            ),
            Self::UnresolvedReference { key, reference } => write!(
                f,
                "unresolved configuration reference '{reference}' in key '{key}'"
            ),
            Self::ReferenceCycle { key } => {
                write!(f, "configuration reference cycle detected at key '{key}'")
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

/// A deterministic collection of resolution failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolutionErrors {
    errors: Vec<ResolutionError>,
}

impl ResolutionErrors {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, error: ResolutionError) {
        self.errors.push(error);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResolutionError> {
        self.errors.iter()
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ResolutionError> {
        self.errors
    }
}

impl fmt::Display for ResolutionErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.errors.iter().enumerate() {
            if index > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{error}")?;
        }

        Ok(())
    }
}

impl std::error::Error for ResolutionErrors {}

fn validate_key(key: &str) -> Result<(), ResolutionError> {
    if key.is_empty() {
        return Err(ResolutionError::InvalidKey {
            key: key.to_owned(),
        });
    }

    Ok(())
}

const REFERENCE_PREFIX: &str = "${";
const REFERENCE_SUFFIX: char = '}';
const REDACTED_REFERENCE: &str = "[REDACTED]";

fn parse_reference(value: &str) -> Result<Option<&str>, ParseReferenceError> {
    if !value.starts_with(REFERENCE_PREFIX) {
        return Ok(None);
    }

    if !value.ends_with(REFERENCE_SUFFIX) {
        return Err(ParseReferenceError::Malformed);
    }

    let reference = &value[REFERENCE_PREFIX.len()..value.len() - 1];

    if reference.is_empty() {
        return Err(ParseReferenceError::Malformed);
    }

    if reference.contains(REFERENCE_PREFIX) || reference.contains(REFERENCE_SUFFIX) {
        return Err(ParseReferenceError::Malformed);
    }

    Ok(Some(reference))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParseReferenceError {
    Malformed,
}

struct ReferenceResolution<'a> {
    source: &'a BTreeMap<String, ConfigurationValue>,
    values: BTreeMap<String, ConfigurationValue>,
    state: BTreeMap<String, VisitState>,
    failures: BTreeMap<String, ResolutionError>,
    stack: Vec<String>,
    errors: ResolutionErrors,
}

impl<'a> ReferenceResolution<'a> {
    fn new(source: &'a BTreeMap<String, ConfigurationValue>) -> Self {
        Self {
            source,
            values: BTreeMap::new(),
            state: BTreeMap::new(),
            failures: BTreeMap::new(),
            stack: Vec::new(),
            errors: ResolutionErrors::new(),
        }
    }

    fn resolve_key(&mut self, key: &str) -> Result<ConfigurationValue, ResolutionError> {
        match self.state.get(key) {
            Some(VisitState::Resolved) => {
                return self.values.get(key).cloned().ok_or_else(|| {
                    ResolutionError::UnresolvedReference {
                        key: key.to_owned(),
                        reference: key.to_owned(),
                    }
                });
            }
            Some(VisitState::Resolving) => {
                return Err(ResolutionError::ReferenceCycle {
                    key: key.to_owned(),
                });
            }
            Some(VisitState::Failed) => {
                return Err(self.failures.get(key).cloned().unwrap_or_else(|| {
                    ResolutionError::UnresolvedReference {
                        key: key.to_owned(),
                        reference: key.to_owned(),
                    }
                }));
            }
            None => {}
        }

        let raw_value = match self.source.get(key) {
            Some(value) => value.clone(),
            None => {
                return Err(ResolutionError::UnresolvedReference {
                    key: key.to_owned(),
                    reference: key.to_owned(),
                });
            }
        };

        self.state.insert(key.to_owned(), VisitState::Resolving);
        self.stack.push(key.to_owned());

        let result = self.resolve_value(key, raw_value);

        self.stack.pop();

        match result {
            Ok(value) => {
                self.state.insert(key.to_owned(), VisitState::Resolved);
                self.values.insert(key.to_owned(), value.clone());
                Ok(value)
            }
            Err(error) => {
                self.state.insert(key.to_owned(), VisitState::Failed);
                self.failures.insert(key.to_owned(), error.clone());
                Err(error)
            }
        }
    }

    fn resolve_value(
        &mut self,
        key: &str,
        value: ConfigurationValue,
    ) -> Result<ConfigurationValue, ResolutionError> {
        let ConfigurationValue::String(raw) = value else {
            return Ok(value);
        };

        match parse_reference(&raw) {
            Ok(Some(reference)) => {
                let referenced_value = self.source.get(reference).ok_or_else(|| {
                    ResolutionError::UnresolvedReference {
                        key: key.to_owned(),
                        reference: reference.to_owned(),
                    }
                })?;

                if !matches!(referenced_value, ConfigurationValue::String(_)) {
                    return Err(ResolutionError::UnsupportedReference {
                        key: key.to_owned(),
                        reference: reference.to_owned(),
                    });
                }

                if self.stack.iter().any(|item| item == reference) {
                    return Err(ResolutionError::ReferenceCycle {
                        key: key.to_owned(),
                    });
                }

                self.resolve_key(reference)
            }
            Ok(None) => Ok(ConfigurationValue::String(raw)),
            Err(ParseReferenceError::Malformed) => Err(ResolutionError::UnsupportedReference {
                key: key.to_owned(),
                reference: REDACTED_REFERENCE.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Resolving,
    Resolved,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(values: &[(&str, ConfigurationValue)]) -> ValidatedConfiguration {
        let mut parsed = super::super::validation::ParsedConfiguration::empty();

        for (key, value) in values {
            parsed.insert(*key, value.clone());
        }

        ConfigurationResolver::new()
            .validate_for_test(parsed)
            .expect("test configuration should be structurally valid")
    }

    impl ConfigurationResolver {
        fn validate_for_test(
            &self,
            parsed: super::super::validation::ParsedConfiguration,
        ) -> Result<ValidatedConfiguration, super::super::validation::ValidationErrors> {
            super::super::validation::ConfigurationValidator::new().validate(parsed)
        }
    }

    #[test]
    fn resolver_can_be_constructed() {
        assert_eq!(ConfigurationResolver::new(), ConfigurationResolver::new());
    }

    #[test]
    fn empty_configuration_resolves() {
        let configuration = configuration(&[]);
        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("empty configuration should resolve");

        assert!(resolved.is_empty());
    }

    #[test]
    fn explicit_values_are_preserved() {
        let configuration = configuration(&[("PORT", ConfigurationValue::Integer(8080))]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("configuration should resolve");

        assert_eq!(
            resolved
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn defaults_fill_missing_values() {
        let configuration = configuration(&[]);
        let resolver = ConfigurationResolver::new()
            .with_default("PORT", ConfigurationValue::Integer(8080))
            .expect("default should be accepted");

        let resolved = resolver
            .resolve(&configuration)
            .expect("configuration should resolve");

        assert_eq!(
            resolved
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn explicit_values_override_defaults() {
        let configuration = configuration(&[("PORT", ConfigurationValue::Integer(9000))]);

        let resolver = ConfigurationResolver::new()
            .with_default("PORT", ConfigurationValue::Integer(8080))
            .expect("default should be accepted");

        let resolved = resolver
            .resolve(&configuration)
            .expect("configuration should resolve");

        assert_eq!(
            resolved
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(9000)
        );
    }

    #[test]
    fn duplicate_defaults_are_rejected() {
        let error = ConfigurationResolver::new()
            .with_default("PORT", ConfigurationValue::Integer(8080))
            .expect("first default should be accepted")
            .with_default("PORT", ConfigurationValue::Integer(9000))
            .expect_err("duplicate default should fail");

        assert_eq!(
            error,
            ResolutionError::DuplicateDefault {
                key: "PORT".to_owned()
            }
        );
    }

    #[test]
    fn empty_default_keys_are_rejected() {
        let error = ConfigurationResolver::new()
            .with_default("", ConfigurationValue::String("value".to_owned()))
            .expect_err("empty key should fail");

        assert_eq!(error, ResolutionError::InvalidKey { key: String::new() });
    }

    #[test]
    fn simple_string_reference_is_resolved() {
        let configuration = configuration(&[
            ("HOST", ConfigurationValue::String("localhost".to_owned())),
            ("ADDRESS", ConfigurationValue::String("${HOST}".to_owned())),
        ]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("reference should resolve");

        assert_eq!(
            resolved
                .get("ADDRESS")
                .and_then(ConfigurationValue::as_string),
            Some("localhost")
        );
    }

    #[test]
    fn multi_level_string_reference_is_resolved() {
        let configuration = configuration(&[
            ("BASE", ConfigurationValue::String("localhost".to_owned())),
            ("HOST", ConfigurationValue::String("${BASE}".to_owned())),
            ("ADDRESS", ConfigurationValue::String("${HOST}".to_owned())),
        ]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("reference chain should resolve");

        assert_eq!(
            resolved
                .get("ADDRESS")
                .and_then(ConfigurationValue::as_string),
            Some("localhost")
        );
    }

    #[test]
    fn malformed_reference_error_does_not_expose_raw_value() {
        let raw = "${unterminated-sensitive-value";
        let configuration = configuration(&[("HOST", ConfigurationValue::String(raw.to_owned()))]);

        let error = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("malformed reference should fail");

        assert_eq!(error.len(), 1);
        let rendered = error.to_string();
        assert!(!rendered.contains(raw));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn missing_reference_is_reported() {
        let configuration =
            configuration(&[("ADDRESS", ConfigurationValue::String("${HOST}".to_owned()))]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("missing reference should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ResolutionError::UnresolvedReference { key, reference }
                if key == "ADDRESS" && reference == "HOST"
        )));
    }

    #[test]
    fn self_reference_is_reported_as_cycle() {
        let configuration =
            configuration(&[("HOST", ConfigurationValue::String("${HOST}".to_owned()))]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("self-reference should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ResolutionError::ReferenceCycle { key }
                if key == "HOST"
        )));
    }

    #[test]
    fn cyclic_reference_is_reported() {
        let configuration = configuration(&[
            ("A", ConfigurationValue::String("${B}".to_owned())),
            ("B", ConfigurationValue::String("${C}".to_owned())),
            ("C", ConfigurationValue::String("${A}".to_owned())),
        ]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("cycle should fail");

        assert!(
            errors
                .iter()
                .any(|error| matches!(error, ResolutionError::ReferenceCycle { .. }))
        );
    }

    #[test]
    fn non_string_values_are_not_reference_resolved() {
        let configuration = configuration(&[("PORT", ConfigurationValue::Integer(8080))]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("integer value should resolve");

        assert_eq!(
            resolved
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn malformed_reference_is_rejected() {
        let configuration =
            configuration(&[("ADDRESS", ConfigurationValue::String("${HOST".to_owned()))]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("malformed reference should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ResolutionError::UnsupportedReference { key, .. }
                if key == "ADDRESS"
        )));
    }

    #[test]
    fn reference_to_non_string_value_is_rejected() {
        let configuration = configuration(&[
            ("PORT", ConfigurationValue::Integer(8080)),
            (
                "PORT_ALIAS",
                ConfigurationValue::String("${PORT}".to_owned()),
            ),
        ]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("non-string reference target should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ResolutionError::UnsupportedReference { key, reference }
                if key == "PORT_ALIAS" && reference == "PORT"
        )));
    }

    #[test]
    fn resolution_is_deterministically_ordered() {
        let configuration = configuration(&[
            ("Z", ConfigurationValue::String("z".to_owned())),
            ("A", ConfigurationValue::String("a".to_owned())),
            ("M", ConfigurationValue::String("m".to_owned())),
        ]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("configuration should resolve");

        let keys: Vec<_> = resolved.iter().map(|(key, _)| key).collect();

        assert_eq!(keys, vec!["A", "M", "Z"]);
    }

    #[test]
    fn resolution_does_not_mutate_the_validated_configuration() {
        let configuration = configuration(&[
            ("HOST", ConfigurationValue::String("localhost".to_owned())),
            ("ADDRESS", ConfigurationValue::String("${HOST}".to_owned())),
        ]);

        let before: Vec<_> = configuration.iter().collect();

        let _resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("configuration should resolve");

        let after: Vec<_> = configuration.iter().collect();

        assert_eq!(before, after);
    }

    #[test]
    fn failed_resolution_does_not_return_partial_output() {
        let configuration = configuration(&[
            ("VALID", ConfigurationValue::String("value".to_owned())),
            (
                "INVALID",
                ConfigurationValue::String("${MISSING}".to_owned()),
            ),
        ]);

        let result = ConfigurationResolver::new().resolve(&configuration);

        assert!(result.is_err());
    }

    #[test]
    fn resolution_errors_do_not_expose_raw_values() {
        let configuration = configuration(&[(
            "SECRET",
            ConfigurationValue::String("${MISSING_SECRET}".to_owned()),
        )]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("missing reference should fail");

        let rendered = errors.to_string();

        assert!(rendered.contains("SECRET"));
        assert!(rendered.contains("MISSING_SECRET"));
        assert!(!rendered.contains("secret-value"));
    }

    #[test]
    fn unused_defaults_are_deterministically_applied() {
        let configuration = configuration(&[]);
        let resolver = ConfigurationResolver::new()
            .with_default("Z", ConfigurationValue::String("z".to_owned()))
            .expect("default should be accepted")
            .with_default("A", ConfigurationValue::String("a".to_owned()))
            .expect("default should be accepted");

        let resolved = resolver
            .resolve(&configuration)
            .expect("configuration should resolve");

        let keys: Vec<_> = resolved.iter().map(|(key, _)| key).collect();

        assert_eq!(keys, vec!["A", "Z"]);
    }

    #[test]
    fn resolution_error_collection_is_deterministic() {
        let configuration = configuration(&[
            ("A", ConfigurationValue::String("${MISSING_A}".to_owned())),
            ("B", ConfigurationValue::String("${MISSING_B}".to_owned())),
        ]);

        let errors = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect_err("missing references should fail");

        let keys: Vec<_> = errors
            .iter()
            .filter_map(|error| match error {
                ResolutionError::UnresolvedReference { key, .. } => Some(key.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(keys, vec!["A", "B"]);
    }

    #[test]
    fn unused_private_resolution_state_does_not_change_public_result() {
        let configuration = configuration(&[("A", ConfigurationValue::String("a".to_owned()))]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("configuration should resolve");

        assert_eq!(resolved.len(), 1);
    }

    #[test]
    fn reference_resolution_does_not_depend_on_visit_order() {
        let configuration = configuration(&[
            ("B", ConfigurationValue::String("${A}".to_owned())),
            ("A", ConfigurationValue::String("a".to_owned())),
        ]);

        let resolved = ConfigurationResolver::new()
            .resolve(&configuration)
            .expect("configuration should resolve");

        assert_eq!(
            resolved.get("B").and_then(ConfigurationValue::as_string),
            Some("a")
        );
    }

    #[test]
    fn reference_errors_are_safe_to_render() {
        let error = ResolutionError::UnresolvedReference {
            key: "API_URL".to_owned(),
            reference: "HOST".to_owned(),
        };

        let rendered = error.to_string();

        assert_eq!(
            rendered,
            "unresolved configuration reference 'HOST' in key 'API_URL'"
        );
    }

    #[test]
    fn resolution_errors_can_be_consumed() {
        let mut errors = ResolutionErrors::new();
        errors.push(ResolutionError::InvalidKey { key: String::new() });

        assert_eq!(errors.len(), 1);
        assert!(!errors.is_empty());
        assert_eq!(errors.into_vec().len(), 1);
    }
}
