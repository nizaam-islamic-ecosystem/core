use nizaam_core::error::{
    ErrorClass, ErrorCode, ErrorContext, ErrorDefinition, ErrorOwner, ErrorSystem,
    GlobalError, ReportError, Severity,
};
use nizaam_core::prelude::*;

fn operation_context() -> OperationContext {
    OperationContext::new(Operation::new(
        OperationId::new("operation-3").unwrap(),
        CorrelationId::new("correlation-3").unwrap(),
    ))
}

fn definition() -> ErrorDefinition {
    ErrorDefinition::new(
        ErrorCode::new("CORE.CONTRACT.001").unwrap(),
        ErrorOwner::new("CORE").unwrap(),
        Version::new(1, 0, 0),
        ErrorClass::Contract,
        Severity::Error,
        "Contract rejected",
        Retryability::NonRetryable,
    )
    .unwrap()
}

#[test]
fn a_consumer_registers_and_reports_a_contextual_error() {
    let mut system = ErrorSystem::new();
    system.register(definition()).unwrap();
    let instance = system.instance();
    let cause = ErrorReference::new("CORE.TRANSPORT.001").unwrap();
    let context = ErrorContext::new(operation_context())
        .from_engine(EngineId::new("CORE").unwrap())
        .for_capability(CapabilityId::new("contract-validation").unwrap());

    let base = instance
        .report(&ErrorCode::new("CORE.CONTRACT.001").unwrap(), context)
        .unwrap();
    let event = instance
        .report_error(
            GlobalError {
                cause: Some(cause.clone()),
                ..base.error
            }
            .with_detail(
                nizaam_core::error::DiagnosticDetail::new("reason", "version mismatch").unwrap(),
            ),
        )
        .unwrap();

    assert_eq!(event.reference.as_str(), "CORE.CONTRACT.001");
    assert_eq!(event.error.context.operation.operation.id.as_str(), "operation-3");
    assert_eq!(event.error.cause, Some(cause));
    assert_eq!(event.error.details[0].key, "reason");
}

#[test]
fn reporting_requires_registered_definitions() {
    let system = ErrorSystem::new();
    let result = system.instance().report(
        &ErrorCode::new("CORE.CONTRACT.404").unwrap(),
        ErrorContext::new(operation_context()),
    );

    assert_eq!(
        result,
        Err(ReportError::Validation(
            nizaam_core::error::ValidationError::UnregisteredDefinition
        ))
    );
}

#[test]
fn reporting_rejects_definition_metadata_mismatches() {
    let mut system = ErrorSystem::new();
    system.register(definition()).unwrap();
    let instance = system.instance();
    let base = instance
        .report(
            &ErrorCode::new("CORE.CONTRACT.001").unwrap(),
            ErrorContext::new(operation_context()),
        )
        .unwrap();

    let mismatched = GlobalError {
        severity: Severity::Critical,
        ..base.error
    };

    assert_eq!(
        instance.report_error(mismatched),
        Err(ReportError::Validation(
            nizaam_core::error::ValidationError::DefinitionMetadataMismatch
        ))
    );
}

#[test]
fn reporting_rejects_empty_diagnostic_fields() {
    let mut system = ErrorSystem::new();
    system.register(definition()).unwrap();
    let instance = system.instance();
    let base = instance
        .report(
            &ErrorCode::new("CORE.CONTRACT.001").unwrap(),
            ErrorContext::new(operation_context()),
        )
        .unwrap();

    let invalid_detail = nizaam_core::error::DiagnosticDetail {
        key: " ".to_owned(),
        value: "version mismatch".to_owned(),
    };

    assert_eq!(
        instance.report_error(base.error.with_detail(invalid_detail)),
        Err(ReportError::Validation(
            nizaam_core::error::ValidationError::EmptyDiagnosticField
        ))
    );
}
