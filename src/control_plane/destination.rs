//! Destination semantics for the Control Plane.
//!
//! This module describes where an operation is allowed or preferred to go.
//!
//! It intentionally does not perform:
//! - membership lookup,
//! - health/readiness evaluation,
//! - capability resolution,
//! - contract compatibility checks,
//! - routing policy selection,
//! - transport,
//! - retry handling,
//! - execution.
//!
//! The ownership boundary is:
//!
//! ```text
//! DestinationRequest
//!        ↓
//! destination requirements
//!        ↓
//! membership + observations
//!        ↓
//! destination eligibility
//!        ↓
//! routing policy
//!        ↓
//! RoutingDecision
//! ```
//!
//! A logical destination identifies a capability requirement.
//! An explicit destination identifies one concrete `EngineInstanceId`.
//!
//! `EngineId` and `EngineInstanceId` intentionally remain distinct.

use core::fmt;

use crate::contracts::ContractDescriptor;
use crate::contracts::compatibility::compare_contracts;
use crate::health::HealthStatus;
use crate::identity::{CapabilityId, EngineId, EngineInstanceId};

use super::capability::is_advertised;
use super::dependency::CapabilityRequirement;
use super::lifecycle::is_routable;
use super::membership::MembershipSnapshot;
use super::observations::ObservationSnapshot;

/// Identifies the kind of destination requested by an operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    /// A logical destination resolved through a capability requirement.
    Logical(LogicalDestination),

    /// A concrete runtime destination.
    Explicit(ExplicitDestination),
}

impl Destination {
    /// Creates a logical destination from a capability requirement.
    ///
    /// This does not resolve the capability to a provider or engine instance.
    #[must_use]
    pub fn logical(requirement: CapabilityRequirement) -> Self {
        Self::Logical(LogicalDestination::new(requirement))
    }

    /// Creates an explicit destination for one concrete engine instance.
    ///
    /// This does not check whether the instance currently exists or is
    /// currently eligible. Those checks belong to membership/routing.
    #[must_use]
    pub fn explicit(instance_id: EngineInstanceId) -> Self {
        Self::Explicit(ExplicitDestination::new(instance_id))
    }

    /// Returns `true` when this is a logical destination.
    #[must_use]
    pub fn is_logical(&self) -> bool {
        matches!(self, Self::Logical(_))
    }

    /// Returns `true` when this is an explicit destination.
    #[must_use]
    pub fn is_explicit(&self) -> bool {
        matches!(self, Self::Explicit(_))
    }

    /// Returns the logical destination when applicable.
    #[must_use]
    pub fn logical_destination(&self) -> Option<&LogicalDestination> {
        match self {
            Self::Logical(destination) => Some(destination),
            Self::Explicit(_) => None,
        }
    }

    /// Returns the explicit destination when applicable.
    #[must_use]
    pub fn explicit_destination(&self) -> Option<&ExplicitDestination> {
        match self {
            Self::Logical(_) => None,
            Self::Explicit(destination) => Some(destination),
        }
    }

    /// Returns the concrete instance identifier when this is an explicit
    /// destination.
    #[must_use]
    pub fn instance_id(&self) -> Option<&EngineInstanceId> {
        self.explicit_destination()
            .map(ExplicitDestination::instance_id)
    }
}

/// A logical destination represented by a capability requirement.
///
/// The Control Plane may later resolve this requirement to a provider and
/// then to an eligible engine instance. This type deliberately contains no
/// provider or instance information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalDestination {
    requirement: CapabilityRequirement,
}

impl LogicalDestination {
    /// Creates a logical destination.
    #[must_use]
    pub fn new(requirement: CapabilityRequirement) -> Self {
        Self { requirement }
    }

    /// Returns the capability requirement.
    #[must_use]
    pub fn requirement(&self) -> &CapabilityRequirement {
        &self.requirement
    }
}

