//! Immutable global coordination plans for the Control Plane.
//!
//! A `Plan` represents one version of ecosystem-level coordination state.
//! It contains only globally relevant coordination nodes and dependency
//! relationships.
//!
//! This module does NOT:
//! - resolve providers;
//! - select engine instances;
//! - perform routing;
//! - execute capabilities;
//! - schedule workers;
//! - manage retries;
//! - inspect health;
//! - interpret domain semantics;
//! - perform replanning.
//!
//! Those responsibilities belong to the surrounding Control Plane modules.

use std::collections::HashMap;
use std::fmt;

use super::dependency::{
    CapabilityRequirement, CapabilityRequirementError, Dependency, DependencyTarget,
    DependencyValidationError,
};

/// Re-export the canonical Core plan identity types through the Control
/// Plane plan module.
///
/// These are NOT new Control Plane identity types.
pub use crate::identity::{NodeId, OperationId, PlanId};

/// Version of one logical global coordination plan.
///
/// `PlanVersion` is intentionally distinct from `PlanId` and `OperationId`.
///
/// Example:
///
/// ```text
/// Operation X
///     │
///     └── Plan P
///          ├── Version 1
///          ├── Version 2
///          └── Version 3
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanVersion(u64);

impl PlanVersion {
    /// The first version of a plan.
    pub const FIRST: Self = Self(1);

    /// Creates a valid plan version.
    ///
    /// Version zero is not valid.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Returns the numeric version.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Returns the next version.
    ///
    /// Returns `None` if the version would overflow `u64`.
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

impl Default for PlanVersion {
    fn default() -> Self {
        Self::FIRST
    }
}

impl fmt::Display for PlanVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Coordination lifecycle state of one global plan version.
///
/// These states describe Control Plane coordination only.
///
/// They MUST NOT be interpreted as domain-semantic completion.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PlanState {
    /// Plan is being constructed.
    #[default]
    Draft,

    /// Plan has passed structural validation.
    Validated,

    /// Plan is the active coordination version.
    Active,

    /// A newer plan version has replaced this version.
    Superseded,

    /// Coordination represented by this version completed.
    Completed,

    /// Coordination represented by this version failed.
    Failed,
}

impl PlanState {
    /// Returns whether this state is terminal.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Superseded | Self::Completed | Self::Failed)
    }

    /// Returns whether the plan's structure may still be constructed.
    pub const fn is_mutable(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Determines whether a lifecycle transition is valid.
    ///
    /// State transitions are deliberately controlled instead of exposing
    /// arbitrary `set_state` mutation.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            // Construction
            (Self::Draft, Self::Validated)
                | (Self::Draft, Self::Failed)

                // Validated coordination
                | (Self::Validated, Self::Active)
                | (Self::Validated, Self::Superseded)
                | (Self::Validated, Self::Failed)

                // Active coordination
                | (Self::Active, Self::Superseded)
                | (Self::Active, Self::Completed)
                | (Self::Active, Self::Failed)
        )
    }
}

/// A globally relevant coordination node.
///
/// A `PlanNode` represents a capability/interaction relevant to the
/// ecosystem boundary.
///
/// It does NOT represent every internal task performed by an engine.
///
/// For example:
///
/// ```text
/// VALID GLOBAL NODE:
///
/// Hadith → requires Quran.Search
///
/// INVALID GLOBAL NODE:
///
/// Hadith → tokenize → normalize → parse → inspect
/// ```
///
/// The second graph belongs to the engine-local workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanNode {
    node_id: NodeId,
    requirement: CapabilityRequirement,
}

impl PlanNode {
    /// Creates a global coordination node.
    pub fn new(node_id: NodeId, requirement: CapabilityRequirement) -> Self {
        Self {
            node_id,
            requirement,
        }
    }

    /// Returns this node's identity.
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Returns the capability requirement represented by this node.
    pub fn requirement(&self) -> &CapabilityRequirement {
        &self.requirement
    }

    /// Returns the capability required by this node.
    pub fn capability_id(&self) -> &crate::identity::CapabilityId {
        self.requirement.capability_id()
    }

    /// Validates the node's structural metadata.
    pub fn validate(&self) -> Result<(), PlanNodeValidationError> {
        self.requirement
            .validate()
            .map_err(PlanNodeValidationError::InvalidCapabilityRequirement)
    }
}

