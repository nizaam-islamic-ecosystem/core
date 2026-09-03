//! Phase 3 boundary for the independent Error System.

mod catalog;
mod definition;
mod event;
mod reference;
mod system;
mod validation;

pub use catalog::{CatalogError, ErrorCatalog};
pub use definition::{
    ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, InvalidDefinition, InvalidErrorCode,
    InvalidErrorOwner, Severity,
};
pub use event::{DiagnosticDetail, ErrorContext, ErrorEvent, GlobalError};
pub use reference::ErrorReference;
pub use system::{ErrorSystem, ErrorSystemInstance, ReportError};
pub use validation::ValidationError;