/// A concrete destination identified by one engine instance.
///
/// The logical `EngineId` is intentionally not stored here. It can be
/// obtained from the membership registration associated with the instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplicitDestination {
    instance_id: EngineInstanceId,
}

impl ExplicitDestination {
    /// Creates an explicit destination.
    #[must_use]
    pub fn new(instance_id: EngineInstanceId) -> Self {
        Self { instance_id }
    }

    /// Returns the concrete engine instance identifier.
    #[must_use]
    pub fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }
}

/// Describes how strongly a destination is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationStrength {
    /// The requested destination is mandatory.
    ///
    /// A hard destination cannot silently fall back to another destination.
    Hard,

    /// The requested destination is preferred but does not necessarily have
    /// to be selected.
    Preferred,
}

impl DestinationStrength {
    /// Returns `true` when the destination is mandatory.
    #[must_use]
    pub const fn is_hard(self) -> bool {
        matches!(self, Self::Hard)
    }

    /// Returns `true` when the destination is merely preferred.
    #[must_use]
    pub const fn is_preferred(self) -> bool {
        matches!(self, Self::Preferred)
    }
}

/// Controls whether routing may use another eligible destination when the
/// requested destination cannot be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FallbackPolicy {
    /// Another destination must not be selected.
    Forbidden,

    /// Another eligible destination may be selected.
    Allowed,
}

impl FallbackPolicy {
    /// Returns `true` when fallback is allowed.
    #[must_use]
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Returns `true` when fallback is forbidden.
    #[must_use]
    pub const fn is_forbidden(self) -> bool {
        matches!(self, Self::Forbidden)
    }
}

/// A complete destination requirement for one routing operation.
///
/// This combines:
///
/// - what destination is requested,
/// - how strongly it is requested,
/// - whether fallback is permitted.
///
/// It contains no routing state and no runtime observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationRequest {
    destination: Destination,
    strength: DestinationStrength,
    fallback: FallbackPolicy,
}

impl DestinationRequest {
    /// Creates a destination request.
    ///
    /// # Errors
    ///
    /// Returns an error when a hard destination is configured with allowed
    /// fallback. A hard destination must never silently become a fallback
    /// request.
    pub fn new(
        destination: Destination,
        strength: DestinationStrength,
        fallback: FallbackPolicy,
    ) -> Result<Self, DestinationValidationError> {
        let request = Self {
            destination,
            strength,
            fallback,
        };

        request.validate()?;

        Ok(request)
    }

    /// Creates a hard destination request.
    ///
    /// Hard destinations always forbid fallback.
    #[must_use]
    pub fn hard(destination: Destination) -> Self {
        Self {
            destination,
            strength: DestinationStrength::Hard,
            fallback: FallbackPolicy::Forbidden,
        }
    }

    /// Creates a preferred destination request.
    ///
    /// The caller explicitly controls whether fallback is permitted.
    #[must_use]
    pub fn preferred(destination: Destination, fallback: FallbackPolicy) -> Self {
        Self {
            destination,
            strength: DestinationStrength::Preferred,
            fallback,
        }
    }

    /// Creates a hard logical destination.
    #[must_use]
    pub fn hard_logical(requirement: CapabilityRequirement) -> Self {
        Self::hard(Destination::logical(requirement))
    }

    /// Creates a hard explicit destination.
    #[must_use]
    pub fn hard_explicit(instance_id: EngineInstanceId) -> Self {
        Self::hard(Destination::explicit(instance_id))
    }

    /// Creates a preferred logical destination.
    #[must_use]
    pub fn preferred_logical(requirement: CapabilityRequirement, fallback: FallbackPolicy) -> Self {
        Self::preferred(Destination::logical(requirement), fallback)
    }

    /// Creates a preferred explicit destination.
    #[must_use]
    pub fn preferred_explicit(instance_id: EngineInstanceId, fallback: FallbackPolicy) -> Self {
        Self::preferred(Destination::explicit(instance_id), fallback)
    }

