use std::collections::BTreeMap;
use std::fmt;

/// Generic parsed configuration values.
///
/// This type intentionally contains only source-neutral scalar values. It does
/// not encode engine-specific configuration semantics.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigurationValue {
    String(String),
    Boolean(bool),
    Integer(i64),
    Float(f64),
}

impl ConfigurationValue {
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Boolean(_) => "boolean",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
        }
    }

    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }
}

/// A parsed, source-neutral configuration representation.
///
/// Parser implementations can construct this value, while validation
/// implementations inspect it without knowing where the configuration came
/// from or what an engine-specific field means.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedConfiguration {
    values: BTreeMap<String, ConfigurationValue>,
}

impl ParsedConfiguration {
    #[must_use]
    pub fn new(values: BTreeMap<String, ConfigurationValue>) -> Self {
        Self { values }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: ConfigurationValue,
    ) -> Option<ConfigurationValue> {
        self.values.insert(key.into(), value)
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

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ConfigurationValue)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }
}

/// A structurally validated configuration.
///
/// The contained parsed configuration is immutable through this type's public
/// API. Engine-specific semantic validation may build on this boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedConfiguration {
    configuration: ParsedConfiguration,
}

impl ValidatedConfiguration {
    fn new(configuration: ParsedConfiguration) -> Self {
        Self { configuration }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ConfigurationValue> {
        self.configuration.get(key)
    }

    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.configuration.contains(key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.configuration.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.configuration.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ConfigurationValue)> {
        self.configuration.iter()
    }

    #[must_use]
    pub fn as_parsed(&self) -> &ParsedConfiguration {
        &self.configuration
    }
}

/// Validates parsed configuration using Core-owned structural rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigurationValidator {
    required_keys: Vec<String>,
}

impl ConfigurationValidator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a required configuration key.
    #[must_use]
    pub fn require_key(mut self, key: impl Into<String>) -> Self {
        self.required_keys.push(key.into());
        self
    }

    /// Validates a parsed configuration without mutating it.
    pub fn validate(
        &self,
        configuration: ParsedConfiguration,
    ) -> Result<ValidatedConfiguration, ValidationErrors> {
        let mut errors = ValidationErrors::default();

        for key in &self.required_keys {
            if key.is_empty() {
                errors.push(ValidationError::InvalidKey { key: key.clone() });
            } else if !configuration.contains(key) {
                errors.push(ValidationError::MissingRequiredKey { key: key.clone() });
            }
        }

        if errors.is_empty() {
            Ok(ValidatedConfiguration::new(configuration))
        } else {
            Err(errors)
        }
    }
}

/// An engine/platform supplied semantic validation boundary.
///
/// Core owns the validation mechanism, while callers supply domain-specific
/// meaning through an implementation of this trait.
pub trait SemanticValidator {
    type Error: fmt::Display;

    fn validate(&self, configuration: &ValidatedConfiguration) -> Result<(), Self::Error>;
}

