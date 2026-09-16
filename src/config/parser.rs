use std::collections::BTreeMap;
use std::fmt;

use super::loader::LoadedConfiguration;
use super::validation::{ConfigurationValue, ParsedConfiguration};

#[cfg(test)]
use super::{environment::Environment, loader::ConfigurationLoader};

/// Describes how a raw configuration value should be converted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConfigurationType {
    String,
    Boolean,
    Integer,
    Float,
}

impl ConfigurationType {
    #[must_use]
    pub const fn type_name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Float => "float",
        }
    }
}

/// Converts loaded raw configuration into typed, source-neutral
/// `ParsedConfiguration` values.
///
/// Parsing is deliberately type-directed. Keys without an explicit type
/// declaration remain strings, so the parser never guesses configuration
/// semantics from raw text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigurationParser {
    schema: BTreeMap<String, ConfigurationType>,
}
impl ConfigurationParser {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            schema: BTreeMap::new(),
        }
    }

    /// Declares the expected type for a configuration key.
    ///
    /// Duplicate declarations are rejected instead of silently replacing the
    /// previous declaration.
    pub fn with_type(
        mut self,
        key: impl Into<String>,
        value_type: ConfigurationType,
    ) -> Result<Self, ParseError> {
        let key = key.into();

        validate_key(&key)?;

        if self.schema.insert(key.clone(), value_type).is_some() {
            return Err(ParseError::DuplicateTypeDeclaration { key });
        }

        Ok(self)
    }

    /// Parses all loaded configuration values.
    ///
    /// Unknown keys are preserved as strings. Parsing failures are collected
    /// and no partial `ParsedConfiguration` is returned.
    pub fn parse(
        &self,
        configuration: &LoadedConfiguration,
    ) -> Result<ParsedConfiguration, ParseErrors> {
        let mut parsed = ParsedConfiguration::empty();
        let mut errors = ParseErrors::new();

        for (key, raw_value) in configuration.iter() {
            let value_type = self
                .schema
                .get(key)
                .copied()
                .unwrap_or(ConfigurationType::String);

            match parse_value(key, raw_value, value_type) {
                Ok(value) => {
                    parsed.insert(key.to_owned(), value);
                }
                Err(error) => errors.push(error),
            }
        }

        if errors.is_empty() {
            Ok(parsed)
        } else {
            Err(errors)
        }
    }
}

