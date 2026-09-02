use crate::contracts::descriptor::{ContractDescriptor, Version};
use crate::identity::{CapabilityId, EngineId, EngineInstanceId};

/// The communicating engine identities associated with a message.
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
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

    #[test]
    fn metadata_keeps_participants_and_execution_hints_together() {
        let metadata = ContractMetadata::new(
            ContractDescriptor::new(
                crate::identity::ContractId::new("lookup.request").unwrap(),
                CapabilityId::new("lookup").unwrap(),
                Version::new(1, 0, 0),
                crate::contracts::descriptor::Interaction::Request,
                crate::contracts::descriptor::PayloadDescriptor::new(
                    "application/json",
                    Version::new(1, 0, 0),
                )
                .unwrap(),
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
}
