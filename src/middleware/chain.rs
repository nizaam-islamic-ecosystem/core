//! Deterministic middleware chain execution for Nizaam Core.
//!
//! `MiddlewareChain` owns the ordered collection of middleware stages and
//! coordinates request-side and response-side processing.
//!
//! The chain is intentionally independent of authentication, authorization,
//! capability resolution, transport, engine lifecycle, and domain semantics.
//! Those concerns are supplied by individual middleware implementations or
//! by the downstream runtime execution boundary.

use crate::{
    contracts::{UniversalRequest, UniversalResponse},
    runtime::EngineContext,
};

use super::stages::{Middleware, MiddlewareError, MiddlewareRejection, MiddlewareResult};

/// Error returned by [`MiddlewareChain`] execution.
///
/// A middleware rejection, middleware failure, and downstream failure remain
/// distinguishable so that the runtime can preserve the original failure
/// category when constructing the final response.
#[derive(Debug, PartialEq)]
pub enum MiddlewareChainError<E> {
    /// Request execution context became invalid before downstream execution.
    Context(crate::runtime::PipelineError),

    /// Middleware intentionally rejected the request.
    Rejected(MiddlewareRejection),

    /// Middleware processing itself failed.
    Middleware(MiddlewareError),

    /// Downstream execution failed after every request middleware continued.
    Downstream(E),
}

/// An ordered collection of middleware stages.
///
/// Middleware is executed in insertion order for requests:
///
/// `A → B → C`
///
/// Response processing is executed in reverse order:
///
/// `C → B → A`
///
/// The chain is normally configured before serving begins and then shared
/// immutably by concurrent request executions.
#[derive(Default)]
pub struct MiddlewareChain {
    stages: Vec<Box<dyn Middleware>>,
}

impl MiddlewareChain {
    /// Creates an empty middleware chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a middleware chain containing one initial stage.
    pub fn with_stage<M>(stage: M) -> Self
    where
        M: Middleware + 'static,
    {
        let mut chain = Self::new();
        chain.push(stage);
        chain
    }

    /// Adds a middleware stage to the end of the chain.
    ///
    /// The resulting order is the order in which request middleware will be
    /// executed.
    pub fn push<M>(&mut self, stage: M)
    where
        M: Middleware + 'static,
    {
        self.stages.push(Box::new(stage));
    }

    /// Returns the number of middleware stages in the chain.
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// Returns whether the chain contains no middleware stages.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Executes request middleware, downstream work, and response middleware.
    ///
    /// Request processing occurs in registration order. A request middleware
    /// stage returning [`MiddlewareResult::Reject`] or
    /// [`MiddlewareResult::Fail`] immediately terminates execution and the
    /// downstream function is not called.
    ///
    /// The same mutable [`EngineContext`] is passed through every request
    /// middleware stage. This allows middleware to establish trusted
    /// request-scoped state without creating a second execution context.
    ///
    /// Once all request middleware continues, `downstream` is invoked with an
    /// immutable reference to the established context.
    ///
    /// Response middleware then executes in reverse registration order.
    pub fn execute<F, E>(
        &self,
        context: &mut EngineContext,
        request: &mut UniversalRequest,
        downstream: F,
    ) -> Result<UniversalResponse, MiddlewareChainError<E>>
    where
        F: FnOnce(&EngineContext, &mut UniversalRequest) -> Result<UniversalResponse, E>,
    {
        for stage in &self.stages {
            match stage.on_request(context, request) {
                MiddlewareResult::Continue => {}
                MiddlewareResult::Reject(rejection) => {
                    return Err(MiddlewareChainError::Rejected(rejection));
                }
                MiddlewareResult::Fail(error) => {
                    return Err(MiddlewareChainError::Middleware(error));
                }
            }
        }

        crate::runtime::check_context(context).map_err(MiddlewareChainError::Context)?;

        let mut response =
            downstream(context, request).map_err(MiddlewareChainError::Downstream)?;

        for stage in self.stages.iter().rev() {
            stage
                .on_response(context, request, &mut response)
                .map_err(MiddlewareChainError::Middleware)?;
        }

        Ok(response)
    }
}

