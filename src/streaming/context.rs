//! Execution context associated with an application-level stream.
//!
//! `StreamContext` is a thin streaming-facing wrapper around the established
//! [`crate::runtime::EngineContext`]. It derives a child execution scope so
//! parent cancellation reaches the stream while stream-local cancellation
//! remains isolated from the parent and sibling scopes.
//!
//! The context deliberately does not introduce separate streaming versions of
//! operation identity, cancellation, deadlines, security, provenance, or
//! configuration.

use crate::{
    config::snapshot::ConfigurationSnapshot,
    operation::OperationContext,
    provenance::ProvenanceContext,
    runtime::{CancellationToken, Deadline, EngineContext},
    security::SecurityContext,
};

/// Stream-scoped execution context derived from an established engine context.
///
/// The contained [`EngineContext`] is a child context created with
/// [`EngineContext::child`]. It preserves the parent operation, deadline,
/// security context, provenance, and configuration snapshot while using a
/// child cancellation token.
#[derive(Clone, Debug)]
pub struct StreamContext {
    engine: EngineContext,
}

impl StreamContext {
    /// Creates a stream context from an already-established engine context.
    ///
    /// This derives a child context rather than constructing a new root
    /// context, preserving the established execution boundary and creating
    /// stream-local cancellation isolation.
    pub fn from_engine_context(context: &EngineContext) -> Self {
        Self {
            engine: context.child(),
        }
    }

    /// Returns the underlying established engine context.
    pub fn engine_context(&self) -> &EngineContext {
        &self.engine
    }

    /// Returns the operation execution context associated with the stream.
    pub fn operation(&self) -> &OperationContext {
        self.engine.operation()
    }

    /// Returns the stream's child cancellation token.
    pub fn cancellation(&self) -> &CancellationToken {
        self.engine.cancellation()
    }

    /// Returns the inherited effective deadline, when one exists.
    pub fn deadline(&self) -> Option<Deadline> {
        self.engine.deadline()
    }

    /// Returns the trusted security context, when authentication established one.
    pub fn security(&self) -> Option<&SecurityContext> {
        self.engine.security()
    }

    /// Returns the established provenance context.
    pub fn provenance(&self) -> &ProvenanceContext {
        self.engine.provenance()
    }

    /// Returns the immutable configuration snapshot attached to the context.
    pub fn configuration(&self) -> Option<&ConfigurationSnapshot> {
        self.engine.configuration()
    }

    /// Returns whether the stream context or one of its ancestors is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.engine.cancellation().is_cancelled()
    }

    /// Returns whether the inherited deadline has expired.
    pub fn is_expired(&self) -> bool {
        self.engine.is_expired()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::{
        config::{
            resolution::ConfigurationResolver,
            snapshot::{ConfigurationSnapshot, ConfigurationSnapshotId},
            validation::{ConfigurationValidator, ConfigurationValue, ParsedConfiguration},
        },
        identity::{CorrelationId, OperationId},
        operation::{Deadline, Operation, OperationContext},
        provenance::ProvenanceContext,
        runtime::EngineContext,
        security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext},
    };

    use super::StreamContext;

    fn sample_operation() -> Operation {
        Operation::new(
            OperationId::new("stream-op-1").unwrap(),
            CorrelationId::new("stream-corr-1").unwrap(),
        )
    }

    fn sample_context() -> EngineContext {
        EngineContext::new(OperationContext::new(sample_operation()))
    }

    fn sample_security() -> SecurityContext {
        let principal = PrincipalIdentity::new(
            PrincipalType::User,
            PrincipalId::new("stream-user-1").unwrap(),
        );

        SecurityContext::new(principal, None)
    }

    fn sample_configuration() -> Arc<ConfigurationSnapshot> {
        let mut parsed = ParsedConfiguration::empty();
        parsed.insert("stream.test", ConfigurationValue::Boolean(true));

        let validated = ConfigurationValidator::new().validate(parsed).unwrap();
        let resolved = ConfigurationResolver::new().resolve(&validated).unwrap();

        Arc::new(ConfigurationSnapshot::new(
            ConfigurationSnapshotId::new(1),
            resolved,
        ))
    }

    fn sample_provenance() -> ProvenanceContext {
        ProvenanceContext::new().with_attribute("source", "stream-test")
    }

    #[test]
    fn derives_from_engine_context_without_changing_operation_identity() {
        let engine = sample_context();
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.operation(), engine.operation());
    }

    #[test]
    fn creates_child_cancellation_scope() {
        let engine = sample_context();
        let stream = StreamContext::from_engine_context(&engine);

        assert!(!engine.cancellation().is_cancelled());
        assert!(!stream.is_cancelled());

        stream.cancellation().cancel();

        assert!(stream.is_cancelled());
        assert!(!engine.cancellation().is_cancelled());
    }

    #[test]
    fn parent_cancellation_reaches_stream_context() {
        let engine = sample_context();
        let stream = StreamContext::from_engine_context(&engine);

        engine.cancellation().cancel();

        assert!(stream.is_cancelled());
    }

    #[test]
    fn stream_cancellation_does_not_affect_sibling_contexts() {
        let engine = sample_context();
        let first = StreamContext::from_engine_context(&engine);
        let second = StreamContext::from_engine_context(&engine);

        first.cancellation().cancel();

        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        assert!(!engine.cancellation().is_cancelled());
    }

    #[test]
    fn preserves_deadline() {
        let deadline = Deadline::from_now(Duration::from_secs(30)).unwrap();
        let engine = sample_context().with_deadline(deadline);
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.deadline(), Some(deadline));
        assert!(!stream.is_expired());
    }

    #[test]
    fn preserves_security_context() {
        let security = sample_security();
        let engine = sample_context().with_security(security.clone());
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.security(), Some(&security));
    }

    #[test]
    fn preserves_provenance_context() {
        let provenance = sample_provenance();
        let engine = sample_context().with_provenance(provenance.clone());
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.provenance(), &provenance);
    }

    #[test]
    fn preserves_configuration_snapshot() {
        let configuration = sample_configuration();
        let engine = sample_context().with_configuration(configuration.clone());
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.configuration(), Some(configuration.as_ref()));
    }

    #[test]
    fn exposes_the_established_engine_context() {
        let engine = sample_context();
        let stream = StreamContext::from_engine_context(&engine);

        assert_eq!(stream.engine_context().operation(), engine.operation());
    }

    #[test]
    fn clone_shares_the_same_stream_cancellation_scope() {
        let stream = StreamContext::from_engine_context(&sample_context());
        let clone = stream.clone();

        stream.cancellation().cancel();

        assert!(clone.is_cancelled());
    }

    #[test]
    fn missing_optional_contexts_remain_absent() {
        let stream = StreamContext::from_engine_context(&sample_context());

        assert!(stream.deadline().is_none());
        assert!(stream.security().is_none());
        assert!(stream.configuration().is_none());
    }

    #[test]
    fn correlation_identity_is_preserved_through_operation_context() {
        let stream = StreamContext::from_engine_context(&sample_context());

        assert_eq!(
            stream.operation().operation.correlation_id,
            CorrelationId::new("stream-corr-1").unwrap()
        );
    }

    #[test]
    fn stream_context_does_not_create_a_new_operation_identity() {
        let operation_id = sample_operation().id;
        let stream = StreamContext::from_engine_context(&sample_context());

        assert_eq!(stream.operation().operation.id, operation_id);
    }
}
