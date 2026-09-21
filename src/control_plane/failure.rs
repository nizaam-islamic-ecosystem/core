//! Control Plane failure classification and shared-result boundary.
//!
//! This module classifies failures that are owned by the Control Plane while
//! reusing the existing Core Error System for the actual technical error
//! contract, metadata, context, diagnostics, and causal references.
//!
//! The Control Plane deliberately does not absorb failures owned by transport,
//! the Engine Runtime, capability execution, cancellation, or deadlines.

use crate::error::GlobalError;

/// Machine-readable classification of a failure owned by the Control Plane.
///
/// The classification answers which Control Plane responsibility failed.
/// Technical error metadata and causal information remain owned by
/// [`GlobalError`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ControlPlaneFailureKind {
    /// The Control Plane rejected the interaction at its admission boundary.
    AdmissionFailure,

    /// Control-plane request or coordination metadata failed structural
    /// validation.
    ValidationFailure,

    /// A compatible contract or contract version could not be resolved.
    ContractResolutionFailure,

    /// The requested capability could not be resolved.
    CapabilityResolutionFailure,

    /// A compatible provider could not be resolved.
    ProviderResolutionFailure,

    /// No registered destination satisfied the eligibility requirements.
    NoEligibleDestination,

    /// A known or explicitly requested destination is unavailable at the
    /// Control Plane routing boundary.
    DestinationUnavailable,

    /// Routing policy evaluation could not produce a valid routing outcome.
    RoutingPolicyFailure,

    /// The Control Plane could not perform required fresh coordination or
    /// resolution.
    ControlPlaneUnavailable,

    /// Required execution context could not be resolved.
    ContextResolutionFailure,
}

/// A failure whose ownership belongs to the Control Plane.
///
/// `ControlPlaneFailure` adds only the Phase 15 failure classification.
/// The existing [`GlobalError`] remains authoritative for:
///
/// - error code and owner;
/// - version;
/// - class and severity;
/// - retryability;
/// - message and solution reference;
/// - operation/error context;
/// - diagnostic details;
/// - causal references.
///
/// This prevents Phase 15 from creating a second error system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlPlaneFailure {
    kind: ControlPlaneFailureKind,
    error: GlobalError,
}

impl ControlPlaneFailure {
    /// Creates a Control Plane failure from an explicit classification and
    /// an existing Core error.
    pub fn new(kind: ControlPlaneFailureKind, error: GlobalError) -> Self {
        Self { kind, error }
    }

    /// Returns the Control Plane-specific failure classification.
    pub fn kind(&self) -> ControlPlaneFailureKind {
        self.kind
    }

    /// Returns the underlying Core error without modifying its metadata or
    /// causal information.
    pub fn error(&self) -> &GlobalError {
        &self.error
    }

    /// Consumes the wrapper and returns the underlying Core error unchanged.
    pub fn into_error(self) -> GlobalError {
        self.error
    }
}

