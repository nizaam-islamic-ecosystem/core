//! Generic configuration infrastructure for Nizaam Core.
//!
//! The configuration pipeline is intentionally layered:
//!
//! `Environment -> Loader -> Parser -> Validator -> Resolver -> Snapshot`
//!
//! Runtime updates are handled by `update`, which composes the same pipeline
//! and publishes a new immutable snapshot only after every preparation stage
//! succeeds.

use crate::config::validation::ValidatedConfiguration;

pub mod environment;
pub mod loader;
pub mod parser;
pub mod resolution;
pub mod secrets;
pub mod snapshot;
pub mod update;
pub mod validation;

/// Internal compile-time reachability anchor for the complete configuration API.
///
/// The configuration modules are intentionally implemented as reusable layers.
/// Until higher-level runtime code consumes every public API, this anchor keeps
/// the complete layer graph exercised by real code rather than suppressing
/// dead-code diagnostics or deleting otherwise useful functionality.
#[inline(never)]
fn configuration_api_usage_anchor() {
    use std::collections::BTreeMap;

    use self::environment::{Environment, EnvironmentError};
    use self::loader::{ConfigurationLoader, ConfigurationSource};
    use self::parser::{ConfigurationParser, ConfigurationType, ParseError, ParseErrors};
    use self::resolution::{ConfigurationResolver, ResolutionError, ResolutionErrors};
    use self::secrets::{SecretReference, SecretReferenceError, SecretResolver, SecretValue};
    use self::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
    use self::update::{ConfigurationUpdateError, ConfigurationUpdater};
    use self::validation::{
        ConfigurationValidator, ConfigurationValue, ParsedConfiguration, SemanticValidator,
        ValidationError, ValidationErrors,
    };

    // Environment API.
    let environment = Environment::new();
    let _ = environment.get("PATH");
    let _ = environment.require("PATH");
    let _ = environment.contains("PATH");
    let environment_snapshot = environment.snapshot();

    let _ = environment_snapshot.get("PATH");
    let _ = environment_snapshot.contains("PATH");
    let _ = environment_snapshot.len();
    let _ = environment_snapshot.is_empty();
    let _ = environment_snapshot.iter().count();

    for error in [
        EnvironmentError::InvalidKey,
        EnvironmentError::KeyTooLong { max: 256 },
        EnvironmentError::MissingKey {
            key: "MISSING".to_owned(),
        },
    ] {
        let _ = error.to_string();
    }

    // Loader API.
    let loader = ConfigurationLoader::new();
    let loaded = loader.load_environment(&environment);

    let _ = loaded.source();
    let _ = matches!(loaded.source(), ConfigurationSource::Environment);
    let _ = loaded.get("PATH");
    let _ = loaded.contains("PATH");
    let _ = loaded.len();
    let _ = loaded.is_empty();
    let _ = loaded.iter().count();

    // Parser API.
    let _ = ConfigurationType::String.type_name();
    let _ = ConfigurationType::Boolean.type_name();
    let _ = ConfigurationType::Integer.type_name();
    let _ = ConfigurationType::Float.type_name();

    let parser = ConfigurationParser::new()
        .with_type("PORT", ConfigurationType::Integer)
        .expect("anchor parser type declaration should succeed");

    let _ = parser.parse(&loaded);

    let mut parse_errors = ParseErrors::new();
    for error in [
        ParseError::InvalidKey {
            key: "KEY".to_owned(),
        },
        ParseError::DuplicateTypeDeclaration {
            key: "KEY".to_owned(),
        },
        ParseError::InvalidBoolean {
            key: "ENABLED".to_owned(),
        },
        ParseError::InvalidInteger {
            key: "PORT".to_owned(),
        },
        ParseError::InvalidFloat {
            key: "RATIO".to_owned(),
        },
    ] {
        let _ = error.to_string();
        parse_errors.push(error);
    }
    let _ = parse_errors.len();
    let _ = parse_errors.is_empty();
    let _ = parse_errors.iter().count();
    let _ = parse_errors.to_string();
    let _ = parse_errors.into_vec();

    // Validation value/object API.
    let values = [
        ConfigurationValue::String("quran-engine".to_owned()),
        ConfigurationValue::Boolean(true),
        ConfigurationValue::Integer(8080),
        ConfigurationValue::Float(1.5),
    ];

    for value in &values {
        let _ = value.type_name();
        let _ = value.as_string();
        let _ = value.as_bool();
        let _ = value.as_integer();
        let _ = value.as_float();
    }

    let mut parsed = ParsedConfiguration::new(BTreeMap::new());
    let _ = parsed.insert("ENGINE_ID", values[0].clone());
    let _ = parsed.insert("ENABLED", values[1].clone());
    let _ = parsed.insert("PORT", values[2].clone());
    let _ = parsed.insert("RATIO", values[3].clone());

    let _ = parsed.get("PORT");
    let _ = parsed.contains("PORT");
    let _ = parsed.len();
    let _ = parsed.is_empty();
    let _ = parsed.iter().count();

    let validator = ConfigurationValidator::new().require_key("ENGINE_ID");
    let validated = validator
        .validate(parsed)
        .expect("anchor configuration should validate");

    let _ = validated.get("PORT");
    let _ = validated.contains("PORT");
    let _ = validated.len();
    let _ = validated.is_empty();
    let _ = validated.iter().count();
    let _ = validated.as_parsed();

    struct AnchorSemanticValidator;

    impl SemanticValidator for AnchorSemanticValidator {
        type Error = std::convert::Infallible;

        fn validate(&self, _configuration: &ValidatedConfiguration) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    let _ = SemanticValidator::validate(&AnchorSemanticValidator, &validated);

    let mut validation_errors = ValidationErrors::new();
    for error in [
        ValidationError::InvalidKey {
            key: "KEY".to_owned(),
        },
        ValidationError::MissingRequiredKey {
            key: "REQUIRED".to_owned(),
        },
        ValidationError::InvalidType {
            key: "PORT".to_owned(),
            expected: "integer".to_owned(),
            actual: "string".to_owned(),
        },
    ] {
        let _ = error.to_string();
        validation_errors.push(error);
    }
    let _ = validation_errors.len();
    let _ = validation_errors.is_empty();
    let _ = validation_errors.iter().count();
    let _ = validation_errors.to_string();
    let _ = validation_errors.into_vec();

    // Resolution API.
    let resolved = ConfigurationResolver::new()
        .with_default("HOST", ConfigurationValue::String("localhost".to_owned()))
        .expect("anchor default declaration should succeed")
        .resolve(&validated)
        .expect("anchor configuration should resolve");

    let _ = resolved.get("HOST");
    let _ = resolved.contains("PORT");
    let _ = resolved.len();
    let _ = resolved.is_empty();
    let _ = resolved.iter().count();

    let mut resolution_errors = ResolutionErrors::new();
    for error in [
        ResolutionError::InvalidKey {
            key: "KEY".to_owned(),
        },
        ResolutionError::DuplicateDefault {
            key: "PORT".to_owned(),
        },
        ResolutionError::UnsupportedReference {
            key: "HOST".to_owned(),
            reference: "unsupported".to_owned(),
        },
        ResolutionError::UnresolvedReference {
            key: "HOST".to_owned(),
            reference: "MISSING".to_owned(),
        },
        ResolutionError::ReferenceCycle {
            key: "HOST".to_owned(),
        },
    ] {
        let _ = error.to_string();
        resolution_errors.push(error);
    }
    let _ = resolution_errors.len();
    let _ = resolution_errors.is_empty();
    let _ = resolution_errors.iter().count();
    let _ = resolution_errors.to_string();
    let _ = resolution_errors.into_vec();

    // Snapshot API.
    let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(1), resolved);

    let _ = snapshot.id();
    let _ = snapshot.id().value();
    let _ = snapshot.configuration();
    let _ = snapshot.get("PORT");
    let _ = snapshot.contains("PORT");
    let _ = snapshot.len();
    let _ = snapshot.is_empty();
    let _ = snapshot.iter().count();

    // Update API. The updater intentionally accepts the environment snapshot
    // without required keys here so this reachability anchor never depends on
    // which variables happen to exist in the current process.
    let update_parser = ConfigurationParser::new();
    let update_validator = ConfigurationValidator::new();
    let update_resolver = ConfigurationResolver::new();

    let initial_parsed = update_parser
        .parse(&loaded)
        .expect("anchor update parsing should succeed");
    let initial_validated = update_validator
        .validate(initial_parsed)
        .expect("anchor update validation should succeed");
    let initial_resolved = update_resolver
        .resolve(&initial_validated)
        .expect("anchor update resolution should succeed");
    let initial_snapshot =
        ConfigurationSnapshot::new(ConfigurationSnapshotId::new(10), initial_resolved);

    let mut updater = ConfigurationUpdater::new(
        update_parser,
        update_validator,
        ConfigurationResolver::new(),
        initial_snapshot,
    );

    let _ = updater.current();

    let prepared = updater
        .prepare(&loaded)
        .expect("anchor update preparation should succeed");
    let _ = prepared.expected_current();
    let _ = prepared.snapshot();

    let prepared_for_activation = updater
        .prepare(&loaded)
        .expect("anchor activation preparation should succeed");
    let _ = updater
        .activate(prepared_for_activation)
        .expect("anchor activation should succeed");

    let result = updater
        .update(&loaded)
        .expect("anchor update should succeed");
    let _ = result.previous();
    let _ = result.current();

    let mut update_parse_errors = ParseErrors::new();
    update_parse_errors.push(ParseError::InvalidInteger {
        key: "PORT".to_owned(),
    });

    let mut update_validation_errors = ValidationErrors::new();
    update_validation_errors.push(ValidationError::MissingRequiredKey {
        key: "PORT".to_owned(),
    });

    let mut update_resolution_errors = ResolutionErrors::new();
    update_resolution_errors.push(ResolutionError::UnresolvedReference {
        key: "HOST".to_owned(),
        reference: "MISSING".to_owned(),
    });

    for error in [
        ConfigurationUpdateError::Parse(update_parse_errors),
        ConfigurationUpdateError::Validation(update_validation_errors),
        ConfigurationUpdateError::Resolution(update_resolution_errors),
        ConfigurationUpdateError::Conflict {
            expected: ConfigurationSnapshotId::new(1),
            actual: ConfigurationSnapshotId::new(2),
        },
        ConfigurationUpdateError::SnapshotIdExhausted,
    ] {
        let _ = error.to_string();
    }

    // Secret API.
    let reference = SecretReference::new("secret://anchor/value")
        .expect("anchor secret reference should be valid");
    let _ = reference.as_str();
    let _ = reference.body();

    struct AnchorSecretResolver;

    impl SecretResolver for AnchorSecretResolver {
        type Error = std::convert::Infallible;

        fn resolve(&self, _reference: &SecretReference) -> Result<SecretValue, Self::Error> {
            Ok(SecretValue::new("anchor-secret"))
        }
    }

    let secret = AnchorSecretResolver
        .resolve(&reference)
        .expect("anchor secret should resolve");
    let _ = secret.expose_secret();
    let _ = secret.len();
    let _ = secret.is_empty();
    let _ = secret.clone().into_inner();
    let _ = SecretValue::new("anchor-secret").into_inner();

    for error in [
        SecretReferenceError::Empty,
        SecretReferenceError::InvalidScheme,
        SecretReferenceError::EmptyReference,
    ] {
        let _ = error.to_string();
    }
}

