//! Immutable Control Plane resolution.
//!
//! This module composes the results of the individual Control Plane
//! resolution stages into one coherent, immutable interaction snapshot.
//!
//! Resolution is deliberately not responsible for:
//! - admission;
//! - contract compatibility evaluation;
//! - capability discovery or execution;
//! - provider discovery;
//! - destination eligibility;
//! - routing-policy evaluation;
//! - membership management;
//! - lifecycle management;
//! - retry decisions;
//! - attempt creation;
//! - transport;
//! - engine-local execution.
//!
//! The intended flow is:
//!
//! ```text
//! request
//!     ↓
//! admission
//!     ↓
//! contract / capability / provider resolution
//!     ↓
//! destination eligibility
//!     ↓
//! routing policy
//!     ↓
//! ResolvedInteraction
//!     ↓
//! routing
//! ```
//!
//! A resolution is a snapshot. Once constructed, later membership, policy,
//! registration, or lifecycle changes do not mutate the existing resolution.
//!
//! This is important for attempt stability: a destination selected for an
//! already-authorized attempt must not be silently rewritten by later
//! Control Plane state changes.

use crate::identity::{CapabilityId, ContractId, EngineId, EngineInstanceId, OperationId};
use crate::operation::OperationContext;

use super::policy::RoutingStrategy;

/// Immutable information about the resolved contract.
///
/// Contract compatibility and version selection belong to the contract
/// subsystem. This type only records the contract that has already been
/// resolved for the interaction.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedContract {
    contract_id: ContractId,
    version: String,
}

impl ResolvedContract {
    /// Creates a resolved contract reference.
    ///
    /// The contract subsystem remains responsible for determining whether the
    /// selected version is compatible with the request.
    pub fn new(contract_id: ContractId, version: impl Into<String>) -> Self {
        Self {
            contract_id,
            version: version.into(),
        }
    }

    /// Returns the resolved contract identity.
    pub fn contract_id(&self) -> &ContractId {
        &self.contract_id
    }

    /// Returns the resolved contract version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Immutable information about the resolved capability.
///
/// Capability identification and implementation remain outside this module.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedCapability {
    capability_id: CapabilityId,
}

impl ResolvedCapability {
    /// Creates a resolved capability reference.
    pub fn new(capability_id: CapabilityId) -> Self {
        Self { capability_id }
    }

    /// Returns the resolved capability identity.
    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }
}

/// Immutable information about the resolved provider.
///
/// Provider resolution is performed by the provider subsystem. A provider is
/// represented here by its logical engine identity because the existing Core
/// identity model does not define a separate `ProviderId`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedProvider {
    engine_id: EngineId,
}

impl ResolvedProvider {
    /// Creates a resolved provider reference.
    pub fn new(engine_id: EngineId) -> Self {
        Self { engine_id }
    }

    /// Returns the logical engine acting as the resolved provider.
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }
}

/// Immutable routing decision captured by the resolution.
///
/// The destination has already been selected by the routing-policy layer.
/// `resolution.rs` does not evaluate the policy again.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedRouting {
    destination: EngineInstanceId,
    strategy: RoutingStrategy,
}

impl ResolvedRouting {
    /// Creates a resolved routing decision.
    ///
    /// The caller must provide a destination that has already passed
    /// destination eligibility and policy selection.
    pub fn new(destination: EngineInstanceId, strategy: RoutingStrategy) -> Self {
        Self {
            destination,
            strategy,
        }
    }

    /// Returns the selected concrete engine instance.
    pub fn destination(&self) -> &EngineInstanceId {
        &self.destination
    }

    /// Returns the strategy that produced the destination decision.
    pub const fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }
}

/// Immutable input used to construct a `Resolution`.
///
/// Every field represents a result from another Control Plane concern.
/// `Resolution` composes those results; it does not discover or recompute
/// them.
///
/// The operation context is optional because some resolution operations may
/// be constructed before an attempt-specific context exists. In particular,
/// `Resolution` does not create an `AttemptId`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionInput {
    operation_id: OperationId,
    contract: ResolvedContract,
    capability: ResolvedCapability,
    provider: Option<ResolvedProvider>,
    routing: ResolvedRouting,
    context: Option<OperationContext>,
}

impl ResolutionInput {
    /// Creates the minimum complete resolution input.
    pub fn new(
        operation_id: OperationId,
        contract: ResolvedContract,
        capability: ResolvedCapability,
        routing: ResolvedRouting,
    ) -> Self {
        Self {
            operation_id,
            contract,
            capability,
            provider: None,
            routing,
            context: None,
        }
    }

