//! Global coordination-plan replanning.
//!
//! `replanning.rs` determines whether an existing global coordination plan
//! remains semantically valid when a new set of capability-level coordination
//! requirements is introduced.
//!
//! This module does not:
//! - construct or mutate plans;
//! - allocate a new PlanId;
//! - increment PlanVersion;
//! - resolve providers;
//! - select EngineInstanceId values;
//! - perform routing;
//! - execute capabilities;
//! - inspect domain payloads;
//! - own engine-local workflows;
//! - perform retry;
//! - perform health evaluation;
//! - manage plan lifecycle;
//! - perform transport;
//! - persist coordination state.
//!
//! The intended relationship is:
//!
//! ```text
//! engine-local planner
//!        |
//!        | global capability requirement
//!        v
//! dependency / coordination representation
//!        |
//!        v
//! replanning.rs
//!        |
//!        | material global change?
//!        +--------------------+
//!        |                    |
//!       no                   yes
//!        |                    |
//!        v                    v
//!  existing plan         plan.rs
//!  remains valid         creates next PlanVersion
//! ```
//!
//! A plan revision is distinct from an operation, an execution attempt, and a
//! routing decision.
//!
//! ```text
//! PlanId + PlanVersion
//!         !=
//! OperationId
//!         !=
//! AttemptId
//! ```
//!
//! Replanning is intentionally synchronous and deterministic. It evaluates
//! explicit coordination snapshots supplied by the caller and performs no
//! external observation.

use std::fmt;

use crate::control_plane::dependency::Dependency;

/// Immutable global coordination snapshot used for replanning comparison.
///
/// The snapshot contains only coordination-level dependencies. Engine-local
/// workflow information must not be represented here.
///
/// Dependencies are de-duplicated by value. Snapshot equality and replanning
/// comparison are semantic and therefore independent of the order in which
/// requirements were supplied.
#[derive(Clone, Debug, Default)]
pub struct CoordinationSnapshot {
    dependencies: Vec<Dependency>,
}

impl PartialEq for CoordinationSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.dependencies.len() == other.dependencies.len()
            && self
                .dependencies
                .iter()
                .all(|dependency| other.dependencies.contains(dependency))
    }
}

impl Eq for CoordinationSnapshot {}

impl CoordinationSnapshot {
    /// Creates an empty coordination snapshot.
    pub const fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    /// Creates a snapshot from coordination dependencies.
    ///
    /// Duplicate dependencies are represented only once when the underlying
    /// dependency value is identical.
    pub fn from_dependencies<I>(dependencies: I) -> Self
    where
        I: IntoIterator<Item = Dependency>,
    {
        let mut normalized = Vec::new();

        for dependency in dependencies {
            if !normalized.contains(&dependency) {
                normalized.push(dependency);
            }
        }

        Self {
            dependencies: normalized,
        }
    }

    /// Returns the de-duplicated coordination dependencies.
    pub fn dependencies(&self) -> impl Iterator<Item = &Dependency> {
        self.dependencies.iter()
    }

    /// Returns the number of distinct coordination dependencies.
    pub fn len(&self) -> usize {
        self.dependencies.len()
    }

    /// Returns whether the snapshot contains no coordination dependencies.
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
    }

    /// Returns whether this snapshot contains the supplied dependency.
    pub fn contains(&self, dependency: &Dependency) -> bool {
        self.dependencies.contains(dependency)
    }

    /// Returns a new snapshot containing this snapshot plus one dependency.
    ///
    /// The existing snapshot is not mutated.
    pub fn with_dependency(&self, dependency: Dependency) -> Self {
        let mut dependencies = self.dependencies.clone();
        if !dependencies.contains(&dependency) {
            dependencies.push(dependency);
        }

        Self { dependencies }
    }

    /// Returns a new snapshot without the supplied dependency.
    ///
    /// The existing snapshot is not mutated.
    pub fn without_dependency(&self, dependency: &Dependency) -> Self {
        let mut dependencies = self.dependencies.clone();
        if let Some(index) = dependencies
            .iter()
            .position(|candidate| candidate == dependency)
        {
            dependencies.remove(index);
        }

        Self { dependencies }
    }

    /// Returns the dependencies that exist in `other` but not in `self`.
    pub fn added_dependencies<'a>(
        &'a self,
        other: &'a Self,
    ) -> impl Iterator<Item = &'a Dependency> {
        other
            .dependencies
            .iter()
            .filter(move |dependency| !self.dependencies.contains(dependency))
    }

    /// Returns the dependencies that exist in `self` but not in `other`.
    pub fn removed_dependencies<'a>(
        &'a self,
        other: &'a Self,
    ) -> impl Iterator<Item = &'a Dependency> {
        self.dependencies
            .iter()
            .filter(move |dependency| !other.dependencies.contains(dependency))
    }
}

