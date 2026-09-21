//! Global coordination dependency vocabulary and structural validation.
//!
//! This module deliberately models only the semantic relationship between
//! global coordination nodes. It does not resolve providers, route to
//! instances, inspect health, execute retries, schedule work, or detect
//! graph-wide cycles. Those responsibilities belong to the surrounding
//! Control Plane modules.

use std::fmt;

use crate::contracts::ContractDescriptor;
use crate::identity::{CapabilityId, NodeId};

/// A capability required by a global coordination relationship.
///
/// A requirement may specify only a capability, or it may additionally
/// constrain the required contract. Contract compatibility remains owned by
/// the contract subsystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityRequirement {
    capability_id: CapabilityId,
    contract: Option<ContractDescriptor>,
}

impl CapabilityRequirement {
    /// Creates a capability-only requirement.
    pub fn new(capability_id: CapabilityId) -> Self {
        Self {
            capability_id,
            contract: None,
        }
    }

    /// Adds a contract constraint after verifying that it describes the same
    /// capability as this requirement.
    pub fn with_contract(
        mut self,
        contract: ContractDescriptor,
    ) -> Result<Self, CapabilityRequirementError> {
        let required_capability = self.capability_id.clone();
        let contract_capability = contract.capability_id.clone();

        if required_capability != contract_capability {
            return Err(CapabilityRequirementError::ContractCapabilityMismatch {
                required_capability,
                contract_capability,
            });
        }

        self.contract = Some(contract);
        Ok(self)
    }

    /// Returns the required capability.
    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }

    /// Returns the optional contract constraint.
    pub fn contract(&self) -> Option<&ContractDescriptor> {
        self.contract.as_ref()
    }

    /// Validates structural coherence.
    pub fn validate(&self) -> Result<(), CapabilityRequirementError> {
        if let Some(contract) = &self.contract {
            let required_capability = self.capability_id.clone();
            let contract_capability = contract.capability_id.clone();

            if required_capability != contract_capability {
                return Err(CapabilityRequirementError::ContractCapabilityMismatch {
                    required_capability,
                    contract_capability,
                });
            }
        }

        Ok(())
    }
}

/// Errors produced when a capability requirement is structurally invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityRequirementError {
    /// The supplied contract belongs to a different capability.
    ContractCapabilityMismatch {
        required_capability: CapabilityId,
        contract_capability: CapabilityId,
    },
}

impl fmt::Display for CapabilityRequirementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContractCapabilityMismatch {
                required_capability,
                contract_capability,
            } => write!(
                f,
                "contract capability does not match required capability: \
                 required={required_capability:?}, contract={contract_capability:?}"
            ),
        }
    }
}

impl std::error::Error for CapabilityRequirementError {}

/// An opaque reference identifying the condition under which a conditional
/// dependency applies.
///
/// This type intentionally does not interpret the condition. Its meaning is
/// owned by the planning authority that created it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConditionReference {
    value: String,
}

impl ConditionReference {
    /// Creates a condition reference.
    ///
    /// Empty and whitespace-only references are rejected because a
    /// conditional dependency must have an identifiable condition.
    pub fn new(value: impl Into<String>) -> Result<Self, ConditionReferenceError> {
        let value = value.into();

        if value.trim().is_empty() {
            return Err(ConditionReferenceError::Empty);
        }

        Ok(Self { value })
    }

    /// Returns the opaque condition identifier.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

/// Errors produced while constructing a condition reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionReferenceError {
    /// The supplied condition reference is empty or whitespace-only.
    Empty,
}

impl fmt::Display for ConditionReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("condition reference cannot be empty"),
        }
    }
}

impl std::error::Error for ConditionReferenceError {}

/// Semantic classification of a global coordination dependency.
///
/// These variants describe coordination semantics only. They are not
/// scheduler instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    /// The dependency must be satisfied before the relevant coordination
    /// path can proceed.
    Blocking,

    /// The relationship exists, but the target is not required before the
    /// relevant coordination path can continue.
    NonBlocking,

    /// The dependency applies only while its referenced condition is active.
    Conditional,
}

/// The target of a global coordination dependency.
///
/// Targets intentionally remain above provider and runtime-instance
/// resolution. In particular, there is no `EngineId`, `ProviderId`, or
/// `EngineInstanceId` here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyTarget {
    /// An already represented global coordination node.
    Node(NodeId),

    /// A capability requirement whose final provider/destination binding can
    /// be resolved later by the Control Plane.
    Capability(CapabilityRequirement),
}

impl DependencyTarget {
    /// Validates the target's contained metadata.
    pub fn validate(&self) -> Result<(), DependencyValidationError> {
        match self {
            Self::Node(_) => Ok(()),
            Self::Capability(requirement) => requirement
                .validate()
                .map_err(DependencyValidationError::InvalidCapabilityRequirement),
        }
    }
}

