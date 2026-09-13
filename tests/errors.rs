use nizaam_core::error::{
    CatalogError, ErrorCatalog, ErrorClass, ErrorCode, ErrorContext, ErrorDefinition, ErrorOwner,
    ErrorSystem, GlobalError, InvalidDefinition, ReportError, Severity,
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
    assert_eq!(
        event.error.context.operation.operation.id.as_str(),
        "operation-3"
    );
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

#[test]
fn catalog_starts_empty() {
    let catalog = ErrorCatalog::new();
    assert!(catalog.is_empty());
    assert_eq!(catalog.len(), 0);
}

#[test]
fn catalog_rejects_duplicate_codes() {
    let mut catalog = ErrorCatalog::new();
    let code = ErrorCode::new("CORE.CATALOG.001").unwrap();

    catalog
        .register(
            nizaam_core::error::ErrorDefinition::new(
                code.clone(),
                nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
                Version::new(1, 0, 0),
                nizaam_core::error::ErrorClass::Contract,
                nizaam_core::error::Severity::Error,
                "Test error message".to_string(),
                nizaam_core::status::Retryability::NonRetryable,
            )
            .unwrap(),
        )
        .unwrap();

    let result = catalog.register(
        nizaam_core::error::ErrorDefinition::new(
            code,
            nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            nizaam_core::error::ErrorClass::Contract,
            nizaam_core::error::Severity::Error,
            "Duplicate definition".to_string(),
            nizaam_core::status::Retryability::NonRetryable,
        )
        .unwrap(),
    );

    assert!(matches!(result, Err(CatalogError::DuplicateCode(_))));
}

#[test]
fn catalog_allows_multiple_codes() {
    let mut catalog = ErrorCatalog::new();

    let code1 = ErrorCode::new("CORE.MULTI.001").unwrap();
    let code2 = ErrorCode::new("CORE.MULTI.002").unwrap();
    let code3 = ErrorCode::new("CORE.MULTI.003").unwrap();

    catalog
        .register(
            nizaam_core::error::ErrorDefinition::new(
                code1,
                nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
                Version::new(1, 0, 0),
                nizaam_core::error::ErrorClass::Contract,
                nizaam_core::error::Severity::Error,
                "Test".to_string(),
                nizaam_core::status::Retryability::NonRetryable,
            )
            .unwrap(),
        )
        .unwrap();
    catalog
        .register(
            nizaam_core::error::ErrorDefinition::new(
                code2,
                nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
                Version::new(1, 0, 0),
                nizaam_core::error::ErrorClass::Contract,
                nizaam_core::error::Severity::Error,
                "Test".to_string(),
                nizaam_core::status::Retryability::NonRetryable,
            )
            .unwrap(),
        )
        .unwrap();
    catalog
        .register(
            nizaam_core::error::ErrorDefinition::new(
                code3,
                nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
                Version::new(1, 0, 0),
                nizaam_core::error::ErrorClass::Contract,
                nizaam_core::error::Severity::Error,
                "Test".to_string(),
                nizaam_core::status::Retryability::NonRetryable,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(catalog.len(), 3);
}

#[test]
fn catalog_returns_none_for_unknown_code() {
    let catalog = ErrorCatalog::new();
    let unknown_code = ErrorCode::new("CORE.UNKNOWN.999").unwrap();
    assert!(catalog.get(&unknown_code).is_none());
}

#[test]
fn definition_rejects_empty_message() {
    let code = ErrorCode::new("CORE.EMPTY.001").unwrap();
    let owner = ErrorOwner::new("CORE").unwrap();

    let result = ErrorDefinition::new(
        code,
        owner,
        Version::new(1, 0, 0),
        ErrorClass::Contract,
        Severity::Error,
        "",
        Retryability::NonRetryable,
    );

    assert!(matches!(result, Err(InvalidDefinition::EmptyMessage)));
}

#[test]
fn definition_rejects_owner_namespace_mismatch() {
    let code = ErrorCode::new("CORE.MISMATCH.001").unwrap();
    let owner = ErrorOwner::new("TRANSPORT").unwrap();

    let result = ErrorDefinition::new(
        code,
        owner,
        Version::new(1, 0, 0),
        ErrorClass::Contract,
        Severity::Error,
        "Namespace mismatch",
        Retryability::NonRetryable,
    );

    assert!(matches!(
        result,
        Err(InvalidDefinition::OwnerNamespaceMismatch)
    ));
}
