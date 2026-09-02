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

    compare_versions(&required.version, &offered.version)
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
}
