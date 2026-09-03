use std::collections::BTreeMap;

use super::{ErrorCode, ErrorDefinition};

/// Definition registration failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    DuplicateCode(ErrorCode),
    OwnerNamespaceMismatch,
}

/// In-process registry of validated static error definitions.
#[derive(Clone, Debug, Default)]
pub struct ErrorCatalog {
    definitions: BTreeMap<ErrorCode, ErrorDefinition>,
}

impl ErrorCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: ErrorDefinition) -> Result<(), CatalogError> {
        if definition.code.namespace().split('.').next() != Some(definition.owner.as_str()) {
            return Err(CatalogError::OwnerNamespaceMismatch);
        }
        if self.definitions.contains_key(&definition.code) {
            return Err(CatalogError::DuplicateCode(definition.code));
        }
        self.definitions.insert(definition.code.clone(), definition);
        Ok(())
    }

    pub fn get(&self, code: &ErrorCode) -> Option<&ErrorDefinition> {
        self.definitions.get(code)
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::error::{ErrorClass, Severity};
    use crate::status::Retryability;

    fn definition() -> ErrorDefinition {
        ErrorDefinition::new(
            ErrorCode::new("CORE.CONTRACT.001").unwrap(),
            super::super::ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            ErrorClass::Contract,
            Severity::Error,
            "Contract rejected",
            Retryability::NonRetryable,
        )
        .unwrap()
    }

    #[test]
    fn catalog_rejects_duplicate_codes() {
        let mut catalog = ErrorCatalog::new();
        catalog.register(definition()).unwrap();
        assert!(matches!(
            catalog.register(definition()),
            Err(CatalogError::DuplicateCode(_))
        ));
    }
}
