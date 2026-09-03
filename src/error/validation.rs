use core::fmt;

/// A failure at the Error System boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    EmptyMessage,
    EmptyDiagnosticField,
    UnregisteredDefinition,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyMessage => "an error occurrence message must not be empty",
            Self::EmptyDiagnosticField => "diagnostic detail keys and values must not be empty",
            Self::UnregisteredDefinition => "the error definition is not registered",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ValidationError {}