    /// Validates the structural invariants of this destination request.
    ///
    /// This method does not perform membership, health, capability,
    /// contract, security, or routing-policy checks.
    pub fn validate(&self) -> Result<(), DestinationValidationError> {
        if self.strength.is_hard() && self.fallback.is_allowed() {
            return Err(DestinationValidationError::HardDestinationAllowsFallback);
        }

        Ok(())
    }

    /// Returns the requested destination.
    #[must_use]
    pub fn destination(&self) -> &Destination {
        &self.destination
    }

    /// Returns the destination strength.
    #[must_use]
    pub const fn strength(&self) -> DestinationStrength {
        self.strength
    }

    /// Returns the fallback policy.
    #[must_use]
    pub const fn fallback(&self) -> FallbackPolicy {
        self.fallback
    }

    /// Returns `true` when this request requires an exact destination.
    #[must_use]
    pub const fn is_hard(&self) -> bool {
        self.strength.is_hard()
    }

    /// Returns `true` when another destination may be selected.
    #[must_use]
    pub const fn allows_fallback(&self) -> bool {
        self.fallback.is_allowed()
    }

    /// Returns the concrete instance identifier when the destination is
    /// explicit.
    #[must_use]
    pub fn explicit_instance_id(&self) -> Option<&EngineInstanceId> {
        self.destination.instance_id()
    }
}

/// A concrete candidate considered by the routing layer.
///
/// This is deliberately smaller than a membership record. It carries
/// identity only; capability, contract, health, readiness, endpoint, and
/// routing metadata remain owned by their respective systems.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestinationCandidate {
    engine_id: EngineId,
    instance_id: EngineInstanceId,
}

impl DestinationCandidate {
    /// Creates a candidate from logical and concrete engine identities.
    #[must_use]
    pub fn new(engine_id: EngineId, instance_id: EngineInstanceId) -> Self {
        Self {
            engine_id,
            instance_id,
        }
    }

    /// Returns the logical engine identifier.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine instance identifier.
    #[must_use]
    pub fn instance_id(&self) -> &EngineInstanceId {
        &self.instance_id
    }
}

/// Inputs required to evaluate whether registered engine instances are eligible
/// for one destination request.
///
/// Eligibility is deliberately evaluated from coherent point-in-time snapshots.
/// The Control Plane therefore does not mix membership, registration, lifecycle,
/// readiness, and health observations from different revisions while producing
/// one candidate set.
#[derive(Debug)]
pub struct DestinationEligibilityInput<'a> {
    request: &'a DestinationRequest,
    membership: &'a MembershipSnapshot,
    observations: &'a ObservationSnapshot,
    capability: &'a CapabilityId,
    contract: &'a ContractDescriptor,
}

impl<'a> DestinationEligibilityInput<'a> {
    /// Creates eligibility input from one destination request and coherent
    /// routing-state snapshots.
    pub const fn new(
        request: &'a DestinationRequest,
        membership: &'a MembershipSnapshot,
        observations: &'a ObservationSnapshot,
        capability: &'a CapabilityId,
        contract: &'a ContractDescriptor,
    ) -> Self {
        Self {
            request,
            membership,
            observations,
            capability,
            contract,
        }
    }

    /// Returns the destination request being evaluated.
    pub const fn request(&self) -> &'a DestinationRequest {
        self.request
    }

    /// Returns the membership snapshot used for this evaluation.
    pub const fn membership(&self) -> &'a MembershipSnapshot {
        self.membership
    }

    /// Returns the observation snapshot used for this evaluation.
    pub const fn observations(&self) -> &'a ObservationSnapshot {
        self.observations
    }

    /// Returns the requested capability.
    pub const fn capability(&self) -> &'a CapabilityId {
        self.capability
    }

    /// Returns the required contract.
    pub const fn contract(&self) -> &'a ContractDescriptor {
        self.contract
    }
}

