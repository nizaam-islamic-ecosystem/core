use super::{ErrorCatalog, ErrorContext, ErrorEvent, GlobalError, ValidationError};

/// Failure while registering or reporting an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReportError {
    Catalog(super::CatalogError),
    Validation(ValidationError),
}

/// The shared Error System factory and registry owner.
#[derive(Clone, Debug, Default)]
pub struct ErrorSystem {
    catalog: ErrorCatalog,
}

/// A scoped view of the shared Error System.
#[derive(Clone, Debug)]
pub struct ErrorSystemInstance {
    catalog: ErrorCatalog,
}

impl ErrorSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: super::ErrorDefinition) -> Result<(), ReportError> {
        self.catalog
            .register(definition)
            .map_err(ReportError::Catalog)
    }

    pub fn instance(&self) -> ErrorSystemInstance {
        ErrorSystemInstance {
            catalog: self.catalog.clone(),
        }
    }

    pub fn catalog(&self) -> &ErrorCatalog {
        &self.catalog
    }
}

impl ErrorSystemInstance {
    pub fn report(
        &self,
        code: &super::ErrorCode,
        context: ErrorContext,
    ) -> Result<ErrorEvent, ReportError> {
        let definition = self.catalog.get(code).ok_or(ReportError::Validation(
            ValidationError::UnregisteredDefinition,
        ))?;
        let error = GlobalError::from_definition(definition, context, None);
        Ok(ErrorEvent::new(error))
    }