impl std::fmt::Debug for MiddlewareChain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MiddlewareChain")
            .field("len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        contracts::{
            descriptor::{
                ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
            },
            envelope::MessageEnvelope,
            metadata::{ContractMetadata, Participants},
        },
        identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId},
        operation::{Operation, OperationContext},
        runtime::Deadline,
        status::Status,
    };
    use std::sync::{Arc, Mutex};

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("middleware-chain-operation").unwrap(),
            CorrelationId::new("middleware-chain-correlation").unwrap(),
        )))
    }

    fn envelope(interaction: Interaction, message_id: &str) -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("middleware.chain.contract").unwrap(),
            CapabilityId::new("middleware.chain.test").unwrap(),
            Version::new(1, 0, 0),
            interaction,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("middleware-chain-sender").unwrap(),
                EngineId::new("middleware-chain-receiver").unwrap(),
            ),
        );

        let operation = OperationContext::new(Operation::new(
            OperationId::new("middleware-chain-operation").unwrap(),
            CorrelationId::new("middleware-chain-correlation").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            operation,
            metadata,
            EncodedPayload::new(descriptor.payload, b"middleware chain payload"),
        )
    }

    fn request() -> UniversalRequest {
        UniversalRequest::new(envelope(Interaction::Request, "middleware-chain-request"))
    }

    fn response() -> UniversalResponse {
        UniversalResponse::new(
            envelope(Interaction::Response, "middleware-chain-response"),
            Status::Success,
        )
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
            MiddlewareResult::Reject(MiddlewareRejection::new("request rejected"))
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
            MiddlewareResult::Fail(MiddlewareError::new("request processing failed"))
        }
    }

    #[derive(Debug)]
    struct RecordingRequestMutationMiddleware;

    impl Middleware for RecordingRequestMutationMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            request.envelope.message_id = MessageId::new("mutated-request").unwrap();

            MiddlewareResult::Continue
        }
    }

    #[test]
    fn new_chain_is_empty() {
        let chain = MiddlewareChain::new();

        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn push_adds_stages_in_order() {
        let mut chain = MiddlewareChain::new();

        chain.push(RecordingRequestMutationMiddleware);

        chain.push(RejectingMiddleware);

        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());
    }

    #[test]
    fn with_stage_creates_single_stage_chain() {
        let chain = MiddlewareChain::with_stage(RecordingRequestMutationMiddleware);

        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
    }

    #[test]
    fn empty_chain_invokes_downstream() {
        let chain = MiddlewareChain::new();

        let mut request = request();
        let mut context = context();
        let mut downstream_called = false;

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());
        assert!(downstream_called);
    }

    #[test]
    fn request_middleware_runs_in_registration_order() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let mut chain = MiddlewareChain::new();

        chain.push(RecordingMiddleware {
            name: "a-request",
            events: Arc::clone(&events),
        });

        chain.push(RecordingMiddleware {
            name: "b-request",
            events: Arc::clone(&events),
        });

        chain.push(RecordingMiddleware {
            name: "c-request",
            events: Arc::clone(&events),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        let recorded = events.lock().unwrap().clone();

        assert_eq!(
            recorded,
            vec![
                "a-request",
                "b-request",
                "c-request",
                "c-request",
                "b-request",
                "a-request",
            ]
        );
    }

    #[test]
    fn response_middleware_runs_in_reverse_registration_order() {
        #[derive(Debug)]
        struct ResponseRecordingMiddleware {
            name: &'static str,
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for ResponseRecordingMiddleware {
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

        let events = Arc::new(Mutex::new(Vec::new()));

        let mut chain = MiddlewareChain::new();

        chain.push(ResponseRecordingMiddleware {
            name: "a",
            events: Arc::clone(&events),
        });

        chain.push(ResponseRecordingMiddleware {
            name: "b",
            events: Arc::clone(&events),
        });

        chain.push(ResponseRecordingMiddleware {
            name: "c",
            events: Arc::clone(&events),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(*events.lock().unwrap(), vec!["c", "b", "a",]);
    }

    #[test]
    fn downstream_runs_after_all_request_middleware() {
        let events = Arc::new(Mutex::new(Vec::new()));

        #[derive(Debug)]
        struct DownstreamOrderingMiddleware {
            name: &'static str,
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for DownstreamOrderingMiddleware {
            fn on_request(
                &self,
                _context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                self.events.lock().unwrap().push(self.name);

                MiddlewareResult::Continue
            }
        }

        let mut chain = MiddlewareChain::new();

        chain.push(DownstreamOrderingMiddleware {
            name: "middleware-a",
            events: Arc::clone(&events),
        });

        chain.push(DownstreamOrderingMiddleware {
            name: "middleware-b",
            events: Arc::clone(&events),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            events.lock().unwrap().push("downstream");

            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(
            *events.lock().unwrap(),
            vec!["middleware-a", "middleware-b", "downstream",]
        );
    }

    #[test]
    fn rejection_short_circuits_request_processing() {
        #[derive(Debug)]
        struct FirstStage {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for FirstStage {
            fn on_request(
                &self,
                _context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                self.events.lock().unwrap().push("first");

                MiddlewareResult::Continue
            }
        }

        let events = Arc::new(Mutex::new(Vec::new()));

        let mut chain = MiddlewareChain::new();

        chain.push(FirstStage {
            events: Arc::clone(&events),
        });

        chain.push(RejectingMiddleware);

        let mut request = request();
        let mut context = context();
        let mut downstream_called = false;

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Rejected(MiddlewareRejection::new(
                "request rejected",
            ),),)
        );

        assert_eq!(*events.lock().unwrap(), vec!["first"]);

        assert!(!downstream_called);
    }

    #[test]
    fn failure_short_circuits_request_processing() {
        let mut chain = MiddlewareChain::new();

        chain.push(FailingMiddleware);

        let mut request = request();
        let mut context = context();
        let mut downstream_called = false;

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Middleware(MiddlewareError::new(
                "request processing failed",
            ),),)
        );

        assert!(!downstream_called);
    }

    #[test]
    fn downstream_failure_is_preserved() {
        let chain = MiddlewareChain::new();

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Err::<UniversalResponse, _>("downstream failure")
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Downstream("downstream failure",),)
        );
    }

    #[test]
    fn response_middleware_failure_stops_response_processing() {
        let events = Arc::new(Mutex::new(Vec::new()));

        #[derive(Debug)]
        struct FirstResponseMiddleware {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for FirstResponseMiddleware {
            fn on_response(
                &self,
                _context: &EngineContext,
                _request: &UniversalRequest,
                _response: &mut UniversalResponse,
            ) -> Result<(), MiddlewareError> {
                self.events.lock().unwrap().push("first");

                Ok(())
            }
        }

        #[derive(Debug)]
        struct SecondResponseMiddleware {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for SecondResponseMiddleware {
            fn on_response(
                &self,
                _context: &EngineContext,
                _request: &UniversalRequest,
                _response: &mut UniversalResponse,
            ) -> Result<(), MiddlewareError> {
                self.events.lock().unwrap().push("second");

                Err(MiddlewareError::new("second response failure"))
            }
        }

        #[derive(Debug)]
        struct ThirdResponseMiddleware {
            events: Arc<Mutex<Vec<&'static str>>>,
        }

        impl Middleware for ThirdResponseMiddleware {
            fn on_response(
                &self,
                _context: &EngineContext,
                _request: &UniversalRequest,
                _response: &mut UniversalResponse,
            ) -> Result<(), MiddlewareError> {
                self.events.lock().unwrap().push("third");

                Ok(())
            }
        }

        let mut chain = MiddlewareChain::new();

        chain.push(FirstResponseMiddleware {
            events: Arc::clone(&events),
        });

        chain.push(SecondResponseMiddleware {
            events: Arc::clone(&events),
        });

        chain.push(ThirdResponseMiddleware {
            events: Arc::clone(&events),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Middleware(MiddlewareError::new(
                "second response failure",
            ),),)
        );

        assert_eq!(*events.lock().unwrap(), vec!["third", "second",]);
    }

    #[test]
    fn invalid_context_after_request_middleware_stops_downstream() {
        #[derive(Debug)]
        struct CancellingMiddleware;

        impl Middleware for CancellingMiddleware {
            fn on_request(
                &self,
                context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                context.cancellation().cancel();
                MiddlewareResult::Continue
            }
        }

        let chain = MiddlewareChain::with_stage(CancellingMiddleware);

        let mut request = request();
        let mut context = context();
        let mut downstream_called = false;

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Context(
                crate::runtime::PipelineError::Cancelled
            ))
        );
        assert!(!downstream_called);
    }

    #[test]
    fn expired_context_after_request_middleware_stops_downstream() {
        #[derive(Debug)]
        struct ExpiringMiddleware;

        impl Middleware for ExpiringMiddleware {
            fn on_request(
                &self,
                context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                *context = context
                    .clone()
                    .with_deadline(Deadline::from_now(std::time::Duration::ZERO).unwrap());
                MiddlewareResult::Continue
            }
        }

        let chain = MiddlewareChain::with_stage(ExpiringMiddleware);

        let mut request = request();
        let mut context = context();
        let mut downstream_called = false;

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(MiddlewareChainError::Context(
                crate::runtime::PipelineError::DeadlineExpired
            ))
        );
        assert!(!downstream_called);
    }

    #[test]
    fn request_mutation_is_visible_to_downstream() {
        let chain = MiddlewareChain::with_stage(RecordingRequestMutationMiddleware);

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, request| {
            assert_eq!(request.envelope.message_id.as_str(), "mutated-request");

            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());
    }

    #[test]
    fn response_is_passed_through_unchanged_when_no_response_middleware_modifies_it() {
        let chain = MiddlewareChain::new();

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        let response = result.unwrap();

        assert_eq!(response.status, Status::Success);

        assert!(response.has_response_interaction());
    }

    #[test]
    fn response_middleware_receives_downstream_response() {
        #[derive(Debug)]
        struct ResponseObserver {
            observed_status: Arc<Mutex<Option<Status>>>,
        }

        impl Middleware for ResponseObserver {
            fn on_response(
                &self,
                _context: &EngineContext,
                _request: &UniversalRequest,
                response: &mut UniversalResponse,
            ) -> Result<(), MiddlewareError> {
                *self.observed_status.lock().unwrap() = Some(response.status);

                Ok(())
            }
        }

        let observed_status = Arc::new(Mutex::new(None));

        let mut chain = MiddlewareChain::new();

        chain.push(ResponseObserver {
            observed_status: Arc::clone(&observed_status),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(*observed_status.lock().unwrap(), Some(Status::Success));
    }

    #[test]
    fn request_and_response_processing_use_the_same_request_context() {
        #[derive(Debug)]
        struct ContextObserver {
            request_operation: Arc<Mutex<Option<String>>>,
            response_operation: Arc<Mutex<Option<String>>>,
        }

        impl Middleware for ContextObserver {
            fn on_request(
                &self,
                context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                *self.request_operation.lock().unwrap() =
                    Some(context.operation().operation.id.to_string());

                MiddlewareResult::Continue
            }

            fn on_response(
                &self,
                context: &EngineContext,
                _request: &UniversalRequest,
                _response: &mut UniversalResponse,
            ) -> Result<(), MiddlewareError> {
                *self.response_operation.lock().unwrap() =
                    Some(context.operation().operation.id.to_string());

                Ok(())
            }
        }

        let request_operation = Arc::new(Mutex::new(None));

        let response_operation = Arc::new(Mutex::new(None));

        let mut chain = MiddlewareChain::new();

        chain.push(ContextObserver {
            request_operation: Arc::clone(&request_operation),
            response_operation: Arc::clone(&response_operation),
        });

        let mut request = request();
        let mut context = context();

        let result = chain.execute(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(
            request_operation.lock().unwrap().as_deref(),
            Some("middleware-chain-operation")
        );

        assert_eq!(
            response_operation.lock().unwrap().as_deref(),
            Some("middleware-chain-operation")
        );
    }

    #[test]
    fn downstream_receives_the_same_engine_context() {
        let chain = MiddlewareChain::new();

        let mut request = request();
        let mut context = context();

        let result = chain.execute(
            &mut context,
            &mut request,
            |downstream_context, _request| {
                assert_eq!(
                    downstream_context.operation().operation.id.as_str(),
                    "middleware-chain-operation"
                );

                Ok::<_, &'static str>(response())
            },
        );

        assert!(result.is_ok());
    }

    #[test]
    fn chain_can_be_shared_immutably() {
        fn assert_sync<T: Sync>() {}

        assert_sync::<MiddlewareChain>();
    }

    #[test]
    fn chain_debug_representation_exposes_stage_count() {
        let mut chain = MiddlewareChain::new();

        chain.push(RecordingRequestMutationMiddleware);

        chain.push(RejectingMiddleware);

        let debug = format!("{chain:?}");

        assert!(debug.contains("MiddlewareChain"));

        assert!(debug.contains("len"));
        assert!(debug.contains("2"));
    }
}
