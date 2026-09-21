//! Engine registration declarations for the Control Plane.
//!
//! This module defines the declarative description presented by an engine
//! runtime when it wants to participate in the Nizaam ecosystem.
//!
//! Registration is intentionally separate from routing membership:
//!
//! ```text
//! Engine Runtime
//!       |
//!       v
//! EngineRegistration
//!       |
//!       v
//! Control Plane / Membership
//! ```
//!
//! `EngineRegistration` describes what an engine instance advertises. It does
//! not own routing state, lifecycle transitions, health evaluation, security
//! enforcement, transport connections, capability execution, or retry policy.
//! Those responsibilities remain with their respective Core subsystems.

use std::collections::BTreeMap;
use std::fmt;

use crate::capability::CapabilityDefinition;
use crate::contracts::{ContractDescriptor, Version};
use crate::health::ReadinessReport;
use crate::identity::{CapabilityId, EngineId, EngineInstanceId};
use crate::runtime::lifecycle::LifecycleState;

/// A generic communication endpoint advertised by an engine instance.
///
/// The Control Plane records endpoint metadata, but transport-specific
/// connection establishment remains owned by the transport subsystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    address: String,
}

impl Endpoint {
    /// Creates an endpoint from a non-empty address.
    ///
    /// The address is treated as opaque metadata here. Protocol-specific
    /// parsing and connection validation belong to the transport layer.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationValidationError::EmptyEndpoint`] when the
    /// supplied address is empty or whitespace-only.
    pub fn new(address: impl Into<String>) -> Result<Self, RegistrationValidationError> {
        let address = address.into();

        if address.trim().is_empty() {
            return Err(RegistrationValidationError::EmptyEndpoint);
        }

        Ok(Self { address })
    }

    /// Returns the advertised endpoint address.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }
}

/// Runtime information carried by an engine registration.
///
/// These values are observations supplied by the runtime. This type does not
/// mutate or derive runtime state and does not decide whether a destination is
/// routable.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeRegistrationMetadata {
    version: Option<Version>,
    lifecycle: Option<LifecycleState>,
    readiness: Option<ReadinessReport>,
}

impl RuntimeRegistrationMetadata {
    /// Creates empty runtime registration metadata.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            version: None,
            lifecycle: None,
            readiness: None,
        }
    }

    /// Adds the runtime version observation.
    #[must_use]
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Adds the runtime lifecycle observation.
    #[must_use]
    pub fn with_lifecycle(mut self, lifecycle: LifecycleState) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }

    /// Adds the runtime readiness observation.
    #[must_use]
    pub fn with_readiness(mut self, readiness: ReadinessReport) -> Self {
        self.readiness = Some(readiness);
        self
    }

    /// Returns the optional runtime version.
    #[must_use]
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Returns the optional lifecycle observation.
    #[must_use]
    pub const fn lifecycle(&self) -> Option<LifecycleState> {
        self.lifecycle
    }

    /// Returns the optional readiness observation.
    #[must_use]
    pub fn readiness(&self) -> Option<&ReadinessReport> {
        self.readiness.as_ref()
    }
}

/// Errors produced by structural validation of an engine registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrationValidationError {
    /// A capability is declared as owned by a different logical engine.
    CapabilityOwnerMismatch {
        capability_id: CapabilityId,
        registration_engine_id: EngineId,
        capability_engine_id: EngineId,
    },

    /// An endpoint address is empty or whitespace-only.
    EmptyEndpoint,

    /// Routing metadata contains an empty or whitespace-only key.
    EmptyRoutingMetadataKey,

    /// Routing metadata contains an empty or whitespace-only value.
    EmptyRoutingMetadataValue,
}