    /// Adds an already-resolved provider.
    pub fn with_provider(mut self, provider: ResolvedProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Adds an already-established operation context.
    pub fn with_context(mut self, context: OperationContext) -> Self {
        self.context = Some(context);
        self
    }

    /// Returns the operation identity.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the resolved contract.
    pub fn contract(&self) -> &ResolvedContract {
        &self.contract
    }

    /// Returns the resolved capability.
    pub fn capability(&self) -> &ResolvedCapability {
        &self.capability
    }

    /// Returns the resolved provider, when one was required.
    pub fn provider(&self) -> Option<&ResolvedProvider> {
        self.provider.as_ref()
    }

    /// Returns the resolved routing decision.
    pub fn routing(&self) -> &ResolvedRouting {
        &self.routing
    }

    /// Returns the operation context, when one has already been established.
    pub fn context(&self) -> Option<&OperationContext> {
        self.context.as_ref()
    }
}

/// Immutable result of Control Plane resolution.
///
/// A `Resolution` represents what the Control Plane has resolved for one
/// interaction. It is not an execution attempt and does not own execution
/// state.
///
/// In particular, this type deliberately does not contain:
/// - `AttemptId`;
/// - retry counters;
/// - transport state;
/// - mutable membership;
/// - lifecycle state;
/// - capability handlers;
/// - domain payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resolution {
    operation_id: OperationId,
    contract: ResolvedContract,
    capability: ResolvedCapability,
    provider: Option<ResolvedProvider>,
    routing: ResolvedRouting,
    context: Option<OperationContext>,
}

impl Resolution {
    /// Builds an immutable resolution from an already-resolved input.
    ///
    /// No discovery, routing-policy evaluation, membership lookup, lifecycle
    /// evaluation, or attempt creation occurs here.
    pub fn resolve(input: ResolutionInput) -> Self {
        Self {
            operation_id: input.operation_id,
            contract: input.contract,
            capability: input.capability,
            provider: input.provider,
            routing: input.routing,
            context: input.context,
        }
    }

    /// Returns the operation associated with this resolution.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the resolved contract.
    pub fn contract(&self) -> &ResolvedContract {
        &self.contract
    }

    /// Returns the resolved capability.
    pub fn capability(&self) -> &ResolvedCapability {
        &self.capability
    }

    /// Returns the resolved provider, when one exists.
    pub fn provider(&self) -> Option<&ResolvedProvider> {
        self.provider.as_ref()
    }

    /// Returns the immutable routing decision.
    pub fn routing(&self) -> &ResolvedRouting {
        &self.routing
    }

    /// Returns the selected concrete destination.
    pub fn destination(&self) -> &EngineInstanceId {
        self.routing.destination()
    }

    /// Returns the policy strategy that produced the destination.
    pub const fn routing_strategy(&self) -> RoutingStrategy {
        self.routing.strategy()
    }

    /// Returns the operation context, when present.
    pub fn context(&self) -> Option<&OperationContext> {
        self.context.as_ref()
    }