/// A concrete candidate that passed Control Plane destination eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleDestination {
    candidate: DestinationCandidate,
}

impl EligibleDestination {
    fn new(candidate: DestinationCandidate) -> Self {
        Self { candidate }
    }

    /// Returns the eligible logical engine identity.
    pub fn engine_id(&self) -> &EngineId {
        self.candidate.engine_id()
    }

    /// Returns the eligible concrete engine instance identity.
    pub fn instance_id(&self) -> &EngineInstanceId {
        self.candidate.instance_id()
    }

    /// Returns the underlying destination candidate.
    pub fn candidate(&self) -> &DestinationCandidate {
        &self.candidate
    }
}

/// Errors produced by the destination eligibility stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationEligibilityError {
    /// The destination request itself is structurally invalid.
    InvalidRequest(DestinationValidationError),

    /// A hard explicit destination is not present in membership.
    HardDestinationNotMember(EngineInstanceId),

    /// No registered member satisfies all eligibility requirements.
    NoEligibleDestination,
}

impl std::fmt::Display for DestinationEligibilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(error) => {
                write!(formatter, "invalid destination request: {error}")
            }
            Self::HardDestinationNotMember(instance_id) => {
                write!(
                    formatter,
                    "hard destination {instance_id} is not present in membership"
                )
            }
            Self::NoEligibleDestination => {
                formatter.write_str("no engine instance satisfied destination eligibility")
            }
        }
    }
}

impl std::error::Error for DestinationEligibilityError {}

impl From<DestinationValidationError> for DestinationEligibilityError {
    fn from(error: DestinationValidationError) -> Self {
        Self::InvalidRequest(error)
    }
}

/// Evaluates the concrete engine instances that are eligible for a destination.
///
/// The eligibility stage consumes, rather than replaces, the authoritative
/// subsystems:
///
/// - `MembershipSnapshot` supplies the current routing membership;
/// - registration supplies capability, contract, endpoint, lifecycle, and
///   readiness metadata;
/// - `ObservationSnapshot` supplies the current health observation.
///
/// Only `Serving` lifecycle and a ready readiness report admit new normal
/// routing. A missing health observation is not treated as healthy.
///
/// Routing policy is intentionally not evaluated here. The returned candidates
/// are the only candidates that the later policy layer may select from.
pub fn eligible_destinations(
    input: DestinationEligibilityInput<'_>,
) -> Result<Vec<EligibleDestination>, DestinationEligibilityError> {
    input.request.validate()?;

    if let Some(explicit) = input.request.explicit_instance_id()
        && input.request.is_hard()
        && !input.membership.contains(explicit)
    {
        return Err(DestinationEligibilityError::HardDestinationNotMember(
            explicit.clone(),
        ));
    }

    let explicit_target = input.request.explicit_instance_id();

    let mut eligible = Vec::new();

    for member in input.membership.candidates() {
        if let Some(target) = explicit_target
            && input.request.is_hard()
            && member.engine_instance_id() != target
        {
            continue;
        }

        let registration = member.registration();

        if registration.endpoint().is_none() {
            continue;
        }

        let Some(lifecycle) = registration.runtime().lifecycle() else {
            continue;
        };

        if !is_routable(lifecycle) {
            continue;
        }

        let Some(readiness) = registration.runtime().readiness() else {
            continue;
        };

        if !readiness.is_ready() {
            continue;
        }

        let Some(observation) = input.observations.get(member.engine_instance_id()) else {
            continue;
        };

        if observation.engine_id() != member.engine_id()
            || observation.health().overall() != HealthStatus::Healthy
        {
            continue;
        }

        if !registration
            .capabilities()
            .iter()
            .any(|capability| is_advertised(input.capability, capability))
        {
            continue;
        }

        if !registration.contracts().iter().any(|offered| {
            compare_contracts(input.contract, offered) == crate::status::Compatibility::Compatible
        }) {
            continue;
        }

        eligible.push(EligibleDestination::new(DestinationCandidate::new(
            member.engine_id().clone(),
            member.engine_instance_id().clone(),
        )));
    }

    if eligible.is_empty() {
        return Err(DestinationEligibilityError::NoEligibleDestination);
    }

    Ok(eligible)
}

