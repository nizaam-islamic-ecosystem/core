//! Declarative provider metadata and provider-resolution helpers for the
//! Control Plane.
//!
//! This module owns the boundary between a requested capability/contract and
//! logical provider candidates. It does not own registry discovery, health,
//! routing, concrete engine-instance selection, transport, execution, retry,
//! or domain semantics.
//!
//! The intended relationship is:
//!
//! ```text
//! Capability
//!     ↓
//! Provider
//!     ↓
//! Provider candidate(s)
//!     ↓
//! destination / routing
//!     ↓
//! EngineInstanceId
//! ```
//!
//! `ProviderDescriptor` is deliberately declarative. It contains logical
//! provider identity and advertised capability/contract metadata, but it does
//! not contain a handler, transport handle, endpoint, or concrete instance.
//!
//! Provider selection remains distinct from instance routing. This module
//! therefore returns compatible provider candidates rather than choosing a
//! single provider according to cost, locality, health, load, or other policy.

use crate::contracts::compatibility::compare_contracts;
use crate::contracts::{ContractDescriptor, Version};
use crate::identity::{CapabilityId, ContractId, EngineId};
use std::collections::BTreeMap;
use std::fmt;

/// Opaque provider identity local to the Control Plane provider metadata
/// boundary.
///
/// Core's canonical identity module does not define a separate `ProviderId`.
/// Provider identity is therefore represented as an owned string here rather
/// than inventing a second Core-wide identity primitive.
pub type ProviderId = String;

/// Declarative metadata describing one logical provider.
///
/// A provider may advertise multiple capabilities and multiple contracts. The
/// descriptor is logical and is intentionally independent from any concrete
/// `EngineInstanceId` or local execution worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDescriptor {
    provider_id: ProviderId,
    owning_engine: EngineId,
    capabilities: Vec<CapabilityId>,
    contracts: Vec<ContractDescriptor>,
    version: Option<Version>,
    metadata: BTreeMap<String, String>,
}

impl ProviderDescriptor {
    /// Creates a provider descriptor with at least one advertised capability.
    pub fn new(
        provider_id: impl Into<ProviderId>,
        owning_engine: EngineId,
        capabilities: Vec<CapabilityId>,
    ) -> Result<Self, ProviderDefinitionError> {
        let descriptor = Self {
            provider_id: provider_id.into(),
            owning_engine,
            capabilities,
            contracts: Vec::new(),
            version: None,
            metadata: BTreeMap::new(),
        };

        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Adds an advertised contract.
    ///
    /// The contract's capability must already be present in the provider's
    /// advertised capability set. This validates metadata coherence without
    /// interpreting domain semantics.
    pub fn with_contract(
        mut self,
        contract: ContractDescriptor,
    ) -> Result<Self, ProviderDefinitionError> {
        if !self
            .capabilities
            .iter()
            .any(|capability| capability == &contract.capability_id)
        {
            return Err(ProviderDefinitionError::ContractCapabilityNotAdvertised {
                provider_id: self.provider_id.clone(),
                capability_id: contract.capability_id.clone(),
            });
        }

        self.contracts.push(contract);
        Ok(self)
    }

    /// Adds provider implementation-version metadata.
    ///
    /// Provider version is metadata only. It is intentionally not treated as
    /// a second compatibility algorithm in this module.
    #[must_use]
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Adds one provider metadata entry.
    ///
    /// Metadata keys and values remain opaque to this module. Interpretation
    /// belongs to higher-level Control Plane policy.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, ProviderDefinitionError> {
        let key = key.into();
        let value = value.into();

        if key.is_empty() {
            return Err(ProviderDefinitionError::EmptyMetadataKey);
        }
        if value.is_empty() {
            return Err(ProviderDefinitionError::EmptyMetadataValue);
        }

        self.metadata.insert(key, value);
        Ok(self)
    }

    /// Returns the logical provider identifier.
    #[must_use]
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Returns the logical engine that owns the provider.
    #[must_use]
    pub fn owning_engine(&self) -> &EngineId {
        &self.owning_engine
    }

