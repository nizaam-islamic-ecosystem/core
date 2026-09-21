//! Contract identification and compatibility helpers for the Control Plane.
//!
//! The existing `contracts` subsystem remains authoritative over the contract
//! model and compatibility algorithm. This module provides the Control Plane
//! boundary for identifying a request's contract and filtering offered
//! contracts without redefining those semantics.
//!
//! ```text
//! Universal Request / Contract Metadata
//!              │
//!              ▼
//!       Control Plane contract.rs
//!              │
//!       ┌──────┴──────┐
//!       ▼             ▼
//!   identify      compatibility
//!                     │
//!                     ▼
//!             compatible candidates
//! ```
//!
//! This module does not own payload decoding, domain semantics, capability
//! execution, provider resolution, destination selection, or routing policy.

use crate::contracts::compatibility::compare_contracts;
use crate::contracts::{ContractDescriptor, ContractMetadata, UniversalRequest, Version};
use crate::identity::{CapabilityId, ContractId};
use crate::status::Compatibility;
use std::fmt;

/// Identifies the contract descriptor carried by Control Plane contract
/// metadata.
///
/// The descriptor is returned by reference and is not reconstructed or
/// rewritten. Contract metadata remains owned by the existing `contracts`
/// subsystem.
#[must_use]
pub fn identify(metadata: &ContractMetadata) -> &ContractDescriptor {
    &metadata.descriptor
}

/// Identifies the contract descriptor carried by a universal request.
///
/// Structural request validation remains the responsibility of the existing
/// contracts validation subsystem. This helper only projects the already
/// declared contract descriptor.
#[must_use]
pub fn identify_request(request: &UniversalRequest) -> &ContractDescriptor {
    &request.event.envelope.metadata.descriptor
}

/// Compares a required contract with one offered contract using the canonical
/// compatibility implementation from the existing Contracts subsystem.
#[must_use]
pub fn compare(required: &ContractDescriptor, offered: &ContractDescriptor) -> Compatibility {
    compare_contracts(required, offered)
}

/// Returns only the offered contract descriptors that are established as
/// compatible with the required descriptor.
///
/// The input order is preserved. This function filters candidates; it does not
/// select a preferred version or provider. Final selection remains the
/// responsibility of later Control Plane resolution and policy composition.
#[must_use]
pub fn filter_compatible<'a>(
    required: &ContractDescriptor,
    offered: &'a [ContractDescriptor],
) -> Vec<&'a ContractDescriptor> {
    offered
        .iter()
        .filter(|candidate| compare(required, candidate) == Compatibility::Compatible)
        .collect()
}

/// Ensures that one offered contract is compatible with the required contract.
///
/// This is useful when an outer resolver has already selected one candidate and
/// wants the Control Plane contract boundary to establish compatibility without
/// making another selection decision.
///
/// [`Compatibility::Unknown`] is preserved as an explicit failure rather than
/// being silently treated as compatible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractResolutionError {
    /// The offered contract does not satisfy the required contract boundary.
    NotCompatible {
        contract_id: ContractId,
        capability_id: CapabilityId,
        required_version: Version,
        offered_version: Version,
        compatibility: Compatibility,
    },
}

impl fmt::Display for ContractResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCompatible {
                contract_id,
                capability_id,
                required_version,
                offered_version,
                compatibility,
            } => write!(
                formatter,
                "contract {contract_id} for capability {capability_id} is not compatible: \
                 required version {required_version}, offered version {offered_version}, \
                 result {compatibility:?}"
            ),
        }
    }
}

impl std::error::Error for ContractResolutionError {}

/// Result alias for Control Plane contract compatibility operations.
pub type ContractResult<T> = Result<T, ContractResolutionError>;

