//! Core-controlled middleware infrastructure.
//!
//! This module exposes the generic middleware stage abstraction and the
//! deterministic middleware chain used by the runtime.
//!
//! Middleware is responsible for cross-cutting request and response processing.
//! Security, validation, tracing, metrics, and other concerns are implemented
//! on top of these primitives.
//!
//! Domain workflows, capability implementation, transport handling, and
//! engine-specific business rules remain outside this module.

pub mod chain;
pub mod stages;

#[cfg(test)]
mod tests {
    use super::chain::{MiddlewareChain, MiddlewareChainError};
    use super::stages::{Middleware, MiddlewareError, MiddlewareRejection, MiddlewareResult};

    use crate::{
        contracts::{
            UniversalRequest, UniversalResponse,
            descriptor::{
                ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
            },
            envelope::MessageEnvelope,
            metadata::{ContractMetadata, Participants},
        },
        identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId},
        operation::{Operation, OperationContext},
        runtime::EngineContext,
        status::Status,
    };

    use std::sync::{Arc, Mutex};

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("middleware-module-operation").unwrap(),
            CorrelationId::new("middleware-module-correlation").unwrap(),
        )))
    }

    fn request_envelope() -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("middleware.module.contract").unwrap(),
            CapabilityId::new("middleware.module.test").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("middleware-module-sender").unwrap(),
                EngineId::new("middleware-module-receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("middleware-module-operation").unwrap(),
            CorrelationId::new("middleware-module-correlation").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new("middleware-module-request").unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"middleware module payload"),
        )
    }

    fn response_envelope() -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("middleware.module.contract").unwrap(),
            CapabilityId::new("middleware.module.test").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Response,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("middleware-module-sender").unwrap(),
                EngineId::new("middleware-module-receiver").unwrap(),
            ),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("middleware-module-operation").unwrap(),
            CorrelationId::new("middleware-module-correlation").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new("middleware-module-response").unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"middleware module response"),
        )
    }

    fn request() -> UniversalRequest {
        UniversalRequest::new(request_envelope())
    }

    fn response() -> UniversalResponse {
        UniversalResponse::new(response_envelope(), Status::Success)
    }

    #[derive(Debug)]
    struct RecordingMiddleware {
        name: &'static str,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Middleware for RecordingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            self.events.lock().unwrap().push(self.name);

            MiddlewareResult::Continue
        }

        fn on_response(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
            _response: &mut UniversalResponse,
        ) -> Result<(), MiddlewareError> {
            self.events.lock().unwrap().push(self.name);

            Ok(())
        }
    }

    #[derive(Debug)]
    struct RejectingMiddleware;

    impl Middleware for RejectingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            MiddlewareResult::Reject(MiddlewareRejection::new(
                "middleware module rejected request",
            ))
        }
    }

    #[derive(Debug)]
    struct FailingMiddleware;

    impl Middleware for FailingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            MiddlewareResult::Fail(MiddlewareError::new("middleware module failed"))
        }
    }

    #[test]
    fn middleware_modules_are_available() {
        let chain = MiddlewareChain::new();

        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn middleware_result_types_are_available() {
        let rejection = MiddlewareRejection::new("test rejection");

        let result = MiddlewareResult::Reject(rejection.clone());

        assert!(result.is_reject());
        assert_eq!(result.as_rejection(), Some(&rejection),);

        let error = MiddlewareError::new("test error");

        let failure = MiddlewareResult::Fail(error.clone());

        assert!(failure.is_fail());
        assert_eq!(failure.as_error(), Some(&error),);
    }

    #[test]
    fn middleware_uses_the_existing_engine_context() {
        let middleware = RecordingMiddleware {
            name: "context",
            events: Arc::new(Mutex::new(Vec::new())),
        };

        let mut request = request();
        let mut context = context();

        assert_eq!(
            context.operation().operation.id.as_str(),
            "middleware-module-operation"
        );

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );

        assert_eq!(*middleware.events.lock().unwrap(), vec!["context"]);
    }

    #[test]
    fn middleware_chain_and_stage_work_together() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut chain = MiddlewareChain::new();

        chain.push(RecordingMiddleware {
            name: "a",
            events: Arc::clone(&events),
        });

        chain.push(RecordingMiddleware {
            name: "b",
            events: Arc::clone(&events),
        });

        let mut request = request();

        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(*events.lock().unwrap(), vec!["a", "b", "b", "a",],);
    }

    #[test]
    fn middleware_rejection_stops_chain_and_downstream() {
        let mut chain = MiddlewareChain::new();

        chain.push(RejectingMiddleware);

        let downstream_called = Arc::new(Mutex::new(false));

        let downstream_called_clone = Arc::clone(&downstream_called);

        let mut request = request();

        let mut context = context();

        let result = chain.execute(&mut context, &mut request, move |_context, _request| {
            *downstream_called_clone.lock().unwrap() = true;

            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Rejected(MiddlewareRejection::new(
                "middleware module rejected request",
            ),),),
        );

        assert!(!*downstream_called.lock().unwrap());
    }

    #[test]
    fn middleware_failure_stops_chain_and_downstream() {
        let mut chain = MiddlewareChain::new();

        chain.push(FailingMiddleware);

        let downstream_called = Arc::new(Mutex::new(false));

        let downstream_called_clone = Arc::clone(&downstream_called);

        let mut request = request();

        let mut context = context();

        let result = chain.execute(&mut context, &mut request, move |_context, _request| {
            *downstream_called_clone.lock().unwrap() = true;

            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Middleware(MiddlewareError::new(
                "middleware module failed",
            ),),),
        );

        assert!(!*downstream_called.lock().unwrap());
    }

    #[test]
    fn middleware_chain_preserves_downstream_error() {
        let chain = MiddlewareChain::new();

        let mut request = request();

        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Err::<UniversalResponse, _>("downstream failure")
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Downstream("downstream failure",),),
        );
    }

    #[test]
    fn middleware_chain_preserves_response_order() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut chain = MiddlewareChain::new();

        chain.push(RecordingMiddleware {
            name: "first",
            events: Arc::clone(&events),
        });

        chain.push(RecordingMiddleware {
            name: "second",
            events: Arc::clone(&events),
        });

        chain.push(RecordingMiddleware {
            name: "third",
            events: Arc::clone(&events),
        });

        let mut request = request();

        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(
            *events.lock().unwrap(),
            vec!["first", "second", "third", "third", "second", "first",],
        );
    }

    #[test]
    fn middleware_chain_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<MiddlewareChain>();
    }

    #[test]
    fn middleware_trait_is_available_from_stages_module() {
        fn accepts_middleware<M: Middleware>(_middleware: M) {}

        accepts_middleware(RecordingMiddleware {
            name: "test",
            events: Arc::new(Mutex::new(Vec::new())),
        });
    }
}
