use super::{ErrorCatalog, ErrorContext, ErrorEvent, ErrorReference, GlobalError, ValidationError};

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
        let reference = ErrorReference::new(code.as_str().to_owned())
            .expect("validated error codes produce valid references");
        Ok(ErrorEvent { reference, error })
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
        let reference = ErrorReference::new(definition.code.as_str().to_owned())
            .expect("validated error codes produce valid references");
        Ok(ErrorEvent { reference, error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::error::{
        DiagnosticDetail, ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, Severity,
    };
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::Operation;
    use crate::operation::OperationContext;
    use crate::status::Retryability;

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("test-op").unwrap(),
            CorrelationId::new("test-corr").unwrap(),
        ))
    }

    #[test]
    fn error_system_new_creates_empty_system() {
        let system = ErrorSystem::new();
        assert!(system.catalog().is_empty());
    }

    #[test]
    fn error_system_register_adds_definition() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.002").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Warning,
            "Warning message",
            Retryability::Retryable,
        )
        .unwrap();
        system.register(def.clone()).unwrap();
        assert!(system.catalog().get(&def.code).is_some());
        let retrieved = system.catalog().get(&def.code).unwrap();
        assert_eq!(retrieved.owner, def.owner);
        assert_eq!(retrieved.severity, def.severity);
    }

    #[test]
    fn error_system_instance_reports_errors() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.003").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Test error",
            Retryability::Retryable,
        )
        .unwrap();
        system.register(def.clone()).unwrap();
        let instance = system.instance();
        let event = instance.report(&def.code, ErrorContext::new(operation_context()));
        assert!(event.is_ok());
        let event = event.unwrap();
        assert_eq!(event.error.code.as_str(), "CORE.TEST.003");
    }

    #[test]
    fn error_system_instance_rejects_unregistered_code() {
        let system = ErrorSystem::new();
        let instance = system.instance();
        let unknown_code = ErrorCode::new("CORE.UNKNOWN.999").unwrap();
        let event = instance.report(&unknown_code, ErrorContext::new(operation_context()));
        assert!(event.is_err());
    }

    #[test]
    fn error_system_instance_clone_is_independent() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.004").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Clone test",
            Retryability::Retryable,
        )
        .unwrap();
        system.register(def).unwrap();

        let instance1 = system.instance();
        let instance2 = system.instance();

        assert!(
            instance1
                .catalog
                .get(&ErrorCode::new("CORE.TEST.004").unwrap())
                .is_some()
        );
        assert!(
            instance2
                .catalog
                .get(&ErrorCode::new("CORE.TEST.004").unwrap())
                .is_some()
        );
    }

    #[test]
    fn error_system_report_error_rejects_metadata_mismatch() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.005").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Original message",
            Retryability::NonRetryable,
        )
        .unwrap();
        system.register(def.clone()).unwrap();
        let instance = system.instance();

        let mut error = instance
            .report(&def.code, ErrorContext::new(operation_context()))
            .unwrap()
            .error;
        error.severity = Severity::Critical;
        let result = instance.report_error(error);
        assert!(result.is_err());
    }

    #[test]
    fn error_system_report_error_with_details() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.006").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Error with details",
            Retryability::Retryable,
        )
        .unwrap();
        system.register(def.clone()).unwrap();
        let instance = system.instance();

        let error = instance
            .report(&def.code, ErrorContext::new(operation_context()))
            .unwrap()
            .error;
        let error_with_detail = error.with_detail(DiagnosticDetail::new("key", "value").unwrap());
        let result = instance.report_error(error_with_detail);
        assert!(result.is_ok());
    }

    #[test]
    fn error_system_report_error_rejects_empty_message() {
        let mut system = ErrorSystem::new();
        let def = ErrorDefinition::new(
            ErrorCode::new("CORE.TEST.007").unwrap(),
            ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Original message",
            Retryability::Retryable,
        )
        .unwrap();
        system.register(def.clone()).unwrap();
        let instance = system.instance();

        let mut error = instance
            .report(&def.code, ErrorContext::new(operation_context()))
            .unwrap()
            .error;
        error = error.with_message("");
        let result = instance.report_error(error);
        assert!(result.is_err());
    }
}
