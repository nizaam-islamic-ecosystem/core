use core::fmt;

use crate::contracts::Version;
use crate::status::Retryability;

/// A validated namespaced code identifying one kind of technical error.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ErrorCode(String);

impl ErrorCode {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidErrorCode> {
        let value = value.into();
        let segments: Vec<_> = value.split('.').collect();
        if segments.len() < 2 || segments.iter().any(|segment| segment.is_empty()) {
            return Err(InvalidErrorCode::Malformed);
        }
        let numeric_suffix = segments.last().expect("at least two segments");
        if !numeric_suffix
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            return Err(InvalidErrorCode::MissingNumericSuffix);
        }
        if segments[..segments.len() - 1].iter().any(|segment| {
            !segment.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
        }) {
            return Err(InvalidErrorCode::InvalidNamespace);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn namespace(&self) -> &str {
        self.0
            .rsplit_once('.')
            .expect("validated code has a namespace")
            .0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The owner namespace of an error definition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ErrorOwner(String);

impl ErrorOwner {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidErrorOwner> {
        let value = value.into();
        if value.trim().is_empty()
            || !value.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(InvalidErrorOwner);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Broad technical classification shared by all engine definitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorClass {
    Transport,
    Contract,
    Validation,
    Authorization,
    Capability,
    Execution,
    Resource,
    Internal,
}

/// Operational importance of an error occurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Warning,
    Error,
    Critical,
}

/// Static meaning and handling guidance for a technical error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorDefinition {
    pub code: ErrorCode,
    pub owner: ErrorOwner,
    pub version: Version,
    pub class: ErrorClass,
    pub severity: Severity,
    pub default_message: String,
    pub retryability: Retryability,
    pub solution_reference: Option<String>,
}

impl ErrorDefinition {
    pub fn new(
        code: ErrorCode,
        owner: ErrorOwner,
        version: Version,
        class: ErrorClass,
        severity: Severity,
        default_message: impl Into<String>,
        retryability: Retryability,
    ) -> Result<Self, InvalidDefinition> {
        let default_message = default_message.into();
        if default_message.trim().is_empty() {
            return Err(InvalidDefinition::EmptyMessage);
        }
        if code.namespace().split('.').next() != Some(owner.as_str()) {
            return Err(InvalidDefinition::OwnerNamespaceMismatch);
        }
        Ok(Self {
            code,
            owner,
            version,
            class,
            severity,
            default_message,
            retryability,
            solution_reference: None,
        })
    }

    pub fn with_solution_reference(mut self, reference: impl Into<String>) -> Self {
        self.solution_reference = Some(reference.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidErrorCode {
    Malformed,
    InvalidNamespace,
    MissingNumericSuffix,
}

impl fmt::Display for InvalidErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid namespaced error code")
    }
}

impl std::error::Error for InvalidErrorCode {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidErrorOwner;

impl fmt::Display for InvalidErrorOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid error owner namespace")
    }
}

impl std::error::Error for InvalidErrorOwner {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDefinition {
    EmptyMessage,
    OwnerNamespaceMismatch,
}

impl fmt::Display for InvalidDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyMessage => "an error definition message must not be empty",
            Self::OwnerNamespaceMismatch => "the error owner must match the code namespace",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for InvalidDefinition {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_require_an_uppercase_namespace_and_numeric_suffix() {
        assert!(ErrorCode::new("CORE.CONTRACT.001").is_ok());
        assert_eq!(
            ErrorCode::new("core.contract.001"),
            Err(InvalidErrorCode::InvalidNamespace)
        );
        assert_eq!(
            ErrorCode::new("CORE.CONTRACT.BAD"),
            Err(InvalidErrorCode::MissingNumericSuffix)
        );
    }

    #[test]
    fn definitions_require_matching_ownership() {
        let code = ErrorCode::new("CORE.CONTRACT.001").unwrap();
        let owner = ErrorOwner::new("CORE").unwrap();
        assert!(
            ErrorDefinition::new(
                code,
                owner,
                Version::new(1, 0, 0),
                ErrorClass::Contract,
                Severity::Error,
                "Contract rejected",
                Retryability::NonRetryable,
            )
            .is_ok()
        );
    }
}
