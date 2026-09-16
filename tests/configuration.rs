//! Phase 11 integration tests for configuration/runtime boundaries.
//!
//! These tests intentionally sit outside `config/` so they do not duplicate the
//! configuration module's internal integration tests. The configuration module
//! owns tests for the relationships among environment, loader, parser,
//! validation, resolution, snapshot, and update primitives. This file verifies
//! that the resulting configuration snapshot behaves correctly when consumed by
//! the runtime's `EngineContext`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nizaam_core::config::environment::Environment;
use nizaam_core::config::loader::{ConfigurationLoader, LoadedConfiguration};
use nizaam_core::config::parser::{ConfigurationParser, ConfigurationType};
use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::update::{ConfigurationUpdateError, ConfigurationUpdater};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;

const CONFIG_KEY: &str = "tests.configuration.value";
const CONFIG_KEY_TWO: &str = "tests.configuration.second";
const MISSING_REQUIRED_KEY: &str = "tests.configuration.required.missing";

fn operation_context(operation_id: &str, correlation_id: &str) -> OperationContext {
    OperationContext::new(Operation::new(
        nizaam_core::identity::OperationId::new(operation_id).unwrap(),
        nizaam_core::identity::CorrelationId::new(correlation_id).unwrap(),
    ))
}

fn resolved_snapshot(id: u64, values: &[(&str, ConfigurationValue)]) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(BTreeMap::new());

    for (key, value) in values {
        parsed.insert(*key, value.clone());
    }

    let validated = ConfigurationValidator::new()
        .validate(parsed)
        .expect("test configuration should validate");

    let resolved = ConfigurationResolver::new()
        .resolve(&validated)
        .expect("test configuration should resolve");

    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}

fn environment_loaded() -> LoadedConfiguration {
    ConfigurationLoader::new().load_environment(&Environment::new())
}

fn initial_updater(snapshot: Arc<ConfigurationSnapshot>) -> ConfigurationUpdater {
    let parser = ConfigurationParser::new();
    let validator = ConfigurationValidator::new();
    let resolver = ConfigurationResolver::new();

    ConfigurationUpdater::new(parser, validator, resolver, (*snapshot).clone())
}

#[test]
fn configuration_snapshot_can_be_attached_to_engine_context() {
    let snapshot = resolved_snapshot(
        7,
        &[(
            CONFIG_KEY,
            ConfigurationValue::String("configuration-v1".to_owned()),
        )],
    );

    let context = EngineContext::new(operation_context(
        "configuration-op-1",
        "configuration-corr-1",
    ))
    .with_configuration(Arc::clone(&snapshot));

    assert_eq!(context.configuration(), Some(snapshot.as_ref()));
    assert_eq!(context.configuration().unwrap().id().value(), 7);
    assert_eq!(
        context
            .configuration()
            .unwrap()
            .get(CONFIG_KEY)
            .and_then(ConfigurationValue::as_string),
        Some("configuration-v1")
    );
}

#[test]
fn child_engine_context_preserves_the_parent_configuration_snapshot() {
    let snapshot = resolved_snapshot(
        11,
        &[(
            CONFIG_KEY,
            ConfigurationValue::String("configuration-v11".to_owned()),
        )],
    );

    let parent = EngineContext::new(operation_context(
        "configuration-op-2",
        "configuration-corr-2",
    ))
    .with_configuration(Arc::clone(&snapshot));
    let child = parent.child();
    let child_with_deadline = parent.child_with_deadline(
        nizaam_core::runtime::Deadline::from_now(Duration::from_secs(5)).unwrap(),
    );

    assert_eq!(child.configuration(), parent.configuration());
    assert_eq!(child_with_deadline.configuration(), parent.configuration());
}

#[test]
fn configuration_snapshot_remains_stable_across_engine_context_clones() {
    let snapshot = resolved_snapshot(
        13,
        &[
            (
                CONFIG_KEY,
                ConfigurationValue::String("configuration-v13".to_owned()),
            ),
            (CONFIG_KEY_TWO, ConfigurationValue::Boolean(true)),
        ],
    );

    let context = EngineContext::new(operation_context(
        "configuration-op-3",
        "configuration-corr-3",
    ))
    .with_configuration(Arc::clone(&snapshot));
    let clone = context.clone();

    assert_eq!(clone.configuration(), context.configuration());
    assert_eq!(
        clone.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(13)
    );
    assert_eq!(
        clone
            .configuration()
            .unwrap()
            .get(CONFIG_KEY_TWO)
            .and_then(ConfigurationValue::as_bool),
        Some(true)
    );
}