/// Establishes that one offered contract is compatible with the required
/// contract.
pub fn require_compatible(
    required: &ContractDescriptor,
    offered: &ContractDescriptor,
) -> ContractResult<()> {
    let compatibility = compare(required, offered);

    if compatibility == Compatibility::Compatible {
        Ok(())
    } else {
        Err(ContractResolutionError::NotCompatible {
            contract_id: required.contract_id.clone(),
            capability_id: required.capability_id.clone(),
            required_version: required.version.clone(),
            offered_version: offered.version.clone(),
            compatibility,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        ContractDescriptor, Interaction, PayloadDescriptor, RequirementsMetadata,
    };
    use crate::contracts::{EncodedPayload, MessageEnvelope, Participants};
    use crate::identity::{CapabilityId, ContractId, EngineId};
    use crate::identity::{CorrelationId, MessageId, OperationId};
    use crate::operation::{Operation, OperationContext};

    fn descriptor(
        contract_id: &str,
        capability_id: &str,
        version: Version,
        interaction: Interaction,
        media_type: &str,
        schema_version: Version,
    ) -> ContractDescriptor {
        ContractDescriptor::new(
            ContractId::new(contract_id).expect("valid contract id"),
            CapabilityId::new(capability_id).expect("valid capability id"),
            version,
            interaction,
            PayloadDescriptor::new(media_type, schema_version).expect("valid payload descriptor"),
        )
    }

    fn metadata(descriptor: ContractDescriptor) -> ContractMetadata {
        ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("caller").expect("valid sender"),
                EngineId::new("provider").expect("valid target"),
            ),
        )
    }

    fn request(descriptor: ContractDescriptor) -> UniversalRequest {
        let payload = EncodedPayload::new(descriptor.payload.clone(), b"payload".to_vec());
        let metadata = metadata(descriptor);
        let operation = OperationContext::new(Operation::new(
            OperationId::new("operation-1").expect("valid operation id"),
            CorrelationId::new("correlation-1").expect("valid correlation id"),
        ));

        UniversalRequest::new(MessageEnvelope::new(
            MessageId::new("message-1").expect("valid message id"),
            operation,
            metadata,
            payload,
        ))
    }

    #[test]
    fn identify_returns_the_declared_contract_descriptor() {
        let descriptor = descriptor(
            "quran.retrieve",
            "quran.retrieve",
            Version::new(2, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let metadata = metadata(descriptor.clone());

        let identified = identify(&metadata);

        assert_eq!(identified, &descriptor);
        assert_eq!(identified.contract_id.as_str(), "quran.retrieve");
    }

    #[test]
    fn identify_request_returns_the_request_contract_descriptor() {
        let descriptor = descriptor(
            "lookup.request",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let request = request(descriptor.clone());

        assert_eq!(identify_request(&request), &descriptor);
    }

    #[test]
    fn compare_delegates_to_canonical_contract_compatibility() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 2, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 1, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Compatible);
    }

    #[test]
    fn incompatible_contract_identity_is_not_accepted() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "other.contract",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Incompatible);
    }

    #[test]
    fn incompatible_capability_is_not_accepted() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "search",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Incompatible);
    }

    #[test]
    fn incompatible_interaction_is_not_accepted() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Response,
            "application/json",
            Version::new(1, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Incompatible);
    }

    #[test]
    fn incompatible_media_type_is_not_accepted() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/problem+json",
            Version::new(1, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Incompatible);
    }

    #[test]
    fn unknown_version_result_is_preserved() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 5, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 2, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Unknown);
    }

    #[test]
    fn incompatible_schema_major_version_is_rejected() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(2, 0, 0),
        );

        assert_eq!(compare(&required, &offered), Compatibility::Incompatible);
    }

    #[test]
    fn filter_compatible_preserves_input_order() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered_a = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered_b = descriptor(
            "lookup",
            "lookup",
            Version::new(2, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered_c = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 1, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let candidates = vec![offered_a.clone(), offered_b, offered_c.clone()];

        let compatible = filter_compatible(&required, &candidates);

        assert_eq!(compatible, vec![&offered_a, &offered_c]);
    }

    #[test]
    fn filter_compatible_excludes_unknown_results() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 5, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let unknown = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 2, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let compatible = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 6, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let candidates = vec![unknown, compatible.clone()];

        let result = filter_compatible(&required, &candidates);

        assert_eq!(result, vec![&compatible]);
    }

    #[test]
    fn filter_compatible_returns_empty_when_no_candidate_matches() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(2, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let candidates = vec![descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        )];

        assert!(filter_compatible(&required, &candidates).is_empty());
    }

    #[test]
    fn require_compatible_accepts_a_compatible_offer() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 1, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 1, 0),
        );

        assert_eq!(require_compatible(&required, &offered), Ok(()));
    }

    #[test]
    fn require_compatible_preserves_unknown_failure() {
        let required = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 5, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let offered = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 2, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );

        assert_eq!(
            require_compatible(&required, &offered),
            Err(ContractResolutionError::NotCompatible {
                contract_id: ContractId::new("lookup").unwrap(),
                capability_id: CapabilityId::new("lookup").unwrap(),
                required_version: Version::new(1, 5, 0),
                offered_version: Version::new(1, 2, 0),
                compatibility: Compatibility::Unknown,
            })
        );
    }

    #[test]
    fn requirement_metadata_is_not_reinterpreted_by_contract_comparison() {
        let descriptor = descriptor(
            "lookup",
            "lookup",
            Version::new(1, 0, 0),
            Interaction::Request,
            "application/json",
            Version::new(1, 0, 0),
        );
        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("provider").unwrap(),
            ),
        )
        .with_requirements(
            RequirementsMetadata::none()
                .requiring_capability(CapabilityId::new("lookup").unwrap())
                .requiring_contract_version(Version::new(1, 0, 0)),
        );

        assert_eq!(identify(&metadata), &descriptor);
    }
}
