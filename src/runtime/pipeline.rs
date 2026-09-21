use super::{EngineContext, check_context};

use crate::{
    contracts::{
        UniversalRequest, UniversalResponse,
        validation::{self, ValidationError},
    },
    middleware::chain::{MiddlewareChain, MiddlewareChainError},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineError {
    Cancelled,
    DeadlineExpired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineConfigurationError {
    MandatoryMiddlewareNotConfigured,
}

#[derive(Debug, PartialEq)]
pub enum RequestPipelineError<E> {
    Context(PipelineError),
    Validation(ValidationError),
    Configuration(PipelineConfigurationError),
    Middleware(MiddlewareChainError<E>),
}

pub type PipelineStage = Box<dyn Fn(&EngineContext) -> Result<(), PipelineError> + Send + Sync>;

/// Runs context checks before each registered execution stage and provides the
/// mandatory middleware boundary for request execution.
#[derive(Default)]
pub struct ExecutionPipeline {
    stages: Vec<PipelineStage>,
    middleware: MiddlewareChain,
}

impl ExecutionPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a context-aware execution stage.
    pub fn with_stage<F>(mut self, stage: F) -> Self
    where
        F: Fn(&EngineContext) -> Result<(), PipelineError> + Send + Sync + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    /// Adds a middleware stage to the mandatory request middleware chain.
    pub fn with_middleware<M>(mut self, middleware: M) -> Self
    where
        M: crate::middleware::stages::Middleware + 'static,
    {
        self.middleware.push(middleware);
        self
    }

    /// Returns the configured middleware chain.
    pub fn middleware(&self) -> &MiddlewareChain {
        &self.middleware
    }

    /// Returns the number of registered execution stages.
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Returns the number of registered middleware stages.
    pub fn middleware_count(&self) -> usize {
        self.middleware.len()
    }

    /// Runs the existing context-aware execution stages.
    pub fn run(&self, context: &EngineContext) -> Result<(), PipelineError> {
        for stage in &self.stages {
            check_context(context)?;
            stage(context)?;
        }

        check_context(context)
    }

    /// Runs a request through the mandatory middleware boundary and then the
    /// supplied downstream execution function.
    ///
    /// Request middleware executes before downstream work.
    /// Response middleware executes after successful downstream work.
    ///
    /// Context cancellation and deadline failures are preserved separately from
    /// middleware and downstream failures.
    pub fn run_request<F, E>(
        &self,
        context: &mut EngineContext,
        request: &mut UniversalRequest,
        downstream: F,
    ) -> Result<UniversalResponse, RequestPipelineError<E>>
    where
        F: FnOnce(&EngineContext, &mut UniversalRequest) -> Result<UniversalResponse, E>,
    {
        self.run(context).map_err(RequestPipelineError::Context)?;

        validation::validate_request(request).map_err(RequestPipelineError::Validation)?;

        if self.middleware.is_empty() {
            return Err(RequestPipelineError::Configuration(
                PipelineConfigurationError::MandatoryMiddlewareNotConfigured,
            ));
        }

        match self.middleware.execute(context, request, downstream) {
            Ok(response) => Ok(response),
            Err(MiddlewareChainError::Context(error)) => Err(RequestPipelineError::Context(error)),
            Err(error) => Err(RequestPipelineError::Middleware(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionPipeline, PipelineConfigurationError, PipelineError, RequestPipelineError,
    };

    use crate::{
        contracts::{
            UniversalRequest, UniversalResponse,
            descriptor::{
                ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
            },
            envelope::MessageEnvelope,
            metadata::{ContractMetadata, Participants},
            validation::ValidationError,
        },
        identity::{CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId},
        middleware::{
            chain::MiddlewareChainError,
            stages::{Middleware, MiddlewareError, MiddlewareRejection, MiddlewareResult},
        },
        operation::{Operation, OperationContext},
        runtime::{Deadline, EngineContext},
        status::Status,
    };

    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("pipeline-operation").unwrap(),
            CorrelationId::new("pipeline-correlation").unwrap(),
        )))
    }

    fn request() -> UniversalRequest {
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

        let descriptor = ContractDescriptor::new(
            ContractId::new("pipeline.contract").unwrap(),
            CapabilityId::new("pipeline.test").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            payload_descriptor.clone(),
        );

        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("pipeline-sender").unwrap(),
                EngineId::new("pipeline-receiver").unwrap(),
            ),
        );

        UniversalRequest::new(MessageEnvelope::new(
            MessageId::new("pipeline-request").unwrap(),
            OperationContext::new(Operation::new(
                OperationId::new("pipeline-operation").unwrap(),
                CorrelationId::new("pipeline-correlation").unwrap(),
            )),
            metadata,
            EncodedPayload::new(payload_descriptor, b"pipeline payload"),
        ))
    }

    fn response() -> UniversalResponse {
        let payload_descriptor =
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();

        let descriptor = ContractDescriptor::new(
            ContractId::new("pipeline.contract").unwrap(),
            CapabilityId::new("pipeline.test").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Response,
            payload_descriptor.clone(),
        );

        let metadata = ContractMetadata::new(
            descriptor,
            Participants::new(
                EngineId::new("pipeline-sender").unwrap(),
                EngineId::new("pipeline-receiver").unwrap(),
            ),
        );

        UniversalResponse::new(
            MessageEnvelope::new(
                MessageId::new("pipeline-response").unwrap(),
                OperationContext::new(Operation::new(
                    OperationId::new("pipeline-operation").unwrap(),
                    CorrelationId::new("pipeline-correlation").unwrap(),
                )),
                metadata,
                EncodedPayload::new(payload_descriptor, b"pipeline response"),
            ),
            Status::Success,
        )
    }

    #[test]
    fn empty_pipeline_succeeds() {
        let pipeline = ExecutionPipeline::new();

        assert!(pipeline.run(&context()).is_ok());
    }

    #[test]
    fn pipeline_runs_stages_in_order() {
        let order = Arc::new(Mutex::new(Vec::new()));

        let first_order = Arc::clone(&order);

        let second_order = Arc::clone(&order);

        let pipeline = ExecutionPipeline::new()
            .with_stage(move |_| {
                first_order.lock().unwrap().push(1);

                Ok(())
            })
            .with_stage(move |_| {
                second_order.lock().unwrap().push(2);

                Ok(())
            });

        pipeline.run(&context()).unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn pipeline_stops_before_cancelled_stage() {
        let context = context();

        context.cancellation().cancel();

        let pipeline = ExecutionPipeline::new().with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context), Err(PipelineError::Cancelled));
    }

    #[test]
    fn pipeline_short_circuits_when_deadline_is_expired() {
        let context = context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

        let pipeline = ExecutionPipeline::new().with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context), Err(PipelineError::DeadlineExpired));
    }

    #[test]
    fn pipeline_stops_after_stage_returns_error() {
        let pipeline = ExecutionPipeline::new()
            .with_stage(|_| Err(PipelineError::Cancelled))
            .with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context()), Err(PipelineError::Cancelled));
    }

    #[test]
    fn pipeline_checks_context_between_stages() {
        let order = Arc::new(Mutex::new(Vec::new()));

        let first_order = Arc::clone(&order);

        let context = context();

        let context_for_stage = context.cancellation().clone();

        let pipeline = ExecutionPipeline::new()
            .with_stage(move |_| {
                first_order.lock().unwrap().push(1);

                Ok(())
            })
            .with_stage(move |_| {
                context_for_stage.cancel();

                Ok(())
            })
            .with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context), Err(PipelineError::Cancelled));

        assert_eq!(*order.lock().unwrap(), vec![1]);
    }

    #[test]
    fn pipeline_error_variants_are_distinct() {
        assert_ne!(PipelineError::Cancelled, PipelineError::DeadlineExpired);
    }

    #[test]
    fn pipeline_rejects_when_deadline_expired_before_first_stage() {
        let context = context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

        let pipeline = ExecutionPipeline::new().with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context), Err(PipelineError::DeadlineExpired));
    }

    #[test]
    fn request_pipeline_executes_registered_stages_before_middleware() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let stage_events = Arc::clone(&events);

        let middleware_events = Arc::clone(&events);

        let pipeline = ExecutionPipeline::new()
            .with_stage(move |_| {
                stage_events.lock().unwrap().push("stage");
                Ok(())
            })
            .with_middleware(RecordingMiddleware {
                events: middleware_events,
            });

        let mut context = context();
        let mut request = request();

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            events.lock().unwrap().push("downstream");
            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());
        assert_eq!(
            *events.lock().unwrap(),
            vec!["stage", "request", "downstream", "response"]
        );
    }

    #[test]
    fn pipeline_allows_multiple_stages_after_context_check() {
        let order = Arc::new(Mutex::new(Vec::new()));

        let first_order = Arc::clone(&order);

        let second_order = Arc::clone(&order);

        let third_order = Arc::clone(&order);

        let pipeline = ExecutionPipeline::new()
            .with_stage(move |_| {
                first_order.lock().unwrap().push(1);

                Ok(())
            })
            .with_stage(move |_| {
                second_order.lock().unwrap().push(2);

                Ok(())
            })
            .with_stage(move |_| {
                third_order.lock().unwrap().push(3);

                Ok(())
            });

        pipeline.run(&context()).unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2, 3]);
    }

    #[derive(Debug)]
    struct RecordingMiddleware {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Middleware for RecordingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            self.events.lock().unwrap().push("request");

            MiddlewareResult::Continue
        }

        fn on_response(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
            _response: &mut UniversalResponse,
        ) -> Result<(), MiddlewareError> {
            self.events.lock().unwrap().push("response");

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

    #[test]
    fn middleware_is_exposed_by_execution_pipeline() {
        let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
            events: Arc::new(Mutex::new(Vec::new())),
        });

        assert_eq!(pipeline.middleware_count(), 1);

        assert_eq!(pipeline.middleware().len(), 1);
    }

    #[test]
    fn request_pipeline_executes_middleware_before_and_after_downstream() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
            events: Arc::clone(&events),
        });

        let mut context = context();

        let mut request = request();

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            events.lock().unwrap().push("downstream");

            Ok::<_, &'static str>(response())
        });

        assert!(result.is_ok());

        assert_eq!(
            *events.lock().unwrap(),
            vec!["request", "downstream", "response",]
        );
    }

    #[test]
    fn request_pipeline_stops_when_middleware_rejects() {
        let pipeline = ExecutionPipeline::new().with_middleware(RejectingMiddleware);

        let mut context = context();

        let mut request = request();

        let mut downstream_called = false;

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            downstream_called = true;

            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Middleware(
                MiddlewareChainError::Rejected(MiddlewareRejection::new("request rejected",),),
            ),)
        );

        assert!(!downstream_called);
    }

    #[test]
    fn request_pipeline_requires_mandatory_middleware() {
        let pipeline = ExecutionPipeline::new();

        let mut context = context();
        let mut request = request();
        let mut downstream_called = false;

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            downstream_called = true;

            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Configuration(
                PipelineConfigurationError::MandatoryMiddlewareNotConfigured,
            ))
        );
        assert!(!downstream_called);
    }

    #[test]
    fn request_pipeline_rejects_structurally_invalid_request_before_middleware() {
        let events = Arc::new(Mutex::new(Vec::new()));

        let pipeline = ExecutionPipeline::new().with_middleware(RecordingMiddleware {
            events: Arc::clone(&events),
        });

        let mut context = context();
        let mut request = request();
        request.event.envelope.metadata.descriptor.interaction = Interaction::Response;

        let mut downstream_called = false;

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            downstream_called = true;
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Validation(
                ValidationError::InteractionMismatch,
            ))
        );
        assert!(!downstream_called);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn request_pipeline_preserves_cancellation_after_middleware() {
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

        let pipeline = ExecutionPipeline::new().with_middleware(CancellingMiddleware);

        let mut context = context();
        let mut request = request();

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Context(PipelineError::Cancelled))
        );
    }

    #[test]
    fn request_pipeline_preserves_cancellation_error() {
        let pipeline = ExecutionPipeline::new();

        let mut context = context();

        context.cancellation().cancel();

        let mut request = request();

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Context(PipelineError::Cancelled,),)
        );
    }

    #[test]
    fn request_pipeline_preserves_deadline_error() {
        let pipeline = ExecutionPipeline::new();

        let mut context = context().with_deadline(Deadline::from_now(Duration::ZERO).unwrap());

        let mut request = request();

        let result = pipeline.run_request(&mut context, &mut request, |_context, _request| {
            Ok::<_, &'static str>(response())
        });

        assert_eq!(
            result,
            Err(RequestPipelineError::Context(
                PipelineError::DeadlineExpired,
            ),)
        );
    }
}
