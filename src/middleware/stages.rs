//! Generic middleware stage primitives for Nizaam Core.
//!
//! A middleware stage is a Core-controlled cross-cutting processing boundary.
//! It may inspect, validate, enrich, observe, authorize, reject, or fail a
//! request without owning the request's domain execution.
//!
//! This module intentionally contains no authentication-provider logic,
//! authorization policy, capability resolution, transport-specific behavior,
//! or engine-domain semantics.

use crate::{
    contracts::{UniversalRequest, UniversalResponse},
    runtime::EngineContext,
};
use core::fmt;

/// The result of request-side middleware processing.
///
/// `Continue` allows processing to proceed to the next middleware stage.
///
/// `Reject` indicates that the middleware intentionally refused the request.
/// Downstream middleware and capability execution must not proceed.
///
/// `Fail` indicates that the middleware itself could not complete its
/// processing. Downstream middleware and capability execution must not
/// proceed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MiddlewareResult {
    /// Continue processing the request.
    Continue,

    /// Intentionally reject the request.
    Reject(MiddlewareRejection),

    /// Middleware processing failed.
    Fail(MiddlewareError),
}

impl MiddlewareResult {
    /// Returns `true` when processing should continue.
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Returns `true` when the middleware intentionally rejected the request.
    pub fn is_reject(&self) -> bool {
        matches!(self, Self::Reject(_))
    }

    /// Returns `true` when middleware processing failed.
    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail(_))
    }

    /// Returns the rejection when this result represents a rejection.
    pub fn as_rejection(&self) -> Option<&MiddlewareRejection> {
        match self {
            Self::Reject(rejection) => Some(rejection),
            Self::Continue | Self::Fail(_) => None,
        }
    }

    /// Returns the middleware error when this result represents a failure.
    pub fn as_error(&self) -> Option<&MiddlewareError> {
        match self {
            Self::Fail(error) => Some(error),
            Self::Continue | Self::Reject(_) => None,
        }
    }
}

/// An intentional middleware rejection.
///
/// A rejection is not itself a middleware implementation failure. It means
/// middleware completed its evaluation and deliberately prevented downstream
/// processing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiddlewareRejection {
    reason: String,
}

impl MiddlewareRejection {
    /// Creates a middleware rejection with a human-readable reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Returns the rejection reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for MiddlewareRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for MiddlewareRejection {}

/// A failure produced while middleware is processing a request or response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiddlewareError {
    reason: String,
}

impl MiddlewareError {
    /// Creates a middleware processing error with a human-readable reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Returns the error reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for MiddlewareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for MiddlewareError {}

/// Core-controlled middleware stage.
///
/// Middleware may participate in request processing, response processing, or
/// both.
///
/// The default implementation is a no-op:
///
/// - [`Middleware::on_request`] returns [`MiddlewareResult::Continue`].
/// - [`Middleware::on_response`] leaves the response unchanged.
///
/// Request middleware receives a mutable reference to the existing
/// [`EngineContext`] so that trusted request-scoped state can be established
/// or enriched without creating a competing execution context.
///
/// Middleware must not replace the existing cancellation, deadline, operation,
/// or other Core context mechanisms with independent mechanisms.
pub trait Middleware: Send + Sync {
    /// Processes the request before downstream execution.
    ///
    /// The context is mutable because request middleware may establish
    /// request-scoped Core state, such as a trusted security context.
    ///
    /// Returning anything other than [`MiddlewareResult::Continue`] stops
    /// downstream middleware and capability execution.
    fn on_request(
        &self,
        _context: &mut EngineContext,
        _request: &mut UniversalRequest,
    ) -> MiddlewareResult {
        MiddlewareResult::Continue
    }

