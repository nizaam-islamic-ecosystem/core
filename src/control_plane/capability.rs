//! Control Plane capability identification and advertisement resolution.
//!
//! This module operates only on declarative capability metadata. It identifies
//! the requested [`CapabilityId`] and finds matching [`CapabilityDefinition`]
//! advertisements presented by registered engines.
//!
//! The existing Phase 6 capability system remains authoritative for executable
//! capability resolution and dispatch. This module therefore never accesses a
//! [`CapabilityRegistry`], obtains capability handlers, or invokes a capability.
//!
//! The boundary is deliberately narrow:
//!
//! ```text
//! request / contract metadata
//!          │
//!          ▼
//!    capability identity
//!          │
//!          ▼
//! advertised CapabilityDefinition values
//!          │
//!          ▼
//! later provider / destination / routing resolution
//! ```
//!
//! Capability readiness is a routing-input concern owned by the existing
//! Health system and broader destination-eligibility layer. This module does
//! not maintain a second readiness or health system.

use crate::capability::CapabilityDefinition;
use crate::contracts::{ContractMetadata, UniversalRequest};
use crate::identity::CapabilityId;
use std::fmt;

/// Identifies the capability requested by contract metadata.
#[must_use]
pub fn identify(metadata: &ContractMetadata) -> &CapabilityId {
    &metadata.descriptor.capability_id
}

/// Identifies the capability requested by a universal request.
#[must_use]
pub fn identify_request(request: &UniversalRequest) -> &CapabilityId {
    identify(&request.event.envelope.metadata)
}

/// Returns whether an advertised capability matches the requested capability
/// identifier.
///
/// Matching is intentionally based only on the established `CapabilityId`.
/// The Control Plane does not interpret capability names, descriptions,
/// payloads, handlers, or domain semantics here.
#[must_use]
pub fn is_advertised(required: &CapabilityId, offered: &CapabilityDefinition) -> bool {
    offered.capability_id() == required
}

/// Returns whether at least one advertised capability matches the requirement.
#[must_use]
pub fn has_advertisement(required: &CapabilityId, offered: &[CapabilityDefinition]) -> bool {
    offered
        .iter()
        .any(|candidate| is_advertised(required, candidate))
}

/// Returns all advertised capability definitions matching the requested
/// capability identifier, preserving the input order.
///
/// The function does not select a provider or concrete engine instance.
/// Multiple matching advertisements are therefore retained for later
/// provider, destination, and routing resolution.
#[must_use]
pub fn filter_advertised<'a>(
    required: &CapabilityId,
    offered: &'a [CapabilityDefinition],
) -> Vec<&'a CapabilityDefinition> {
    offered
        .iter()
        .filter(|candidate| is_advertised(required, candidate))
        .collect()
}

/// Errors produced while resolving declarative capability advertisements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityResolutionError {
    /// No advertised capability matches the requested capability identifier.
    NotAdvertised { capability_id: CapabilityId },
}

impl fmt::Display for CapabilityResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAdvertised { capability_id } => {
                write!(formatter, "capability {capability_id} is not advertised")
            }
        }
    }
}

impl std::error::Error for CapabilityResolutionError {}

/// Result type for declarative capability advertisement resolution.
pub type CapabilityResult<T> = Result<T, CapabilityResolutionError>;

