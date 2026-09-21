//! Concrete routing of an already-resolved Control Plane interaction.
//!
//! This module is the boundary between immutable resolution and the
//! communication layer.
//!
//! `routing.rs` does not:
//! - discover destinations;
//! - maintain routing membership;
//! - evaluate destination eligibility;
//! - evaluate routing policy;
//! - create execution attempts;
//! - decide whether a retry should occur;
//! - mutate lifecycle state;
//! - invoke capabilities;
//! - perform transport;
//! - route individual transport frames.
//!
//! The intended flow is:
//!
//! ```text
//! requirement
//!     ↓
//! destination eligibility
//!     ↓
//! routing policy
//!     ↓
//! resolution
//!     ↓
//! Router
//!     ↓
//! RoutingDecision
//!     ↓
//! communication / transport
//!     ↓
//! target Engine Runtime
//! ```
//!
//! One routing decision belongs to one already-authorized execution attempt.
//! A later retry is a new attempt and must obtain its own routing decision.
//!
//! Once a `RoutingDecision` has been created, later membership, registration,
//! lifecycle, or policy changes do not mutate that decision.

use crate::identity::{AttemptId, EngineInstanceId, OperationId};
use crate::retry::Attempt;

use super::resolution::Resolution;

/// Input required to bind a resolved interaction to one concrete attempt.
///
/// The attempt is supplied by the retry/attempt subsystem. Routing does not
/// create or advance attempts.
///
/// The resolution is already immutable and contains the destination selected
/// by the Control Plane policy layer.
#[derive(Debug)]
pub struct RoutingInput<'a> {
    resolution: &'a Resolution,
    attempt: &'a Attempt,
}

impl<'a> RoutingInput<'a> {
    /// Creates routing input from an existing resolution and attempt.
    pub const fn new(resolution: &'a Resolution, attempt: &'a Attempt) -> Self {
        Self {
            resolution,
            attempt,
        }
    }

    /// Returns the immutable resolution snapshot.
    pub const fn resolution(&self) -> &'a Resolution {
        self.resolution
    }

    /// Returns the already-created execution attempt.
    pub const fn attempt(&self) -> &'a Attempt {
        self.attempt
    }
}

/// The immutable routing decision for one concrete execution attempt.
///
/// This is the stable association:
///
/// ```text
/// AttemptId
///     ↕
/// EngineInstanceId
/// ```
///
/// The decision does not own the attempt lifecycle and does not provide any
/// operation for changing the destination.
///
/// A different attempt may receive a different destination, but doing so
/// requires another routing decision.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoutingDecision {
    operation_id: OperationId,
    attempt_id: AttemptId,
    destination: EngineInstanceId,
}

impl RoutingDecision {
    fn new(
        operation_id: OperationId,
        attempt_id: AttemptId,
        destination: EngineInstanceId,
    ) -> Self {
        Self {
            operation_id,
            attempt_id,
            destination,
        }
    }

    /// Returns the logical operation associated with this route.
    pub const fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the concrete execution attempt associated with this route.
    pub const fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }

    /// Returns the concrete engine instance selected by resolution.
    pub const fn destination(&self) -> &EngineInstanceId {
        &self.destination
    }

    /// Consumes the decision and returns its constituent values.
    pub fn into_parts(self) -> (OperationId, AttemptId, EngineInstanceId) {
        (self.operation_id, self.attempt_id, self.destination)
    }
}

/// Errors owned by the routing boundary.
///
/// These errors concern the integrity of the already-resolved routing
/// operation. They do not represent transport failures or Engine Runtime
/// rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingError {
    /// The resolution belongs to a different logical operation than the
    /// supplied execution attempt.
    OperationMismatch {
        resolution_operation_id: OperationId,
        attempt_operation_id: OperationId,
    },

    /// The resolution already carries an attempt-specific context, but that
    /// context belongs to a different attempt than the one being routed.
    AttemptMismatch {
        resolution_attempt_id: AttemptId,
        routing_attempt_id: AttemptId,
    },
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OperationMismatch {
                resolution_operation_id,
                attempt_operation_id,
            } => write!(
                formatter,
                "routing operation mismatch: resolution belongs to operation \
                 {resolution_operation_id}, attempt belongs to operation \
                 {attempt_operation_id}"
            ),

            Self::AttemptMismatch {
                resolution_attempt_id,
                routing_attempt_id,
            } => write!(
                formatter,
                "routing attempt mismatch: resolution context belongs to attempt \
                 {resolution_attempt_id}, routing was requested for attempt \
                 {routing_attempt_id}"
            ),
        }
    }
}

