//! Structured operational diagnostics for Nizaam Core.
//!
//! Diagnostics describe current or recent operational condition. They are
//! intentionally distinct from logging, historical provenance, the shared
//! Error System, and the later Health system. A diagnostic is an immutable
//! observation; retention, aggregation, and health interpretation belong to
//! their owning systems.

use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::error::ErrorReference;
use crate::identity::{CapabilityId, EngineId};
use crate::observability::correlation::CorrelationContext;

/// Maximum number of structured detail entries carried by one diagnostic.
pub const MAX_DIAGNOSTIC_DETAILS: usize = 16;

/// Maximum length of a diagnostic message.
pub const MAX_DIAGNOSTIC_MESSAGE_LENGTH: usize = 512;

/// Maximum length of a diagnostic source name.
pub const MAX_DIAGNOSTIC_SOURCE_LENGTH: usize = 128;

/// Maximum length of a diagnostic detail key.
pub const MAX_DIAGNOSTIC_DETAIL_KEY_LENGTH: usize = 128;

/// Maximum length of a diagnostic detail value.
pub const MAX_DIAGNOSTIC_DETAIL_VALUE_LENGTH: usize = 256;

/// Categories of operational conditions that Core diagnostics can describe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiagnosticKind {
    Lifecycle,
    Dependency,
    Runtime,
    Capability,
    Configuration,
    Operational,
}

/// A condition-oriented classification for a diagnostic.
///
/// This is deliberately not a logging severity and is also not the later
/// Health status model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiagnosticCondition {
    Operational,
    Degraded,
    Unavailable,
    Failed,
}

/// The entity or subsystem to which a diagnostic applies.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum DiagnosticSubject {
    Component(String),
    Engine(EngineId),
    Capability(CapabilityId),
    Dependency(String),
    Runtime,
    Configuration,
}

impl DiagnosticSubject {
    /// Creates a generic Core component subject.
    pub fn component(value: impl Into<String>) -> Result<Self, DiagnosticError> {
        let value = validate_bounded_text(
            value.into(),
            "component",
            MAX_DIAGNOSTIC_SOURCE_LENGTH,
            DiagnosticError::InvalidSubject,
        )?;
        Ok(Self::Component(value))
    }

    /// Creates a dependency subject.
    pub fn dependency(value: impl Into<String>) -> Result<Self, DiagnosticError> {
        let value = validate_bounded_text(
            value.into(),
            "dependency",
            MAX_DIAGNOSTIC_SOURCE_LENGTH,
            DiagnosticError::InvalidSubject,
        )?;
        Ok(Self::Dependency(value))
    }
}

#[derive(serde::Deserialize)]
enum DiagnosticSubjectWire {
    Component(String),
    Engine(EngineId),
    Capability(CapabilityId),
    Dependency(String),
    Runtime,
    Configuration,
}

impl<'de> serde::Deserialize<'de> for DiagnosticSubject {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match DiagnosticSubjectWire::deserialize(deserializer)? {
            DiagnosticSubjectWire::Component(value) => {
                DiagnosticSubject::component(value).map_err(serde::de::Error::custom)
            }
            DiagnosticSubjectWire::Engine(value) => Ok(Self::Engine(value)),
            DiagnosticSubjectWire::Capability(value) => Ok(Self::Capability(value)),
            DiagnosticSubjectWire::Dependency(value) => {
                DiagnosticSubject::dependency(value).map_err(serde::de::Error::custom)
            }
            DiagnosticSubjectWire::Runtime => Ok(Self::Runtime),
            DiagnosticSubjectWire::Configuration => Ok(Self::Configuration),
        }
    }
}

/// Structured, bounded diagnostic details.
#[derive(Clone, Debug, Eq, PartialEq, Default, serde::Serialize)]
pub struct DiagnosticDetails(BTreeMap<String, String>);

impl<'de> serde::Deserialize<'de> for DiagnosticDetails {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = BTreeMap::<String, String>::deserialize(deserializer)?;
        let mut details = Self::new();

        for (key, value) in values {
            details
                .insert(key, value)
                .map_err(serde::de::Error::custom)?;
        }

        Ok(details)
    }
}