    /// Processes the response after downstream execution.
    ///
    /// Response processing is only reached when request-side middleware and
    /// downstream execution have successfully produced a response.
    ///
    /// Returning an error stops further response middleware processing.
    fn on_response(
        &self,
        _context: &EngineContext,
        _request: &UniversalRequest,
        _response: &mut UniversalResponse,
    ) -> Result<(), MiddlewareError> {
        Ok(())
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
        status::Status,
    };
    use std::sync::{Arc, Mutex};

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("middleware-operation").unwrap(),
            CorrelationId::new("middleware-correlation").unwrap(),
        )))
    }

    fn envelope(interaction: Interaction, message_id: &str) -> MessageEnvelope {
        let descriptor = ContractDescriptor::new(
            ContractId::new("middleware.contract").unwrap(),
            CapabilityId::new("middleware.test").unwrap(),
            Version::new(1, 0, 0),
            interaction,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(
                EngineId::new("middleware-sender").unwrap(),
                EngineId::new("middleware-receiver").unwrap(),
            ),
        );

        let operation = OperationContext::new(Operation::new(
            OperationId::new("middleware-operation").unwrap(),
            CorrelationId::new("middleware-correlation").unwrap(),
        ));

        MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            operation,
            metadata,
            EncodedPayload::new(descriptor.payload, b"middleware payload"),
        )
    }

    fn request() -> UniversalRequest {
        UniversalRequest::new(envelope(Interaction::Request, "middleware-request"))
    }

    fn response() -> UniversalResponse {
        UniversalResponse::new(
            envelope(Interaction::Response, "middleware-response"),
            Status::Success,
        )
    }

    #[derive(Debug)]
    struct RecordingMiddleware {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Middleware for RecordingMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            self.calls.lock().unwrap().push("request");
            MiddlewareResult::Continue
        }

        fn on_response(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
            _response: &mut UniversalResponse,
        ) -> Result<(), MiddlewareError> {
            self.calls.lock().unwrap().push("response");
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
            MiddlewareResult::Reject(MiddlewareRejection::new("request rejected by middleware"))
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
            MiddlewareResult::Fail(MiddlewareError::new("middleware processing failed"))
        }
    }

    #[derive(Debug)]
    struct ResponseFailingMiddleware;

    impl Middleware for ResponseFailingMiddleware {
        fn on_response(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
            _response: &mut UniversalResponse,
        ) -> Result<(), MiddlewareError> {
            Err(MiddlewareError::new(
                "response middleware processing failed",
            ))
        }
    }

    #[derive(Debug)]
    struct RequestOnlyMiddleware;

    impl Middleware for RequestOnlyMiddleware {
        fn on_request(
            &self,
            _context: &mut EngineContext,
            _request: &mut UniversalRequest,
        ) -> MiddlewareResult {
            MiddlewareResult::Continue
        }
    }

    #[derive(Debug)]
    struct ResponseOnlyMiddleware;

    impl Middleware for ResponseOnlyMiddleware {
        fn on_response(
            &self,
            _context: &EngineContext,
            _request: &UniversalRequest,
            _response: &mut UniversalResponse,
        ) -> Result<(), MiddlewareError> {
            Ok(())
        }
    }

    #[test]
    fn continue_result_is_reported_correctly() {
        let result = MiddlewareResult::Continue;

        assert!(result.is_continue());
        assert!(!result.is_reject());
        assert!(!result.is_fail());
        assert!(result.as_rejection().is_none());
        assert!(result.as_error().is_none());
    }

    #[test]
    fn rejection_result_is_reported_correctly() {
        let rejection = MiddlewareRejection::new("not allowed");
        let result = MiddlewareResult::Reject(rejection.clone());

        assert!(!result.is_continue());
        assert!(result.is_reject());
        assert!(!result.is_fail());
        assert_eq!(result.as_rejection(), Some(&rejection));
        assert!(result.as_error().is_none());
    }

    #[test]
    fn failure_result_is_reported_correctly() {
        let error = MiddlewareError::new("processing failed");
        let result = MiddlewareResult::Fail(error.clone());

        assert!(!result.is_continue());
        assert!(!result.is_reject());
        assert!(result.is_fail());
        assert!(result.as_rejection().is_none());
        assert_eq!(result.as_error(), Some(&error));
    }

    #[test]
    fn rejection_and_failure_are_distinct() {
        let rejection = MiddlewareResult::Reject(MiddlewareRejection::new("rejected"));

        let failure = MiddlewareResult::Fail(MiddlewareError::new("failed"));

        assert_ne!(rejection, failure);
    }

    #[test]
    fn rejection_preserves_reason() {
        let rejection = MiddlewareRejection::new("authorization denied");

        assert_eq!(rejection.reason(), "authorization denied");
        assert_eq!(rejection.to_string(), "authorization denied");
    }

    #[test]
    fn middleware_error_preserves_reason() {
        let error = MiddlewareError::new("backend unavailable");

        assert_eq!(error.reason(), "backend unavailable");
        assert_eq!(error.to_string(), "backend unavailable");
    }

    #[test]
    fn default_middleware_continues_request_processing() {
        struct NoOpMiddleware;

        impl Middleware for NoOpMiddleware {}

        let middleware = NoOpMiddleware;
        let mut request = request();
        let mut context = context();

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(result, MiddlewareResult::Continue);
    }

    #[test]
    fn default_middleware_allows_response_processing() {
        struct NoOpMiddleware;

        impl Middleware for NoOpMiddleware {}

        let middleware = NoOpMiddleware;
        let request = request();
        let mut response = response();
        let context = context();

        assert!(
            middleware
                .on_response(&context, &request, &mut response)
                .is_ok()
        );
    }

    #[test]
    fn middleware_can_process_request_and_response() {
        let calls = Arc::new(Mutex::new(Vec::new()));

        let middleware = RecordingMiddleware {
            calls: Arc::clone(&calls),
        };

        let mut request = request();
        let mut context = context();

        let request_result = middleware.on_request(&mut context, &mut request);

        assert_eq!(request_result, MiddlewareResult::Continue);

        let mut response = response();

        middleware
            .on_response(&context, &request, &mut response)
            .unwrap();

        assert_eq!(*calls.lock().unwrap(), vec!["request", "response"]);
    }

    #[test]
    fn middleware_can_be_request_only() {
        let middleware = RequestOnlyMiddleware;
        let mut request = request();
        let mut context = context();

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );

        let mut response = response();

        assert!(
            middleware
                .on_response(&context, &request, &mut response)
                .is_ok()
        );
    }

    #[test]
    fn middleware_can_be_response_only() {
        let middleware = ResponseOnlyMiddleware;
        let mut request = request();
        let mut context = context();

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );

        let mut response = response();

        assert!(
            middleware
                .on_response(&context, &request, &mut response)
                .is_ok()
        );
    }

    #[test]
    fn rejecting_middleware_returns_reject_result() {
        let middleware = RejectingMiddleware;
        let mut request = request();
        let mut context = context();

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Reject(MiddlewareRejection::new("request rejected by middleware",))
        );
    }

    #[test]
    fn failing_middleware_returns_fail_result() {
        let middleware = FailingMiddleware;
        let mut request = request();
        let mut context = context();

        let result = middleware.on_request(&mut context, &mut request);

        assert_eq!(
            result,
            MiddlewareResult::Fail(MiddlewareError::new("middleware processing failed",))
        );
    }

    #[test]
    fn response_middleware_can_fail() {
        let middleware = ResponseFailingMiddleware;
        let request = request();
        let mut response = response();
        let context = context();

        let result = middleware.on_response(&context, &request, &mut response);

        assert_eq!(
            result,
            Err(MiddlewareError::new(
                "response middleware processing failed",
            ))
        );
    }

    #[test]
    fn middleware_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<RecordingMiddleware>();
        assert_send_sync::<RejectingMiddleware>();
        assert_send_sync::<FailingMiddleware>();
        assert_send_sync::<ResponseFailingMiddleware>();
        assert_send_sync::<RequestOnlyMiddleware>();
        assert_send_sync::<ResponseOnlyMiddleware>();
    }

    #[test]
    fn middleware_can_read_existing_engine_context() {
        struct ContextReadingMiddleware;

        impl Middleware for ContextReadingMiddleware {
            fn on_request(
                &self,
                context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                assert_eq!(
                    context.operation().operation.id.as_str(),
                    "middleware-operation"
                );

                MiddlewareResult::Continue
            }
        }

        let middleware = ContextReadingMiddleware;
        let mut request = request();
        let mut context = context();

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );
    }

    #[test]
    fn middleware_can_be_used_as_trait_object() {
        let middleware: Box<dyn Middleware> = Box::new(RequestOnlyMiddleware);

        let mut request = request();
        let mut context = context();

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );
    }

    #[test]
    fn middleware_receives_mutable_existing_engine_context() {
        #[derive(Debug)]
        struct ContextAwareMiddleware {
            observed_operation: Arc<Mutex<Option<String>>>,
        }

        impl Middleware for ContextAwareMiddleware {
            fn on_request(
                &self,
                context: &mut EngineContext,
                _request: &mut UniversalRequest,
            ) -> MiddlewareResult {
                *self.observed_operation.lock().unwrap() =
                    Some(context.operation().operation.id.to_string());

                MiddlewareResult::Continue
            }
        }

        let observed_operation = Arc::new(Mutex::new(None));

        let middleware = ContextAwareMiddleware {
            observed_operation: Arc::clone(&observed_operation),
        };

        let mut request = request();
        let mut context = context();

        assert_eq!(
            middleware.on_request(&mut context, &mut request),
            MiddlewareResult::Continue
        );

        assert_eq!(
            observed_operation.lock().unwrap().as_deref(),
            Some("middleware-operation")
        );
    }

    #[test]
    fn request_rejection_is_distinct_from_processing_failure() {
        let rejecting = RejectingMiddleware;
        let failing = FailingMiddleware;

        let mut rejecting_request = request();
        let mut failing_request = request();
        let mut rejecting_context = context();
        let mut failing_context = context();

        let rejection = rejecting.on_request(&mut rejecting_context, &mut rejecting_request);

        let failure = failing.on_request(&mut failing_context, &mut failing_request);

        assert!(rejection.is_reject());
        assert!(failure.is_fail());
        assert_ne!(rejection, failure);
    }
}