    /// Returns advertised capability identifiers.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityId] {
        &self.capabilities
    }

    /// Returns advertised contract descriptors.
    #[must_use]
    pub fn contracts(&self) -> &[ContractDescriptor] {
        &self.contracts
    }

    /// Returns the optional provider implementation version.
    #[must_use]
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Returns opaque provider metadata.
    #[must_use]
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    /// Validates structural provider metadata coherence.
    pub fn validate(&self) -> Result<(), ProviderDefinitionError> {
        if self.capabilities.is_empty() {
            return Err(ProviderDefinitionError::NoCapabilities);
        }

        for contract in &self.contracts {
            if !self
                .capabilities
                .iter()
                .any(|capability| capability == &contract.capability_id)
            {
                return Err(ProviderDefinitionError::ContractCapabilityNotAdvertised {
                    provider_id: self.provider_id.clone(),
                    capability_id: contract.capability_id.clone(),
                });
            }
        }

        for key in self.metadata.keys() {
            if key.is_empty() {
                return Err(ProviderDefinitionError::EmptyMetadataKey);
            }
        }

        for value in self.metadata.values() {
            if value.is_empty() {
                return Err(ProviderDefinitionError::EmptyMetadataValue);
            }
        }

        Ok(())
    }
}

/// Errors raised while constructing or validating provider metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderDefinitionError {
    /// A provider must advertise at least one capability.
    NoCapabilities,
    /// An advertised contract refers to a capability that this provider does
    /// not declare.
    ContractCapabilityNotAdvertised {
        provider_id: ProviderId,
        capability_id: CapabilityId,
    },
    /// Provider metadata keys must not be empty.
    EmptyMetadataKey,
    /// Provider metadata values must not be empty.
    EmptyMetadataValue,
}

impl fmt::Display for ProviderDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCapabilities => formatter.write_str("provider advertises no capabilities"),
            Self::ContractCapabilityNotAdvertised {
                provider_id,
                capability_id,
            } => write!(
                formatter,
                "provider {provider_id} advertises contract capability {capability_id} without advertising the capability itself"
            ),
            Self::EmptyMetadataKey => formatter.write_str("provider metadata key is empty"),
            Self::EmptyMetadataValue => formatter.write_str("provider metadata value is empty"),
        }
    }
}

impl std::error::Error for ProviderDefinitionError {}

/// Returns whether a provider advertises the requested capability.
#[must_use]
pub fn supports_capability(provider: &ProviderDescriptor, capability: &CapabilityId) -> bool {
    provider
        .capabilities()
        .iter()
        .any(|advertised| advertised == capability)
}

/// Returns all providers advertising the requested capability, preserving
/// input order.
#[must_use]
pub fn filter_by_capability<'a>(
    capability: &CapabilityId,
    providers: &'a [ProviderDescriptor],
) -> Vec<&'a ProviderDescriptor> {
    providers
        .iter()
        .filter(|provider| supports_capability(provider, capability))
        .collect()
}

/// Returns contracts advertised by `provider` that are canonically
/// compatible with the required contract.
///
/// Contract compatibility is delegated to the existing Contracts subsystem.
/// This function never selects a single contract version.
#[must_use]
pub fn compatible_contracts<'a>(
    required: &ContractDescriptor,
    provider: &'a ProviderDescriptor,
) -> Vec<&'a ContractDescriptor> {
    provider
        .contracts()
        .iter()
        .filter(|offered| {
            compare_contracts(required, offered) == crate::status::Compatibility::Compatible
        })
        .collect()
}

/// Returns whether a provider can satisfy the requested capability and
/// contract.
///
/// Provider health, authorization, routing policy, and instance availability
/// are intentionally outside this predicate.
#[must_use]
pub fn is_compatible(
    capability: &CapabilityId,
    required_contract: &ContractDescriptor,
    provider: &ProviderDescriptor,
) -> bool {
    capability == &required_contract.capability_id
        && supports_capability(provider, capability)
        && !compatible_contracts(required_contract, provider).is_empty()
}

/// Returns all compatible logical providers, preserving input order.
///
/// No provider is selected here. Higher-level Control Plane policy is
/// responsible for selection and provider substitution semantics.
#[must_use]
pub fn filter_compatible<'a>(
    capability: &CapabilityId,
    required_contract: &ContractDescriptor,
    providers: &'a [ProviderDescriptor],
) -> Vec<&'a ProviderDescriptor> {
    providers
        .iter()
        .filter(|provider| is_compatible(capability, required_contract, provider))
        .collect()
}

/// Requires at least one provider to satisfy the capability and contract.
///
/// The result remains a candidate set. This helper does not choose one
/// provider and does not perform routing.
pub fn require_compatible<'a>(
    capability: &CapabilityId,
    required_contract: &ContractDescriptor,
    providers: &'a [ProviderDescriptor],
) -> ProviderResult<Vec<&'a ProviderDescriptor>> {
    let matches = filter_compatible(capability, required_contract, providers);

    if !matches.is_empty() {
        return Ok(matches);
    }

    if !providers
        .iter()
        .any(|provider| supports_capability(provider, capability))
    {
        Err(ProviderResolutionError::NoProviderForCapability {
            capability_id: capability.clone(),
        })
    } else {
        Err(ProviderResolutionError::NoCompatibleProvider {
            capability_id: capability.clone(),
            contract_id: required_contract.contract_id.clone(),
        })
    }
}