/// Structural validation errors for a single plan node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanNodeValidationError {
    /// The capability requirement is internally inconsistent.
    InvalidCapabilityRequirement(CapabilityRequirementError),
}

impl fmt::Display for PlanNodeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapabilityRequirement(error) => {
                write!(formatter, "invalid capability requirement: {error}")
            }
        }
    }
}

impl std::error::Error for PlanNodeValidationError {}

/// Error returned when an invalid lifecycle transition is attempted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidPlanStateTransition {
    /// Previous state.
    pub from: PlanState,

    /// Requested state.
    pub to: PlanState,
}

impl fmt::Display for InvalidPlanStateTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid plan state transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for InvalidPlanStateTransition {}

/// Structural validation errors for a complete global coordination plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanValidationError {
    /// The same `NodeId` appears more than once.
    DuplicateNodeId { node_id: NodeId },

    /// A plan node contains invalid structural metadata.
    InvalidNode {
        node_id: NodeId,
        error: PlanNodeValidationError,
    },

    /// A dependency references a source node that does not exist.
    MissingSourceNode { node_id: NodeId },

    /// A concrete node target does not exist.
    ///
    /// `DependencyTarget::Capability` does not trigger this error because
    /// capability requirements may remain unresolved until later Control
    /// Plane resolution.
    MissingTargetNode { node_id: NodeId },

    /// A dependency itself is structurally invalid.
    InvalidDependency {
        source: NodeId,
        error: DependencyValidationError,
    },

    /// A concrete node-to-node dependency cycle exists.
    DependencyCycle { nodes: Vec<NodeId> },
}

impl fmt::Display for PlanValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNodeId { node_id } => {
                write!(formatter, "duplicate plan node id: {node_id}")
            }

            Self::InvalidNode { node_id, error } => {
                write!(formatter, "invalid plan node {node_id}: {error}")
            }

            Self::MissingSourceNode { node_id } => {
                write!(
                    formatter,
                    "dependency source node is missing from plan: {node_id}"
                )
            }

            Self::MissingTargetNode { node_id } => {
                write!(
                    formatter,
                    "dependency target node is missing from plan: {node_id}"
                )
            }

            Self::InvalidDependency { source, error } => {
                write!(formatter, "invalid dependency from {source}: {error}")
            }

            Self::DependencyCycle { nodes } => {
                formatter.write_str("dependency cycle detected: ")?;

                for (index, node) in nodes.iter().enumerate() {
                    if index > 0 {
                        formatter.write_str(" -> ")?;
                    }

                    write!(formatter, "{node}")?;
                }

                Ok(())
            }
        }
    }
}

impl std::error::Error for PlanValidationError {}

/// Errors produced while constructing a plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanBuildError {
    /// A node with this identity has already been added.
    DuplicateNodeId { node_id: NodeId },

    /// The dependency itself violates its structural invariants.
    InvalidDependency {
        source: NodeId,
        error: DependencyValidationError,
    },

    /// The complete plan failed structural validation.
    Validation(PlanValidationError),
}

impl fmt::Display for PlanBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNodeId { node_id } => {
                write!(formatter, "duplicate plan node id: {node_id}")
            }

            Self::InvalidDependency { source, error } => {
                write!(formatter, "invalid dependency from {source}: {error}")
            }

            Self::Validation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PlanBuildError {}

impl From<PlanValidationError> for PlanBuildError {
    fn from(error: PlanValidationError) -> Self {
        Self::Validation(error)
    }
}

/// One immutable version of the global Control Plane coordination plan.
///
/// Structural contents are private and cannot be mutated after construction.
///
/// A plan contains:
///
/// - plan identity;
/// - operation identity;
/// - plan version;
/// - coordination state;
/// - globally relevant coordination nodes;
/// - global dependency relationships.
///
/// It does NOT contain:
///
/// - provider implementation;
/// - engine instance routing;
/// - worker state;
/// - retry state;
/// - queue state;
/// - health state;
/// - domain payload;
/// - domain results;
/// - engine-local workflow steps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    plan_id: PlanId,
    operation_id: OperationId,
    version: PlanVersion,
    state: PlanState,
    nodes: Vec<PlanNode>,
    dependencies: Vec<Dependency>,
}