impl std::error::Error for RoutingError {}

/// Stateless Control Plane router.
///
/// `Router` contains no membership, policy, lifecycle, transport, retry, or
/// mutable routing state. Every call operates on an explicit immutable
/// resolution and an already-created attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Router;

impl Router {
    /// Creates a stateless router.
    pub const fn new() -> Self {
        Self
    }

    /// Binds an already-resolved interaction to an already-authorized attempt.
    ///
    /// This method deliberately performs no destination selection. The
    /// destination has already been selected by `policy.rs` and captured by
    /// `resolution.rs`.
    ///
    /// The method verifies:
    ///
    /// 1. the attempt belongs to the same operation as the resolution;
    /// 2. if the resolution carries attempt-specific context, that context
    ///    refers to the same attempt.
    ///
    /// On success, the returned decision is an immutable snapshot.
    pub fn route(&self, input: RoutingInput<'_>) -> Result<RoutingDecision, RoutingError> {
        let resolution = input.resolution();
        let attempt = input.attempt();

        if resolution.operation_id() != attempt.operation_id() {
            return Err(RoutingError::OperationMismatch {
                resolution_operation_id: resolution.operation_id().clone(),
                attempt_operation_id: attempt.operation_id().clone(),
            });
        }

        if let Some(resolution_attempt_id) = resolution
            .context()
            .and_then(|context| context.attempt_id.as_ref())
            && resolution_attempt_id != attempt.attempt_id()
        {
            return Err(RoutingError::AttemptMismatch {
                resolution_attempt_id: resolution_attempt_id.clone(),
                routing_attempt_id: attempt.attempt_id().clone(),
            });
        }

        Ok(RoutingDecision::new(
            resolution.operation_id().clone(),
            attempt.attempt_id().clone(),
            resolution.destination().clone(),
        ))
    }