    pub fn report_error(&self, error: GlobalError) -> Result<ErrorEvent, ReportError> {
        let definition = self
            .catalog
            .get(&error.code)
            .ok_or(ReportError::Validation(
                ValidationError::UnregisteredDefinition,
            ))?;
        if error.message.trim().is_empty() {
            return Err(ReportError::Validation(ValidationError::EmptyMessage));
        }
        if error.owner != definition.owner
            || error.version != definition.version
            || error.class != definition.class
            || error.severity != definition.severity
            || error.retryability != definition.retryability
            || error.solution_reference != definition.solution_reference
        {
            return Err(ReportError::Validation(
                ValidationError::DefinitionMetadataMismatch,
            ));
        }
        if error
            .details()
            .iter()
            .any(|detail| detail.key.trim().is_empty() || detail.value.trim().is_empty())
        {
            return Err(ReportError::Validation(
                ValidationError::EmptyDiagnosticField,
            ));
        }
        Ok(ErrorEvent::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::error::{
        CatalogError, DiagnosticDetail, ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner,
        ErrorReference, Severity,
    };
    use crate::identity::{CapabilityId, CorrelationId, EngineId, OperationId};
    use crate::operation::{Operation, OperationContext};
    use crate::status::Retryability;

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("error-level-2-operation").unwrap(),
            CorrelationId::new("error-level-2-correlation").unwrap(),
        ))
    }

    fn definition(
        code: &str,
        class: ErrorClass,
        severity: Severity,
        retryability: Retryability,
    ) -> ErrorDefinition {
        ErrorDefinition::new(
            ErrorCode::new(code).unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            class,
            severity,
            "Core error",
            retryability,
        )
        .unwrap()
    }

    #[test]
    fn level_2_error_system_composes_registration_reporting_and_error_event() {
        let mut system = ErrorSystem::new();
        let definition = definition(
            "CORE.RUNTIME.001",
            ErrorClass::Execution,
            Severity::Error,
            Retryability::Retryable,
        );

        system.register(definition.clone()).unwrap();

        let instance = system.instance();
        let context = ErrorContext::new(operation_context())
            .from_engine(EngineId::new("quran-engine").unwrap())
            .for_capability(CapabilityId::new("quran.read").unwrap());

        let event = instance.report(&definition.code, context.clone()).unwrap();

        assert_eq!(event.reference().as_str(), "CORE.RUNTIME.001");
        assert_eq!(event.error.code, definition.code);
        assert_eq!(event.error.owner, definition.owner);
        assert_eq!(event.error.version, definition.version);
        assert_eq!(event.error.class, definition.class);
        assert_eq!(event.error.severity, definition.severity);
        assert_eq!(event.error.retryability, definition.retryability);
        assert_eq!(event.error.message, definition.default_message());
        assert_eq!(event.error.context, context);
        assert_eq!(event.event_name().as_str(), "error.occurred");
        assert_eq!(event.event_type(), "error");
        assert_eq!(event.event_scope(), "engine:quran-engine");
        assert!(event.message_id().as_str().starts_with("error-message-"));
        assert!(event.error.details().is_empty());
        assert!(event.error.cause.is_none());
    }

    #[test]
    fn level_2_registered_definitions_are_isolated_from_unknown_codes() {
        let mut system = ErrorSystem::new();
        let registered = definition(
            "CORE.CONTRACT.001",
            ErrorClass::Contract,
            Severity::Warning,
            Retryability::NonRetryable,
        );
        system.register(registered.clone()).unwrap();

        let instance = system.instance();
        let unknown = ErrorCode::new("CORE.CONTRACT.999").unwrap();

        assert!(
            instance
                .report(&registered.code, ErrorContext::new(operation_context()))
                .is_ok()
        );

        assert_eq!(
            instance.report(&unknown, ErrorContext::new(operation_context())),
            Err(ReportError::Validation(
                ValidationError::UnregisteredDefinition
            ))
        );
    }

    #[test]
    fn level_2_error_occurrence_enrichment_survives_report_validation() {
        let mut system = ErrorSystem::new();
        let definition = definition(
            "CORE.CAPABILITY.001",
            ErrorClass::Capability,
            Severity::Error,
            Retryability::Retryable,
        );
        system.register(definition.clone()).unwrap();

        let instance = system.instance();
        let base = instance
            .report(&definition.code, ErrorContext::new(operation_context()))
            .unwrap()
            .error;

        let cause = ErrorReference::new("CORE.DEPENDENCY.001").unwrap();
        let enriched = base
            .with_message("capability dependency failed")
            .with_detail(DiagnosticDetail::new("dependency", "quran-db").unwrap())
            .caused_by(cause.clone());

        let event = instance.report_error(enriched).unwrap();

        assert_eq!(event.reference().as_str(), definition.code.as_str());
        assert_eq!(event.error.message, "capability dependency failed");
        assert_eq!(
            event.error.details(),
            &[DiagnosticDetail {
                key: "dependency".to_owned(),
                value: "quran-db".to_owned(),
            }]
        );
        assert_eq!(event.error.cause, Some(cause));
    }

    #[test]
    fn level_2_report_error_rejects_occurrences_that_drift_from_catalog_metadata() {
        let mut system = ErrorSystem::new();
        let definition = definition(
            "CORE.AUTHORIZATION.001",
            ErrorClass::Authorization,
            Severity::Error,
            Retryability::NonRetryable,
        );
        system.register(definition.clone()).unwrap();

        let instance = system.instance();
        let mut error = instance
            .report(&definition.code, ErrorContext::new(operation_context()))
            .unwrap()
            .error;

        error.severity = Severity::Critical;

        assert_eq!(
            instance.report_error(error),
            Err(ReportError::Validation(
                ValidationError::DefinitionMetadataMismatch
            ))
        );
    }

    #[test]
    fn level_2_catalog_boundary_rejects_duplicate_definition_registration() {
        let mut system = ErrorSystem::new();
        let definition = definition(
            "CORE.TRANSPORT.001",
            ErrorClass::Transport,
            Severity::Error,
            Retryability::Retryable,
        );

        system.register(definition.clone()).unwrap();

        assert_eq!(
            system.register(definition),
            Err(ReportError::Catalog(CatalogError::DuplicateCode(
                ErrorCode::new("CORE.TRANSPORT.001").unwrap()
            )))
        );
    }

    #[test]
    fn level_2_error_context_preserves_operation_and_engine_information() {
        let context = ErrorContext::new(operation_context())
            .from_engine(EngineId::new("hadith-engine").unwrap())
            .for_capability(CapabilityId::new("hadith.search").unwrap());

        assert_eq!(
            context.operation.operation.correlation_id.as_str(),
            "error-level-2-correlation"
        );
        assert_eq!(
            context.operation.operation.id.as_str(),
            "error-level-2-operation"
        );
        assert_eq!(
            context.engine_id.as_ref().unwrap().as_str(),
            "hadith-engine"
        );
        assert_eq!(
            context.capability_id.as_ref().unwrap().as_str(),
            "hadith.search"
        );
    }
}