/// Errors raised while resolving logical provider candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderResolutionError {
    /// No provider advertises the requested capability.
    NoProviderForCapability { capability_id: CapabilityId },
    /// Providers advertise the capability, but none can satisfy the required
    /// contract according to the canonical Contracts compatibility rules.
    NoCompatibleProvider {
        capability_id: CapabilityId,
        contract_id: ContractId,
    },
}

impl fmt::Display for ProviderResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProviderForCapability { capability_id } => {
                write!(
                    formatter,
                    "no provider advertises capability {capability_id}"
                )
            }
            Self::NoCompatibleProvider {
                capability_id,
                contract_id,
            } => write!(
                formatter,
                "no compatible provider for capability {capability_id} and contract {contract_id}"
            ),
        }
    }
}

impl std::error::Error for ProviderResolutionError {}

/// Result type for logical provider resolution.
pub type ProviderResult<T> = Result<T, ProviderResolutionError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Interaction, PayloadDescriptor};

    fn provider_id(value: &str) -> ProviderId {
        value.to_owned()
    }

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).unwrap()
    }

    fn capability_id(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    fn contract(
        contract: &str,
        capability: &str,
        version: Version,
        schema_version: Version,
    ) -> ContractDescriptor {
        ContractDescriptor::new(
            ContractId::new(contract).unwrap(),
            capability_id(capability),
            version,
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", schema_version).unwrap(),
        )
    }

    fn provider(id: &str, engine: &str, capabilities: &[&str]) -> ProviderDescriptor {
        ProviderDescriptor::new(
            provider_id(id),
            engine_id(engine),
            capabilities
                .iter()
                .map(|value| capability_id(value))
                .collect(),
        )
        .unwrap()
    }

    fn provider_with_contract(
        id: &str,
        engine: &str,
        contract_id: &str,
        capability: &str,
        offered_version: Version,
    ) -> ProviderDescriptor {
        provider(id, engine, &[capability])
            .with_contract(contract(
                contract_id,
                capability,
                offered_version,
                Version::new(1, 0, 0),
            ))
            .unwrap()
    }

    #[test]
    fn valid_provider_definition_preserves_metadata() {
        let descriptor = provider("provider-a", "engine-a", &["quran.search"])
            .with_version(Version::new(2, 1, 0))
            .with_metadata("region", "in-west")
            .unwrap();

        assert_eq!(descriptor.provider_id().as_str(), "provider-a");
        assert_eq!(descriptor.owning_engine().as_ref(), "engine-a");
        assert_eq!(descriptor.capabilities().len(), 1);
        assert_eq!(descriptor.version(), Some(&Version::new(2, 1, 0)));
        assert_eq!(
            descriptor.metadata().get("region").map(String::as_str),
            Some("in-west")
        );
        assert!(descriptor.contracts().is_empty());
        assert_eq!(descriptor.validate(), Ok(()));
    }

    #[test]
    fn provider_requires_at_least_one_capability() {
        let result =
            ProviderDescriptor::new(provider_id("provider-a"), engine_id("engine-a"), vec![]);

        assert_eq!(result, Err(ProviderDefinitionError::NoCapabilities));
    }

    #[test]
    fn contract_must_reference_an_advertised_capability() {
        let result = provider("provider-a", "engine-a", &["quran.search"]).with_contract(contract(
            "arabic.contract",
            "arabic.analyze",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        ));

        assert_eq!(
            result,
            Err(ProviderDefinitionError::ContractCapabilityNotAdvertised {
                provider_id: provider_id("provider-a"),
                capability_id: capability_id("arabic.analyze"),
            })
        );
    }

    #[test]
    fn metadata_rejects_empty_key() {
        let result =
            provider("provider-a", "engine-a", &["quran.search"]).with_metadata("", "value");

        assert_eq!(result, Err(ProviderDefinitionError::EmptyMetadataKey));
    }

    #[test]
    fn metadata_rejects_empty_value() {
        let result =
            provider("provider-a", "engine-a", &["quran.search"]).with_metadata("region", "");

        assert_eq!(result, Err(ProviderDefinitionError::EmptyMetadataValue));
    }

    #[test]
    fn supports_matching_capability() {
        let descriptor = provider("provider-a", "engine-a", &["quran.search"]);

        assert!(supports_capability(
            &descriptor,
            &capability_id("quran.search")
        ));
        assert!(!supports_capability(
            &descriptor,
            &capability_id("arabic.analyze")
        ));
    }

    #[test]
    fn filter_by_capability_preserves_input_order_and_multiple_providers() {
        let providers = vec![
            provider("provider-a", "engine-a", &["quran.search"]),
            provider("provider-b", "engine-b", &["arabic.analyze"]),
            provider("provider-c", "engine-c", &["quran.search"]),
        ];

        let matches = filter_by_capability(&capability_id("quran.search"), &providers);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].provider_id().as_str(), "provider-a");
        assert_eq!(matches[1].provider_id().as_str(), "provider-c");
    }

    #[test]
    fn compatible_contracts_reuses_canonical_contract_compatibility() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        );
        let provider = provider("provider-a", "engine-a", &["quran.search"])
            .with_contract(contract(
                "quran.contract",
                "quran.search",
                Version::new(1, 1, 0),
                Version::new(1, 0, 0),
            ))
            .unwrap()
            .with_contract(contract(
                "other.contract",
                "quran.search",
                Version::new(1, 0, 0),
                Version::new(1, 0, 0),
            ))
            .unwrap();

        let matches = compatible_contracts(&required, &provider);

        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].contract_id,
            ContractId::new("quran.contract").unwrap()
        );
    }

    #[test]
    fn unknown_contract_compatibility_is_not_treated_as_compatible() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 1, 0),
            Version::new(1, 0, 0),
        );
        let provider = provider_with_contract(
            "provider-a",
            "engine-a",
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
        );

        assert!(compatible_contracts(&required, &provider).is_empty());
        assert!(!is_compatible(
            &capability_id("quran.search"),
            &required,
            &provider
        ));
    }

    #[test]
    fn is_compatible_requires_matching_capability_and_contract() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        );
        let provider = provider_with_contract(
            "provider-a",
            "engine-a",
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
        );

        assert!(is_compatible(
            &capability_id("quran.search"),
            &required,
            &provider
        ));
        assert!(!is_compatible(
            &capability_id("arabic.analyze"),
            &required,
            &provider
        ));
    }

    #[test]
    fn filter_compatible_returns_all_compatible_providers_in_order() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        );
        let providers = vec![
            provider_with_contract(
                "provider-a",
                "engine-a",
                "quran.contract",
                "quran.search",
                Version::new(1, 0, 0),
            ),
            provider_with_contract(
                "provider-b",
                "engine-b",
                "quran.contract",
                "quran.search",
                Version::new(2, 0, 0),
            ),
            provider_with_contract(
                "provider-c",
                "engine-c",
                "quran.contract",
                "quran.search",
                Version::new(1, 2, 0),
            ),
        ];

        let matches = filter_compatible(&capability_id("quran.search"), &required, &providers);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].provider_id().as_str(), "provider-a");
        assert_eq!(matches[1].provider_id().as_str(), "provider-c");
    }

    #[test]
    fn require_compatible_distinguishes_missing_capability_from_incompatibility() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        );

        let missing = vec![provider("provider-a", "engine-a", &["arabic.analyze"])];
        assert_eq!(
            require_compatible(&capability_id("quran.search"), &required, &missing),
            Err(ProviderResolutionError::NoProviderForCapability {
                capability_id: capability_id("quran.search"),
            })
        );

        let incompatible = vec![provider_with_contract(
            "provider-a",
            "engine-a",
            "quran.contract",
            "quran.search",
            Version::new(2, 0, 0),
        )];
        assert_eq!(
            require_compatible(&capability_id("quran.search"), &required, &incompatible),
            Err(ProviderResolutionError::NoCompatibleProvider {
                capability_id: capability_id("quran.search"),
                contract_id: ContractId::new("quran.contract").unwrap(),
            })
        );
    }

    #[test]
    fn require_compatible_returns_candidates_without_selecting_one() {
        let required = contract(
            "quran.contract",
            "quran.search",
            Version::new(1, 0, 0),
            Version::new(1, 0, 0),
        );
        let providers = vec![
            provider_with_contract(
                "provider-a",
                "engine-a",
                "quran.contract",
                "quran.search",
                Version::new(1, 0, 0),
            ),
            provider_with_contract(
                "provider-b",
                "engine-b",
                "quran.contract",
                "quran.search",
                Version::new(1, 0, 0),
            ),
        ];

        let matches =
            require_compatible(&capability_id("quran.search"), &required, &providers).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].provider_id().as_str(), "provider-a");
        assert_eq!(matches[1].provider_id().as_str(), "provider-b");
    }

    #[test]
    fn provider_descriptor_contains_no_concrete_engine_instance_identity() {
        let descriptor = provider("provider-a", "engine-a", &["quran.search"]);

        let _provider_id = descriptor.provider_id();
        let _engine_id = descriptor.owning_engine();
        assert!(descriptor.contracts().is_empty());
    }
}