/// Requires at least one advertised capability to match the requested
/// capability identifier and returns all matching advertisements.
///
/// No provider or concrete instance is selected here.
pub fn require_advertised<'a>(
    required: &CapabilityId,
    offered: &'a [CapabilityDefinition],
) -> CapabilityResult<Vec<&'a CapabilityDefinition>> {
    let matches = filter_advertised(required, offered);
    if matches.is_empty() {
        Err(CapabilityResolutionError::NotAdvertised {
            capability_id: required.clone(),
        })
    } else {
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityDefinition;
    use crate::contracts::{
        ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
        Participants, PayloadDescriptor, UniversalRequest, Version,
    };
    use crate::identity::{ContractId, CorrelationId, EngineId, MessageId, OperationId};
    use crate::operation::{Operation, OperationContext};

    fn capability_id(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).unwrap()
    }

    fn definition(capability: &str, engine: &str, name: &str) -> CapabilityDefinition {
        CapabilityDefinition::new(capability_id(capability), engine_id(engine), name).unwrap()
    }

    fn metadata(capability: &str) -> ContractMetadata {
        let descriptor = ContractDescriptor::new(
            ContractId::new(format!("{capability}.contract")).unwrap(),
            capability_id(capability),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        ContractMetadata::new(
            descriptor.clone(),
            Participants::new(engine_id("caller"), engine_id("target")),
        )
    }

    fn request(capability: &str) -> UniversalRequest {
        let metadata = metadata(capability);
        let payload_descriptor = metadata.descriptor.payload.clone();
        let context = OperationContext::new(Operation::new(
            OperationId::new("operation-1").unwrap(),
            CorrelationId::new("correlation-1").unwrap(),
        ));

        UniversalRequest::new(MessageEnvelope::new(
            MessageId::new("message-1").unwrap(),
            context,
            metadata,
            EncodedPayload::new(payload_descriptor, b"opaque payload"),
        ))
    }

    #[test]
    fn identify_returns_descriptor_capability_id() {
        let metadata = metadata("quran.search");
        assert_eq!(identify(&metadata), &capability_id("quran.search"));
    }

    #[test]
    fn identify_request_returns_descriptor_capability_id() {
        let request = request("quran.search");
        assert_eq!(identify_request(&request), &capability_id("quran.search"));
    }

    #[test]
    fn matching_capability_is_advertised() {
        let offered = definition("quran.search", "quran-engine", "Quran Search");
        assert!(is_advertised(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn different_capability_is_not_advertised() {
        let offered = definition("quran.search", "quran-engine", "Quran Search");
        assert!(!is_advertised(&capability_id("quran.retrieve"), &offered));
    }

    #[test]
    fn filter_returns_matching_advertisements() {
        let offered = vec![
            definition("quran.search", "quran-engine", "Quran Search"),
            definition("quran.retrieve", "quran-engine", "Quran Retrieve"),
            definition("quran.search", "search-engine", "Search"),
        ];

        let matches = filter_advertised(&capability_id("quran.search"), &offered);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].owning_engine(), &engine_id("quran-engine"));
        assert_eq!(matches[1].owning_engine(), &engine_id("search-engine"));
    }

    #[test]
    fn filter_preserves_input_order() {
        let offered = vec![
            definition("quran.search", "engine-c", "Search C"),
            definition("quran.search", "engine-a", "Search A"),
            definition("quran.search", "engine-b", "Search B"),
        ];

        let matches = filter_advertised(&capability_id("quran.search"), &offered);
        let engines: Vec<&str> = matches
            .iter()
            .map(|candidate| candidate.owning_engine().as_str())
            .collect();

        assert_eq!(engines, vec!["engine-c", "engine-a", "engine-b"]);
    }

    #[test]
    fn filter_returns_all_matching_engines() {
        let offered = vec![
            definition("quran.search", "quran-engine", "Quran Search"),
            definition("quran.search", "knowledge-engine", "Search"),
            definition("quran.search", "index-engine", "Indexed Search"),
        ];

        let matches = filter_advertised(&capability_id("quran.search"), &offered);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn filter_ignores_unrelated_capabilities() {
        let offered = vec![
            definition("quran.retrieve", "quran-engine", "Retrieve"),
            definition("arabic.analyze", "arabic-engine", "Analyze"),
        ];

        assert!(filter_advertised(&capability_id("quran.search"), &offered).is_empty());
    }

    #[test]
    fn filter_empty_input_returns_empty() {
        let offered: Vec<CapabilityDefinition> = Vec::new();
        assert!(filter_advertised(&capability_id("quran.search"), &offered).is_empty());
    }

    #[test]
    fn has_advertisement_returns_true_when_present() {
        let offered = vec![definition("quran.search", "quran-engine", "Search")];
        assert!(has_advertisement(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn has_advertisement_returns_false_when_absent() {
        let offered = vec![definition("quran.retrieve", "quran-engine", "Retrieve")];
        assert!(!has_advertisement(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn require_advertised_succeeds_when_present() {
        let offered = vec![
            definition("quran.search", "quran-engine", "Search"),
            definition("quran.search", "search-engine", "Search"),
        ];

        let matches = require_advertised(&capability_id("quran.search"), &offered).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn require_advertised_returns_not_advertised_when_missing() {
        let offered = vec![definition("quran.retrieve", "quran-engine", "Retrieve")];

        let result = require_advertised(&capability_id("quran.search"), &offered);
        assert!(matches!(
            result,
            Err(CapabilityResolutionError::NotAdvertised { .. })
        ));
    }

    #[test]
    fn not_advertised_error_preserves_capability_id() {
        let result = require_advertised(&capability_id("quran.search"), &[]);
        let error = result.unwrap_err();

        assert_eq!(
            error,
            CapabilityResolutionError::NotAdvertised {
                capability_id: capability_id("quran.search")
            }
        );
    }

    #[test]
    fn description_does_not_affect_identity_match() {
        let offered = definition("quran.search", "quran-engine", "Completely Different Name")
            .with_description("A description unrelated to the requested identity")
            .unwrap();

        assert!(is_advertised(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn name_does_not_affect_identity_match() {
        let offered = definition("quran.search", "quran-engine", "Another Name");
        assert!(is_advertised(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn owning_engine_does_not_change_capability_identity_match() {
        let first = definition("quran.search", "engine-a", "Search");
        let second = definition("quran.search", "engine-b", "Search");

        assert!(is_advertised(&capability_id("quran.search"), &first));
        assert!(is_advertised(&capability_id("quran.search"), &second));
    }

    #[test]
    fn optional_definition_version_does_not_change_identity_match() {
        let offered = definition("quran.search", "quran-engine", "Search")
            .with_version(Version::new(9, 9, 9));

        assert!(is_advertised(&capability_id("quran.search"), &offered));
    }

    #[test]
    fn capability_resolution_error_has_clear_display_text() {
        let error = CapabilityResolutionError::NotAdvertised {
            capability_id: capability_id("quran.search"),
        };

        assert_eq!(
            error.to_string(),
            "capability quran.search is not advertised"
        );
    }

    #[test]
    fn capability_resolution_error_implements_error_trait() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<CapabilityResolutionError>();
    }
}
