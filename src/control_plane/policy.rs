//! Deterministic routing-policy evaluation for the Control Plane.
//!
//! This module owns destination *selection* among destinations that have
//! already passed Control Plane eligibility checks.
//!
//! It does not own:
//! - destination discovery or membership;
//! - lifecycle or readiness;
//! - capability or contract compatibility;
//! - security or authorization;
//! - retry policy or attempt creation;
//! - transport;
//! - capability execution.
//!
//! The intended routing boundary is:
//!
//! ```text
//! requirement
//!     ↓
//! destination eligibility
//!     ↓
//! eligible candidates
//!     ↓
//! RoutingPolicy
//!     ↓
//! selected EngineInstanceId
//! ```
//!
//! A `RoutingPolicy` is immutable after construction. This is intentional:
//! policy changes should create a new policy snapshot rather than silently
//! changing decisions that are already being evaluated.

use crate::identity::EngineInstanceId;

/// Strategy used to select one destination from an already-eligible set.
///
/// These strategies operate only on routing metadata. They do not determine
/// whether a destination is eligible.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RoutingStrategy {
    /// Select the deterministically first candidate according to
    /// `EngineInstanceId` ordering.
    Deterministic,

    /// Select candidates in deterministic cyclic order.
    ///
    /// The caller supplies the current offset through [`PolicyInput`].
    /// `policy.rs` therefore remains stateless and does not own a mutable
    /// round-robin cursor.
    RoundRobin,

    /// Select according to candidate weights and a caller-supplied stable
    /// selection key.
    ///
    /// The selection is deterministic for equivalent inputs.
    Weighted,

    /// Prefer the candidate with the lowest reported load.
    ///
    /// Missing load is treated as less informative than an explicitly
    /// reported load and therefore ranks after known load values.
    CapacityAware,

    /// Prefer candidates matching the requested locality and then apply
    /// deterministic ordering.
    LocalityAware,
}

/// Explicit destination semantics supplied to the routing-policy layer.
///
/// A hard destination is not a preference and MUST NOT silently fall back to
/// another candidate. A preference may fall back to another eligible
/// destination when the preferred destination is unavailable.
///
/// Contract-level semantics should determine whether a caller's routing
/// constraint is permitted before it reaches this module.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum DestinationConstraint {
    /// No explicit destination constraint.
    #[default]
    None,

    /// Prefer this concrete instance when it is eligible.
    Preference(EngineInstanceId),

    /// Require this concrete instance.
    ///
    /// If the instance is absent from the already-eligible candidate set,
    /// policy evaluation fails instead of selecting another instance.
    Hard(EngineInstanceId),
}

/// Routing constraints that affect policy selection.
///
/// These are routing-level constraints only. They do not represent security
/// authorization, contract semantics, capability semantics, or domain policy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoutingConstraints {
    destination: DestinationConstraint,
    preferred_locality: Option<String>,
}

impl RoutingConstraints {
    /// Creates unconstrained routing input.
    pub const fn new() -> Self {
        Self {
            destination: DestinationConstraint::None,
            preferred_locality: None,
        }
    }

    /// Returns a copy of the constraints requiring a concrete destination.
    pub fn with_hard_destination(mut self, destination: EngineInstanceId) -> Self {
        self.destination = DestinationConstraint::Hard(destination);
        self
    }

    /// Returns a copy of the constraints preferring a concrete destination.
    pub fn with_preferred_destination(mut self, destination: EngineInstanceId) -> Self {
        self.destination = DestinationConstraint::Preference(destination);
        self
    }

    /// Returns a copy of the constraints preferring a locality.
    pub fn with_preferred_locality(mut self, locality: impl Into<String>) -> Self {
        self.preferred_locality = Some(locality.into());
        self
    }

    /// Returns the explicit destination constraint.
    pub const fn destination(&self) -> &DestinationConstraint {
        &self.destination
    }

    /// Returns the preferred locality, if one was supplied.
    pub fn preferred_locality(&self) -> Option<&str> {
        self.preferred_locality.as_deref()
    }
}

/// Metadata consumed by routing policy when selecting an eligible instance.
///
/// Eligibility itself is intentionally absent from this type. The candidate
/// is expected to have already passed destination eligibility checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingCandidate {
    instance_id: EngineInstanceId,
    weight: u32,
    load_percent: Option<u8>,
    locality: Option<String>,
}

