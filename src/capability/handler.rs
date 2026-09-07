//! Capability handler interface.
//!
//! Engines own their typed request and response types. Core provides the
//! boundary contract that capability dispatch invokes against.

use std::sync::Arc;

use crate::runtime::EngineContext;

use super::{CapabilityError, CapabilityInvocation, CapabilityOutcome};

/// The typed result of invoking a capability handler.
#[derive(Clone, Debug)]
pub struct CapabilityResponse {
    /// The raw response payload bytes produced by the handler.
    pub response_bytes: Vec<u8>,
}

impl CapabilityResponse {
    /// Creates a new capability response from raw bytes.
    pub fn new(response_bytes: Vec<u8>) -> Self {
        Self { response_bytes }
    }

    /// Returns the response payload bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.response_bytes
    }
}

impl From<CapabilityResponse> for Vec<u8> {
    fn from(response: CapabilityResponse) -> Self {
        response.response_bytes
    }
}

/// The trait that engine handlers implement.
///
/// Each engine implements its own typed request and response on top of this
/// boundary. Core does not prescribe typed request or response structures.
pub trait CapabilityHandler: Send + Sync {
    /// Invokes the capability handler.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityError` if the handler cannot be invoked.
    fn invoke(
        &self,
        context: &EngineContext,
        invocation: &CapabilityInvocation,
    ) -> Result<CapabilityOutcome, CapabilityError>;
}

/// A boxed capability handler.
pub type BoxedCapabilityHandler = Box<dyn CapabilityHandler>;

/// Adapter that turns a plain function into a boxed capability handler.
pub struct FunctionHandler<F> {
    function: F,
}

impl<F> FunctionHandler<F> {
    /// Creates a new function handler adapter.
    pub fn new(function: F) -> Self {
        Self { function }
    }
}

impl<F> CapabilityHandler for FunctionHandler<F>
where
    F: Fn(&EngineContext, &CapabilityInvocation) -> Result<CapabilityOutcome, CapabilityError>
        + Send
        + Sync
        + 'static,
{
    fn invoke(
        &self,
        context: &EngineContext,
        invocation: &CapabilityInvocation,
    ) -> Result<CapabilityOutcome, CapabilityError> {
        (self.function)(context, invocation)
    }
}

impl<F> From<F> for BoxedCapabilityHandler
where
    F: Fn(&EngineContext, &CapabilityInvocation) -> Result<CapabilityOutcome, CapabilityError>
        + Send
        + Sync
        + 'static,
{
    fn from(function: F) -> Self {
        Box::new(FunctionHandler::new(function))
    }
}

/// Wraps a plain function into an `Arc<dyn CapabilityHandler>`.
pub fn arc_handler<F>(function: F) -> Arc<dyn CapabilityHandler>
where
    F: Fn(&EngineContext, &CapabilityInvocation) -> Result<CapabilityOutcome, CapabilityError>
        + Send
        + Sync
        + 'static,
{
    Arc::new(FunctionHandler::new(function))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CapabilityId, ContractId, CorrelationId, OperationId};
    use crate::operation::{Operation, OperationContext};
    use crate::runtime::EngineContext;

    fn make_context() -> EngineContext {
        let operation = Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        );
        EngineContext::new(OperationContext::new(operation))
    }

    fn make_invocation() -> CapabilityInvocation {
        CapabilityInvocation::new(
            CapabilityId::new("test.cap").unwrap(),
            ContractId::new("test.request").unwrap(),
            b"payload".to_vec(),
        )
    }

    #[test]
    fn function_adapter_produces_a_boxed_handler() {
        let handler: BoxedCapabilityHandler =
            (|_context: &EngineContext, _invocation: &CapabilityInvocation| {
                Ok(CapabilityOutcome::new(b"response".to_vec()))
            })
            .into();

        let context = make_context();
        let invocation = make_invocation();
        let outcome = handler.invoke(&context, &invocation).unwrap();
        assert_eq!(outcome.into_bytes(), b"response");
    }

    #[test]
    fn function_adapter_propagates_handler_errors() {
        let handler: BoxedCapabilityHandler =
            (|_context: &EngineContext, _invocation: &CapabilityInvocation| {
                Err(CapabilityError::HandlerFailed("boom".into()))
            })
            .into();

        let context = make_context();
        let invocation = make_invocation();
        let result = handler.invoke(&context, &invocation);
        assert!(matches!(result, Err(CapabilityError::HandlerFailed(_))));
    }

    #[test]
    fn arc_handler_wraps_a_function() {
        let handler = arc_handler(
            |_context: &EngineContext, _invocation: &CapabilityInvocation| {
                Ok(CapabilityOutcome::new(b"arc-response".to_vec()))
            },
        );

        let context = make_context();
        let invocation = make_invocation();
        let outcome = handler.invoke(&context, &invocation).unwrap();
        assert_eq!(outcome.into_bytes(), b"arc-response");
    }

    #[test]
    fn boxed_handler_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BoxedCapabilityHandler>();
    }

    #[test]
    fn capability_response_new_stores_bytes() {
        let bytes = b"response".to_vec();
        let response = CapabilityResponse::new(bytes.clone());
        assert_eq!(response.response_bytes, bytes);
    }

    #[test]
    fn capability_response_into_bytes() {
        let bytes = b"into bytes".to_vec();
        let response = CapabilityResponse::new(bytes.clone());
        let extracted = response.into_bytes();
        assert_eq!(extracted, bytes);
    }

    #[test]
    fn capability_response_from_implementation() {
        let bytes = b"from impl".to_vec();
        let response = CapabilityResponse::new(bytes.clone());
        let result: Vec<u8> = response.into();
        assert_eq!(result, bytes);
    }

    #[test]
    fn capability_response_debug() {
        let response = CapabilityResponse::new(b"debug".to_vec());
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("CapabilityResponse"));
    }

    #[test]
    fn function_handler_new_stores_function() {
        let handler = FunctionHandler::new(|_: &EngineContext, _: &CapabilityInvocation| {
            Ok(CapabilityOutcome::new(b"stored".to_vec()))
        });
        // Calling new does not invoke — it just stores.
        // We verify it compiles and can be invoked via the trait.
        let context = make_context();
        let invocation = make_invocation();
        let result = handler.invoke(&context, &invocation).unwrap();
        assert_eq!(result.into_bytes(), b"stored");
    }
}