#[test]
fn existing_engine_context_keeps_original_configuration_after_an_update() {
    let original = resolved_snapshot(
        20,
        &[(
            CONFIG_KEY,
            ConfigurationValue::String("original".to_owned()),
        )],
    );
    let context = EngineContext::new(operation_context(
        "configuration-op-4",
        "configuration-corr-4",
    ))
    .with_configuration(Arc::clone(&original));

    let mut updater = initial_updater(Arc::clone(&original));
    let loaded = environment_loaded();
    let result = updater
        .update(&loaded)
        .expect("an environment-backed update should succeed without validation requirements");

    assert_eq!(result.previous(), ConfigurationSnapshotId::new(20));
    assert_eq!(result.current(), ConfigurationSnapshotId::new(21));
    assert_eq!(context.configuration(), Some(original.as_ref()));
    assert_eq!(
        context.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(20)
    );
}

#[test]
fn new_engine_context_can_receive_the_newly_activated_configuration_snapshot() {
    let original = resolved_snapshot(
        30,
        &[(
            CONFIG_KEY,
            ConfigurationValue::String("original".to_owned()),
        )],
    );

    let mut updater = initial_updater(Arc::clone(&original));
    let loaded = environment_loaded();
    updater
        .update(&loaded)
        .expect("environment-backed update should succeed");

    let active = Arc::new(updater.current().clone());
    let new_context = EngineContext::new(operation_context(
        "configuration-op-5",
        "configuration-corr-5",
    ))
    .with_configuration(Arc::clone(&active));

    assert_eq!(
        new_context.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(31)
    );
    assert_ne!(new_context.configuration(), Some(original.as_ref()));
}

#[test]
fn failed_configuration_update_does_not_replace_the_snapshot_used_by_runtime() {
    let original = resolved_snapshot(
        40,
        &[(CONFIG_KEY, ConfigurationValue::String("stable".to_owned()))],
    );
    let context = EngineContext::new(operation_context(
        "configuration-op-6",
        "configuration-corr-6",
    ))
    .with_configuration(Arc::clone(&original));

    let parser = ConfigurationParser::new()
        .with_type("PATH", ConfigurationType::String)
        .expect("type declaration should succeed");
    let validator = ConfigurationValidator::new().require_key(MISSING_REQUIRED_KEY);
    let resolver = ConfigurationResolver::new();
    let mut updater = ConfigurationUpdater::new(parser, validator, resolver, (*original).clone());

    let error = updater
        .update(&environment_loaded())
        .expect_err("missing required configuration must reject the update");

    assert!(matches!(error, ConfigurationUpdateError::Validation(_)));
    assert_eq!(updater.current(), original.as_ref());
    assert_eq!(context.configuration(), Some(original.as_ref()));
}

#[test]
fn different_engine_contexts_can_use_different_configuration_snapshots() {
    let first = resolved_snapshot(
        50,
        &[(CONFIG_KEY, ConfigurationValue::String("first".to_owned()))],
    );
    let second = resolved_snapshot(
        51,
        &[(CONFIG_KEY, ConfigurationValue::String("second".to_owned()))],
    );

    let first_context = EngineContext::new(operation_context(
        "configuration-op-7",
        "configuration-corr-7",
    ))
    .with_configuration(Arc::clone(&first));
    let second_context = EngineContext::new(operation_context(
        "configuration-op-8",
        "configuration-corr-8",
    ))
    .with_configuration(Arc::clone(&second));

    assert_eq!(
        first_context.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(50)
    );
    assert_eq!(
        second_context.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(51)
    );
    assert_eq!(
        first_context
            .configuration()
            .unwrap()
            .get(CONFIG_KEY)
            .and_then(ConfigurationValue::as_string),
        Some("first")
    );
    assert_eq!(
        second_context
            .configuration()
            .unwrap()
            .get(CONFIG_KEY)
            .and_then(ConfigurationValue::as_string),
        Some("second")
    );
}