impl DiagnosticDetails {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts one bounded diagnostic field.
    ///
    /// Existing keys are replaced without increasing the entry count.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), DiagnosticError> {
        let key = validate_bounded_text(
            key.into(),
            "detail key",
            MAX_DIAGNOSTIC_DETAIL_KEY_LENGTH,
            DiagnosticError::InvalidDetail,
        )?;
        let value = validate_bounded_text(
            value.into(),
            "detail value",
            MAX_DIAGNOSTIC_DETAIL_VALUE_LENGTH,
            DiagnosticError::InvalidDetail,
        )?;

        if !self.0.contains_key(&key) && self.0.len() >= MAX_DIAGNOSTIC_DETAILS {
            return Err(DiagnosticError::DetailLimitExceeded);
        }

        self.0.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

/// Errors produced while constructing a diagnostic value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticError {
    InvalidMessage,
    MessageTooLong,
    InvalidSource,
    SourceTooLong,
    InvalidSubject,
    InvalidDetail,
    DetailLimitExceeded,
}

impl std::fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMessage => formatter.write_str("diagnostic message must not be empty"),
            Self::MessageTooLong => formatter.write_str("diagnostic message exceeds its limit"),
            Self::InvalidSource => formatter.write_str("diagnostic source must not be empty"),
            Self::SourceTooLong => formatter.write_str("diagnostic source exceeds its limit"),
            Self::InvalidSubject => formatter.write_str("diagnostic subject must not be empty"),
            Self::InvalidDetail => {
                formatter.write_str("diagnostic detail key and value must not be empty")
            }
            Self::DetailLimitExceeded => formatter.write_str("diagnostic detail limit exceeded"),
        }
    }
}

impl std::error::Error for DiagnosticError {}

/// One immutable observation of current or recent operational condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    kind: DiagnosticKind,
    condition: DiagnosticCondition,
    message: String,
    observed_at: SystemTime,
    source: Option<String>,
    subject: Option<DiagnosticSubject>,
    correlation: Option<CorrelationContext>,
    error_reference: Option<ErrorReference>,
    details: DiagnosticDetails,
}

impl Diagnostic {
    /// Creates a diagnostic with an observation timestamp taken at creation.
    pub fn new(
        kind: DiagnosticKind,
        condition: DiagnosticCondition,
        message: impl Into<String>,
    ) -> Result<Self, DiagnosticError> {
        let message = validate_bounded_text(
            message.into(),
            "message",
            MAX_DIAGNOSTIC_MESSAGE_LENGTH,
            DiagnosticError::InvalidMessage,
        )?;

        Ok(Self {
            kind,
            condition,
            message,
            observed_at: SystemTime::now(),
            source: None,
            subject: None,
            correlation: None,
            error_reference: None,
            details: DiagnosticDetails::new(),
        })
    }

    pub fn kind(&self) -> DiagnosticKind {
        self.kind
    }

