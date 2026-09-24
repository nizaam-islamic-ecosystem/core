use crate::support::{section, show_arrow, step, success};
use nizaam_core::config::environment::Environment;
use nizaam_core::config::loader::{ConfigurationLoader, LoadedConfiguration};
use nizaam_core::config::parser::{ConfigurationParser, ConfigurationType};
use nizaam_core::config::resolution::ConfigurationResolver;
use nizaam_core::config::snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId};
use nizaam_core::config::update::{ConfigurationUpdateError, ConfigurationUpdater};
use nizaam_core::config::validation::{
    ConfigurationValidator, ConfigurationValue, ParsedConfiguration,
};
use nizaam_core::identity::{CorrelationId, OperationId};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::runtime::EngineContext;
use std::collections::BTreeMap;
use std::sync::Arc;

fn context() -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new("visual-config-operation").unwrap(),
        CorrelationId::new("visual-config-correlation").unwrap(),
    ))
}
fn snapshot(id: u64, value: &str) -> Arc<ConfigurationSnapshot> {
    let mut parsed = ParsedConfiguration::new(BTreeMap::new());
    parsed.insert(
        "visual.value".to_owned(),
        ConfigurationValue::String(value.to_owned()),
    );
    let validated = ConfigurationValidator::new().validate(parsed).unwrap();
    let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();
    Arc::new(ConfigurationSnapshot::new(
        ConfigurationSnapshotId::new(id),
        resolved,
    ))
}
fn loaded(value: &str) -> LoadedConfiguration {
    LoadedConfiguration::from_values([(String::from("visual.value"), value.to_owned())])
}

#[test]
fn visual_configuration_lineage_and_failed_update() {
    section("NIZAAM CORE — CONFIGURATION + CONTEXT");
    step(1, "controlled environment loading");
    let loaded_environment = ConfigurationLoader::new()
        .load_environment(&Environment::new())
        .unwrap();
    println!("  source: {:?}", loaded_environment.source());
    success("environment source is loaded without printing environment secrets");

    step(2, "parse → validate → resolve → immutable snapshot");
    let active = snapshot(101, "stable");
    println!("  snapshot id : {}", active.id());
    println!("  value       : safe visual value");
    success("resolved configuration is represented by an immutable snapshot");
    show_arrow("source", "load → parse → validate → resolve → snapshot");

    step(3, "propagate snapshot through EngineContext");
    let parent = EngineContext::new(context()).with_configuration(Arc::clone(&active));
    let child = parent.child();
    assert_eq!(parent.configuration(), Some(active.as_ref()));
    assert_eq!(child.configuration(), Some(active.as_ref()));
    assert_eq!(
        child.configuration().unwrap().id(),
        ConfigurationSnapshotId::new(101)
    );
    success("child context retains the same snapshot identity");

    step(4, "failed update preserves active snapshot");
    let parser = ConfigurationParser::new()
        .with_type("visual.required", ConfigurationType::String)
        .unwrap();
    let validator = ConfigurationValidator::new().require_key("visual.required");
    let resolver = ConfigurationResolver::new();
    let mut updater = ConfigurationUpdater::new(parser, validator, resolver, (*active).clone());
    let result = updater.update(&loaded("changed"));
    assert!(matches!(
        result,
        Err(ConfigurationUpdateError::Validation(_))
    ));
    assert_eq!(updater.current(), active.as_ref());
    assert_eq!(parent.configuration(), Some(active.as_ref()));
    println!("  old snapshot remains active: {}", active.id());
    success("invalid activation does not replace the active snapshot");
}
