use crate::contracts::Version;
use crate::identity::{CapabilityId, EngineId};
use crate::operation::OperationContext;
use crate::status::Retryability;

use super::{ErrorClass, ErrorCode, ErrorOwner, ErrorReference, Severity};

/// One machine-readable diagnostic field attached to an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticDetail {
    pub key: String,
    pub value: String,
}

impl DiagnosticDetail {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Option<Self> {
        let key = key.into();
        let value = value.into();
        (!key.trim().is_empty() && !value.trim().is_empty()).then_some(Self { key, value })
    }
}

/// Trusted execution context associated with an error occurrence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorContext {
    pub operation: OperationContext,
    pub engine_id: Option<EngineId>,
    pub capability_id: Option<CapabilityId>,
}

impl ErrorContext {
    pub fn new(operation: OperationContext) -> Self {
        Self {
            operation,
            engine_id: None,
            capability_id: None,
        }
    }

    pub fn from_engine(mut self, engine_id: EngineId) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    pub fn for_capability(mut self, capability_id: CapabilityId) -> Self {
        self.capability_id = Some(capability_id);
        self
    }
}

/// The strict common error contract emitted by Core systems.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalError {
    pub code: ErrorCode,
    pub owner: ErrorOwner,
    pub version: Version,
    pub class: ErrorClass,
    pub severity: Severity,
    pub retryability: Retryability,
    pub message: String,
    pub details: Vec<DiagnosticDetail>,
    pub solution_reference: Option<String>,
    pub context: ErrorContext,
    pub cause: Option<ErrorReference>,
}

/// A runtime occurrence that references a catalog definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorEvent {
    pub reference: ErrorReference,
    pub error: GlobalError,
}

impl GlobalError {
    pub(crate) fn from_definition(
        definition: &super::ErrorDefinition,
        context: ErrorContext,
        message: Option<String>,
    ) -> Self {
        Self {
            code: definition.code.clone(),
            owner: definition.owner.clone(),
            version: definition.version.clone(),
            class: definition.class,
            severity: definition.severity,
            retryability: definition.retryability,
            message: message.unwrap_or_else(|| definition.default_message.clone()),
            details: Vec::new(),
            solution_reference: definition.solution_reference.clone(),
            context,
            cause: None,
        }
    }

    pub fn with_detail(mut self, detail: DiagnosticDetail) -> Self {
        self.details.push(detail);
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn caused_by(mut self, cause: ErrorReference) -> Self {
        self.cause = Some(cause);
        self
    }
}