impl Plan {
    /// Creates a builder for the first version of a plan.
    pub fn builder(plan_id: PlanId, operation_id: OperationId) -> PlanBuilder {
        PlanBuilder::new(plan_id, operation_id)
    }

    /// Returns the plan identity.
    pub fn plan_id(&self) -> &PlanId {
        &self.plan_id
    }

    /// Returns the operation identity.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns this plan's version.
    pub const fn version(&self) -> PlanVersion {
        self.version
    }

    /// Returns the current coordination state.
    pub const fn state(&self) -> PlanState {
        self.state
    }

    /// Returns all global coordination nodes.
    ///
    /// No mutable accessor is exposed.
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    /// Returns all global dependencies.
    ///
    /// No mutable accessor is exposed.
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Returns a node by its identity.
    pub fn node(&self, node_id: &NodeId) -> Option<&PlanNode> {
        self.nodes.iter().find(|node| node.node_id() == node_id)
    }

    /// Returns all dependencies originating from a node.
    pub fn dependencies_from(&self, source: &NodeId) -> impl Iterator<Item = &Dependency> {
        self.dependencies
            .iter()
            .filter(move |dependency| dependency.source() == source)
    }

    /// Performs complete structural validation of the plan.
    ///
    /// This method validates only the global plan structure.
    ///
    /// It intentionally does NOT perform:
    ///
    /// - capability resolution;
    /// - provider resolution;
    /// - contract negotiation;
    /// - destination resolution;
    /// - routing;
    /// - policy evaluation;
    /// - health evaluation;
    /// - security authorization;
    /// - runtime admission;
    /// - domain validation.
    pub fn validate(&self) -> Result<(), PlanValidationError> {
        let mut node_indices = HashMap::with_capacity(self.nodes.len());

        // ------------------------------------------------------------------
        // Validate nodes and establish the node identity index.
        // ------------------------------------------------------------------

        for (index, node) in self.nodes.iter().enumerate() {
            if node_indices.insert(node.node_id().clone(), index).is_some() {
                return Err(PlanValidationError::DuplicateNodeId {
                    node_id: node.node_id().clone(),
                });
            }

            node.validate()
                .map_err(|error| PlanValidationError::InvalidNode {
                    node_id: node.node_id().clone(),
                    error,
                })?;
        }

        // ------------------------------------------------------------------
        // Validate every dependency.
        // ------------------------------------------------------------------

        for dependency in &self.dependencies {
            dependency
                .validate()
                .map_err(|error| PlanValidationError::InvalidDependency {
                    source: dependency.source().clone(),
                    error,
                })?;

            // Every dependency source must correspond to an actual global
            // coordination node.
            if !node_indices.contains_key(dependency.source()) {
                return Err(PlanValidationError::MissingSourceNode {
                    node_id: dependency.source().clone(),
                });
            }

            // Only concrete Node targets participate in the global graph.
            //
            // Capability targets intentionally remain unresolved and therefore
            // do not need a concrete node at this stage.
            if let DependencyTarget::Node(target) = dependency.target()
                && !node_indices.contains_key(target)
            {
                return Err(PlanValidationError::MissingTargetNode {
                    node_id: target.clone(),
                });
            }
        }

        // ------------------------------------------------------------------
        // Validate graph topology.
        // ------------------------------------------------------------------

        if let Some(cycle) = self.find_dependency_cycle(&node_indices) {
            return Err(PlanValidationError::DependencyCycle { nodes: cycle });
        }

        Ok(())
    }

    /// Performs a controlled coordination-state transition.
    ///
    /// This changes lifecycle state only. It does not create a new plan
    /// version.
    pub fn transition_state(&mut self, next: PlanState) -> Result<(), InvalidPlanStateTransition> {
        if !self.state.can_transition_to(next) {
            return Err(InvalidPlanStateTransition {
                from: self.state,
                to: next,
            });
        }

        self.state = next;
        Ok(())
    }