/// Structural errors produced by destination construction or validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationValidationError {
    /// A hard destination was configured with fallback enabled.
    ///
    /// Allowing this would turn a mandatory destination into a preference
    /// and could cause work to be routed somewhere other than the explicitly
    /// required destination.
    HardDestinationAllowsFallback,
}

impl fmt::Display for DestinationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HardDestinationAllowsFallback => {
                formatter.write_str("a hard destination cannot allow fallback")
            }
        }
    }
}

impl std::error::Error for DestinationValidationError {}

/// Result alias for destination-local operations.
pub type DestinationResult<T> = Result<T, DestinationValidationError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::contracts::descriptor::{Interaction, PayloadDescriptor};
    use crate::identity::{CapabilityId, ContractId};

    fn capability_id(value: &str) -> CapabilityId {
        CapabilityId::new(value).expect("valid capability id")
    }

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).expect("valid engine id")
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).expect("valid instance id")
    }

    fn capability_requirement(value: &str) -> CapabilityRequirement {
        CapabilityRequirement::new(capability_id(value))
    }

    fn contract(capability: &str) -> crate::contracts::descriptor::ContractDescriptor {
        crate::contracts::descriptor::ContractDescriptor::new(
            ContractId::new("test.contract").expect("valid contract id"),
            capability_id(capability),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/json", Version::new(1, 0, 0))
                .expect("valid payload descriptor"),
        )
    }

    #[test]
    fn logical_destination_preserves_capability_requirement() {
        let requirement = capability_requirement("quran.search");
        let destination = Destination::logical(requirement.clone());

        assert!(destination.is_logical());
        assert!(!destination.is_explicit());
        assert_eq!(
            destination
                .logical_destination()
                .expect("logical destination")
                .requirement(),
            &requirement
        );
    }

    #[test]
    fn explicit_destination_preserves_instance_id() {
        let instance = instance_id("quran-02");
        let destination = Destination::explicit(instance.clone());

        assert!(destination.is_explicit());
        assert!(!destination.is_logical());
        assert_eq!(
            destination
                .explicit_destination()
                .expect("explicit destination")
                .instance_id(),
            &instance
        );
    }

    #[test]
    fn explicit_destination_does_not_store_engine_id() {
        let destination = Destination::explicit(instance_id("quran-02"));

        assert_eq!(
            destination.instance_id().expect("instance id").as_str(),
            "quran-02"
        );
    }

    #[test]
    fn hard_destination_forbids_fallback() {
        let destination = Destination::explicit(instance_id("quran-02"));
        let request = DestinationRequest::hard(destination);

        assert!(request.is_hard());
        assert!(!request.allows_fallback());
        assert_eq!(request.fallback(), FallbackPolicy::Forbidden);
        assert!(request.validate().is_ok());
    }

    #[test]
    fn preferred_destination_can_allow_fallback() {
        let destination = Destination::explicit(instance_id("quran-02"));
        let request = DestinationRequest::preferred(destination, FallbackPolicy::Allowed);

        assert!(request.strength().is_preferred());
        assert!(request.allows_fallback());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn preferred_destination_can_forbid_fallback() {
        let destination = Destination::explicit(instance_id("quran-02"));
        let request = DestinationRequest::preferred(destination, FallbackPolicy::Forbidden);

        assert!(request.strength().is_preferred());
        assert!(!request.allows_fallback());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn hard_destination_with_allowed_fallback_is_rejected() {
        let destination = Destination::explicit(instance_id("quran-02"));

        let result = DestinationRequest::new(
            destination,
            DestinationStrength::Hard,
            FallbackPolicy::Allowed,
        );

        assert_eq!(
            result,
            Err(DestinationValidationError::HardDestinationAllowsFallback)
        );
    }

    #[test]
    fn hard_logical_destination_preserves_requirement() {
        let requirement = capability_requirement("quran.search");
        let request = DestinationRequest::hard_logical(requirement.clone());

        assert!(request.is_hard());
        assert!(!request.allows_fallback());

        let logical = request
            .destination()
            .logical_destination()
            .expect("logical destination");

        assert_eq!(logical.requirement(), &requirement);
    }

    #[test]
    fn hard_explicit_destination_targets_exact_instance() {
        let request = DestinationRequest::hard_explicit(instance_id("quran-02"));

        assert_eq!(
            request
                .explicit_instance_id()
                .expect("explicit instance")
                .as_str(),
            "quran-02"
        );
    }

    #[test]
    fn preferred_logical_destination_can_allow_fallback() {
        let requirement = capability_requirement("quran.search");

        let request =
            DestinationRequest::preferred_logical(requirement.clone(), FallbackPolicy::Allowed);

        assert!(request.strength().is_preferred());
        assert!(request.allows_fallback());

        assert_eq!(
            request
                .destination()
                .logical_destination()
                .expect("logical destination")
                .requirement(),
            &requirement
        );
    }

    #[test]
    fn preferred_explicit_destination_can_allow_fallback() {
        let request = DestinationRequest::preferred_explicit(
            instance_id("quran-02"),
            FallbackPolicy::Allowed,
        );

        assert_eq!(
            request
                .explicit_instance_id()
                .expect("explicit instance")
                .as_str(),
            "quran-02"
        );
        assert!(request.allows_fallback());
    }

    #[test]
    fn destination_candidate_preserves_both_engine_identities() {
        let candidate = DestinationCandidate::new(engine_id("quran"), instance_id("quran-02"));

        assert_eq!(candidate.engine_id().as_str(), "quran");
        assert_eq!(candidate.instance_id().as_str(), "quran-02");
    }

    #[test]
    fn destination_request_is_independent_of_membership_state() {
        let request = DestinationRequest::hard_explicit(instance_id("quran-02"));

        // Construction succeeds without consulting membership.
        // Whether quran-02 currently exists or is eligible is intentionally
        // outside this module.
        assert_eq!(
            request
                .explicit_instance_id()
                .expect("explicit instance")
                .as_str(),
            "quran-02"
        );
    }

    #[test]
    fn logical_destination_does_not_resolve_provider_or_instance() {
        let request = DestinationRequest::hard_logical(capability_requirement("quran.search"));

        assert!(request.explicit_instance_id().is_none());
        assert_eq!(
            request
                .destination()
                .logical_destination()
                .expect("logical destination")
                .requirement()
                .capability_id()
                .as_str(),
            "quran.search"
        );
    }

    #[test]
    fn destination_strength_semantics_are_distinct() {
        assert!(DestinationStrength::Hard.is_hard());
        assert!(!DestinationStrength::Hard.is_preferred());

        assert!(!DestinationStrength::Preferred.is_hard());
        assert!(DestinationStrength::Preferred.is_preferred());
    }

    #[test]
    fn fallback_policy_semantics_are_distinct() {
        assert!(FallbackPolicy::Allowed.is_allowed());
        assert!(!FallbackPolicy::Allowed.is_forbidden());

        assert!(!FallbackPolicy::Forbidden.is_allowed());
        assert!(FallbackPolicy::Forbidden.is_forbidden());
    }

    #[test]
    fn capability_requirement_contract_can_be_used_as_logical_destination() {
        let requirement = CapabilityRequirement::new(capability_id("quran.search"))
            .with_contract(contract("quran.search"))
            .expect("compatible contract");

        let destination = Destination::logical(requirement.clone());

        assert_eq!(
            destination
                .logical_destination()
                .expect("logical destination")
                .requirement(),
            &requirement
        );
    }
}
