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
}