impl RoutingCandidate {
    /// Creates a candidate with the default routing weight.
    pub fn new(instance_id: EngineInstanceId) -> Self {
        Self {
            instance_id,
            weight: 1,
            load_percent: None,
            locality: None,
        }
    }

    /// Sets the candidate's routing weight.
    ///
    /// A zero weight is permitted at construction time so that policy
    /// evaluation can report the appropriate selection failure when every
    /// candidate has zero weight.
    pub const fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Sets the candidate's observed load percentage.
    ///
    /// Load is an input to routing policy, not an independent eligibility
    /// authority.
    pub const fn with_load_percent(mut self, load_percent: u8) -> Self {
        self.load_percent = Some(load_percent);
        self
    }

    /// Sets the candidate's routing locality metadata.
    pub fn with_locality(mut self, locality: impl Into<String>) -> Self {
        self.locality = Some(locality.into());
        self
    }

    /// Returns the concrete engine instance identity.
    pub const fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }

    /// Returns the configured routing weight.
    pub const fn weight(&self) -> u32 {
        self.weight
    }

    /// Returns the observed load percentage.
    pub const fn load_percent(&self) -> Option<u8> {
        self.load_percent
    }

    /// Returns the routing locality.
    pub fn locality(&self) -> Option<&str> {
        self.locality.as_deref()
    }
}

/// Inputs used for one policy evaluation.
///
/// The caller supplies the candidate snapshot. This prevents `policy.rs` from
/// maintaining a second membership database and ensures selection operates on
/// one coherent routing-state view.
///
/// `round_robin_offset` and `selection_key` are supplied by the caller rather
/// than generated here. Policy therefore does not create attempts, maintain
/// mutable routing state, or introduce randomness.
#[derive(Debug)]
pub struct PolicyInput<'a> {
    candidates: &'a [RoutingCandidate],
    constraints: &'a RoutingConstraints,
    round_robin_offset: usize,
    selection_key: Option<u64>,
}

impl<'a> PolicyInput<'a> {
    /// Creates policy input from an eligible candidate snapshot and routing
    /// constraints.
    pub const fn new(
        candidates: &'a [RoutingCandidate],
        constraints: &'a RoutingConstraints,
    ) -> Self {
        Self {
            candidates,
            constraints,
            round_robin_offset: 0,
            selection_key: None,
        }
    }

    /// Supplies the current externally-owned round-robin offset.
    pub const fn with_round_robin_offset(mut self, offset: usize) -> Self {
        self.round_robin_offset = offset;
        self
    }

    /// Supplies a stable selection key for deterministic weighted selection.
    ///
    /// The key should come from the owning routing/attempt layer. This module
    /// does not manufacture an `AttemptId` or decide whether another attempt
    /// should exist.
    pub const fn with_selection_key(mut self, selection_key: u64) -> Self {
        self.selection_key = Some(selection_key);
        self
    }

    /// Returns the eligible candidates supplied to this evaluation.
    pub fn candidates(&self) -> &[RoutingCandidate] {
        self.candidates
    }

    /// Returns the routing constraints.
    pub const fn constraints(&self) -> &RoutingConstraints {
        self.constraints
    }

    /// Returns the supplied round-robin offset.
    pub const fn round_robin_offset(&self) -> usize {
        self.round_robin_offset
    }

    /// Returns the stable weighted-selection key, if supplied.
    pub const fn selection_key(&self) -> Option<u64> {
        self.selection_key
    }
}

/// The result of successful policy selection.
///
/// This represents a selection only. It does not create an execution attempt,
/// establish transport, or guarantee that Engine Runtime will subsequently
/// admit the request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicySelection {
    instance_id: EngineInstanceId,
}

impl PolicySelection {
    fn new(instance_id: EngineInstanceId) -> Self {
        Self { instance_id }
    }

    /// Returns the selected concrete engine instance.
    pub const fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }

    /// Consumes the selection and returns the selected instance identity.
    pub fn into_instance_id(self) -> EngineInstanceId {
        self.instance_id
    }
}