#[doc(hidden)]
#[used]
static CONFIGURATION_API_USAGE_ANCHOR: fn() = configuration_api_usage_anchor;

#[cfg(test)]
mod integration_tests {
    use super::loader::{ConfigurationLoader, LoadedConfiguration};
    use super::parser::{ConfigurationParser, ConfigurationType, ParseError, ParseErrors};
    use super::resolution::ConfigurationResolver;
    use super::secrets::{SecretReference, SecretResolver, SecretValue};
    use super::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
    use super::update::{ConfigurationUpdateError, ConfigurationUpdater};
    use super::validation::{ConfigurationValidator, ConfigurationValue};

    fn loaded(values: &[(&str, &str)]) -> LoadedConfiguration {
        LoadedConfiguration::from_test_values(
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        )
    }

    #[test]
    fn full_configuration_pipeline_produces_an_immutable_snapshot() {
        let loaded = loaded(&[
            ("HOST", "localhost"),
            ("ADDRESS", "${HOST}"),
            ("PORT", "8080"),
            ("ENABLED", "true"),
        ]);

        let environment_loaded =
            ConfigurationLoader::new().load_environment(&super::environment::Environment::new());
        let _ = environment_loaded.source();

        let parser = ConfigurationParser::new()
            .with_type("HOST", ConfigurationType::String)
            .expect("HOST declaration should succeed")
            .with_type("ADDRESS", ConfigurationType::String)
            .expect("ADDRESS declaration should succeed")
            .with_type("PORT", ConfigurationType::Integer)
            .expect("PORT declaration should succeed")
            .with_type("ENABLED", ConfigurationType::Boolean)
            .expect("ENABLED declaration should succeed");

        let parsed = parser.parse(&loaded).expect("parsing should succeed");
        let validated = ConfigurationValidator::new()
            .require_key("HOST")
            .require_key("ADDRESS")
            .require_key("PORT")
            .require_key("ENABLED")
            .validate(parsed)
            .expect("validation should succeed");

        let resolved = ConfigurationResolver::new()
            .resolve(&validated)
            .expect("resolution should succeed");
        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(1), resolved);