    pub fn condition(&self) -> DiagnosticCondition {
        self.condition
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn observed_at(&self) -> SystemTime {
        self.observed_at
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn subject(&self) -> Option<&DiagnosticSubject> {
        self.subject.as_ref()
    }

    pub fn correlation(&self) -> Option<&CorrelationContext> {
        self.correlation.as_ref()
    }

    pub fn error_reference(&self) -> Option<&ErrorReference> {
        self.error_reference.as_ref()
    }

    pub fn details(&self) -> &DiagnosticDetails {
        &self.details
    }

    /// Derives a diagnostic with a bounded source name.
    pub fn with_source(mut self, source: impl Into<String>) -> Result<Self, DiagnosticError> {
        self.source = Some(validate_bounded_text(
            source.into(),
            "source",
            MAX_DIAGNOSTIC_SOURCE_LENGTH,
            DiagnosticError::InvalidSource,
        )?);
        Ok(self)
    }

    /// Derives a diagnostic with an associated subject.
    pub fn with_subject(mut self, subject: DiagnosticSubject) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Derives a diagnostic with existing Core correlation context.
    pub fn with_correlation(mut self, correlation: CorrelationContext) -> Self {
        self.correlation = Some(correlation);
        self
    }

    /// Derives a diagnostic with an existing shared Error System reference.
    pub fn with_error_reference(mut self, reference: ErrorReference) -> Self {
        self.error_reference = Some(reference);
        self
    }

    /// Derives a diagnostic with one bounded structured detail.
    pub fn with_detail(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, DiagnosticError> {
        self.details.insert(key, value)?;
        Ok(self)
    }
}

fn validate_bounded_text(
    value: String,
    field: &'static str,
    max_length: usize,
    empty_error: DiagnosticError,
) -> Result<String, DiagnosticError> {
    if value.trim().is_empty() {
        return Err(empty_error);
    }

    if value.chars().count() > max_length {
        return Err(match field {
            "message" => DiagnosticError::MessageTooLong,
            "source" => DiagnosticError::SourceTooLong,
            _ => empty_error,
        });
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CorrelationId, OperationId};
    use crate::operation::{Operation, OperationContext};

    fn correlation_context() -> CorrelationContext {
        let operation = Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        );
        CorrelationContext::from_operation_context(&OperationContext::new(operation))
    }

    #[test]
    fn diagnostic_requires_a_non_empty_message() {
        assert!(matches!(
            Diagnostic::new(
                DiagnosticKind::Runtime,
                DiagnosticCondition::Operational,
                ""
            ),
            Err(DiagnosticError::InvalidMessage)
        ));
        assert!(matches!(
            Diagnostic::new(
                DiagnosticKind::Runtime,
                DiagnosticCondition::Operational,
                "   "
            ),
            Err(DiagnosticError::InvalidMessage)
        ));
    }

    #[test]
    fn diagnostic_rejects_overlong_messages() {
        let message = "x".repeat(MAX_DIAGNOSTIC_MESSAGE_LENGTH + 1);
        assert!(matches!(
            Diagnostic::new(
                DiagnosticKind::Runtime,
                DiagnosticCondition::Operational,
                message
            ),
            Err(DiagnosticError::MessageTooLong)
        ));
    }

    #[test]
    fn diagnostic_records_kind_condition_message_and_timestamp() {
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Dependency,
            DiagnosticCondition::Degraded,
            "database latency elevated",
        )
        .unwrap();

        assert_eq!(diagnostic.kind(), DiagnosticKind::Dependency);
        assert_eq!(diagnostic.condition(), DiagnosticCondition::Degraded);
        assert_eq!(diagnostic.message(), "database latency elevated");
        assert!(diagnostic.observed_at().elapsed().is_ok());
    }

    #[test]
    fn source_is_bounded_and_optional() {
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Operational,
            DiagnosticCondition::Operational,
            "running",
        )
        .unwrap()
        .with_source("health-monitor")
        .unwrap();

        assert_eq!(diagnostic.source(), Some("health-monitor"));
    }

    #[test]
    fn source_rejects_empty_and_overlong_values() {
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Operational,
            DiagnosticCondition::Operational,
            "running",
        )
        .unwrap();

        assert!(matches!(
            diagnostic.clone().with_source(""),
            Err(DiagnosticError::InvalidSource)
        ));