/// Input supplied to the replanning evaluator.
///
/// `current` describes the global coordination state represented by the
/// existing plan. `proposed` describes the global coordination state after
/// accepting the newly supplied coordination requirements.
///
/// Replanning compares these states semantically; it does not modify either
/// state.
#[derive(Clone, Copy, Debug)]
pub struct ReplanningInput<'a> {
    current: &'a CoordinationSnapshot,
    proposed: &'a CoordinationSnapshot,
}

impl<'a> ReplanningInput<'a> {
    /// Creates replanning input from the current and proposed snapshots.
    pub const fn new(
        current: &'a CoordinationSnapshot,
        proposed: &'a CoordinationSnapshot,
    ) -> Self {
        Self { current, proposed }
    }

    /// Returns the current global coordination snapshot.
    pub const fn current(&self) -> &'a CoordinationSnapshot {
        self.current
    }

    /// Returns the proposed global coordination snapshot.
    pub const fn proposed(&self) -> &'a CoordinationSnapshot {
        self.proposed
    }
}

/// The reason a new global plan version is required.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplanningReason {
    /// The globally relevant coordination dependency set changed.
    CoordinationRequirementsChanged,
}

impl fmt::Display for ReplanningReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CoordinationRequirementsChanged => {
                write!(formatter, "global coordination requirements changed")
            }
        }
    }
}

/// Result of evaluating whether the current global coordination plan remains
/// valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplanningDecision {
    /// The proposed coordination state is semantically equivalent to the
    /// current state, so no new plan version is required.
    NotRequired,

    /// The proposed coordination state differs materially from the current
    /// state and must be represented by a new plan version.
    Required(ReplanningReason),
}

impl ReplanningDecision {
    /// Returns whether a new plan version is required.
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required(_))
    }

    /// Returns the replanning reason when replanning is required.
    pub const fn reason(self) -> Option<ReplanningReason> {
        match self {
            Self::NotRequired => None,
            Self::Required(reason) => Some(reason),
        }
    }
}

/// Stateless evaluator for global coordination-plan changes.
///
/// `Replanner` owns no mutable state. It does not own a plan and does not
/// perform plan mutation. Its sole responsibility is determining whether two
/// coordination snapshots represent materially different global coordination
/// requirements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Replanner;

impl Replanner {
    /// Creates a stateless replanner.
    pub const fn new() -> Self {
        Self
    }

    /// Determines whether the proposed global coordination state requires a
    /// new plan version.
    ///
    /// Equality is semantic rather than arrival-order based because
    /// `CoordinationSnapshot` compares its de-duplicated dependency collection as a set.
    pub fn evaluate(&self, input: ReplanningInput<'_>) -> ReplanningDecision {
        if input.current() == input.proposed() {
            ReplanningDecision::NotRequired
        } else {
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        }
    }

    /// Convenience function for callers that do not need to instantiate a
    /// `Replanner`.
    pub fn evaluate_snapshots(
        current: &CoordinationSnapshot,
        proposed: &CoordinationSnapshot,
    ) -> ReplanningDecision {
        Self::new().evaluate(ReplanningInput::new(current, proposed))
    }
}

/// Describes the concrete coordination difference between two snapshots.
///
/// This is intentionally limited to global coordination dependencies. It does
/// not resolve or reinterpret the dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplanningDelta {
    added: Vec<Dependency>,
    removed: Vec<Dependency>,
}

impl ReplanningDelta {
    /// Computes the semantic difference between two coordination snapshots.
    pub fn between(current: &CoordinationSnapshot, proposed: &CoordinationSnapshot) -> Self {
        let added = current.added_dependencies(proposed).cloned().collect();
        let removed = current.removed_dependencies(proposed).cloned().collect();

        Self { added, removed }
    }

    /// Returns dependencies introduced by the proposed coordination state.
    pub fn added(&self) -> &[Dependency] {
        &self.added
    }

    /// Returns dependencies removed from the current coordination state.
    pub fn removed(&self) -> &[Dependency] {
        &self.removed
    }

    /// Returns whether the coordination state changed.
    pub fn is_changed(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }

    /// Returns the number of added dependencies.
    pub fn added_count(&self) -> usize {
        self.added.len()
    }

    /// Returns the number of removed dependencies.
    pub fn removed_count(&self) -> usize {
        self.removed.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::contracts::{ContractDescriptor, Interaction, PayloadDescriptor, Version};
    use crate::control_plane::dependency::{
        CapabilityRequirement, ConditionReference, DependencyKind, DependencyTarget,
    };
    use crate::identity::{CapabilityId, ContractId, NodeId};

    fn capability(value: &str) -> CapabilityRequirement {
        CapabilityRequirement::new(CapabilityId::new(value).unwrap())
    }

    fn contract_descriptor(capability: &str, contract: &str) -> ContractDescriptor {
        ContractDescriptor::new(
            ContractId::new(contract).unwrap(),
            CapabilityId::new(capability).unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        )
    }

    fn dependency(source: &str, capability_name: &str, kind: DependencyKind) -> Dependency {
        let source = NodeId::new(source).unwrap();
        let target = DependencyTarget::Capability(capability(capability_name));

        match kind {
            DependencyKind::Blocking => Dependency::blocking(source, target).unwrap(),
            DependencyKind::NonBlocking => Dependency::non_blocking(source, target).unwrap(),
            DependencyKind::Conditional => Dependency::conditional(
                source,
                target,
                ConditionReference::new("condition").unwrap(),
            )
            .unwrap(),
        }
    }

    fn contract_requirement(source: &str, capability_name: &str, contract_id: &str) -> Dependency {
        let requirement = CapabilityRequirement::new(CapabilityId::new(capability_name).unwrap())
            .with_contract(contract_descriptor(capability_name, contract_id))
            .unwrap();

        Dependency::blocking(
            NodeId::new(source).unwrap(),
            DependencyTarget::Capability(requirement),
        )
        .unwrap()
    }

    #[test]
    fn identical_snapshots_do_not_require_replanning() {
        let dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([dependency.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([dependency]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(decision, ReplanningDecision::NotRequired);
        assert!(!decision.is_required());
        assert_eq!(decision.reason(), None);
    }

    #[test]
    fn newly_added_global_requirement_requires_replanning() {
        let existing = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let added = dependency("hadith", "arabic.analyze", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([existing.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([existing, added]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(
            decision,
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
        assert!(decision.is_required());
    }

    #[test]
    fn removed_global_requirement_requires_replanning() {
        let retained = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let removed = dependency("hadith", "arabic.analyze", DependencyKind::NonBlocking);

        let current = CoordinationSnapshot::from_dependencies([retained.clone(), removed.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([retained]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(
            decision,
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn changing_dependency_kind_requires_replanning() {
        let current_dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let proposed_dependency = dependency("hadith", "quran.search", DependencyKind::NonBlocking);

        let current = CoordinationSnapshot::from_dependencies([current_dependency]);

        let proposed = CoordinationSnapshot::from_dependencies([proposed_dependency]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert!(decision.is_required());
        assert_eq!(
            decision.reason(),
            Some(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn changing_contract_requirement_requires_replanning() {
        let current_dependency = contract_requirement("hadith", "quran.search", "quran.search.v1");

        let proposed_dependency = contract_requirement("hadith", "quran.search", "quran.search.v2");

        let current = CoordinationSnapshot::from_dependencies([current_dependency]);

        let proposed = CoordinationSnapshot::from_dependencies([proposed_dependency]);

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &proposed),
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn requirement_arrival_order_does_not_change_replanning_result() {
        let first = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let second = dependency("hadith", "arabic.analyze", DependencyKind::NonBlocking);

        let third = dependency("hadith", "hadith.lookup", DependencyKind::Conditional);

        let current =
            CoordinationSnapshot::from_dependencies([first.clone(), second.clone(), third.clone()]);

        let reordered = CoordinationSnapshot::from_dependencies([third, first, second]);

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &reordered,),
            ReplanningDecision::NotRequired
        );
    }

    #[test]
    fn duplicate_identical_requirements_do_not_create_a_false_change() {
        let dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([dependency.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([dependency.clone(), dependency]);

        assert_eq!(current, proposed,);

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &proposed),
            ReplanningDecision::NotRequired
        );
    }

    #[test]
    fn empty_snapshots_do_not_require_replanning() {
        let current = CoordinationSnapshot::new();
        let proposed = CoordinationSnapshot::new();

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &proposed),
            ReplanningDecision::NotRequired
        );
    }

    #[test]
    fn adding_first_global_requirement_requires_replanning() {
        let current = CoordinationSnapshot::new();

        let proposed = CoordinationSnapshot::from_dependencies([dependency(
            "hadith",
            "quran.search",
            DependencyKind::Blocking,
        )]);

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &proposed),
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn removing_last_global_requirement_requires_replanning() {
        let current = CoordinationSnapshot::from_dependencies([dependency(
            "hadith",
            "quran.search",
            DependencyKind::Blocking,
        )]);

        let proposed = CoordinationSnapshot::new();

        assert_eq!(
            Replanner::evaluate_snapshots(&current, &proposed),
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn delta_reports_added_and_removed_dependencies() {
        let retained = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let removed = dependency("hadith", "arabic.analyze", DependencyKind::NonBlocking);

        let added = dependency("hadith", "aqeedah.governance", DependencyKind::Conditional);

        let current = CoordinationSnapshot::from_dependencies([retained.clone(), removed.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([retained.clone(), added.clone()]);

        let delta = ReplanningDelta::between(&current, &proposed);

        assert_eq!(delta.added_count(), 1);
        assert_eq!(delta.removed_count(), 1);
        assert_eq!(delta.added(), &[added]);
        assert_eq!(delta.removed(), &[removed]);
        assert!(delta.is_changed());
    }

    #[test]
    fn identical_snapshots_have_empty_delta() {
        let dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([dependency.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([dependency]);

        let delta = ReplanningDelta::between(&current, &proposed);

        assert!(!delta.is_changed());
        assert!(delta.added().is_empty());
        assert!(delta.removed().is_empty());
    }

    #[test]
    fn adding_and_removing_requirements_are_both_material_changes() {
        let removed = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let added = dependency("hadith", "arabic.analyze", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([removed.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([added.clone()]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert!(decision.is_required());

        let delta = ReplanningDelta::between(&current, &proposed);

        assert_eq!(delta.added(), &[added]);
        assert_eq!(delta.removed(), &[removed]);
    }

    #[test]
    fn snapshots_are_immutable_values() {
        let existing_dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([existing_dependency.clone()]);

        let proposed = current.with_dependency(dependency(
            "hadith",
            "arabic.analyze",
            DependencyKind::Blocking,
        ));

        assert_eq!(current.len(), 1);
        assert_eq!(proposed.len(), 2);
    }

    #[test]
    fn removing_from_snapshot_does_not_mutate_original() {
        let first = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let second = dependency("hadith", "arabic.analyze", DependencyKind::Blocking);

        let current = CoordinationSnapshot::from_dependencies([first.clone(), second.clone()]);

        let proposed = current.without_dependency(&second);

        assert_eq!(current.len(), 2);
        assert_eq!(proposed.len(), 1);
        assert!(current.contains(&second));
        assert!(!proposed.contains(&second));
    }

    #[test]
    fn replanning_does_not_select_provider_or_destination() {
        let dependency = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let current = CoordinationSnapshot::new();

        let proposed = CoordinationSnapshot::from_dependencies([dependency]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(
            decision,
            ReplanningDecision::Required(ReplanningReason::CoordinationRequirementsChanged)
        );
    }

    #[test]
    fn replanning_is_deterministic_for_same_inputs() {
        let first = dependency("hadith", "quran.search", DependencyKind::Blocking);

        let second = dependency("hadith", "arabic.analyze", DependencyKind::NonBlocking);

        let current = CoordinationSnapshot::from_dependencies([first.clone()]);

        let proposed = CoordinationSnapshot::from_dependencies([first, second]);

        let first_result = Replanner::evaluate_snapshots(&current, &proposed);

        let second_result = Replanner::evaluate_snapshots(&current, &proposed);

        assert_eq!(first_result, second_result);
    }

    #[test]
    fn replanning_decision_does_not_create_plan_identity() {
        let current = CoordinationSnapshot::new();

        let proposed = CoordinationSnapshot::from_dependencies([dependency(
            "hadith",
            "quran.search",
            DependencyKind::Blocking,
        )]);

        let decision = Replanner::evaluate_snapshots(&current, &proposed);

        // The decision only says that a new plan version is necessary.
        // PlanId/PlanVersion creation remains the responsibility of plan.rs.
        assert!(decision.is_required());
        assert_eq!(
            decision.reason(),
            Some(ReplanningReason::CoordinationRequirementsChanged)
        );
    }
}