    /// Convenience function for callers that do not need to instantiate a
    /// `Router` value.
    pub fn route_resolved(
        resolution: &Resolution,
        attempt: &Attempt,
    ) -> Result<RoutingDecision, RoutingError> {
        Self::new().route(RoutingInput::new(resolution, attempt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::control_plane::policy::RoutingStrategy;
    use crate::control_plane::resolution::{
        Resolution, ResolutionInput, ResolvedCapability, ResolvedContract, ResolvedProvider,
        ResolvedRouting,
    };
    use crate::identity::{
        AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::retry::Attempt;

    fn operation_id(value: &str) -> OperationId {
        OperationId::new(value).unwrap()
    }

    fn attempt_id(value: &str) -> AttemptId {
        AttemptId::new(value).unwrap()
    }

    fn engine_instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).unwrap()
    }

    fn contract() -> ResolvedContract {
        ResolvedContract::new(ContractId::new("quran.analyze").unwrap(), "1.0")
    }

    fn capability() -> ResolvedCapability {
        ResolvedCapability::new(CapabilityId::new("quran.analyze").unwrap())
    }

    fn resolution_for(operation: &OperationId, destination: &EngineInstanceId) -> Resolution {
        let routing = ResolvedRouting::new(destination.clone(), RoutingStrategy::Deterministic);

        Resolution::resolve(ResolutionInput::new(
            operation.clone(),
            contract(),
            capability(),
            routing,
        ))
    }

    fn attempt_for(operation: &OperationId, attempt: &AttemptId) -> Attempt {
        Attempt::new(operation.clone(), attempt.clone(), 1).unwrap()
    }

    #[test]
    fn routes_resolved_interaction_to_selected_destination() {
        let operation = operation_id("operation-1");
        let attempt_id = attempt_id("attempt-1");
        let destination = engine_instance_id("quran-01");

        let resolution = resolution_for(&operation, &destination);
        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(decision.operation_id(), &operation);
        assert_eq!(decision.attempt_id(), &attempt_id);
        assert_eq!(decision.destination(), &destination);
    }

    #[test]
    fn route_does_not_select_a_new_destination() {
        let operation = operation_id("operation-2");
        let attempt_id = attempt_id("attempt-2");
        let destination = engine_instance_id("arabic-02");

        let resolution = resolution_for(&operation, &destination);
        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::new()
            .route(RoutingInput::new(&resolution, &attempt))
            .unwrap();

        assert_eq!(decision.destination().as_str(), "arabic-02");
    }

    #[test]
    fn operation_mismatch_is_rejected() {
        let resolution_operation = operation_id("operation-resolution");
        let attempt_operation = operation_id("operation-attempt");

        let resolution = resolution_for(&resolution_operation, &engine_instance_id("engine-01"));

        let attempt = attempt_for(&attempt_operation, &attempt_id("attempt-1"));

        let error = Router::route_resolved(&resolution, &attempt).unwrap_err();

        assert_eq!(
            error,
            RoutingError::OperationMismatch {
                resolution_operation_id: resolution_operation,
                attempt_operation_id: attempt_operation,
            }
        );
    }

    #[test]
    fn attempt_context_mismatch_is_rejected() {
        let operation = operation_id("operation-3");

        let resolution_base = resolution_for(&operation, &engine_instance_id("engine-01"));

        let context_operation = Operation::new(
            operation.clone(),
            CorrelationId::new("correlation-3").unwrap(),
        );

        let context = OperationContext::new(context_operation).for_attempt(
            crate::identity::NodeId::new("node-1").unwrap(),
            attempt_id("attempt-resolution"),
        );

        let input = ResolutionInput::new(
            operation.clone(),
            contract(),
            capability(),
            ResolvedRouting::new(
                engine_instance_id("engine-01"),
                RoutingStrategy::Deterministic,
            ),
        )
        .with_context(context);

        let resolution = Resolution::resolve(input);

        let attempt = attempt_for(&operation, &attempt_id("attempt-routing"));

        let error = Router::route_resolved(&resolution, &attempt).unwrap_err();

        assert_eq!(
            error,
            RoutingError::AttemptMismatch {
                resolution_attempt_id: attempt_id("attempt-resolution"),
                routing_attempt_id: attempt_id("attempt-routing"),
            }
        );

        // Keep the independently constructed resolution alive long enough to
        // make the test's snapshot semantics explicit.
        assert_eq!(resolution_base.destination().as_str(), "engine-01");
    }

    #[test]
    fn matching_attempt_context_is_accepted() {
        let operation = operation_id("operation-4");
        let attempt_id = attempt_id("attempt-4");

        let operation_context = Operation::new(
            operation.clone(),
            CorrelationId::new("correlation-4").unwrap(),
        );

        let context = OperationContext::new(operation_context).for_attempt(
            crate::identity::NodeId::new("node-4").unwrap(),
            attempt_id.clone(),
        );

        let resolution = Resolution::resolve(
            ResolutionInput::new(
                operation.clone(),
                contract(),
                capability(),
                ResolvedRouting::new(
                    engine_instance_id("engine-04"),
                    RoutingStrategy::CapacityAware,
                ),
            )
            .with_context(context),
        );

        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(decision.operation_id(), &operation);
        assert_eq!(decision.attempt_id(), &attempt_id);
        assert_eq!(decision.destination().as_str(), "engine-04");
    }

    #[test]
    fn resolution_without_attempt_context_is_valid() {
        let operation = operation_id("operation-5");
        let attempt_id = attempt_id("attempt-5");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-05"));

        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(decision.attempt_id(), &attempt_id);
        assert_eq!(decision.destination().as_str(), "engine-05");
    }

    #[test]
    fn routing_decision_is_immutable_snapshot() {
        let operation = operation_id("operation-6");
        let attempt_id = attempt_id("attempt-6");

        let first_destination = engine_instance_id("engine-a");

        let first_resolution = resolution_for(&operation, &first_destination);

        let attempt = attempt_for(&operation, &attempt_id);

        let first_decision = Router::route_resolved(&first_resolution, &attempt).unwrap();

        let second_resolution = resolution_for(&operation, &engine_instance_id("engine-b"));

        let second_decision = Router::route_resolved(&second_resolution, &attempt).unwrap();

        assert_eq!(first_decision.destination().as_str(), "engine-a");
        assert_eq!(second_decision.destination().as_str(), "engine-b");

        // Creating a later resolution does not mutate the existing route.
        assert_eq!(first_decision.destination().as_str(), "engine-a");
    }

    #[test]
    fn different_attempts_may_have_different_destinations() {
        let operation = operation_id("operation-7");

        let attempt_one = attempt_for(&operation, &attempt_id("attempt-1"));

        let attempt_two = attempt_for(&operation, &attempt_id("attempt-2"));

        let resolution_one = resolution_for(&operation, &engine_instance_id("engine-a"));

        let resolution_two = resolution_for(&operation, &engine_instance_id("engine-b"));

        let route_one = Router::route_resolved(&resolution_one, &attempt_one).unwrap();

        let route_two = Router::route_resolved(&resolution_two, &attempt_two).unwrap();

        assert_eq!(route_one.attempt_id().as_str(), "attempt-1");
        assert_eq!(route_one.destination().as_str(), "engine-a");

        assert_eq!(route_two.attempt_id().as_str(), "attempt-2");
        assert_eq!(route_two.destination().as_str(), "engine-b");
    }

    #[test]
    fn routing_is_deterministic_for_same_inputs() {
        let operation = operation_id("operation-8");
        let attempt_id = attempt_id("attempt-8");
        let destination = engine_instance_id("engine-08");

        let resolution = resolution_for(&operation, &destination);
        let attempt = attempt_for(&operation, &attempt_id);

        let first = Router::route_resolved(&resolution, &attempt).unwrap();

        let second = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn routing_decision_into_parts_preserves_identity() {
        let operation = operation_id("operation-9");
        let attempt_id = attempt_id("attempt-9");
        let destination = engine_instance_id("engine-09");

        let resolution = resolution_for(&operation, &destination);
        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::route_resolved(&resolution, &attempt).unwrap();

        let (resolved_operation, resolved_attempt, resolved_destination) = decision.into_parts();

        assert_eq!(resolved_operation, operation);
        assert_eq!(resolved_attempt, attempt_id);
        assert_eq!(resolved_destination, destination);
    }

    #[test]
    fn routing_does_not_create_a_second_attempt() {
        let operation = operation_id("operation-10");
        let attempt_id = attempt_id("attempt-10");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-10"));

        let attempt = attempt_for(&operation, &attempt_id);

        let _decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(attempt.attempt_id(), &attempt_id);
        assert_eq!(attempt.attempt_number(), 1);
    }

    #[test]
    fn routing_does_not_change_attempt_lifecycle() {
        let operation = operation_id("operation-11");
        let attempt_id = attempt_id("attempt-11");

        let resolution = resolution_for(&operation, &engine_instance_id("engine-11"));

        let attempt = attempt_for(&operation, &attempt_id);

        assert_eq!(
            attempt.state(),
            crate::retry::AttemptLifecycleState::Created
        );

        let _decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(
            attempt.state(),
            crate::retry::AttemptLifecycleState::Created
        );
    }

    #[test]
    fn provider_information_does_not_change_routing_boundary() {
        let operation = operation_id("operation-12");
        let attempt_id = attempt_id("attempt-12");

        let resolution = Resolution::resolve(
            ResolutionInput::new(
                operation.clone(),
                contract(),
                capability(),
                ResolvedRouting::new(
                    engine_instance_id("provider-target"),
                    RoutingStrategy::Weighted,
                ),
            )
            .with_provider(ResolvedProvider::new(
                EngineId::new("provider-engine").unwrap(),
            )),
        );

        let attempt = attempt_for(&operation, &attempt_id);

        let decision = Router::route_resolved(&resolution, &attempt).unwrap();

        assert_eq!(decision.destination().as_str(), "provider-target");
    }
}
