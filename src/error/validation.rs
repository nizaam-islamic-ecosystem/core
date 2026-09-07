use core::fmt;

/// A failure at the Error System boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyMessage,
    EmptyDiagnosticField,
    UnregisteredDefinition,
    DefinitionMetadataMismatch,
    InvalidMessageFormat,
    MissingRequiredField,
    ExceededMaxLength,
    DuplicateDetailKey,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyMessage => "an error occurrence message must not be empty",
            Self::EmptyDiagnosticField => "diagnostic detail keys and values must not be empty",
            Self::UnregisteredDefinition => "the error definition is not registered",
            Self::DefinitionMetadataMismatch => {
                "the error metadata does not match the registered definition"
            }
            Self::InvalidMessageFormat => "the error message format is invalid",
            Self::MissingRequiredField => "a required field is missing",
            Self::ExceededMaxLength => "a field exceeds the maximum allowed length",
            Self::DuplicateDetailKey => "a diagnostic detail key is duplicated",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn validation_error_display_empty_message() {
        let err = ValidationError::EmptyMessage;
        assert_eq!(
            err.to_string(),
            "an error occurrence message must not be empty"
        );
    }

    #[test]
    fn validation_error_display_empty_diagnostic_field() {
        let err = ValidationError::EmptyDiagnosticField;
        assert_eq!(
            err.to_string(),
            "diagnostic detail keys and values must not be empty"
        );
    }

    #[test]
    fn validation_error_display_unregistered_definition() {
        let err = ValidationError::UnregisteredDefinition;
        assert_eq!(err.to_string(), "the error definition is not registered");
    }

    #[test]
    fn validation_error_display_definition_metadata_mismatch() {
        let err = ValidationError::DefinitionMetadataMismatch;
        assert_eq!(
            err.to_string(),
            "the error metadata does not match the registered definition"
        );
    }

    #[test]
    fn validation_error_display_invalid_message_format() {
        let err = ValidationError::InvalidMessageFormat;
        assert_eq!(err.to_string(), "the error message format is invalid");
    }

    #[test]
    fn validation_error_display_missing_required_field() {
        let err = ValidationError::MissingRequiredField;
        assert_eq!(err.to_string(), "a required field is missing");
    }

    #[test]
    fn validation_error_display_exceeded_max_length() {
        let err = ValidationError::ExceededMaxLength;
        assert_eq!(
            err.to_string(),
            "a field exceeds the maximum allowed length"
        );
    }

    #[test]
    fn validation_error_display_duplicate_detail_key() {
        let err = ValidationError::DuplicateDetailKey;
        assert_eq!(err.to_string(), "a diagnostic detail key is duplicated");
    }

    #[test]
    fn validation_error_is_error_trait() {
        let err = ValidationError::EmptyMessage;
        assert!(err.source().is_none());
    }

    #[test]
    fn validation_error_variants_are_distinct() {
        // Existing variants
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::EmptyDiagnosticField
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::UnregisteredDefinition
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::DefinitionMetadataMismatch
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::InvalidMessageFormat
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::MissingRequiredField
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::EmptyMessage,
            ValidationError::DuplicateDetailKey
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::UnregisteredDefinition
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::DefinitionMetadataMismatch
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::InvalidMessageFormat
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::MissingRequiredField
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::EmptyDiagnosticField,
            ValidationError::DuplicateDetailKey
        );
        assert_ne!(
            ValidationError::UnregisteredDefinition,
            ValidationError::DefinitionMetadataMismatch
        );
        assert_ne!(
            ValidationError::UnregisteredDefinition,
            ValidationError::InvalidMessageFormat
        );
        assert_ne!(
            ValidationError::UnregisteredDefinition,
            ValidationError::MissingRequiredField
        );
        assert_ne!(
            ValidationError::UnregisteredDefinition,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::UnregisteredDefinition,
            ValidationError::DuplicateDetailKey
        );
        assert_ne!(
            ValidationError::DefinitionMetadataMismatch,
            ValidationError::InvalidMessageFormat
        );
        assert_ne!(
            ValidationError::DefinitionMetadataMismatch,
            ValidationError::MissingRequiredField
        );
        assert_ne!(
            ValidationError::DefinitionMetadataMismatch,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::DefinitionMetadataMismatch,
            ValidationError::DuplicateDetailKey
        );
        // New variants
        assert_ne!(
            ValidationError::InvalidMessageFormat,
            ValidationError::MissingRequiredField
        );
        assert_ne!(
            ValidationError::InvalidMessageFormat,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::InvalidMessageFormat,
            ValidationError::DuplicateDetailKey
        );
        assert_ne!(
            ValidationError::MissingRequiredField,
            ValidationError::ExceededMaxLength
        );
        assert_ne!(
            ValidationError::MissingRequiredField,
            ValidationError::DuplicateDetailKey
        );
        assert_ne!(
            ValidationError::ExceededMaxLength,
            ValidationError::DuplicateDetailKey
        );
    }
}
