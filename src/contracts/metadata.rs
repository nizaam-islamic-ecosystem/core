use crate::contracts::descriptor::{ContractDescriptor, Version};
use crate::identity::{CapabilityId, EngineId, EngineInstanceId};

/// The communicating engine identities associated with a message.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Participants {
    pub sender: EngineId,
    pub sender_instance: Option<EngineInstanceId>,
    pub target: EngineId,
    pub target_instance: Option<EngineInstanceId>,
}

impl Participants {
    pub fn new(sender: EngineId, target: EngineId) -> Self {
        Self {
            sender,
            sender_instance: None,
            target,
            target_instance: None,
        }
    }

    pub fn with_sender_instance(mut self, instance: EngineInstanceId) -> Self {
        self.sender_instance = Some(instance);
        self
    }

    pub fn with_target_instance(mut self, instance: EngineInstanceId) -> Self {
        self.target_instance = Some(instance);
        self
    }
}

/// Declares non semantic requirements for handling a message.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RequirementsMetadata {
    pub required_capability: Option<CapabilityId>,
    pub minimum_contract_version: Option<Version>,
}

impl RequirementsMetadata {
    pub const fn none() -> Self {
        Self {
            required_capability: None,
            minimum_contract_version: None,
        }
    }

    pub fn requiring_capability(mut self, capability: CapabilityId) -> Self {
        self.required_capability = Some(capability);
        self
    }

    pub fn requiring_contract_version(mut self, version: Version) -> Self {
        self.minimum_contract_version = Some(version);
        self
    }
}

/// Declares execution hints without deciding execution policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionMetadata {
    pub priority: Option<u32>,
    pub idempotent: bool,
}

impl ExecutionMetadata {
    pub const fn standard() -> Self {
        Self {
            priority: None,
            idempotent: false,
        }
    }

    pub const fn with_priority(mut self, priority: u32) -> Self {
        self.priority = Some(priority);
        self
    }

    pub const fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }
}

/// Metadata shared by requests, responses, and messages.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContractMetadata {
    pub descriptor: ContractDescriptor,
    pub participants: Participants,
    pub requirements: RequirementsMetadata,
    pub execution: ExecutionMetadata,
}

impl ContractMetadata {
    pub fn new(descriptor: ContractDescriptor, participants: Participants) -> Self {
        Self {
            descriptor,
            participants,
            requirements: RequirementsMetadata::none(),
            execution: ExecutionMetadata::standard(),
        }
    }

    pub fn with_requirements(mut self, requirements: RequirementsMetadata) -> Self {
        self.requirements = requirements;
        self
    }

    pub fn with_execution(mut self, execution: ExecutionMetadata) -> Self {
        self.execution = execution;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{Interaction, PayloadDescriptor};
    use crate::identity::ContractId;

    #[test]
    fn metadata_keeps_participants_and_execution_hints_together() {
        let metadata = ContractMetadata::new(
            ContractDescriptor::new(
                ContractId::new("lookup.request").unwrap(),
                CapabilityId::new("lookup").unwrap(),
                Version::new(1, 0, 0),
                Interaction::Request,
                PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap(),
            ),
            Participants::new(
                EngineId::new("caller").unwrap(),
                EngineId::new("provider").unwrap(),
            ),
        )
        .with_execution(ExecutionMetadata::standard().idempotent());

        assert_eq!(metadata.participants.sender.as_str(), "caller");
        assert!(metadata.execution.idempotent);
    }

    #[test]
    fn participants_builder_methods_chain() {
        let p = Participants::new(
            EngineId::new("sender").unwrap(),
            EngineId::new("target").unwrap(),
        )
        .with_sender_instance(EngineInstanceId::new("sender-1").unwrap())
        .with_target_instance(EngineInstanceId::new("target-1").unwrap());

        assert_eq!(p.sender.as_str(), "sender");
        assert_eq!(p.target.as_str(), "target");
        assert_eq!(p.sender_instance.as_ref().unwrap().as_str(), "sender-1");
        assert_eq!(p.target_instance.as_ref().unwrap().as_str(), "target-1");
    }

    #[test]
    fn participants_builder_partial_instances() {
        let p = Participants::new(
            EngineId::new("sender").unwrap(),
            EngineId::new("target").unwrap(),
        )
        .with_sender_instance(EngineInstanceId::new("only-sender").unwrap());

        assert!(p.sender_instance.is_some());
        assert!(p.target_instance.is_none());
    }

    #[test]
    fn requirements_metadata_builder_methods_chain() {
        let r = RequirementsMetadata::none()
            .requiring_capability(CapabilityId::new("cap-x").unwrap())
            .requiring_contract_version(Version::new(2, 0, 0));

        assert_eq!(r.required_capability.as_ref().unwrap().as_str(), "cap-x");
        assert_eq!(r.minimum_contract_version.as_ref().unwrap().major(), 2);
    }

    #[test]
    fn execution_metadata_builder_methods_chain() {
        let e = ExecutionMetadata::standard().with_priority(99).idempotent();

        assert_eq!(e.priority, Some(99));
        assert!(e.idempotent);
    }

    #[test]
    fn execution_metadata_standard_is_default() {
        let e = ExecutionMetadata::standard();
        assert!(e.priority.is_none());
        assert!(!e.idempotent);
    }

    #[test]
    fn contract_metadata_with_requirements_and_execution() {
        let desc = ContractDescriptor::new(
            ContractId::new("c1").unwrap(),
            CapabilityId::new("cap1").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );
        let participants = Participants::new(
            EngineId::new("caller").unwrap(),
            EngineId::new("provider").unwrap(),
        );
        let metadata = ContractMetadata::new(desc, participants)
            .with_requirements(
                RequirementsMetadata::none()
                    .requiring_capability(CapabilityId::new("cap1").unwrap()),
            )
            .with_execution(ExecutionMetadata::standard().idempotent());

        assert!(metadata.requirements.required_capability.is_some());
        assert!(metadata.execution.idempotent);
    }
}