/// A single failure encountered while converting a raw configuration value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    InvalidKey { key: String },
    DuplicateTypeDeclaration { key: String },
    InvalidBoolean { key: String },
    InvalidInteger { key: String },
    InvalidFloat { key: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey { key } => {
                write!(f, "invalid configuration key: {key}")
            }
            Self::DuplicateTypeDeclaration { key } => {
                write!(f, "duplicate configuration type declaration: {key}")
            }
            Self::InvalidBoolean { key } => {
                write!(
                    f,
                    "configuration key '{key}' could not be parsed as a boolean"
                )
            }
            Self::InvalidInteger { key } => {
                write!(
                    f,
                    "configuration key '{key}' could not be parsed as an integer"
                )
            }
            Self::InvalidFloat { key } => {
                write!(
                    f,
                    "configuration key '{key}' could not be parsed as a float"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// A deterministic collection of parse failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParseErrors {
    errors: Vec<ParseError>,
}

impl ParseErrors {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, error: ParseError) {
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

    pub fn iter(&self) -> impl Iterator<Item = &ParseError> {
        self.errors.iter()
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ParseError> {
        self.errors
    }
}

impl fmt::Display for ParseErrors {
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

impl std::error::Error for ParseErrors {}

fn validate_key(key: &str) -> Result<(), ParseError> {
    if key.is_empty() {
        return Err(ParseError::InvalidKey {
            key: key.to_owned(),
        });
    }

    Ok(())
}

fn parse_value(
    key: &str,
    raw_value: &str,
    value_type: ConfigurationType,
) -> Result<ConfigurationValue, ParseError> {
    match value_type {
        ConfigurationType::String => Ok(ConfigurationValue::String(raw_value.to_owned())),
        ConfigurationType::Boolean => raw_value
            .parse::<bool>()
            .map(ConfigurationValue::Boolean)
            .map_err(|_| ParseError::InvalidBoolean {
                key: key.to_owned(),
            }),
        ConfigurationType::Integer => raw_value
            .parse::<i64>()
            .map(ConfigurationValue::Integer)
            .map_err(|_| ParseError::InvalidInteger {
                key: key.to_owned(),
            }),
        ConfigurationType::Float => raw_value
            .parse::<f64>()
            .map(ConfigurationValue::Float)
            .map_err(|_| ParseError::InvalidFloat {
                key: key.to_owned(),
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_key(name: &str) -> String {
        format!("NIZAAM_PARSER_TEST_{name}")
    }

    fn loaded_one(key: &str, value: &str) -> LoadedConfiguration {
        LoadedConfiguration::from_test_values([(key.to_owned(), value.to_owned())])
    }

    #[test]
    fn parser_can_be_constructed() {
        assert_eq!(ConfigurationParser::new(), ConfigurationParser::new());
    }

    #[test]
    fn type_names_are_stable() {
        assert_eq!(ConfigurationType::String.type_name(), "string");
        assert_eq!(ConfigurationType::Boolean.type_name(), "boolean");
        assert_eq!(ConfigurationType::Integer.type_name(), "integer");
        assert_eq!(ConfigurationType::Float.type_name(), "float");
    }

    #[test]
    fn explicit_string_values_are_preserved() {
        let key = unique_key("STRING");
        let configuration = loaded_one(&key, " hello ");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::String)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_string),
            Some(" hello ")
        );
    }

    #[test]
    fn unknown_keys_are_preserved_as_strings() {
        let key = unique_key("UNKNOWN");
        let configuration = loaded_one(&key, "123");

        let parsed = ConfigurationParser::new()
            .parse(&configuration)
            .expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_string),
            Some("123")
        );
    }

    #[test]
    fn booleans_are_parsed() {
        let key = unique_key("BOOLEAN");
        let configuration = loaded_one(&key, "true");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Boolean)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_bool),
            Some(true)
        );
    }

    #[test]
    fn invalid_boolean_is_reported_without_raw_value() {
        let key = unique_key("INVALID_BOOLEAN");
        let configuration = loaded_one(&key, "not-a-boolean");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Boolean)
            .expect("type declaration should succeed");

        let errors = parser.parse(&configuration).expect_err("parse should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ParseError::InvalidBoolean { key: error_key } if error_key == &key
        )));
        assert!(!errors.to_string().contains("not-a-boolean"));
    }

    #[test]
    fn integers_are_parsed() {
        let key = unique_key("INTEGER");
        let configuration = loaded_one(&key, "8080");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Integer)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn negative_integers_are_parsed() {
        let key = unique_key("NEGATIVE_INTEGER");
        let configuration = loaded_one(&key, "-42");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Integer)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_integer),
            Some(-42)
        );
    }

    #[test]
    fn invalid_integer_is_reported() {
        let key = unique_key("INVALID_INTEGER");
        let configuration = loaded_one(&key, "abc");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Integer)
            .expect("type declaration should succeed");

        let errors = parser.parse(&configuration).expect_err("parse should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ParseError::InvalidInteger { key: error_key } if error_key == &key
        )));
    }

    #[test]
    fn floats_are_parsed() {
        let key = unique_key("FLOAT");
        let configuration = loaded_one(&key, "0.75");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Float)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_float),
            Some(0.75)
        );
    }

    #[test]
    fn invalid_float_is_reported() {
        let key = unique_key("INVALID_FLOAT");
        let configuration = loaded_one(&key, "abc");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Float)
            .expect("type declaration should succeed");

        let errors = parser.parse(&configuration).expect_err("parse should fail");

        assert!(errors.iter().any(|error| matches!(
            error,
            ParseError::InvalidFloat { key: error_key } if error_key == &key
        )));
    }

    #[test]
    fn empty_strings_are_valid_strings() {
        let key = unique_key("EMPTY_STRING");
        let configuration = loaded_one(&key, "");

        let parser = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::String)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        assert_eq!(
            parsed.get(&key).and_then(ConfigurationValue::as_string),
            Some("")
        );
    }

    #[test]
    fn references_are_preserved_for_resolution() {
        let host_key = unique_key("HOST");
        let address_key = unique_key("ADDRESS");
        unsafe { std::env::set_var(&host_key, "localhost") };
        unsafe { std::env::set_var(&address_key, format!("${{{host_key}}}")) };
        let configuration = ConfigurationLoader::new().load_environment(&Environment::new());
        unsafe { std::env::remove_var(&host_key) };
        unsafe { std::env::remove_var(&address_key) };

        let parser = ConfigurationParser::new()
            .with_type(&host_key, ConfigurationType::String)
            .expect("type declaration should succeed")
            .with_type(&address_key, ConfigurationType::String)
            .expect("type declaration should succeed");

        let parsed = parser.parse(&configuration).expect("parse should succeed");

        let expected = format!("${{{host_key}}}");
        assert_eq!(
            parsed
                .get(&address_key)
                .and_then(ConfigurationValue::as_string),
            Some(expected.as_str())
        );
    }

    #[test]
    fn duplicate_type_declarations_are_rejected() {
        let key = unique_key("DUPLICATE");

        let error = ConfigurationParser::new()
            .with_type(&key, ConfigurationType::Integer)
            .expect("first declaration should succeed")
            .with_type(&key, ConfigurationType::String)
            .expect_err("duplicate declaration should fail");

        assert_eq!(error, ParseError::DuplicateTypeDeclaration { key });
    }

    #[test]
    fn empty_type_keys_are_rejected() {
        let error = ConfigurationParser::new()
            .with_type("", ConfigurationType::String)
            .expect_err("empty key should fail");

        assert_eq!(error, ParseError::InvalidKey { key: String::new() });
    }

    #[test]
    fn multiple_parse_errors_are_deterministically_collected() {
        let first_key = unique_key("A");
        let second_key = unique_key("B");

        unsafe { std::env::set_var(&first_key, "bad") };
        unsafe { std::env::set_var(&second_key, "also-bad") };

        let configuration = ConfigurationLoader::new().load_environment(&Environment::new());

        unsafe { std::env::remove_var(&first_key) };
        unsafe { std::env::remove_var(&second_key) };

        let parser = ConfigurationParser::new()
            .with_type(&first_key, ConfigurationType::Integer)
            .expect("type declaration should succeed")
            .with_type(&second_key, ConfigurationType::Boolean)
            .expect("type declaration should succeed");

        let errors = parser.parse(&configuration).expect_err("parse should fail");

        let keys: Vec<_> = errors
            .iter()
            .map(|error| match error {
                ParseError::InvalidInteger { key }
                | ParseError::InvalidBoolean { key }
                | ParseError::InvalidFloat { key }
                | ParseError::InvalidKey { key }
                | ParseError::DuplicateTypeDeclaration { key } => key.as_str(),
            })
            .filter(|key| *key == first_key || *key == second_key)
            .collect();

        assert_eq!(keys, vec![first_key.as_str(), second_key.as_str()]);
    }

    #[test]
    fn invalid_configuration_does_not_produce_partial_result() {
        let valid_key = unique_key("VALID");
        let invalid_key = unique_key("INVALID");

        unsafe { std::env::set_var(&valid_key, "value") };
        unsafe { std::env::set_var(&invalid_key, "not-an-integer") };

        let configuration = ConfigurationLoader::new().load_environment(&Environment::new());

        unsafe { std::env::remove_var(&valid_key) };
        unsafe { std::env::remove_var(&invalid_key) };

        let parser = ConfigurationParser::new()
            .with_type(&invalid_key, ConfigurationType::Integer)
            .expect("type declaration should succeed");

        assert!(parser.parse(&configuration).is_err());
    }

    #[test]
    fn parser_does_not_mutate_loaded_configuration() {
        let port_key = unique_key("PORT");
        let host_key = unique_key("HOST");

        unsafe { std::env::set_var(&port_key, "8080") };
        unsafe { std::env::set_var(&host_key, "localhost") };

        let configuration = ConfigurationLoader::new().load_environment(&Environment::new());

        unsafe { std::env::remove_var(&port_key) };
        unsafe { std::env::remove_var(&host_key) };

        let before: Vec<_> = configuration.iter().collect();

        let parser = ConfigurationParser::new()
            .with_type(&port_key, ConfigurationType::Integer)
            .expect("type declaration should succeed");

        let _parsed = parser.parse(&configuration).expect("parse should succeed");

        let after: Vec<_> = configuration.iter().collect();

        assert_eq!(before, after);
    }

    #[test]
    fn parse_errors_can_be_consumed() {
        let mut errors = ParseErrors::new();

        errors.push(ParseError::InvalidInteger {
            key: "PORT".to_owned(),
        });

        assert_eq!(errors.len(), 1);
        assert!(!errors.is_empty());
        assert_eq!(errors.into_vec().len(), 1);
    }
}
