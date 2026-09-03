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
        let reference = ErrorReference::new(definition.code.as_str().to_owned())
            .expect("validated error codes produce valid references");
        Ok(ErrorEvent { reference, error })
    }
}