    /// Consumes the resolution and returns its constituent parts.
    ///
    /// This is useful to the routing layer when ownership of the resolved
    /// values is required.
    pub fn into_parts(
        self,
    ) -> (
        OperationId,
        ResolvedContract,
        ResolvedCapability,
        Option<ResolvedProvider>,
        ResolvedRouting,
        Option<OperationContext>,
    ) {
        (
            self.operation_id,
            self.contract,
            self.capability,
            self.provider,
            self.routing,
            self.context,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CorrelationId, NodeId};
    use crate::operation::Operation;

    fn operation_id() -> OperationId {
        OperationId::new("operation-1").unwrap()
    }

    fn contract() -> ResolvedContract {
        ResolvedContract::new(ContractId::new("quran.analyze").unwrap(), "1.0")
    }

    fn capability() -> ResolvedCapability {
        ResolvedCapability::new(CapabilityId::new("quran.analyze").unwrap())
    }

    fn destination() -> EngineInstanceId {
        EngineInstanceId::new("quran-01").unwrap()
    }

    fn routing() -> ResolvedRouting {
        ResolvedRouting::new(destination(), RoutingStrategy::Deterministic)
    }

    fn operation_context() -> OperationContext {
        let operation =
            Operation::new(operation_id(), CorrelationId::new("correlation-1").unwrap());

        OperationContext::new(operation)
    }

    fn input() -> ResolutionInput {
        ResolutionInput::new(operation_id(), contract(), capability(), routing())
    }

    #[test]
    fn creates_resolution_from_resolved_input() {
        let resolution = Resolution::resolve(input());

        assert_eq!(resolution.operation_id().as_str(), "operation-1");
        assert_eq!(
            resolution.contract().contract_id().as_str(),
            "quran.analyze"
        );
        assert_eq!(resolution.contract().version(), "1.0");
        assert_eq!(
            resolution.capability().capability_id().as_str(),
            "quran.analyze"
        );
        assert_eq!(resolution.destination().as_str(), "quran-01");
        assert_eq!(
            resolution.routing_strategy(),
            RoutingStrategy::Deterministic
        );
        assert!(resolution.provider().is_none());
        assert!(resolution.context().is_none());
    }

    #[test]
    fn provider_is_preserved_when_present() {
        let provider = ResolvedProvider::new(EngineId::new("quran-engine").unwrap());

        let resolution = Resolution::resolve(input().with_provider(provider.clone()));

        assert_eq!(resolution.provider(), Some(&provider));
        assert_eq!(
            resolution.provider().unwrap().engine_id().as_str(),
            "quran-engine"
        );
    }

    #[test]
    fn context_is_preserved_without_creating_attempt_identity() {
        let context = operation_context();

        let resolution = Resolution::resolve(input().with_context(context.clone()));

        assert_eq!(resolution.context(), Some(&context));
        assert!(resolution.context().unwrap().attempt_id.is_none());
        assert!(resolution.context().unwrap().node_id.is_none());
    }

    #[test]
    fn attempt_specific_context_can_be_preserved_without_resolution_owning_attempt() {
        let operation = Operation::new(
            operation_id(),
            CorrelationId::new("correlation-attempt").unwrap(),
        );

        let context = OperationContext::new(operation).for_attempt(
            NodeId::new("node-1").unwrap(),
            crate::identity::AttemptId::new("attempt-1").unwrap(),
        );

        let resolution = Resolution::resolve(input().with_context(context.clone()));

        assert_eq!(resolution.context(), Some(&context));
        assert_eq!(
            resolution
                .context()
                .unwrap()
                .attempt_id
                .as_ref()
                .unwrap()
                .as_str(),
            "attempt-1"
        );
    }

    #[test]
    fn routing_destination_is_preserved_exactly() {
        let selected = EngineInstanceId::new("arabic-02").unwrap();

        let routing = ResolvedRouting::new(selected.clone(), RoutingStrategy::RoundRobin);

        let resolution = Resolution::resolve(ResolutionInput::new(
            operation_id(),
            contract(),
            capability(),
            routing,
        ));

        assert_eq!(resolution.destination(), &selected);
        assert_eq!(resolution.routing_strategy(), RoutingStrategy::RoundRobin);
    }

    #[test]
    fn later_external_state_changes_cannot_mutate_resolution() {
        let resolution = Resolution::resolve(input());

        let original_destination = resolution.destination().clone();
        let original_strategy = resolution.routing_strategy();

        let later_routing = ResolvedRouting::new(
            EngineInstanceId::new("quran-02").unwrap(),
            RoutingStrategy::CapacityAware,
        );

        let later_resolution = Resolution::resolve(ResolutionInput::new(
            operation_id(),
            contract(),
            capability(),
            later_routing,
        ));

        assert_eq!(resolution.destination(), &original_destination);
        assert_eq!(resolution.routing_strategy(), original_strategy);

        assert_eq!(later_resolution.destination().as_str(), "quran-02");
        assert_eq!(
            later_resolution.routing_strategy(),
            RoutingStrategy::CapacityAware
        );
    }

    #[test]
    fn same_input_produces_equivalent_resolution() {
        let first = Resolution::resolve(input());
        let second = Resolution::resolve(input());

        assert_eq!(first, second);
    }

    #[test]
    fn resolution_clone_is_equal_to_original() {
        let original = Resolution::resolve(input().with_provider(ResolvedProvider::new(
            EngineId::new("quran-engine").unwrap(),
        )));

        let cloned = original.clone();

        assert_eq!(original, cloned);
    }

    #[test]
    fn input_builder_preserves_all_components() {
        let provider = ResolvedProvider::new(EngineId::new("provider-engine").unwrap());
        let context = operation_context();

        let input = input()
            .with_provider(provider.clone())
            .with_context(context.clone());

        assert_eq!(input.operation_id().as_str(), "operation-1");
        assert_eq!(input.contract(), &contract());
        assert_eq!(input.capability(), &capability());
        assert_eq!(input.provider(), Some(&provider));
        assert_eq!(input.routing(), &routing());
        assert_eq!(input.context(), Some(&context));
    }

    #[test]
    fn into_parts_preserves_every_resolution_component() {
        let provider = ResolvedProvider::new(EngineId::new("provider-engine").unwrap());
        let context = operation_context();

        let resolution = Resolution::resolve(
            input()
                .with_provider(provider.clone())
                .with_context(context.clone()),
        );

        let (
            operation,
            resolved_contract,
            resolved_capability,
            resolved_provider,
            resolved_routing,
            resolved_context,
        ) = resolution.into_parts();

        assert_eq!(operation, operation_id());
        assert_eq!(resolved_contract, contract());
        assert_eq!(resolved_capability, capability());
        assert_eq!(resolved_provider, Some(provider));
        assert_eq!(resolved_routing, routing());
        assert_eq!(resolved_context, Some(context));
    }

    #[test]
    fn resolution_does_not_change_when_a_new_resolution_is_created() {
        let first = Resolution::resolve(input());

        let second = Resolution::resolve(ResolutionInput::new(
            operation_id(),
            ResolvedContract::new(ContractId::new("quran.analyze").unwrap(), "2.0"),
            capability(),
            ResolvedRouting::new(
                EngineInstanceId::new("quran-02").unwrap(),
                RoutingStrategy::Weighted,
            ),
        ));

        assert_eq!(first.contract().version(), "1.0");
        assert_eq!(first.destination().as_str(), "quran-01");

        assert_eq!(second.contract().version(), "2.0");
        assert_eq!(second.destination().as_str(), "quran-02");
    }
}