impl fmt::Display for RegistrationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityOwnerMismatch {
                capability_id,
                registration_engine_id,
                capability_engine_id,
            } => write!(
                formatter,
                "capability {capability_id} is owned by engine {capability_engine_id}, \
                 but registration belongs to engine {registration_engine_id}"
            ),
            Self::EmptyEndpoint => {
                formatter.write_str("an engine registration endpoint must not be empty")
            }
            Self::EmptyRoutingMetadataKey => {
                formatter.write_str("a routing metadata key must not be empty")
            }
            Self::EmptyRoutingMetadataValue => {
                formatter.write_str("a routing metadata value must not be empty")
            }
        }
    }
}

impl std::error::Error for RegistrationValidationError {}

/// Result alias for registration-local operations.
pub type RegistrationResult<T> = Result<T, RegistrationValidationError>;

/// Declarative description presented by one concrete engine runtime.
///
/// `EngineId` identifies the logical engine. `EngineInstanceId` identifies one
/// concrete runtime participant. Multiple registrations may therefore belong
/// to the same logical engine while representing different instances.
///
/// The registration is a value description. Control Plane membership state is
/// maintained separately by `membership.rs`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineRegistration {
    engine_id: EngineId,
    engine_instance_id: EngineInstanceId,
    capabilities: Vec<CapabilityDefinition>,
    contracts: Vec<ContractDescriptor>,
    endpoint: Option<Endpoint>,
    runtime: RuntimeRegistrationMetadata,
    routing_metadata: BTreeMap<String, String>,
}

impl EngineRegistration {
    /// Creates a registration for one logical engine and one concrete instance.
    #[must_use]
    pub fn new(engine_id: EngineId, engine_instance_id: EngineInstanceId) -> Self {
        Self {
            engine_id,
            engine_instance_id,
            capabilities: Vec::new(),
            contracts: Vec::new(),
            endpoint: None,
            runtime: RuntimeRegistrationMetadata::new(),
            routing_metadata: BTreeMap::new(),
        }
    }

    /// Adds a capability advertisement.
    ///
    /// The capability must identify the same logical owner as the registration.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationValidationError::CapabilityOwnerMismatch`] when
    /// the capability belongs to another logical engine.
    pub fn with_capability(mut self, capability: CapabilityDefinition) -> RegistrationResult<Self> {
        if capability.owning_engine() != &self.engine_id {
            return Err(RegistrationValidationError::CapabilityOwnerMismatch {
                capability_id: capability.capability_id().clone(),
                registration_engine_id: self.engine_id.clone(),
                capability_engine_id: capability.owning_engine().clone(),
            });
        }

        self.capabilities.push(capability);
        Ok(self)
    }

    /// Adds a supported contract descriptor.
    ///
    /// Contract compatibility remains owned by the contract subsystem and is
    /// intentionally not recalculated during registration construction.
    #[must_use]
    pub fn with_contract(mut self, contract: ContractDescriptor) -> Self {
        self.contracts.push(contract);
        self
    }

    /// Adds a transport endpoint description.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Adds runtime metadata observations.
    #[must_use]
    pub fn with_runtime_metadata(mut self, runtime: RuntimeRegistrationMetadata) -> Self {
        self.runtime = runtime;
        self
    }

    /// Adds one routing metadata entry.
    ///
    /// Routing metadata is declarative input for later policy evaluation. This
    /// method does not interpret or assign policy meaning to the key/value pair.
    ///
    /// # Errors
    ///
    /// Returns an error when the key or value is empty or whitespace-only.
    pub fn with_routing_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> RegistrationResult<Self> {
        let key = key.into();
        let value = value.into();

        if key.trim().is_empty() {
            return Err(RegistrationValidationError::EmptyRoutingMetadataKey);
        }

        if value.trim().is_empty() {
            return Err(RegistrationValidationError::EmptyRoutingMetadataValue);
        }

        self.routing_metadata.insert(key, value);
        Ok(self)
    }