/// Errors produced by routing-policy evaluation.
///
/// These are policy-layer failures. They are deliberately distinct from
/// destination eligibility, transport failure, and Engine Runtime rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyError {
    /// Policy evaluation requires at least one already-eligible candidate.
    EmptyCandidateSet,

    /// A hard destination was not present in the already-eligible candidate
    /// set.
    HardDestinationUnavailable(EngineInstanceId),

    /// Weighted selection was requested without a stable selection key.
    MissingSelectionKey,

    /// Every candidate has zero weight.
    NoPositiveWeight,

    /// A candidate contained an invalid load value.
    InvalidLoad {
        instance_id: EngineInstanceId,
        load_percent: u8,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyCandidateSet => {
                formatter.write_str("routing policy received no eligible candidates")
            }
            Self::HardDestinationUnavailable(instance_id) => {
                write!(
                    formatter,
                    "hard routing destination is not among the eligible candidates: {instance_id}"
                )
            }
            Self::MissingSelectionKey => {
                formatter.write_str("weighted routing requires a stable selection key")
            }
            Self::NoPositiveWeight => {
                formatter.write_str("weighted routing has no candidate with a positive weight")
            }
            Self::InvalidLoad {
                instance_id,
                load_percent,
            } => {
                write!(
                    formatter,
                    "routing candidate {instance_id} has invalid load percentage {load_percent}"
                )
            }
        }
    }
}

impl std::error::Error for PolicyError {}

/// Immutable routing policy used for destination selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingPolicy {
    strategy: RoutingStrategy,
}

impl RoutingPolicy {
    /// Creates a routing policy using the supplied selection strategy.
    pub const fn new(strategy: RoutingStrategy) -> Self {
        Self { strategy }
    }

    /// Creates the baseline deterministic policy.
    pub const fn deterministic() -> Self {
        Self::new(RoutingStrategy::Deterministic)
    }

    /// Creates a round-robin policy.
    pub const fn round_robin() -> Self {
        Self::new(RoutingStrategy::RoundRobin)
    }

    /// Creates a weighted policy.
    pub const fn weighted() -> Self {
        Self::new(RoutingStrategy::Weighted)
    }

    /// Creates a capacity-aware policy.
    pub const fn capacity_aware() -> Self {
        Self::new(RoutingStrategy::CapacityAware)
    }

    /// Creates a locality-aware policy.
    pub const fn locality_aware() -> Self {
        Self::new(RoutingStrategy::LocalityAware)
    }

    /// Returns the configured routing strategy.
    pub const fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }

    /// Evaluates this policy against an already-eligible candidate snapshot.
    ///
    /// The evaluation is deterministic for equivalent:
    ///
    /// ```text
    /// candidates
    /// constraints
    /// policy
    /// policy inputs
    /// ```
    ///
    /// No membership, lifecycle, readiness, security, retry, or transport
    /// state is mutated by this operation.
    pub fn evaluate(&self, input: &PolicyInput<'_>) -> Result<PolicySelection, PolicyError> {
        if input.candidates.is_empty() {
            return Err(PolicyError::EmptyCandidateSet);
        }

        self.validate_candidates(input.candidates)?;

        match input.constraints.destination() {
            DestinationConstraint::Hard(destination) => {
                let candidate = input
                    .candidates
                    .iter()
                    .find(|candidate| candidate.instance_id == *destination)
                    .ok_or_else(|| PolicyError::HardDestinationUnavailable(destination.clone()))?;

                return Ok(PolicySelection::new(candidate.instance_id.clone()));
            }
            DestinationConstraint::Preference(destination) => {
                if let Some(candidate) = input
                    .candidates
                    .iter()
                    .find(|candidate| candidate.instance_id == *destination)
                {
                    return Ok(PolicySelection::new(candidate.instance_id.clone()));
                }
            }
            DestinationConstraint::None => {}
        }

        let candidate = match self.strategy {
            RoutingStrategy::Deterministic => select_deterministic(input.candidates),
            RoutingStrategy::RoundRobin => {
                select_round_robin(input.candidates, input.round_robin_offset)
            }
            RoutingStrategy::Weighted => select_weighted(input.candidates, input.selection_key)?,
            RoutingStrategy::CapacityAware => select_capacity_aware(input.candidates),
            RoutingStrategy::LocalityAware => {
                select_locality_aware(input.candidates, input.constraints.preferred_locality())
            }
        };

        Ok(PolicySelection::new(candidate.instance_id.clone()))
    }

    fn validate_candidates(&self, candidates: &[RoutingCandidate]) -> Result<(), PolicyError> {
        for candidate in candidates {
            if let Some(load_percent) = candidate.load_percent
                && load_percent > 100
            {
                return Err(PolicyError::InvalidLoad {
                    instance_id: candidate.instance_id.clone(),
                    load_percent,
                });
            }
        }

        Ok(())
    }
}

