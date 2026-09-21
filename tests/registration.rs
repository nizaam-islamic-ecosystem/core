//! Phase 15 Control Plane registration integration tests.
//!
//! These tests exercise the public registration boundary from outside the
//! `control_plane` module. Registration is treated as declarative engine
//! metadata. Registry owns discovery metadata, while Membership owns the
//! routing membership view.

use nizaam_core::capability::CapabilityDefinition;
use nizaam_core::contracts::{ContractDescriptor, Interaction, PayloadDescriptor, Version};
use nizaam_core::control_plane::destination::{
    DestinationEligibilityError, DestinationEligibilityInput, DestinationRequest,
    eligible_destinations,
};
use nizaam_core::control_plane::membership::Membership;
use nizaam_core::control_plane::observations::Observations;
use nizaam_core::control_plane::registration::{
    Endpoint, EngineRegistration, RuntimeRegistrationMetadata,
};
use nizaam_core::control_plane::registry::{EngineRegistry, RegistryError};
use nizaam_core::health::ReadinessReport;
use nizaam_core::identity::{CapabilityId, ContractId, EngineId, EngineInstanceId};
use nizaam_core::runtime::LifecycleState;
use std::sync::{Arc, Barrier};

fn engine_id(value: &str) -> EngineId {
    EngineId::new(value).expect("test engine id must be valid")
}

fn instance_id(value: &str) -> EngineInstanceId {
    EngineInstanceId::new(value).expect("test engine instance id must be valid")
}

fn capability_id(value: &str) -> CapabilityId {
    CapabilityId::new(value).expect("test capability id must be valid")
}

fn contract_id(value: &str) -> ContractId {
    ContractId::new(value).expect("test contract id must be valid")
}

fn capability(engine: &str, capability: &str) -> CapabilityDefinition {
    CapabilityDefinition::new(capability_id(capability), engine_id(engine), capability)
        .expect("test capability definition must be valid")
}

fn contract(contract: &str, capability: &str, version: Version) -> ContractDescriptor {
    ContractDescriptor::new(
        contract_id(contract),
        capability_id(capability),
        version,
        Interaction::Request,
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0))
            .expect("test payload descriptor must be valid"),
    )
}

fn registration(
    engine: &str,
    instance: &str,
    capabilities: &[&str],
    contracts: &[(&str, &str, Version)],
    endpoint: Option<&str>,
    runtime_version: Version,
    region: Option<&str>,
) -> EngineRegistration {
    let mut registration = EngineRegistration::new(engine_id(engine), instance_id(instance));

    for capability_id in capabilities {
        registration = registration
            .with_capability(capability(engine, capability_id))
            .expect("capability owner must match registration engine");
    }

    for (contract_id, capability_id, version) in contracts {
        registration =
            registration.with_contract(contract(contract_id, capability_id, version.clone()));
    }

    if let Some(address) = endpoint {
        registration = registration
            .with_endpoint(Endpoint::new(address.to_owned()).expect("test endpoint must be valid"));
    }

    let runtime = RuntimeRegistrationMetadata::new()
        .with_version(runtime_version)
        .with_lifecycle(LifecycleState::Serving)
        .with_readiness(ReadinessReport::ready());

    registration = registration.with_runtime_metadata(runtime);

    if let Some(region) = region {
        registration = registration
            .with_routing_metadata("region", region)
            .expect("test routing metadata must be valid");
    }

    registration
}

#[test]
fn complete_registration_is_preserved_across_registry_and_membership() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let registration = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.tokenize", "arabic.analyze"],
        &[
            ("arabic.analyze.v1", "arabic.analyze", Version::new(1, 0, 0)),
            ("arabic.analyze.v2", "arabic.analyze", Version::new(2, 0, 0)),
        ],
        Some("memory://arabic-01"),
        Version::new(1, 2, 3),
        Some("ap-south"),
    );

    registry
        .register(registration.clone())
        .expect("registry registration must succeed");
    membership
        .register(registration.clone())
        .expect("membership registration must succeed");

    let registry_record = registry
        .get(&instance_id("arabic-01"))
        .expect("registry record");
    let membership_record = membership
        .get(&instance_id("arabic-01"))
        .expect("membership record");

    assert_eq!(registry_record.registration(), &registration);
    assert_eq!(membership_record.registration(), &registration);
    assert_eq!(
        registry_record.registration(),
        membership_record.registration()
    );
}