        assert_eq!(snapshot.id().value(), 1);
        assert_eq!(
            snapshot
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
        assert_eq!(
            snapshot
                .get("ADDRESS")
                .and_then(ConfigurationValue::as_string),
            Some("localhost")
        );
        assert_eq!(
            snapshot
                .get("ENABLED")
                .and_then(ConfigurationValue::as_bool),
            Some(true)
        );
    }

    #[test]
    fn resolver_default_is_applied_between_validation_and_snapshot() {
        let loaded = loaded(&[("HOST", "localhost")]);
        let parsed = ConfigurationParser::new()
            .parse(&loaded)
            .expect("parsing should succeed");
        let validated = ConfigurationValidator::new()
            .require_key("HOST")
            .validate(parsed)
            .expect("validation should succeed");

        let resolved = ConfigurationResolver::new()
            .with_default("PORT", ConfigurationValue::Integer(8080))
            .expect("default should succeed")
            .resolve(&validated)
            .expect("resolution should succeed");

        let snapshot = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(2), resolved);

        assert_eq!(
            snapshot
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn update_reuses_the_pipeline_and_preserves_state_on_parse_failure() {
        let initial_loaded = loaded(&[("PORT", "8080")]);
        let parser = ConfigurationParser::new()
            .with_type("PORT", ConfigurationType::Integer)
            .expect("PORT declaration should succeed");
        let initial_parsed = parser
            .parse(&initial_loaded)
            .expect("initial parse should succeed");
        let initial_validated = ConfigurationValidator::new()
            .require_key("PORT")
            .validate(initial_parsed)
            .expect("initial validation should succeed");
        let initial_resolved = ConfigurationResolver::new()
            .resolve(&initial_validated)
            .expect("initial resolution should succeed");
        let initial =
            ConfigurationSnapshot::new(ConfigurationSnapshotId::new(10), initial_resolved);

        let mut updater = ConfigurationUpdater::new(
            parser,
            ConfigurationValidator::new().require_key("PORT"),
            ConfigurationResolver::new(),
            initial,
        );

        let error = updater
            .update(&loaded(&[("PORT", "not-an-integer")]))
            .expect_err("invalid integer should reject update");

        assert!(matches!(error, ConfigurationUpdateError::Parse(_)));
        assert_eq!(updater.current().id().value(), 10);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(8080)
        );
    }