    /// Finds the first deterministic node-to-node cycle.
    ///
    /// Capability-target dependencies are deliberately excluded because they
    /// are unresolved requirements rather than concrete graph edges.
    fn find_dependency_cycle(&self, node_indices: &HashMap<NodeId, usize>) -> Option<Vec<NodeId>> {
        let mut adjacency = vec![Vec::<usize>::new(); self.nodes.len()];

        for dependency in &self.dependencies {
            if let DependencyTarget::Node(target) = dependency.target() {
                let source_index = node_indices[dependency.source()];
                let target_index = node_indices[target];

                adjacency[source_index].push(target_index);
            }
        }

        // Ensure deterministic traversal independent of dependency insertion
        // order.
        for edges in &mut adjacency {
            edges.sort_by(|left, right| {
                self.nodes[*left]
                    .node_id()
                    .cmp(self.nodes[*right].node_id())
            });

            edges.dedup();
        }

        let mut color = vec![VisitState::Unvisited; self.nodes.len()];
        let mut stack = Vec::new();
        let mut positions = HashMap::<usize, usize>::new();

        // Traverse roots by canonical node identity rather than insertion
        // order. This makes cycle reporting deterministic even when callers
        // construct the same graph in a different node order.
        let mut starts: Vec<usize> = (0..self.nodes.len()).collect();
        starts.sort_by(|left, right| {
            self.nodes[*left]
                .node_id()
                .cmp(self.nodes[*right].node_id())
        });

        for start in starts {
            if color[start] != VisitState::Unvisited {
                continue;
            }

            if let Some(cycle) =
                self.visit_for_cycle(start, &adjacency, &mut color, &mut stack, &mut positions)
            {
                return Some(cycle);
            }
        }

        None
    }

    fn visit_for_cycle(
        &self,
        current: usize,
        adjacency: &[Vec<usize>],
        color: &mut [VisitState],
        stack: &mut Vec<usize>,
        positions: &mut HashMap<usize, usize>,
    ) -> Option<Vec<NodeId>> {
        color[current] = VisitState::Visiting;

        positions.insert(current, stack.len());
        stack.push(current);

        for &next in &adjacency[current] {
            match color[next] {
                VisitState::Unvisited => {
                    if let Some(cycle) =
                        self.visit_for_cycle(next, adjacency, color, stack, positions)
                    {
                        return Some(cycle);
                    }
                }

                VisitState::Visiting => {
                    let start = positions[&next];

                    let mut cycle = stack[start..]
                        .iter()
                        .map(|index| self.nodes[*index].node_id().clone())
                        .collect::<Vec<_>>();

                    // Close the cycle.
                    cycle.push(self.nodes[next].node_id().clone());

                    return Some(cycle);
                }

                VisitState::Visited => {}
            }
        }

        stack.pop();
        positions.remove(&current);
        color[current] = VisitState::Visited;

        None
    }
}

/// DFS traversal state used only by plan-level cycle detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

/// Builder for one immutable plan version.
///
/// Construction happens here; replanning belongs to `replanning.rs`.
#[derive(Clone, Debug)]
pub struct PlanBuilder {
    plan_id: PlanId,
    operation_id: OperationId,
    version: PlanVersion,
    nodes: Vec<PlanNode>,
    dependencies: Vec<Dependency>,
}