#[test]
fn registration_preserves_logical_engine_and_concrete_instance_identity() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    for instance in ["arabic-01", "arabic-02", "arabic-03"] {
        let registration = registration(
            "ArabicEngine",
            instance,
            &["arabic.analyze"],
            &[],
            Some("memory://arabic"),
            Version::new(1, 0, 0),
            Some("ap-south"),
        );
        registry
            .register(registration.clone())
            .expect("registry registration");
        membership
            .register(registration)
            .expect("membership registration");
    }

    let logical_engine = engine_id("ArabicEngine");
    assert_eq!(registry.instances_for_engine(&logical_engine).len(), 3);
    assert_eq!(
        membership
            .snapshot()
            .instances_for_engine(&logical_engine)
            .count(),
        3
    );
    assert_ne!(instance_id("arabic-01"), instance_id("arabic-02"));
    assert_ne!(instance_id("arabic-02"), instance_id("arabic-03"));

    for instance in ["arabic-01", "arabic-02", "arabic-03"] {
        assert_eq!(
            registry
                .get(&instance_id(instance))
                .expect("registry record")
                .engine_id(),
            &logical_engine
        );
        assert_eq!(
            membership
                .get(&instance_id(instance))
                .expect("membership record")
                .engine_id(),
            &logical_engine
        );
    }
}

#[test]
fn registration_preserves_multiple_capabilities_and_contract_versions() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let registration = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.tokenize", "arabic.analyze", "arabic.normalize"],
        &[
            ("arabic.analyze", "arabic.analyze", Version::new(1, 0, 0)),
            ("arabic.analyze", "arabic.analyze", Version::new(2, 0, 0)),
        ],
        Some("memory://arabic-01"),
        Version::new(1, 2, 3),
        Some("ap-south"),
    );

    registry
        .register(registration.clone())
        .expect("registry registration");
    membership
        .register(registration)
        .expect("membership registration");

    let registry_registration = registry
        .get(&instance_id("arabic-01"))
        .expect("registry record")
        .registration()
        .clone();
    let membership_registration = membership
        .get(&instance_id("arabic-01"))
        .expect("membership record")
        .registration()
        .clone();

    for stored in [&registry_registration, &membership_registration] {
        assert_eq!(stored.capabilities().len(), 3);
        assert_eq!(stored.contracts().len(), 2);
        assert!(
            stored
                .capabilities()
                .iter()
                .any(|c| c.capability_id() == &capability_id("arabic.tokenize"))
        );
        assert!(
            stored
                .capabilities()
                .iter()
                .any(|c| c.capability_id() == &capability_id("arabic.analyze"))
        );
        assert!(
            stored
                .contracts()
                .iter()
                .any(|contract| contract.version == Version::new(1, 0, 0))
        );
        assert!(
            stored
                .contracts()
                .iter()
                .any(|contract| contract.version == Version::new(2, 0, 0))
        );
    }
}