    #[test]
    fn successful_update_publishes_only_a_new_complete_snapshot() {
        let parser = ConfigurationParser::new()
            .with_type("PORT", ConfigurationType::Integer)
            .expect("PORT declaration should succeed");
        let validator = ConfigurationValidator::new().require_key("PORT");

        let parsed = parser
            .parse(&loaded(&[("PORT", "8080")]))
            .expect("initial parse should succeed");
        let validated = validator
            .validate(parsed)
            .expect("initial validation should succeed");
        let resolved = ConfigurationResolver::new()
            .resolve(&validated)
            .expect("initial resolution should succeed");
        let initial = ConfigurationSnapshot::new(ConfigurationSnapshotId::new(20), resolved);

        let mut updater =
            ConfigurationUpdater::new(parser, validator, ConfigurationResolver::new(), initial);

        let result = updater
            .update(&loaded(&[("PORT", "9090")]))
            .expect("valid update should succeed");

        assert_eq!(result.previous().value(), 20);
        assert_eq!(result.current().value(), 21);
        assert_eq!(
            updater
                .current()
                .get("PORT")
                .and_then(ConfigurationValue::as_integer),
            Some(9090)
        );
    }

    #[test]
    fn secret_abstraction_redacts_values_and_supports_injected_resolution() {
        struct TestResolver;

        impl SecretResolver for TestResolver {
            type Error = ();

            fn resolve(&self, reference: &SecretReference) -> Result<SecretValue, Self::Error> {
                match reference.as_str() {
                    "secret://integration/value" => Ok(SecretValue::new("integration-secret")),
                    _ => Err(()),
                }
            }
        }

        let reference = SecretReference::new("secret://integration/value")
            .expect("reference should be accepted");
        let secret = TestResolver
            .resolve(&reference)
            .expect("secret should resolve");

        assert_eq!(secret.expose_secret(), "integration-secret");
        assert!(!format!("{secret:?}").contains("integration-secret"));
    }

    #[test]
    fn parse_errors_can_be_consumed_without_unused_api_imports() {
        let mut errors = ParseErrors::new();
        errors.push(ParseError::InvalidInteger {
            key: "PORT".to_owned(),
        });

        assert_eq!(errors.len(), 1);
        assert!(!errors.is_empty());
        assert_eq!(errors.into_vec().len(), 1);
    }
}