    /// Validates structural and cross-field registration invariants.
    ///
    /// This validation intentionally does not:
    ///
    /// - authenticate or authorize the registering runtime,
    /// - query the Registry,
    /// - query Health,
    /// - mutate Membership,
    /// - test transport connectivity,
    /// - execute capabilities,
    /// - resolve providers,
    /// - choose a routing destination,
    /// - interpret domain semantics.
    pub fn validate(&self) -> RegistrationResult<()> {
        for capability in &self.capabilities {
            if capability.owning_engine() != &self.engine_id {
                return Err(RegistrationValidationError::CapabilityOwnerMismatch {
                    capability_id: capability.capability_id().clone(),
                    registration_engine_id: self.engine_id.clone(),
                    capability_engine_id: capability.owning_engine().clone(),
                });
            }
        }

        if let Some(endpoint) = &self.endpoint
            && endpoint.address().trim().is_empty()
        {
            return Err(RegistrationValidationError::EmptyEndpoint);
        }

        for (key, value) in &self.routing_metadata {
            if key.trim().is_empty() {
                return Err(RegistrationValidationError::EmptyRoutingMetadataKey);
            }

            if value.trim().is_empty() {
                return Err(RegistrationValidationError::EmptyRoutingMetadataValue);
            }
        }

        Ok(())
    }