fn select_deterministic(candidates: &[RoutingCandidate]) -> &RoutingCandidate {
    candidates
        .iter()
        .min_by(|left, right| left.instance_id.cmp(&right.instance_id))
        .expect("candidate set was checked before policy selection")
}

fn select_round_robin(candidates: &[RoutingCandidate], offset: usize) -> &RoutingCandidate {
    let mut ordered: Vec<&RoutingCandidate> = candidates.iter().collect();

    ordered.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));

    let index = offset % ordered.len();
    ordered[index]
}

fn select_weighted(
    candidates: &[RoutingCandidate],
    selection_key: Option<u64>,
) -> Result<&RoutingCandidate, PolicyError> {
    let selection_key = selection_key.ok_or(PolicyError::MissingSelectionKey)?;

    let mut selected: Option<(&RoutingCandidate, u128)> = None;

    for candidate in candidates {
        if candidate.weight == 0 {
            continue;
        }

        let hash = stable_hash(selection_key, candidate.instance_id.as_str());

        // Multiplying the deterministic hash by the candidate weight makes a
        // larger weight produce a proportionally larger selection score.
        // The operation is entirely deterministic and does not use a random
        // number generator.
        let score = u128::from(hash) * u128::from(candidate.weight);

        match selected {
            None => {
                selected = Some((candidate, score));
            }
            Some((current, current_score)) => {
                if score > current_score
                    || (score == current_score && candidate.instance_id < current.instance_id)
                {
                    selected = Some((candidate, score));
                }
            }
        }
    }

    selected
        .map(|(candidate, _)| candidate)
        .ok_or(PolicyError::NoPositiveWeight)
}

fn select_capacity_aware(candidates: &[RoutingCandidate]) -> &RoutingCandidate {
    candidates
        .iter()
        .min_by(|left, right| {
            compare_optional_load(left.load_percent, right.load_percent)
                .then_with(|| left.instance_id.cmp(&right.instance_id))
        })
        .expect("candidate set was checked before policy selection")
}

fn select_locality_aware<'a>(
    candidates: &'a [RoutingCandidate],
    preferred_locality: Option<&str>,
) -> &'a RoutingCandidate {
    let Some(preferred_locality) = preferred_locality else {
        return select_deterministic(candidates);
    };

    candidates
        .iter()
        .min_by(|left, right| {
            locality_rank(left, preferred_locality)
                .cmp(&locality_rank(right, preferred_locality))
                .then_with(|| left.instance_id.cmp(&right.instance_id))
        })
        .expect("candidate set was checked before policy selection")
}

fn locality_rank(candidate: &RoutingCandidate, preferred_locality: &str) -> u8 {
    match candidate.locality() {
        Some(locality) if locality == preferred_locality => 0,
        Some(_) => 1,
        None => 2,
    }
}