#[test]
fn same_capability_can_be_advertised_by_multiple_logical_engines() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let arabic = registration(
        "ArabicEngine",
        "arabic-01",
        &["text.normalize"],
        &[],
        Some("memory://arabic-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );
    let quran = registration(
        "QuranEngine",
        "quran-01",
        &["text.normalize"],
        &[],
        Some("memory://quran-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );

    registry
        .register(arabic.clone())
        .expect("ArabicEngine registry registration");
    registry
        .register(quran.clone())
        .expect("QuranEngine registry registration");
    membership
        .register(arabic)
        .expect("ArabicEngine membership registration");
    membership
        .register(quran)
        .expect("QuranEngine membership registration");

    let capability = capability_id("text.normalize");

    let registry_matches = registry
        .records()
        .into_iter()
        .filter(|record| {
            record
                .registration()
                .capabilities()
                .iter()
                .any(|d| d.capability_id() == &capability)
        })
        .collect::<Vec<_>>();

    assert_eq!(registry_matches.len(), 2);
    assert_eq!(registry_matches[0].engine_id(), &engine_id("ArabicEngine"));
    assert_eq!(registry_matches[1].engine_id(), &engine_id("QuranEngine"));

    let membership_snapshot = membership.snapshot();
    let membership_matches = membership_snapshot
        .candidates()
        .filter(|record| {
            record
                .registration()
                .capabilities()
                .iter()
                .any(|definition| definition.capability_id() == &capability)
        })
        .collect::<Vec<_>>();

    assert_eq!(membership_matches.len(), 2);
    assert_ne!(
        membership_matches[0].engine_id(),
        membership_matches[1].engine_id()
    );
}

#[test]
fn registration_update_changes_instance_metadata_but_preserves_engine_identity() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let initial = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[],
        Some("memory://arabic-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );

    registry
        .register(initial.clone())
        .expect("initial registry registration");
    membership
        .register(initial)
        .expect("initial membership registration");

    let updated = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[],
        Some("memory://arabic-01-v2"),
        Version::new(2, 0, 0),
        Some("eu-west"),
    );

    registry.update(updated.clone()).expect("registry update");
    membership.update(updated).expect("membership update");

    let registry_registration = registry
        .get(&instance_id("arabic-01"))
        .expect("registry record")
        .registration()
        .clone();
    let membership_registration = membership
        .get(&instance_id("arabic-01"))
        .expect("membership record")
        .registration()
        .clone();

    for stored in [&registry_registration, &membership_registration] {
        assert_eq!(stored.engine_id(), &engine_id("ArabicEngine"));
        assert_eq!(stored.engine_instance_id(), &instance_id("arabic-01"));
        assert_eq!(stored.runtime().version(), Some(&Version::new(2, 0, 0)));
        assert_eq!(
            stored.endpoint().expect("endpoint").address(),
            "memory://arabic-01-v2"
        );
        assert_eq!(
            stored.routing_metadata().get("region").expect("region"),
            "eu-west"
        );
    }
}

#[test]
fn existing_instance_cannot_be_reassigned_to_another_logical_engine() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let initial = registration(
        "ArabicEngine",
        "shared-instance",
        &["text.normalize"],
        &[],
        Some("memory://shared"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );

    registry
        .register(initial.clone())
        .expect("initial registry registration");
    membership
        .register(initial)
        .expect("initial membership registration");

    let reassignment = registration(
        "QuranEngine",
        "shared-instance",
        &["text.normalize"],
        &[],
        Some("memory://shared"),
        Version::new(2, 0, 0),
        Some("eu-west"),
    );

    let registry_result = registry.register(reassignment.clone());
    let membership_result = membership.update(reassignment);

    assert!(matches!(
        registry_result,
        Err(RegistryError::EngineIdentityMismatch {
            instance_id: registered_instance_id,
            registered_engine_id,
            requested_engine_id,
        }) if registered_instance_id == instance_id("shared-instance")
            && registered_engine_id == engine_id("ArabicEngine")
            && requested_engine_id == engine_id("QuranEngine")
    ));

    assert!(matches!(
        membership_result,
        Err(
            nizaam_core::control_plane::membership::MembershipError::EngineIdentityMismatch {
                instance_id: registered_instance_id,
                registered_engine_id,
                updated_engine_id,
            },
        ) if registered_instance_id == instance_id("shared-instance")
            && registered_engine_id == engine_id("ArabicEngine")
            && updated_engine_id == engine_id("QuranEngine")
    ));

    assert_eq!(
        registry
            .get(&instance_id("shared-instance"))
            .expect("registry state")
            .engine_id(),
        &engine_id("ArabicEngine")
    );
    assert_eq!(
        membership
            .get(&instance_id("shared-instance"))
            .expect("membership state")
            .engine_id(),
        &engine_id("ArabicEngine")
    );
}

#[test]
fn registry_and_membership_remain_independent_after_registration_changes() {
    let registry = EngineRegistry::new();
    let membership = Membership::new();

    let registration = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[],
        Some("memory://arabic-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );

    registry
        .register(registration.clone())
        .expect("registry registration");
    membership
        .register(registration)
        .expect("membership registration");

    registry
        .unregister(&instance_id("arabic-01"))
        .expect("registry unregister");
    assert!(!registry.contains(&instance_id("arabic-01")));
    assert!(membership.contains(&instance_id("arabic-01")));

    membership
        .unregister(&instance_id("arabic-01"))
        .expect("membership unregister");
    assert!(!registry.contains(&instance_id("arabic-01")));
    assert!(!membership.contains(&instance_id("arabic-01")));
}

#[test]
fn membership_snapshot_preserves_registration_view_from_which_it_was_created() {
    let membership = Membership::new();

    let initial = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[],
        Some("memory://arabic-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );
    membership
        .register(initial)
        .expect("initial membership registration");

    let snapshot = membership.snapshot();
    let snapshot_version = snapshot.version();

    let updated = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[],
        Some("memory://arabic-01-v2"),
        Version::new(2, 0, 0),
        Some("eu-west"),
    );
    membership.update(updated).expect("membership update");

    let live = membership
        .get(&instance_id("arabic-01"))
        .expect("live record");
    assert_eq!(
        live.registration().runtime().version(),
        Some(&Version::new(2, 0, 0))
    );
    assert_eq!(
        live.registration()
            .routing_metadata()
            .get("region")
            .expect("live region"),
        "eu-west"
    );

    let old = snapshot
        .get(&instance_id("arabic-01"))
        .expect("snapshot record");
    assert_eq!(
        old.registration().runtime().version(),
        Some(&Version::new(1, 0, 0))
    );
    assert_eq!(
        old.registration()
            .routing_metadata()
            .get("region")
            .expect("snapshot region"),
        "ap-south"
    );

    assert_eq!(snapshot.version(), snapshot_version);
    assert!(membership.version() > snapshot.version());
}

#[test]
fn registration_does_not_fabricate_routing_eligibility_without_an_observation() {
    let membership = Membership::new();
    let observations = Observations::new();

    let registration = registration(
        "ArabicEngine",
        "arabic-01",
        &["arabic.analyze"],
        &[("arabic.analyze", "arabic.analyze", Version::new(1, 0, 0))],
        Some("memory://arabic-01"),
        Version::new(1, 0, 0),
        Some("ap-south"),
    );
    membership
        .register(registration)
        .expect("membership registration");

    let request = DestinationRequest::hard_logical(
        nizaam_core::control_plane::dependency::CapabilityRequirement::new(capability_id(
            "arabic.analyze",
        )),
    );

    let result = eligible_destinations(DestinationEligibilityInput::new(
        &request,
        &membership.snapshot(),
        &observations.snapshot(),
        &capability_id("arabic.analyze"),
        &contract("arabic.analyze", "arabic.analyze", Version::new(1, 0, 0)),
    ));

    assert!(matches!(
        result,
        Err(DestinationEligibilityError::NoEligibleDestination)
    ));
}

#[test]
fn concurrent_distinct_registrations_remain_coherent_in_registry_and_membership() {
    const INSTANCE_COUNT: usize = 8;

    let registry = Arc::new(EngineRegistry::new());
    let membership = Arc::new(Membership::new());
    let barrier = Arc::new(Barrier::new(INSTANCE_COUNT));

    let mut handles = Vec::with_capacity(INSTANCE_COUNT);

    for index in 0..INSTANCE_COUNT {
        let registry = Arc::clone(&registry);
        let membership = Arc::clone(&membership);
        let barrier = Arc::clone(&barrier);

        handles.push(std::thread::spawn(move || {
            let instance = format!("arabic-{index:02}");
            let registration = registration(
                "ArabicEngine",
                &instance,
                &["arabic.analyze"],
                &[],
                Some("memory://arabic"),
                Version::new(1, 0, 0),
                Some("ap-south"),
            );

            barrier.wait();

            registry
                .register(registration.clone())
                .expect("distinct registry registration");
            membership
                .register(registration)
                .expect("distinct membership registration");
        }));
    }

    for handle in handles {
        handle.join().expect("registration worker must not panic");
    }

    assert_eq!(registry.len(), INSTANCE_COUNT);
    assert_eq!(membership.len(), INSTANCE_COUNT);

    for index in 0..INSTANCE_COUNT {
        let id = instance_id(&format!("arabic-{index:02}"));

        assert!(registry.contains(&id));
        assert!(membership.contains(&id));

        assert_eq!(
            registry.get(&id).expect("registry record").engine_id(),
            &engine_id("ArabicEngine")
        );
        assert_eq!(
            membership.get(&id).expect("membership record").engine_id(),
            &engine_id("ArabicEngine")
        );
    }
}