/// A single global coordination dependency relationship.
///
/// This value describes the relationship itself. It contains no health state,
/// provider identity, runtime instance, retry state, scheduler state, or
/// domain payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    source: NodeId,
    target: DependencyTarget,
    kind: DependencyKind,
    condition: Option<ConditionReference>,
}

impl Dependency {
    /// Creates a blocking dependency.
    pub fn blocking(
        source: NodeId,
        target: DependencyTarget,
    ) -> Result<Self, DependencyValidationError> {
        Self::new(source, target, DependencyKind::Blocking, None)
    }

    /// Creates a non-blocking dependency.
    pub fn non_blocking(
        source: NodeId,
        target: DependencyTarget,
    ) -> Result<Self, DependencyValidationError> {
        Self::new(source, target, DependencyKind::NonBlocking, None)
    }

    /// Creates a conditional dependency.
    pub fn conditional(
        source: NodeId,
        target: DependencyTarget,
        condition: ConditionReference,
    ) -> Result<Self, DependencyValidationError> {
        Self::new(source, target, DependencyKind::Conditional, Some(condition))
    }

    fn new(
        source: NodeId,
        target: DependencyTarget,
        kind: DependencyKind,
        condition: Option<ConditionReference>,
    ) -> Result<Self, DependencyValidationError> {
        let dependency = Self {
            source,
            target,
            kind,
            condition,
        };

        dependency.validate()?;
        Ok(dependency)
    }

    /// Returns the source coordination node.
    pub fn source(&self) -> &NodeId {
        &self.source
    }

    /// Returns the dependency target.
    pub fn target(&self) -> &DependencyTarget {
        &self.target
    }

    /// Returns the dependency kind.
    pub fn kind(&self) -> DependencyKind {
        self.kind
    }

    /// Returns the optional condition reference.
    pub fn condition(&self) -> Option<&ConditionReference> {
        self.condition.as_ref()
    }

    /// Validates structural coherence of this individual dependency.
    ///
    /// Graph-wide checks such as cycle detection and source/target node
    /// existence intentionally belong to `plan.rs`.
    pub fn validate(&self) -> Result<(), DependencyValidationError> {
        match &self.target {
            DependencyTarget::Node(target) if &self.source == target => {
                return Err(DependencyValidationError::SelfDependency {
                    node_id: self.source.clone(),
                });
            }
            _ => {}
        }

        self.target.validate()?;

        match (self.kind, self.condition.is_some()) {
            (DependencyKind::Blocking, true) | (DependencyKind::NonBlocking, true) => {
                Err(DependencyValidationError::ConditionNotAllowed)
            }
            (DependencyKind::Conditional, false) => {
                Err(DependencyValidationError::ConditionRequired)
            }
            _ => Ok(()),
        }
    }
}

/// Structural errors for an individual global dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyValidationError {
    /// The dependency points from a node to itself.
    SelfDependency { node_id: NodeId },

    /// A conditional dependency was created without a condition.
    ConditionRequired,

    /// A blocking or non-blocking dependency was given a condition.
    ConditionNotAllowed,

    /// The dependency contains an invalid capability requirement.
    InvalidCapabilityRequirement(CapabilityRequirementError),
}

impl fmt::Display for DependencyValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfDependency { node_id } => {
                write!(
                    f,
                    "dependency cannot target its own source node: {node_id:?}"
                )
            }
            Self::ConditionRequired => {
                f.write_str("conditional dependency requires a condition reference")
            }
            Self::ConditionNotAllowed => {
                f.write_str("blocking and non-blocking dependencies cannot have a condition")
            }
            Self::InvalidCapabilityRequirement(error) => {
                write!(f, "invalid capability requirement: {error}")
            }
        }
    }
}

impl std::error::Error for DependencyValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests intentionally keep construction focused on the public API.
    // Concrete NodeId/CapabilityId/ContractDescriptor construction depends on
    // the surrounding Control Plane types and is therefore delegated to the
    // corresponding module tests where those constructors are canonical.

    #[test]
    fn condition_reference_preserves_value() {
        let condition = ConditionReference::new("creed-sensitive").unwrap();
        assert_eq!(condition.as_str(), "creed-sensitive");
    }

    #[test]
    fn empty_condition_reference_is_rejected() {
        assert_eq!(
            ConditionReference::new(""),
            Err(ConditionReferenceError::Empty)
        );
    }

    #[test]
    fn whitespace_condition_reference_is_rejected() {
        assert_eq!(
            ConditionReference::new("   \t\n"),
            Err(ConditionReferenceError::Empty)
        );
    }

    #[test]
    fn dependency_kind_variants_are_distinct() {
        assert_ne!(DependencyKind::Blocking, DependencyKind::NonBlocking);
        assert_ne!(DependencyKind::NonBlocking, DependencyKind::Conditional);
        assert_ne!(DependencyKind::Blocking, DependencyKind::Conditional);
    }
}
