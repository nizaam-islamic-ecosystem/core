//! Integration tests for the Error Catalog system.

use nizaam_core::error::{CatalogError, ErrorCode, ErrorCatalog};
use nizaam_core::prelude::*;

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
    
    catalog.register(
        nizaam_core::error::ErrorDefinition::new(
            code.clone(),
            nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            nizaam_core::error::ErrorClass::Contract,
            nizaam_core::error::Severity::Error,
            "Test error message".to_string(),
            nizaam_core::status::Retryability::NonRetryable,
        ).unwrap(),
    ).unwrap();
    
    let result = catalog.register(
        nizaam_core::error::ErrorDefinition::new(
            code,
            nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            nizaam_core::error::ErrorClass::Contract,
            nizaam_core::error::Severity::Error,
            "Duplicate definition".to_string(),
            nizaam_core::status::Retryability::NonRetryable,
        ).unwrap(),
    );
    
    assert!(matches!(result, Err(CatalogError::DuplicateCode(_))));
}

#[test]
fn catalog_allows_multiple_codes() {
    let mut catalog = ErrorCatalog::new();
    
    let code1 = ErrorCode::new("CORE.MULTI.001").unwrap();
    let code2 = ErrorCode::new("CORE.MULTI.002").unwrap();
    let code3 = ErrorCode::new("CORE.MULTI.003").unwrap();
    
    catalog.register(
        nizaam_core::error::ErrorDefinition::new(code1, nizaam_core::error::ErrorOwner::new("CORE").unwrap(), Version::new(1, 0, 0), nizaam_core::error::ErrorClass::Contract, nizaam_core::error::Severity::Error, "Test".to_string(), nizaam_core::status::Retryability::NonRetryable).unwrap(),
    ).unwrap();
    catalog.register(
        nizaam_core::error::ErrorDefinition::new(code2, nizaam_core::error::ErrorOwner::new("CORE").unwrap(), Version::new(1, 0, 0), nizaam_core::error::ErrorClass::Contract, nizaam_core::error::Severity::Error, "Test".to_string(), nizaam_core::status::Retryability::NonRetryable).unwrap(),
    ).unwrap();
    catalog.register(
        nizaam_core::error::ErrorDefinition::new(code3, nizaam_core::error::ErrorOwner::new("CORE").unwrap(), Version::new(1, 0, 0), nizaam_core::error::ErrorClass::Contract, nizaam_core::error::Severity::Error, "Test".to_string(), nizaam_core::status::Retryability::NonRetryable).unwrap(),
    ).unwrap();
    
    assert_eq!(catalog.len(), 3);
}

#[test]
fn catalog_returns_none_for_unknown_code() {
    let catalog = ErrorCatalog::new();
    let unknown_code = ErrorCode::new("CORE.UNKNOWN.999").unwrap();
    assert!(catalog.get(&unknown_code).is_none());
}

#[test]
fn catalog_rejects_empty_message() {
    let mut catalog = ErrorCatalog::new();
    let code = ErrorCode::new("CORE.EMPTY.001").unwrap();
    
    let result = catalog.register(
        nizaam_core::error::ErrorDefinition::new(
            code,
            nizaam_core::error::ErrorOwner::new("CORE").unwrap(),
            Version::new(1, 0, 0),
            nizaam_core::error::ErrorClass::Contract,
            nizaam_core::error::Severity::Error,
            "",
            nizaam_core::status::Retryability::NonRetryable,
        ).unwrap(),
    );
    
    assert!(matches!(result, Err(CatalogError::EmptyMessage)));
}

#[test]
fn catalog_rejects_owner_namespace_mismatch() {
    let mut catalog = ErrorCatalog::new();
    let code = ErrorCode::new("CORE.MISMATCH.001").unwrap();
    let result = catalog.register(
        nizaam_core::error::ErrorDefinition::new(
            code,
            nizaam_core::error::ErrorOwner::new("TRANSPORT").unwrap(),
            Version::new(1, 0, 0),
            nizaam_core::error::ErrorClass::Contract,
            nizaam_core::error::Severity::Error,
            "Namespace mismatch".to_string(),
            nizaam_core::status::Retryability::NonRetryable,
        ).unwrap(),
    );
    assert!(matches!(result, Err(CatalogError::OwnerNamespaceMismatch)));
}