/// Standard result type used by Control Plane implementation modules.
pub type ControlPlaneResult<T> = Result<T, ControlPlaneFailure>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::error::{
        DiagnosticDetail, ErrorClass, ErrorCode, ErrorContext, ErrorOwner, ErrorReference, Severity,
    };
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::{Operation, OperationContext};
    use crate::status::Retryability;
    use std::collections::HashSet;

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("control-plane-test-operation").unwrap(),
            CorrelationId::new("control-plane-test-correlation").unwrap(),
        ))
    }

    fn global_error() -> GlobalError {
        GlobalError {
            code: ErrorCode::new("CORE.CONTROL_PLANE.TEST.001").unwrap(),
            owner: ErrorOwner::new("CORE").unwrap(),
            version: Version::new(1, 0, 0),
            class: ErrorClass::Contract,
            severity: Severity::Error,
            retryability: Retryability::NonRetryable,
            message: "control plane test failure".to_owned(),
            details: vec![
                DiagnosticDetail::new("stage", "resolution").expect("valid diagnostic detail"),
                DiagnosticDetail::new("component", "control-plane")
                    .expect("valid diagnostic detail"),
            ],
            solution_reference: Some("CORE.CONTROL_PLANE.TEST.HELP".to_owned()),
            context: ErrorContext::new(operation_context()),
            cause: ErrorReference::new("CORE.TEST.CAUSE.001"),
        }
    }

    #[test]
    fn all_control_plane_failure_kinds_are_distinct() {
        let kinds = [
            ControlPlaneFailureKind::AdmissionFailure,
            ControlPlaneFailureKind::ValidationFailure,
            ControlPlaneFailureKind::ContractResolutionFailure,
            ControlPlaneFailureKind::CapabilityResolutionFailure,
            ControlPlaneFailureKind::ProviderResolutionFailure,
            ControlPlaneFailureKind::NoEligibleDestination,
            ControlPlaneFailureKind::DestinationUnavailable,
            ControlPlaneFailureKind::RoutingPolicyFailure,
            ControlPlaneFailureKind::ControlPlaneUnavailable,
            ControlPlaneFailureKind::ContextResolutionFailure,
        ];

        let unique = kinds.into_iter().collect::<HashSet<_>>();

        assert_eq!(kinds.len(), 10);
        assert_eq!(unique.len(), 10);
    }

    #[test]
    fn failure_preserves_classification() {
        let failure = ControlPlaneFailure::new(
            ControlPlaneFailureKind::CapabilityResolutionFailure,
            global_error(),
        );

        assert_eq!(
            failure.kind(),
            ControlPlaneFailureKind::CapabilityResolutionFailure
        );
    }

    #[test]
    fn failure_preserves_global_error() {
        let error = global_error();
        let failure = ControlPlaneFailure::new(
            ControlPlaneFailureKind::ProviderResolutionFailure,
            error.clone(),
        );

        assert_eq!(failure.error(), &error);
    }

    #[test]
    fn into_error_returns_the_original_global_error() {
        let error = global_error();
        let expected = error.clone();
        let failure = ControlPlaneFailure::new(ControlPlaneFailureKind::ValidationFailure, error);

        assert_eq!(failure.into_error(), expected);
    }

    #[test]
    fn cause_is_preserved() {
        let error = global_error();
        let expected_cause = error.cause.clone();

        let failure =
            ControlPlaneFailure::new(ControlPlaneFailureKind::DestinationUnavailable, error);

        assert_eq!(failure.error().cause, expected_cause);
    }

    #[test]
    fn diagnostic_details_are_preserved() {
        let error = global_error();

        let failure =
            ControlPlaneFailure::new(ControlPlaneFailureKind::RoutingPolicyFailure, error);

        assert_eq!(
            failure.error().details,
            vec![
                DiagnosticDetail::new("stage", "resolution").unwrap(),
                DiagnosticDetail::new("component", "control-plane").unwrap(),
            ]
        );
    }

    #[test]
    fn context_is_preserved() {
        let error = global_error();

        let failure = ControlPlaneFailure::new(ControlPlaneFailureKind::AdmissionFailure, error);

        assert_eq!(
            failure.error().context.operation.operation.id.as_str(),
            "control-plane-test-operation"
        );
        assert_eq!(
            failure
                .error()
                .context
                .operation
                .operation
                .correlation_id
                .as_str(),
            "control-plane-test-correlation"
        );
    }

    #[test]
    fn error_metadata_is_preserved() {
        let error = global_error();

        let failure = ControlPlaneFailure::new(ControlPlaneFailureKind::ValidationFailure, error);

        assert_eq!(failure.error().code.as_str(), "CORE.CONTROL_PLANE.TEST.001");
        assert_eq!(failure.error().owner.as_str(), "CORE");
        assert_eq!(failure.error().version, Version::new(1, 0, 0));
        assert_eq!(failure.error().class, ErrorClass::Contract);
        assert_eq!(failure.error().severity, Severity::Error);
        assert_eq!(failure.error().retryability, Retryability::NonRetryable);
        assert_eq!(failure.error().message, "control plane test failure");
        assert_eq!(
            failure.error().solution_reference.as_deref(),
            Some("CORE.CONTROL_PLANE.TEST.HELP")
        );
    }

    #[test]
    fn failure_kind_does_not_change_global_error_metadata() {
        let error = global_error();

        let failure = ControlPlaneFailure::new(
            ControlPlaneFailureKind::ControlPlaneUnavailable,
            error.clone(),
        );

        assert_eq!(failure.error(), &error);
    }
}