    /// Returns the logical engine identifier.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine instance identifier.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        &self.engine_instance_id
    }

    /// Returns the advertised capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityDefinition] {
        &self.capabilities
    }

    /// Returns the advertised contracts.
    #[must_use]
    pub fn contracts(&self) -> &[ContractDescriptor] {
        &self.contracts
    }

    /// Returns the optional transport endpoint metadata.
    #[must_use]
    pub fn endpoint(&self) -> Option<&Endpoint> {
        self.endpoint.as_ref()
    }

    /// Returns runtime registration metadata observations.
    #[must_use]
    pub fn runtime(&self) -> &RuntimeRegistrationMetadata {
        &self.runtime
    }

    /// Returns routing metadata.
    #[must_use]
    pub fn routing_metadata(&self) -> &BTreeMap<String, String> {
        &self.routing_metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityDefinition;
    use crate::contracts::Version;
    use crate::contracts::descriptor::{Interaction, PayloadDescriptor};
    use crate::health::{ReadinessReport, ReadinessState};
    use crate::identity::{CapabilityId, ContractId, EngineId, EngineInstanceId};
    use crate::runtime::lifecycle::LifecycleState;

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("valid engine id")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("valid instance id")
    }

    fn capability(id: &str, owner: &str) -> CapabilityDefinition {
        CapabilityDefinition::new(
            CapabilityId::new(id).expect("valid capability id"),
            engine_id(owner),
            "Test Capability",
        )
        .expect("valid capability definition")
    }

    fn contract(capability_id: &str) -> ContractDescriptor {
        ContractDescriptor::new(
            ContractId::new("test.contract").expect("valid contract id"),
            CapabilityId::new(capability_id).expect("valid capability id"),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/json", Version::new(1, 0, 0))
                .expect("valid payload descriptor"),
        )
    }

    #[test]
    fn new_registration_preserves_required_identity() {
        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));

        assert_eq!(registration.engine_id().as_str(), "arabic");
        assert_eq!(registration.engine_instance_id().as_str(), "arabic-01");
        assert!(registration.capabilities().is_empty());
        assert!(registration.contracts().is_empty());
        assert!(registration.endpoint().is_none());
        assert!(registration.routing_metadata().is_empty());
        assert_eq!(registration.runtime(), &RuntimeRegistrationMetadata::new());
    }

    #[test]
    fn multiple_instances_of_one_logical_engine_are_valid() {
        let first = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));
        let second = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-02"));

        assert_eq!(first.engine_id(), second.engine_id());
        assert_ne!(first.engine_instance_id(), second.engine_instance_id());
    }

    #[test]
    fn multiple_capabilities_can_be_advertised() {
        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_capability(capability("arabic.tokenize", "arabic"))
            .expect("first capability")
            .with_capability(capability("arabic.normalize", "arabic"))
            .expect("second capability");

        assert_eq!(registration.capabilities().len(), 2);
        assert_eq!(
            registration.capabilities()[0].capability_id().as_str(),
            "arabic.tokenize"
        );
        assert_eq!(
            registration.capabilities()[1].capability_id().as_str(),
            "arabic.normalize"
        );
    }

    #[test]
    fn foreign_engine_capability_is_rejected() {
        let result = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_capability(capability("quran.search", "quran"));

        assert!(matches!(
            result,
            Err(RegistrationValidationError::CapabilityOwnerMismatch {
                capability_id,
                registration_engine_id,
                capability_engine_id
            })
                if capability_id.as_str() == "quran.search"
                    && registration_engine_id.as_str() == "arabic"
                    && capability_engine_id.as_str() == "quran"
        ));
    }

    #[test]
    fn contract_versions_can_coexist() {
        let registration = EngineRegistration::new(engine_id("quran"), instance_id("quran-01"))
            .with_contract(contract("quran.search"))
            .with_contract(ContractDescriptor::new(
                ContractId::new("test.contract").expect("valid contract id"),
                CapabilityId::new("quran.search").expect("valid capability id"),
                Version::new(2, 0, 0),
                Interaction::Request,
                PayloadDescriptor::new("application/json", Version::new(2, 0, 0))
                    .expect("valid payload descriptor"),
            ));

        assert_eq!(registration.contracts().len(), 2);
        assert_eq!(registration.contracts()[0].version, Version::new(1, 0, 0));
        assert_eq!(registration.contracts()[1].version, Version::new(2, 0, 0));
    }

    #[test]
    fn endpoint_rejects_empty_address() {
        assert_eq!(
            Endpoint::new(""),
            Err(RegistrationValidationError::EmptyEndpoint)
        );
        assert_eq!(
            Endpoint::new("   "),
            Err(RegistrationValidationError::EmptyEndpoint)
        );
    }

    #[test]
    fn endpoint_preserves_opaque_address() {
        let endpoint = Endpoint::new("grpc://arabic-01:50051").expect("valid endpoint");

        assert_eq!(endpoint.address(), "grpc://arabic-01:50051");
    }

    #[test]
    fn runtime_metadata_preserves_partial_observations() {
        let readiness = ReadinessReport::at(ReadinessState::Ready, std::time::UNIX_EPOCH);

        let metadata = RuntimeRegistrationMetadata::new()
            .with_version(Version::new(3, 2, 1))
            .with_lifecycle(LifecycleState::Serving)
            .with_readiness(readiness.clone());

        assert_eq!(metadata.version(), Some(&Version::new(3, 2, 1)));
        assert_eq!(metadata.lifecycle(), Some(LifecycleState::Serving));
        assert_eq!(metadata.readiness(), Some(&readiness));
    }

    #[test]
    fn readiness_state_is_preserved_without_interpretation() {
        for state in [
            ReadinessState::Ready,
            ReadinessState::NotReady,
            ReadinessState::Unknown,
        ] {
            let report = ReadinessReport::at(state, std::time::UNIX_EPOCH);
            let metadata = RuntimeRegistrationMetadata::new().with_readiness(report.clone());

            assert_eq!(metadata.readiness(), Some(&report));
        }
    }

    #[test]
    fn lifecycle_observation_is_preserved_without_mutating_runtime() {
        for state in [
            LifecycleState::Created,
            LifecycleState::Registering,
            LifecycleState::Serving,
            LifecycleState::Draining,
            LifecycleState::Stopped,
        ] {
            let metadata = RuntimeRegistrationMetadata::new().with_lifecycle(state);

            assert_eq!(metadata.lifecycle(), Some(state));
        }
    }

    #[test]
    fn routing_metadata_accepts_multiple_entries() {
        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_routing_metadata("region", "ap-south")
            .expect("region metadata")
            .with_routing_metadata("zone", "mumbai-a")
            .expect("zone metadata");

        assert_eq!(
            registration.routing_metadata().get("region"),
            Some(&"ap-south".to_string())
        );
        assert_eq!(
            registration.routing_metadata().get("zone"),
            Some(&"mumbai-a".to_string())
        );
    }

    #[test]
    fn routing_metadata_rejects_empty_key() {
        let result = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_routing_metadata("   ", "value");

        assert_eq!(
            result,
            Err(RegistrationValidationError::EmptyRoutingMetadataKey)
        );
    }

    #[test]
    fn routing_metadata_rejects_empty_value() {
        let result = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_routing_metadata("region", "   ");

        assert_eq!(
            result,
            Err(RegistrationValidationError::EmptyRoutingMetadataValue)
        );
    }

    #[test]
    fn validate_accepts_coherent_registration() {
        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_capability(capability("arabic.search", "arabic"))
            .expect("capability")
            .with_contract(contract("arabic.search"))
            .with_endpoint(Endpoint::new("grpc://arabic-01:50051").expect("endpoint"))
            .with_routing_metadata("region", "ap-south")
            .expect("metadata");

        assert_eq!(registration.validate(), Ok(()));
    }

    #[test]
    fn validate_detects_corrupted_capability_ownership() {
        let mut registration =
            EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));

        registration
            .capabilities
            .push(capability("quran.search", "quran"));

        assert!(matches!(
            registration.validate(),
            Err(RegistrationValidationError::CapabilityOwnerMismatch { .. })
        ));
    }

    #[test]
    fn validate_detects_corrupted_empty_endpoint() {
        let mut registration =
            EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));

        registration.endpoint = Some(Endpoint {
            address: "   ".to_string(),
        });

        assert_eq!(
            registration.validate(),
            Err(RegistrationValidationError::EmptyEndpoint)
        );
    }

    #[test]
    fn validate_detects_corrupted_empty_routing_metadata() {
        let mut registration =
            EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));

        registration
            .routing_metadata
            .insert("region".to_string(), "   ".to_string());

        assert_eq!(
            registration.validate(),
            Err(RegistrationValidationError::EmptyRoutingMetadataValue)
        );
    }

    #[test]
    fn registration_clone_preserves_all_declared_state() {
        let readiness = ReadinessReport::at(ReadinessState::Unknown, std::time::UNIX_EPOCH);

        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"))
            .with_capability(capability("arabic.search", "arabic"))
            .expect("capability")
            .with_contract(contract("arabic.search"))
            .with_endpoint(Endpoint::new("grpc://arabic-01:50051").expect("endpoint"))
            .with_runtime_metadata(
                RuntimeRegistrationMetadata::new()
                    .with_version(Version::new(1, 0, 0))
                    .with_lifecycle(LifecycleState::Serving)
                    .with_readiness(readiness),
            )
            .with_routing_metadata("region", "ap-south")
            .expect("metadata");

        let cloned = registration.clone();

        assert_eq!(cloned, registration);
    }

    #[test]
    fn debug_output_is_available_for_registration_types() {
        let registration = EngineRegistration::new(engine_id("arabic"), instance_id("arabic-01"));
        let endpoint = Endpoint::new("grpc://arabic-01:50051").expect("endpoint");
        let runtime = RuntimeRegistrationMetadata::new();

        let registration_debug = format!("{registration:?}");
        let endpoint_debug = format!("{endpoint:?}");
        let runtime_debug = format!("{runtime:?}");

        assert!(registration_debug.contains("EngineRegistration"));
        assert!(endpoint_debug.contains("Endpoint"));
        assert!(runtime_debug.contains("RuntimeRegistrationMetadata"));
    }

    #[test]
    fn validation_does_not_require_capabilities_or_contracts() {
        let registration =
            EngineRegistration::new(engine_id("observer"), instance_id("observer-01"));

        assert!(registration.validate().is_ok());
        assert!(registration.capabilities().is_empty());
        assert!(registration.contracts().is_empty());
    }
}