fn compare_optional_load(left: Option<u8>, right: Option<u8>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Produces a stable 64-bit hash from a policy selection key and instance id.
///
/// This is intentionally a small deterministic hash rather than a random
/// source. Routing policy must produce predictable decisions for equivalent
/// inputs.
fn stable_hash(selection_key: u64, instance_id: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in selection_key
        .to_be_bytes()
        .into_iter()
        .chain(instance_id.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).unwrap()
    }

    fn candidate(value: &str) -> RoutingCandidate {
        RoutingCandidate::new(instance(value))
    }

    fn candidates() -> Vec<RoutingCandidate> {
        vec![
            candidate("engine-c"),
            candidate("engine-a"),
            candidate("engine-b"),
        ]
    }

    #[test]
    fn deterministic_policy_selects_stable_instance_order() {
        let candidates = candidates();
        let constraints = RoutingConstraints::new();
        let input = PolicyInput::new(&candidates, &constraints);

        let selection = RoutingPolicy::deterministic().evaluate(&input).unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn deterministic_policy_is_independent_of_candidate_insertion_order() {
        let first = vec![
            candidate("engine-c"),
            candidate("engine-a"),
            candidate("engine-b"),
        ];

        let second = vec![
            candidate("engine-b"),
            candidate("engine-c"),
            candidate("engine-a"),
        ];

        let constraints = RoutingConstraints::new();

        let first_selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&first, &constraints))
            .unwrap();

        let second_selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&second, &constraints))
            .unwrap();

        assert_eq!(first_selection, second_selection);
        assert_eq!(first_selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn empty_candidate_set_is_rejected() {
        let candidates = [];
        let constraints = RoutingConstraints::new();

        let result =
            RoutingPolicy::deterministic().evaluate(&PolicyInput::new(&candidates, &constraints));

        assert_eq!(result, Err(PolicyError::EmptyCandidateSet));
    }

    #[test]
    fn single_candidate_is_selected() {
        let candidates = [candidate("engine-a")];
        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn hard_destination_selects_required_candidate() {
        let candidates = candidates();

        let constraints = RoutingConstraints::new().with_hard_destination(instance("engine-b"));

        let selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn unavailable_hard_destination_does_not_fallback() {
        let candidates = candidates();

        let constraints = RoutingConstraints::new().with_hard_destination(instance("engine-x"));

        let result =
            RoutingPolicy::deterministic().evaluate(&PolicyInput::new(&candidates, &constraints));

        assert_eq!(
            result,
            Err(PolicyError::HardDestinationUnavailable(instance(
                "engine-x"
            )))
        );
    }

    #[test]
    fn preferred_destination_is_selected_when_eligible() {
        let candidates = candidates();

        let constraints =
            RoutingConstraints::new().with_preferred_destination(instance("engine-b"));

        let selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn unavailable_preference_falls_back_to_policy_selection() {
        let candidates = candidates();

        let constraints =
            RoutingConstraints::new().with_preferred_destination(instance("engine-x"));

        let selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn round_robin_uses_stable_candidate_order() {
        let candidates = candidates();
        let constraints = RoutingConstraints::new();

        let first = RoutingPolicy::round_robin()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        let second = RoutingPolicy::round_robin()
            .evaluate(&PolicyInput::new(&candidates, &constraints).with_round_robin_offset(1))
            .unwrap();

        let third = RoutingPolicy::round_robin()
            .evaluate(&PolicyInput::new(&candidates, &constraints).with_round_robin_offset(2))
            .unwrap();

        assert_eq!(first.instance_id().as_str(), "engine-a");
        assert_eq!(second.instance_id().as_str(), "engine-b");
        assert_eq!(third.instance_id().as_str(), "engine-c");
    }

    #[test]
    fn round_robin_wraps_around() {
        let candidates = candidates();
        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::round_robin()
            .evaluate(&PolicyInput::new(&candidates, &constraints).with_round_robin_offset(4))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn weighted_policy_requires_selection_key() {
        let candidates = candidates();
        let constraints = RoutingConstraints::new();

        let result =
            RoutingPolicy::weighted().evaluate(&PolicyInput::new(&candidates, &constraints));

        assert_eq!(result, Err(PolicyError::MissingSelectionKey));
    }

    #[test]
    fn weighted_policy_is_deterministic() {
        let candidates = vec![
            candidate("engine-a").with_weight(1),
            candidate("engine-b").with_weight(2),
            candidate("engine-c").with_weight(4),
        ];

        let constraints = RoutingConstraints::new();

        let input = PolicyInput::new(&candidates, &constraints).with_selection_key(42);

        let first = RoutingPolicy::weighted().evaluate(&input).unwrap();
        let second = RoutingPolicy::weighted().evaluate(&input).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn zero_weight_candidates_are_not_selected() {
        let candidates = vec![
            candidate("engine-a").with_weight(0),
            candidate("engine-b").with_weight(1),
        ];

        let constraints = RoutingConstraints::new();

        let input = PolicyInput::new(&candidates, &constraints).with_selection_key(42);

        let selection = RoutingPolicy::weighted().evaluate(&input).unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn all_zero_weights_are_rejected() {
        let candidates = vec![
            candidate("engine-a").with_weight(0),
            candidate("engine-b").with_weight(0),
        ];

        let constraints = RoutingConstraints::new();

        let input = PolicyInput::new(&candidates, &constraints).with_selection_key(42);

        let result = RoutingPolicy::weighted().evaluate(&input);

        assert_eq!(result, Err(PolicyError::NoPositiveWeight));
    }

    #[test]
    fn capacity_aware_policy_prefers_lower_load() {
        let candidates = vec![
            candidate("engine-a").with_load_percent(90),
            candidate("engine-b").with_load_percent(30),
            candidate("engine-c").with_load_percent(60),
        ];

        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::capacity_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn capacity_aware_policy_uses_stable_tie_breaking() {
        let candidates = vec![
            candidate("engine-c").with_load_percent(50),
            candidate("engine-a").with_load_percent(50),
            candidate("engine-b").with_load_percent(50),
        ];

        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::capacity_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn capacity_aware_policy_prefers_known_load_over_unknown_load() {
        let candidates = vec![
            candidate("engine-a"),
            candidate("engine-b").with_load_percent(70),
        ];

        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::capacity_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn locality_aware_policy_prefers_matching_locality() {
        let candidates = vec![
            candidate("engine-a").with_locality("eu-west"),
            candidate("engine-b").with_locality("asia-south"),
            candidate("engine-c").with_locality("us-east"),
        ];

        let constraints = RoutingConstraints::new().with_preferred_locality("asia-south");

        let selection = RoutingPolicy::locality_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-b");
    }

    #[test]
    fn locality_aware_policy_uses_deterministic_fallback() {
        let candidates = vec![
            candidate("engine-c").with_locality("eu-west"),
            candidate("engine-a").with_locality("us-east"),
            candidate("engine-b").with_locality("eu-west"),
        ];

        let constraints = RoutingConstraints::new().with_preferred_locality("asia-south");

        let selection = RoutingPolicy::locality_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn locality_aware_policy_without_locality_is_deterministic() {
        let candidates = candidates();
        let constraints = RoutingConstraints::new();

        let selection = RoutingPolicy::locality_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }

    #[test]
    fn policy_does_not_mutate_candidates() {
        let candidates = vec![
            candidate("engine-a").with_load_percent(20),
            candidate("engine-b").with_load_percent(80),
        ];

        let before = candidates.clone();
        let constraints = RoutingConstraints::new();

        let _ = RoutingPolicy::capacity_aware()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(candidates, before);
    }

    #[test]
    fn policy_evaluation_does_not_change_policy() {
        let policy = RoutingPolicy::capacity_aware();
        let candidates = vec![
            candidate("engine-a").with_load_percent(20),
            candidate("engine-b").with_load_percent(80),
        ];
        let constraints = RoutingConstraints::new();

        let _ = policy
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(policy.strategy(), RoutingStrategy::CapacityAware);
    }

    #[test]
    fn policy_does_not_create_or_manage_attempts() {
        // The policy API has no attempt-construction operation. A caller may
        // provide an externally-derived selection key, but policy evaluation
        // itself remains independent of retry/attempt ownership.
        let candidates = candidates();
        let constraints = RoutingConstraints::new();

        let input = PolicyInput::new(&candidates, &constraints).with_selection_key(7);

        let selection = RoutingPolicy::weighted().evaluate(&input).unwrap();

        assert!(!selection.instance_id().as_str().is_empty());
    }

    #[test]
    fn invalid_load_is_rejected() {
        let candidates = vec![
            candidate("engine-a").with_load_percent(101),
            candidate("engine-b").with_load_percent(20),
        ];

        let constraints = RoutingConstraints::new();

        let result =
            RoutingPolicy::deterministic().evaluate(&PolicyInput::new(&candidates, &constraints));

        assert_eq!(
            result,
            Err(PolicyError::InvalidLoad {
                instance_id: instance("engine-a"),
                load_percent: 101,
            })
        );
    }

    #[test]
    fn explicit_preference_is_not_an_authorization_bypass() {
        // The policy only searches the already-eligible candidate set. A
        // preferred instance that is absent cannot be manufactured or selected
        // merely because the caller requested it.
        let candidates = vec![candidate("engine-a")];

        let constraints =
            RoutingConstraints::new().with_preferred_destination(instance("engine-x"));

        let selection = RoutingPolicy::deterministic()
            .evaluate(&PolicyInput::new(&candidates, &constraints))
            .unwrap();

        assert_eq!(selection.instance_id().as_str(), "engine-a");
    }
}