/// A single configuration validation failure.
///
/// Raw configuration values are intentionally excluded from errors so that
/// invalid configuration cannot accidentally expose sensitive values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    InvalidKey {
        key: String,
    },
    MissingRequiredKey {
        key: String,
    },
    InvalidType {
        key: String,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey { key } => {
                write!(f, "invalid configuration key: {key}")
            }
            Self::MissingRequiredKey { key } => {
                write!(f, "required configuration key is missing: {key}")
            }
            Self::InvalidType {
                key,
                expected,
                actual,
            } => write!(
                f,
                "configuration key '{key}' has type '{actual}', expected '{expected}'"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// A deterministic collection of validation failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValidationErrors {
    errors: Vec<ValidationError>,
}

impl ValidationErrors {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, error: ValidationError) {
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

    pub fn iter(&self) -> impl Iterator<Item = &ValidationError> {
        self.errors.iter()
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ValidationError> {
        self.errors
    }
}

impl fmt::Display for ValidationErrors {
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

impl std::error::Error for ValidationErrors {}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration() -> ParsedConfiguration {
        let mut configuration = ParsedConfiguration::empty();
        configuration.insert(
            "ENGINE_ID",
            ConfigurationValue::String("quran-engine".to_owned()),
        );
        configuration.insert("PORT", ConfigurationValue::Integer(8080));
        configuration.insert("ENABLED", ConfigurationValue::Boolean(true));
        configuration
    }

    #[test]
    fn parsed_configuration_preserves_typed_values() {
        let configuration = configuration();

        assert_eq!(
            configuration
                .get("ENGINE_ID")
                .and_then(ConfigurationValue::as_string),
            Some("quran-engine")
        );
        assert_eq!(
            configuration
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
        assert_eq!(
            configuration
                .get("ENABLED")
                .and_then(ConfigurationValue::as_bool),
            Some(true)
        );
    }

    #[test]
    fn validator_accepts_present_required_keys() {
        let validator = ConfigurationValidator::new().require_key("ENGINE_ID");

        let validated = validator
            .validate(configuration())
            .expect("configuration should be valid");

        assert_eq!(validated.len(), 3);
        assert_eq!(
            validated
                .get("ENGINE_ID")
                .and_then(ConfigurationValue::as_string),
            Some("quran-engine")
        );
    }

    #[test]
    fn validator_reports_missing_required_keys() {
        let validator = ConfigurationValidator::new()
            .require_key("ENGINE_ID")
            .require_key("DATABASE_URL");

        let mut configuration = ParsedConfiguration::empty();
        configuration.insert(
            "ENGINE_ID",
            ConfigurationValue::String("quran-engine".to_owned()),
        );

        let errors = validator
            .validate(configuration)
            .expect_err("missing key should fail validation");

        assert_eq!(errors.len(), 1);
        assert!(matches!(
            errors.iter().next(),
            Some(ValidationError::MissingRequiredKey { key })
                if key == "DATABASE_URL"
        ));
    }

    #[test]
    fn validator_reports_multiple_errors_deterministically() {
        let validator = ConfigurationValidator::new()
            .require_key("SECOND")
            .require_key("FIRST");

        let errors = validator
            .validate(ParsedConfiguration::empty())
            .expect_err("required keys should be missing");

        let messages: Vec<_> = errors.iter().map(ToString::to_string).collect();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "required configuration key is missing: SECOND");
        assert_eq!(messages[1], "required configuration key is missing: FIRST");
    }

    #[test]
    fn invalid_empty_required_key_is_reported() {
        let validator = ConfigurationValidator::new().require_key("");

        let errors = validator
            .validate(ParsedConfiguration::empty())
            .expect_err("empty key should fail validation");

        assert_eq!(
            errors.into_vec(),
            vec![ValidationError::InvalidKey { key: String::new() }]
        );
    }

    #[test]
    fn validation_does_not_mutate_input() {
        let configuration = configuration();
        let original = configuration.clone();

        let validator = ConfigurationValidator::new().require_key("ENGINE_ID");

        let validated = validator
            .validate(configuration)
            .expect("configuration should be valid");

        assert_eq!(*validated.as_parsed(), original);
    }

    #[test]
    fn validation_errors_do_not_contain_raw_values() {
        let error = ValidationError::InvalidType {
            key: "API_KEY".to_owned(),
            expected: "integer".to_owned(),
            actual: "string".to_owned(),
        };

        let rendered = error.to_string();

        assert!(rendered.contains("API_KEY"));
        assert!(rendered.contains("string"));
        assert!(rendered.contains("integer"));
        assert!(!rendered.contains("secret-value"));
    }

    struct EngineSemantics;

    impl SemanticValidator for EngineSemantics {
        type Error = SemanticError;

        fn validate(&self, configuration: &ValidatedConfiguration) -> Result<(), Self::Error> {
            match configuration.get("ENGINE_ID") {
                Some(ConfigurationValue::String(value)) if !value.is_empty() => Ok(()),
                _ => Err(SemanticError),
            }
        }
    }

    #[derive(Debug)]
    struct SemanticError;

    impl fmt::Display for SemanticError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("engine semantic validation failed")
        }
    }

    #[test]
    fn engine_semantic_validation_is_an_explicit_extension_point() {
        let validator = ConfigurationValidator::new().require_key("ENGINE_ID");
        let validated = validator
            .validate(configuration())
            .expect("core validation should succeed");

        EngineSemantics
            .validate(&validated)
            .expect("engine semantics should succeed");
    }
}