impl PlanBuilder {
    /// Creates a builder for the first plan version.
    pub fn new(plan_id: PlanId, operation_id: OperationId) -> Self {
        Self {
            plan_id,
            operation_id,
            version: PlanVersion::FIRST,
            nodes: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    /// Sets the version represented by the plan.
    pub fn version(mut self, version: PlanVersion) -> Self {
        self.version = version;
        self
    }

    /// Adds a global coordination node.
    ///
    /// Duplicate `NodeId`s are rejected immediately.
    pub fn add_node(mut self, node: PlanNode) -> Result<Self, PlanBuildError> {
        if self
            .nodes
            .iter()
            .any(|existing| existing.node_id() == node.node_id())
        {
            return Err(PlanBuildError::DuplicateNodeId {
                node_id: node.node_id().clone(),
            });
        }

        self.nodes.push(node);
        Ok(self)
    }

    /// Adds one global dependency relationship.
    ///
    /// The individual dependency is structurally validated here. Plan-wide
    /// checks such as source/target existence and cycles happen during
    /// `build()`.
    pub fn add_dependency(mut self, dependency: Dependency) -> Result<Self, PlanBuildError> {
        dependency
            .validate()
            .map_err(|error| PlanBuildError::InvalidDependency {
                source: dependency.source().clone(),
                error,
            })?;

        self.dependencies.push(dependency);

        Ok(self)
    }

    /// Builds and validates the immutable plan.
    pub fn build(self) -> Result<Plan, PlanBuildError> {
        let plan = Plan {
            plan_id: self.plan_id,
            operation_id: self.operation_id,
            version: self.version,
            state: PlanState::Draft,
            nodes: self.nodes,
            dependencies: self.dependencies,
        };

        plan.validate()?;

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::dependency::{DependencyKind, DependencyTarget};
    use crate::identity::CapabilityId;

    fn plan_id(value: &str) -> PlanId {
        PlanId::new(value).unwrap()
    }

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn node_id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    fn capability(value: &str) -> CapabilityId {
        CapabilityId::new(value).unwrap()
    }

    fn node(id: &str, capability_id: &str) -> PlanNode {
        PlanNode::new(
            node_id(id),
            CapabilityRequirement::new(capability(capability_id)),
        )
    }

    // -------------------------------------------------------------------------
    // PlanVersion
    // -------------------------------------------------------------------------

    #[test]
    fn plan_version_starts_at_one() {
        assert_eq!(PlanVersion::FIRST.value(), 1);
        assert_eq!(PlanVersion::default(), PlanVersion::FIRST);
    }

    #[test]
    fn plan_version_rejects_zero() {
        assert_eq!(PlanVersion::new(0), None);
    }

    #[test]
    fn plan_version_preserves_numeric_value() {
        let version = PlanVersion::new(42).unwrap();

        assert_eq!(version.value(), 42);
        assert_eq!(version.to_string(), "42");
    }

    #[test]
    fn plan_version_next_does_not_mutate_previous_version() {
        let version = PlanVersion::FIRST;

        let next = version.next().unwrap();

        assert_eq!(version.value(), 1);
        assert_eq!(next.value(), 2);
    }

    // -------------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------------

    #[test]
    fn builder_constructs_draft_plan() {
        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("n1", "quran.search"))
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(plan.plan_id().as_str(), "plan-1");
        assert_eq!(plan.operation_id().as_str(), "op-1");
        assert_eq!(plan.version(), PlanVersion::FIRST);
        assert_eq!(plan.state(), PlanState::Draft);
        assert_eq!(plan.nodes().len(), 1);
        assert!(plan.dependencies().is_empty());
    }

    #[test]
    fn duplicate_node_ids_are_rejected() {
        let result = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("n1", "quran.search"))
            .unwrap()
            .add_node(node("n1", "quran.retrieve"));

        assert!(matches!(
            result,
            Err(PlanBuildError::DuplicateNodeId { .. })
        ));
    }

    #[test]
    fn explicit_plan_version_is_preserved() {
        let version = PlanVersion::new(7).unwrap();

        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .version(version)
            .build()
            .unwrap();

        assert_eq!(plan.version(), version);
    }

    // -------------------------------------------------------------------------
    // Dependency validation
    // -------------------------------------------------------------------------

    #[test]
    fn missing_dependency_source_is_rejected() {
        let dependency = Dependency::blocking(
            node_id("missing"),
            DependencyTarget::Node(node_id("target")),
        )
        .unwrap();

        let result = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("target", "quran.search"))
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build();

        assert!(matches!(
            result,
            Err(PlanBuildError::Validation(
                PlanValidationError::MissingSourceNode { .. }
            ))
        ));
    }

    #[test]
    fn missing_node_target_is_rejected() {
        let dependency = Dependency::blocking(
            node_id("source"),
            DependencyTarget::Node(node_id("missing")),
        )
        .unwrap();

        let result = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("source", "hadith.search"))
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build();

