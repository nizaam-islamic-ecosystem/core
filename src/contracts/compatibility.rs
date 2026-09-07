use crate::contracts::descriptor::{ContractDescriptor, Version};
use crate::status::Compatibility;

/// Compares contract versions using structural compatibility rules only.
pub fn compare_versions(required: &Version, offered: &Version) -> Compatibility {
    if required.major() != offered.major() {
        Compatibility::Incompatible
    } else if offered >= required {
        Compatibility::Compatible
    } else {
        Compatibility::Unknown
    }
}

/// Compares the contract identity, interaction, and version.
pub fn compare_contracts(
    required: &ContractDescriptor,
    offered: &ContractDescriptor,
) -> Compatibility {
    if required.contract_id != offered.contract_id
        || required.capability_id != offered.capability_id
        || required.interaction != offered.interaction
        || required.payload.media_type() != offered.payload.media_type()
    {
        return Compatibility::Incompatible;
    }

    let contract_compatibility = compare_versions(&required.version, &offered.version);
    if contract_compatibility != Compatibility::Compatible {
        return contract_compatibility;
    }

    compare_versions(
        required.payload.schema_version(),
        offered.payload.schema_version(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{ContractDescriptor, Interaction, PayloadDescriptor};
    use crate::identity::{CapabilityId, ContractId};

    #[test]
    fn versions_require_the_same_major_version() {
        assert_eq!(
            compare_versions(&Version::new(1, 0, 0), &Version::new(1, 1, 0)),
            Compatibility::Compatible
        );
        assert_eq!(
            compare_versions(&Version::new(1, 0, 0), &Version::new(2, 0, 0)),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn contracts_reject_incompatible_payload_schema_major_versions() {
        let required_payload = crate::contracts::descriptor::PayloadDescriptor::new(
            "application/json",
            Version::new(1, 0, 0),
        )
        .unwrap();
        let offered_payload = crate::contracts::descriptor::PayloadDescriptor::new(
            "application/json",
            Version::new(2, 0, 0),
        )
        .unwrap();
        let required = ContractDescriptor::new(
            crate::identity::ContractId::new("lookup").unwrap(),
            crate::identity::CapabilityId::new("lookup").unwrap(),
            Version::new(1, 0, 0),
            crate::contracts::descriptor::Interaction::Request,
            required_payload,
        );
        let offered = ContractDescriptor::new(
            required.contract_id.clone(),
            required.capability_id.clone(),
            required.version.clone(),
            required.interaction,
            offered_payload,
        );

        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn versions_return_unknown_when_offered_is_less_than_required_same_major() {
        // Same major (1.x.x) but offered is lower patch than required
        assert_eq!(
            compare_versions(&Version::new(1, 0, 5), &Version::new(1, 0, 3)),
            Compatibility::Unknown
        );
        // Same major but offered is lower minor
        assert_eq!(
            compare_versions(&Version::new(1, 5, 0), &Version::new(1, 2, 9)),
            Compatibility::Unknown
        );
    }

    #[test]
    fn contracts_require_same_contract_id() {
        let payload = PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let required = ContractDescriptor::new(
            ContractId::new("contract-a").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        let offered = ContractDescriptor::new(
            ContractId::new("contract-b").unwrap(), // different contract_id
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn contracts_require_same_capability_id() {
        let payload = PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let required = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap-a").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        let offered = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap-b").unwrap(), // different capability_id
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn contracts_require_same_interaction() {
        let payload = PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let required = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        let offered = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Response, // different interaction
            payload.clone(),
        );
        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn contracts_require_same_media_type() {
        let required_payload =
            PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let offered_payload =
            PayloadDescriptor::new("application/problem+json", Version::new(1, 0, 0)).unwrap();
        let required = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            required_payload,
        );
        let offered = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            offered_payload,
        );
        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Incompatible
        );
    }

    #[test]
    fn contracts_return_compatible_when_all_fields_match_with_higher_version() {
        let payload = PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let required = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload.clone(),
        );
        let offered = ContractDescriptor::new(
            ContractId::new("contract").unwrap(),
            CapabilityId::new("cap").unwrap(),
            Version::new(1, 2, 5), // higher minor + patch, same major
            Interaction::Request,
            payload.clone(),
        );
        assert_eq!(
            compare_contracts(&required, &offered),
            Compatibility::Compatible
        );
    }
}