        let source = "x".repeat(MAX_DIAGNOSTIC_SOURCE_LENGTH + 1);
        assert!(matches!(
            diagnostic.with_source(source),
            Err(DiagnosticError::SourceTooLong)
        ));
    }

    #[test]
    fn subjects_support_core_identities_and_generic_targets() {
        let engine = DiagnosticSubject::Engine(EngineId::new("engine-1").unwrap());
        let capability = DiagnosticSubject::Capability(CapabilityId::new("cap-1").unwrap());
        let component = DiagnosticSubject::component("core-runtime").unwrap();
        let dependency = DiagnosticSubject::dependency("database").unwrap();

        assert!(matches!(engine, DiagnosticSubject::Engine(_)));
        assert!(matches!(capability, DiagnosticSubject::Capability(_)));
        assert!(matches!(component, DiagnosticSubject::Component(_)));
        assert!(matches!(dependency, DiagnosticSubject::Dependency(_)));
    }

    #[test]
    fn diagnostic_subject_deserialization_preserves_bounds() {
        let component =
            serde_json::from_str::<DiagnosticSubject>(r#"{"Component":"core-runtime"}"#)
                .expect("valid component subject should deserialize");
        assert_eq!(
            component,
            DiagnosticSubject::component("core-runtime").unwrap()
        );

        assert!(serde_json::from_str::<DiagnosticSubject>(r#"{"Component":""}"#).is_err());
        assert!(serde_json::from_str::<DiagnosticSubject>(r#"{"Dependency":"   "}"#).is_err());
    }

    #[test]
    fn diagnostic_details_deserialization_preserves_bounds() {
        let details = serde_json::from_str::<DiagnosticDetails>(r#"{"state":"active"}"#)
            .expect("valid details should deserialize");
        assert_eq!(details.get("state"), Some("active"));

        let oversized = format!(
            r#"{{"state":"{}"}}"#,
            "x".repeat(MAX_DIAGNOSTIC_DETAIL_VALUE_LENGTH + 1)
        );
        assert!(serde_json::from_str::<DiagnosticDetails>(&oversized).is_err());
    }

    #[test]
    fn generic_subjects_reject_empty_values() {
        assert!(matches!(
            DiagnosticSubject::component(""),
            Err(DiagnosticError::InvalidSubject)
        ));
        assert!(matches!(
            DiagnosticSubject::dependency("   "),
            Err(DiagnosticError::InvalidSubject)
        ));
    }

    #[test]
    fn details_are_bounded_and_replace_existing_keys() {
        let mut details = DiagnosticDetails::new();
        details.insert("dependency", "postgres").unwrap();
        details.insert("dependency", "redis").unwrap();

        assert_eq!(details.len(), 1);
        assert_eq!(details.get("dependency"), Some("redis"));
    }

    #[test]
    fn details_reject_empty_values() {
        let mut details = DiagnosticDetails::new();

        assert!(matches!(
            details.insert("", "value"),
            Err(DiagnosticError::InvalidDetail)
        ));
        assert!(matches!(
            details.insert("key", ""),
            Err(DiagnosticError::InvalidDetail)
        ));
    }

    #[test]
    fn details_enforce_entry_limit() {
        let mut details = DiagnosticDetails::new();

        for index in 0..MAX_DIAGNOSTIC_DETAILS {
            details
                .insert(format!("key-{index}"), format!("value-{index}"))
                .unwrap();
        }

        assert!(matches!(
            details.insert("one-more", "value"),
            Err(DiagnosticError::DetailLimitExceeded)
        ));
    }

    #[test]
    fn optional_context_is_preserved() {
        let error_reference = ErrorReference::new("CORE.RUNTIME.001").unwrap();
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Operational,
            DiagnosticCondition::Failed,
            "runtime condition failed",
        )
        .unwrap()
        .with_correlation(correlation_context())
        .with_error_reference(error_reference.clone())
        .with_subject(DiagnosticSubject::Runtime);

        assert_eq!(diagnostic.correlation(), Some(&correlation_context()));
        assert_eq!(diagnostic.error_reference(), Some(&error_reference));
        assert_eq!(diagnostic.subject(), Some(&DiagnosticSubject::Runtime));
    }

    #[test]
    fn enrichment_does_not_mutate_original_diagnostic() {
        let original = Diagnostic::new(
            DiagnosticKind::Runtime,
            DiagnosticCondition::Operational,
            "running",
        )
        .unwrap();

        let derived = original
            .clone()
            .with_source("runtime")
            .unwrap()
            .with_detail("state", "active")
            .unwrap();

        assert!(original.source().is_none());
        assert!(original.details().is_empty());
        assert_eq!(derived.source(), Some("runtime"));
        assert_eq!(derived.details().get("state"), Some("active"));
    }

    #[test]
    fn diagnostics_are_independent_values() {
        let first = Diagnostic::new(
            DiagnosticKind::Runtime,
            DiagnosticCondition::Operational,
            "first",
        )
        .unwrap();

        let second = Diagnostic::new(
            DiagnosticKind::Runtime,
            DiagnosticCondition::Operational,
            "second",
        )
        .unwrap();

        assert_ne!(first.message(), second.message());
        assert_ne!(first, second);
    }
}