        assert!(matches!(
            result,
            Err(PlanBuildError::Validation(
                PlanValidationError::MissingTargetNode { .. }
            ))
        ));
    }

    #[test]
    fn capability_target_does_not_require_concrete_node() {
        let dependency = Dependency::blocking(
            node_id("source"),
            DependencyTarget::Capability(CapabilityRequirement::new(capability("quran.search"))),
        )
        .unwrap();

        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("source", "hadith.search"))
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build()
            .unwrap();

        assert!(plan.validate().is_ok());
    }

    // -------------------------------------------------------------------------
    // Cycle detection
    // -------------------------------------------------------------------------

    #[test]
    fn node_to_node_cycle_is_rejected() {
        let a_to_b =
            Dependency::blocking(node_id("a"), DependencyTarget::Node(node_id("b"))).unwrap();

        let b_to_c =
            Dependency::blocking(node_id("b"), DependencyTarget::Node(node_id("c"))).unwrap();

        let c_to_a =
            Dependency::blocking(node_id("c"), DependencyTarget::Node(node_id("a"))).unwrap();

        let result = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("a", "cap.a"))
            .unwrap()
            .add_node(node("b", "cap.b"))
            .unwrap()
            .add_node(node("c", "cap.c"))
            .unwrap()
            .add_dependency(a_to_b)
            .unwrap()
            .add_dependency(b_to_c)
            .unwrap()
            .add_dependency(c_to_a)
            .unwrap()
            .build();

        assert!(matches!(
            result,
            Err(PlanBuildError::Validation(
                PlanValidationError::DependencyCycle { .. }
            ))
        ));
    }

    #[test]
    fn acyclic_node_graph_is_accepted() {
        let a_to_b =
            Dependency::blocking(node_id("a"), DependencyTarget::Node(node_id("b"))).unwrap();

        let b_to_c =
            Dependency::blocking(node_id("b"), DependencyTarget::Node(node_id("c"))).unwrap();

        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("a", "cap.a"))
            .unwrap()
            .add_node(node("b", "cap.b"))
            .unwrap()
            .add_node(node("c", "cap.c"))
            .unwrap()
            .add_dependency(a_to_b)
            .unwrap()
            .add_dependency(b_to_c)
            .unwrap()
            .build()
            .unwrap();

        assert!(plan.validate().is_ok());
        assert_eq!(plan.dependencies_from(&node_id("a")).count(), 1);
    }

    #[test]
    fn cycle_detection_is_deterministic() {
        let dependencies = [
            Dependency::blocking(node_id("b"), DependencyTarget::Node(node_id("c"))).unwrap(),
            Dependency::blocking(node_id("a"), DependencyTarget::Node(node_id("b"))).unwrap(),
            Dependency::blocking(node_id("c"), DependencyTarget::Node(node_id("a"))).unwrap(),
        ];

        let mut builder = Plan::builder(plan_id("plan-1"), operation_id("op-1"));

        for node_name in ["c", "a", "b"] {
            builder = builder.add_node(node(node_name, "cap")).unwrap();
        }

        for dependency in dependencies {
            builder = builder.add_dependency(dependency).unwrap();
        }

        let error = builder.build().unwrap_err();

        match error {
            PlanBuildError::Validation(PlanValidationError::DependencyCycle { nodes }) => {
                assert_eq!(nodes.len(), 4);
                assert_eq!(nodes[0].as_str(), "a");
                assert_eq!(nodes[1].as_str(), "b");
                assert_eq!(nodes[2].as_str(), "c");
                assert_eq!(nodes[3].as_str(), "a");
            }

            other => panic!("unexpected error: {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Accessors
    // -------------------------------------------------------------------------

    #[test]
    fn node_lookup_returns_expected_node() {
        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("n1", "quran.search"))
            .unwrap()
            .build()
            .unwrap();

        let found = plan.node(&node_id("n1")).unwrap();

        assert_eq!(found.node_id().as_str(), "n1");
        assert_eq!(found.capability_id().as_str(), "quran.search");
    }

    #[test]
    fn dependency_lookup_filters_by_source() {
        let dependency =
            Dependency::blocking(node_id("a"), DependencyTarget::Node(node_id("b"))).unwrap();

        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("a", "cap.a"))
            .unwrap()
            .add_node(node("b", "cap.b"))
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(plan.dependencies_from(&node_id("a")).count(), 1);

        assert_eq!(plan.dependencies_from(&node_id("b")).count(), 0);
    }

    // -------------------------------------------------------------------------
    // Lifecycle
    // -------------------------------------------------------------------------

    #[test]
    fn draft_can_transition_to_validated() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        assert!(plan.transition_state(PlanState::Validated).is_ok());

        assert_eq!(plan.state(), PlanState::Validated);
    }

    #[test]
    fn validated_can_transition_to_active() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        assert_eq!(plan.state(), PlanState::Active);
    }

    #[test]
    fn draft_cannot_transition_directly_to_active() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        assert_eq!(
            plan.transition_state(PlanState::Active),
            Err(InvalidPlanStateTransition {
                from: PlanState::Draft,
                to: PlanState::Active,
            })
        );
    }

    #[test]
    fn active_can_complete() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        plan.transition_state(PlanState::Completed).unwrap();

        assert_eq!(plan.state(), PlanState::Completed);
    }

    #[test]
    fn active_can_fail() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        plan.transition_state(PlanState::Failed).unwrap();

        assert_eq!(plan.state(), PlanState::Failed);
    }

    #[test]
    fn active_can_be_superseded() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        plan.transition_state(PlanState::Superseded).unwrap();

        assert_eq!(plan.state(), PlanState::Superseded);
    }

    #[test]
    fn terminal_state_cannot_be_reactivated() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        plan.transition_state(PlanState::Completed).unwrap();

        assert!(plan.transition_state(PlanState::Active).is_err());
    }

    #[test]
    fn state_transition_does_not_change_plan_version() {
        let mut plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        let version = plan.version();

        plan.transition_state(PlanState::Validated).unwrap();

        plan.transition_state(PlanState::Active).unwrap();

        assert_eq!(plan.version(), version);
    }

    // -------------------------------------------------------------------------
    // Architectural boundaries
    // -------------------------------------------------------------------------

    #[test]
    fn plan_keeps_dependencies_at_plan_level() {
        let dependency =
            Dependency::blocking(node_id("a"), DependencyTarget::Node(node_id("b"))).unwrap();

        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .add_node(node("a", "cap.a"))
            .unwrap()
            .add_node(node("b", "cap.b"))
            .unwrap()
            .add_dependency(dependency)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(plan.dependencies()[0].kind(), DependencyKind::Blocking);

        assert_eq!(plan.nodes()[0].capability_id().as_str(), "cap.a");
    }

    #[test]
    fn capability_requirement_remains_unresolved_at_plan_level() {
        let node = node("n1", "quran.search");

        assert_eq!(node.capability_id().as_str(), "quran.search");

        assert!(node.requirement().contract().is_none());
    }

    #[test]
    fn plan_has_no_runtime_attempt_state() {
        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        assert!(plan.nodes().is_empty());
        assert!(plan.dependencies().is_empty());
    }

    // -------------------------------------------------------------------------
    // State semantics
    // -------------------------------------------------------------------------

    #[test]
    fn terminal_state_semantics_are_correct() {
        assert!(!PlanState::Draft.is_terminal());
        assert!(!PlanState::Validated.is_terminal());
        assert!(!PlanState::Active.is_terminal());

        assert!(PlanState::Superseded.is_terminal());
        assert!(PlanState::Completed.is_terminal());
        assert!(PlanState::Failed.is_terminal());
    }

    #[test]
    fn only_draft_is_structurally_mutable() {
        assert!(PlanState::Draft.is_mutable());
        assert!(!PlanState::Validated.is_mutable());
        assert!(!PlanState::Active.is_mutable());
        assert!(!PlanState::Superseded.is_mutable());
        assert!(!PlanState::Completed.is_mutable());
        assert!(!PlanState::Failed.is_mutable());
    }

    // -------------------------------------------------------------------------
    // Identity separation
    // -------------------------------------------------------------------------

    #[test]
    fn plan_identity_and_operation_identity_are_distinct_values() {
        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .build()
            .unwrap();

        assert_eq!(plan.plan_id().as_str(), "plan-1");
        assert_eq!(plan.operation_id().as_str(), "op-1");
        assert_ne!(plan.plan_id().as_str(), plan.operation_id().as_str());
    }

    #[test]
    fn changing_plan_version_does_not_change_operation_identity() {
        let plan = Plan::builder(plan_id("plan-1"), operation_id("op-1"))
            .version(PlanVersion::new(2).unwrap())
            .build()
            .unwrap();

        assert_eq!(plan.operation_id().as_str(), "op-1");
        assert_eq!(plan.version().value(), 2);
    }
}
