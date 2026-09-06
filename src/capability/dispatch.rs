//! Capability dispatch.
//!
//! The `dispatch` function resolves a capability from the registry and invokes
//! its handler. It checks cancellation and deadline through the `EngineContext`
//! before invoking the handler, ensuring safe execution boundaries.

use crate::runtime::EngineContext;

use super::{CapabilityError, CapabilityInvocation, CapabilityOutcome, CapabilityRegistry};

/// The result of a capability dispatch operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityDispatchResult {
    /// The handler was found and invoked successfully.
    Outcome(CapabilityOutcome),
    /// An error occurred during dispatch.
    Error(CapabilityError),
}

impl CapabilityDispatchResult {
    /// Returns true if the result is a successful outcome.
    pub fn is_ok(&self) -> bool {
        matches!(self, CapabilityDispatchResult::Outcome(_))
    }

    /// Returns true if the result is an error.
    pub fn is_err(&self) -> bool {
        !self.is_ok()
    }

    /// If the result is an outcome, returns a reference to it.
    pub fn as_outcome(&self) -> Option<&CapabilityOutcome> {
        match self {
            CapabilityDispatchResult::Outcome(o) => Some(o),
            CapabilityDispatchResult::Error(_) => None,
        }
    }

    /// If the result is an error, returns a reference to it.
    pub fn as_error(&self) -> Option<&CapabilityError> {
        match self {
            CapabilityDispatchResult::Outcome(_) => None,
            CapabilityDispatchResult::Error(e) => Some(e),
        }
    }

    /// Consumes self and returns the outcome, if any.
    pub fn into_outcome(self) -> Option<CapabilityOutcome> {
        match self {
            CapabilityDispatchResult::Outcome(o) => Some(o),
            CapabilityDispatchResult::Error(_) => None,
        }
    }

    /// Consumes self and returns the error, if any.
    pub fn into_error(self) -> Option<CapabilityError> {
        match self {
            CapabilityDispatchResult::Outcome(_) => None,
            CapabilityDispatchResult::Error(e) => Some(e),
        }
    }
}

/// Dispatches a capability invocation to its registered handler.
///
/// This function performs the following steps:
/// 1. Checks if the context has been cancelled. If so, returns `CapabilityError::Cancelled`.
/// 2. Checks if the context has expired. If so, returns `CapabilityError::DeadlineExpired`.
/// 3. Looks up the capability in the registry. If not found, returns `CapabilityError::Unknown`.
/// 4. Invokes the handler with the context and invocation.
/// 5. Returns the handler's result as `CapabilityDispatchResult`.
pub fn dispatch(
    registry: &CapabilityRegistry,
    context: &EngineContext,
    invocation: &CapabilityInvocation,
) -> CapabilityDispatchResult {
    // Check cancellation first
    if context.cancellation().is_cancelled() {
        return CapabilityDispatchResult::Error(CapabilityError::Cancelled);
    }

    // Check deadline expiration
    if context.is_expired() {
        return CapabilityDispatchResult::Error(CapabilityError::DeadlineExpired);
    }

    // Look up the handler in the registry
    let entry = match registry.get(invocation.capability_id()) {
        Some(entry) => entry,
        None => return CapabilityDispatchResult::Error(CapabilityError::Unknown),
    };

    // Invoke the handler
    match entry.handler().invoke(context, invocation) {
        Ok(outcome) => CapabilityDispatchResult::Outcome(outcome),
        Err(error) => CapabilityDispatchResult::Error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{
        CapabilityDefinition, CapabilityError, CapabilityHandler, CapabilityInvocation,
        CapabilityOutcome, arc_handler,
    };
    use crate::identity::{CapabilityId, ContractId, EngineId};
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::{Operation, OperationContext};
    use crate::runtime::Deadline;
    use std::sync::Arc;
    use std::time::Duration;

    fn make_context() -> EngineContext {
        let operation = Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        );
        EngineContext::new(OperationContext::new(operation))
    }

    fn make_invocation(cap_id: CapabilityId) -> CapabilityInvocation {
        CapabilityInvocation::new(
            cap_id,
            ContractId::new("test.request").unwrap(),
            b"test payload".to_vec(),
        )
    }

    fn make_handler() -> Arc<dyn CapabilityHandler> {
        arc_handler(|_ctx: &EngineContext, _invocation: &CapabilityInvocation| {
            Ok(CapabilityOutcome::new(b"handler response".to_vec()))
        })
    }

    fn register_handler(registry: &CapabilityRegistry, cap_id: CapabilityId) {
        let def = CapabilityDefinition::new(
            cap_id.clone(),
            EngineId::new("test.engine").unwrap(),
            "Test Handler",
        )
        .unwrap();
        registry.register(def, make_handler()).unwrap();
    }

    #[test]
    fn dispatch_invokes_registered_handler() {
        let registry = CapabilityRegistry::new();
        let cap_id = CapabilityId::new("test.cap").unwrap();
        register_handler(&registry, cap_id.clone());

        let context = make_context();
        let invocation = make_invocation(cap_id);

        let result = dispatch(&registry, &context, &invocation);
        assert!(result.is_ok());

        let outcome = result.into_outcome().unwrap();
        let bytes = outcome.into_bytes();
        assert_eq!(bytes, b"handler response");
    }

    #[test]
    fn dispatch_returns_unknown_for_missing_capability() {
        let registry = CapabilityRegistry::new();
        let context = make_context();
        let invocation = make_invocation(CapabilityId::new("missing").unwrap());

        let result = dispatch(&registry, &context, &invocation);
        assert!(result.is_err());
        assert!(matches!(result.as_error(), Some(CapabilityError::Unknown)));
    }

    #[test]
    fn dispatch_returns_cancelled_error_for_cancelled_context() {
        let registry = CapabilityRegistry::new();
        let cap_id = CapabilityId::new("test.cap").unwrap();
        register_handler(&registry, cap_id.clone());

        let context = make_context();
        context.cancellation().cancel();

        let invocation = make_invocation(cap_id);
        let result = dispatch(&registry, &context, &invocation);
        assert!(matches!(
            result.as_error(),
            Some(CapabilityError::Cancelled)
        ));
    }

    #[test]
    fn dispatch_returns_expired_error_for_expired_context() {
        let registry = CapabilityRegistry::new();
        let cap_id = CapabilityId::new("test.cap").unwrap();
        register_handler(&registry, cap_id.clone());

        let context = make_context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

        let invocation = make_invocation(cap_id);
        let result = dispatch(&registry, &context, &invocation);
        assert!(matches!(
            result.as_error(),
            Some(CapabilityError::DeadlineExpired)
        ));
    }

    #[test]
    fn dispatch_propagates_handler_errors() {
        let failing_handler: Arc<dyn CapabilityHandler> =
            arc_handler(|__: &EngineContext, _: &CapabilityInvocation| {
                Err(CapabilityError::HandlerFailed("handler crashed".into()))
            });

        let registry = CapabilityRegistry::new();
        let def = CapabilityDefinition::new(
            CapabilityId::new("failing.cap").unwrap(),
            EngineId::new("test.engine").unwrap(),
            "Failing Handler",
        )
        .unwrap();
        registry.register(def, failing_handler).unwrap();

        let context = make_context();
        let invocation = make_invocation(CapabilityId::new("failing.cap").unwrap());

        let result = dispatch(&registry, &context, &invocation);
        assert!(result.is_err());
        assert!(matches!(
            result.as_error(),
            Some(CapabilityError::HandlerFailed(_))
        ));
    }
}
