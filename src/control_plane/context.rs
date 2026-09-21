//! Context exchange and propagation semantics for the Phase 15 Control Plane.
//!
//! This module defines the structural boundary for requesting and supplying
//! additional inter-engine context. It deliberately reuses the existing Core
//! operation, provenance, and artifact context types rather than introducing
//! replacement runtime, security, cancellation, deadline, or tracing systems.

use std::collections::BTreeMap;
use std::fmt;

use crate::{
    artifact::ArtifactReference, operation::OperationContext, provenance::ProvenanceContext,
};

/// Describes how important a context requirement is to the consuming engine.
///
/// The Control Plane preserves this level; it does not decide whether a
/// returned context package is semantically sufficient for the consumer.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum ContextRequirementLevel {
    /// Context required for the consumer to proceed safely.
    Required,

    /// Context preferred by the consumer when available.
    Preferred,

    /// Context that may be supplied when available but is not required.
    Optional,
}

/// Error returned when a context exchange value violates its structural
/// invariants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextError {
    /// A context name, key, or value was empty or whitespace-only.
    EmptyValue(&'static str),

    /// An artifact reference supplied to a context package is invalid.
    InvalidArtifactReference,

    /// A partial acceptance was created without identifying missing context.
    EmptyMissingRequirements,
}

impl fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyValue(field) => {
                write!(
                    formatter,
                    "context {field} must not be empty or whitespace-only"
                )
            }
            Self::InvalidArtifactReference => {
                formatter.write_str("context package contains an invalid artifact reference")
            }
            Self::EmptyMissingRequirements => {
                formatter.write_str("partial context acceptance must identify missing requirements")
            }
        }
    }
}

impl std::error::Error for ContextError {}

/// A single generic requirement for context needed by a consuming engine.
///
/// The Control Plane treats `name` and the constraint map as opaque metadata.
/// Their semantic interpretation remains owned by the consuming/provider
/// engines and their applicable contracts.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContextRequirement {
    name: String,
    level: ContextRequirementLevel,
    constraints: BTreeMap<String, String>,
}

impl ContextRequirement {
    /// Creates a context requirement with no additional constraints.
    pub fn new(
        name: impl Into<String>,
        level: ContextRequirementLevel,
    ) -> Result<Self, ContextError> {
        let name = name.into();

        validate_non_empty(&name, "requirement name")?;

        Ok(Self {
            name,
            level,
            constraints: BTreeMap::new(),
        })
    }

    /// Adds or replaces one opaque requirement constraint.
    pub fn with_constraint(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, ContextError> {
        let key = key.into();
        let value = value.into();

        validate_non_empty(&key, "constraint key")?;
        validate_non_empty(&value, "constraint value")?;

        self.constraints.insert(key, value);

        Ok(self)
    }

    /// Returns the requirement name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the requirement level.
    pub fn level(&self) -> ContextRequirementLevel {
        self.level
    }

    /// Returns all opaque requirement constraints.
    pub fn constraints(&self) -> &BTreeMap<String, String> {
        &self.constraints
    }

    /// Returns one constraint when present.
    pub fn constraint(&self, key: &str) -> Option<&str> {
        self.constraints.get(key).map(String::as_str)
    }
}

/// A request to obtain context satisfying one context requirement.
///
/// The established `OperationContext` is carried through unchanged. This
/// preserves operation, correlation, node, and attempt identity instead of
/// generating Control Plane-specific replacement identities.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContextRequest {
    operation: OperationContext,
    requirement: ContextRequirement,
}

impl ContextRequest {
    /// Creates a context request from trusted operation context and a
    /// context requirement.
    pub fn new(operation: OperationContext, requirement: ContextRequirement) -> Self {
        Self {
            operation,
            requirement,
        }
    }

    /// Returns the established operation context carried by the request.
    pub fn operation(&self) -> &OperationContext {
        &self.operation
    }

    /// Returns the requested context requirement.
    pub fn requirement(&self) -> &ContextRequirement {
        &self.requirement
    }
}

/// Generic context supplied by a context-owning provider.
///
/// The entries are intentionally opaque to the Control Plane. Domain meaning,
/// authority, relevance, completeness, freshness, and consistency are owned
/// by the provider and consumer rather than interpreted here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPackage {
    entries: BTreeMap<String, String>,
    artifacts: Vec<ArtifactReference>,
    provenance: ProvenanceContext,
}

impl Default for ContextPackage {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextPackage {
    /// Creates an empty context package with empty provenance context.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            artifacts: Vec::new(),
            provenance: ProvenanceContext::new(),
        }
    }

    /// Adds or replaces an opaque context entry.
    pub fn with_entry(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, ContextError> {
        self.insert(key, value)?;
        Ok(self)
    }

    /// Adds or replaces an opaque context entry in an existing package.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), ContextError> {
        let key = key.into();
        let value = value.into();

        validate_non_empty(&key, "context entry key")?;
        validate_non_empty(&value, "context entry value")?;

        self.entries.insert(key, value);

        Ok(())
    }

    /// Adds an externally managed artifact reference without resolving it.
    pub fn with_artifact(mut self, artifact: ArtifactReference) -> Result<Self, ContextError> {
        self.add_artifact(artifact)?;
        Ok(self)
    }

    /// Adds an externally managed artifact reference without resolving it.
    pub fn add_artifact(&mut self, artifact: ArtifactReference) -> Result<(), ContextError> {
        if !artifact.is_valid() {
            return Err(ContextError::InvalidArtifactReference);
        }

        self.artifacts.push(artifact);

        Ok(())
    }

    /// Derives the package with the supplied provenance context.
    pub fn with_provenance(mut self, provenance: ProvenanceContext) -> Self {
        self.provenance = provenance;
        self
    }

    /// Returns all opaque context entries.
    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    /// Returns one context entry when present.
    pub fn entry(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Returns artifact references carried by the package.
    pub fn artifacts(&self) -> &[ArtifactReference] {
        &self.artifacts
    }

    /// Returns the provider-neutral provenance context carried by the package.
    pub fn provenance(&self) -> &ProvenanceContext {
        &self.provenance
    }

    /// Returns whether the package contains no entries or artifact references.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.artifacts.is_empty()
    }
}

/// The consumer's structural acceptance state for a supplied context package.
///
/// This is an acknowledgement of context sufficiency from the consuming side;
/// it is not a provider execution status and is not itself a Control Plane
/// failure.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum ContextAcceptanceStatus {
    /// The consumer has accepted the supplied context as sufficient.
    Accepted,

    /// The consumer can identify missing context and may request more.
    Partial,

    /// The consumer has rejected the supplied context.
    Rejected,
}

/// Consumer-side response describing whether a context package is sufficient.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ContextAcceptance {
    status: ContextAcceptanceStatus,
    missing_requirements: Vec<String>,
}

impl ContextAcceptance {
    /// Creates an accepted context result.
    pub fn accepted() -> Self {
        Self {
            status: ContextAcceptanceStatus::Accepted,
            missing_requirements: Vec::new(),
        }
    }

    /// Creates a partial result and records the missing context requirements.
    pub fn partial(
        missing_requirements: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ContextError> {
        let missing_requirements = collect_non_empty(missing_requirements, "missing requirement")?;

        if missing_requirements.is_empty() {
            return Err(ContextError::EmptyMissingRequirements);
        }

        Ok(Self {
            status: ContextAcceptanceStatus::Partial,
            missing_requirements,
        })
    }

    /// Creates a rejected result and records the requirements that were not
    /// satisfied by the supplied package.
    pub fn rejected(
        missing_requirements: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ContextError> {
        let missing_requirements = collect_non_empty(missing_requirements, "missing requirement")?;

        Ok(Self {
            status: ContextAcceptanceStatus::Rejected,
            missing_requirements,
        })
    }

    /// Returns the acceptance status.
    pub fn status(&self) -> ContextAcceptanceStatus {
        self.status
    }

    /// Returns requirements identified by the consumer as missing.
    pub fn missing_requirements(&self) -> &[String] {
        &self.missing_requirements
    }

    /// Returns whether the consumer accepted the supplied package.
    pub fn is_accepted(&self) -> bool {
        self.status == ContextAcceptanceStatus::Accepted
    }
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), ContextError> {
    if value.trim().is_empty() {
        Err(ContextError::EmptyValue(field))
    } else {
        Ok(())
    }
}

fn collect_non_empty<I, T>(values: I, field: &'static str) -> Result<Vec<String>, ContextError>
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    values
        .into_iter()
        .map(|value| {
            let value = value.into();

            validate_non_empty(&value, field).map(|()| value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        identity::{ArtifactId, CorrelationId, OperationId},
        operation::Operation,
    };

    fn operation_context() -> OperationContext {
        OperationContext::new(Operation::new(
            OperationId::new("context-operation").unwrap(),
            CorrelationId::new("context-correlation").unwrap(),
        ))
    }

    fn requirement() -> ContextRequirement {
        ContextRequirement::new("quranic-context", ContextRequirementLevel::Required).unwrap()
    }

    #[test]
    fn requirement_preserves_name_and_level() {
        let requirement = requirement();

        assert_eq!(requirement.name(), "quranic-context");
        assert_eq!(requirement.level(), ContextRequirementLevel::Required);
        assert!(requirement.constraints().is_empty());
    }

    #[test]
    fn requirement_supports_opaque_constraints() {
        let requirement = requirement()
            .with_constraint("version", "v12")
            .unwrap()
            .with_constraint("freshness", "canonical")
            .unwrap();

        assert_eq!(requirement.constraint("version"), Some("v12"));
        assert_eq!(requirement.constraint("freshness"), Some("canonical"));
    }

    #[test]
    fn requirement_rejects_empty_name_and_constraints() {
        assert!(ContextRequirement::new("   ", ContextRequirementLevel::Required).is_err());

        let error = requirement()
            .with_constraint("   ", "value")
            .expect_err("empty constraint key must be rejected");

        assert_eq!(error, ContextError::EmptyValue("constraint key"));
    }

    #[test]
    fn request_preserves_operation_context_and_requirement() {
        let operation = operation_context();
        let request = ContextRequest::new(operation.clone(), requirement());

        assert_eq!(request.operation(), &operation);
        assert_eq!(request.requirement().name(), "quranic-context");
        assert_eq!(
            request.operation().operation.id.as_str(),
            "context-operation"
        );
        assert_eq!(
            request.operation().operation.correlation_id.as_str(),
            "context-correlation"
        );
    }

    #[test]
    fn package_preserves_opaque_entries_and_artifacts() {
        let artifact = ArtifactReference::new(ArtifactId::new("quran-context").unwrap(), "v1");

        let package = ContextPackage::new()
            .with_entry("verse", "2:255")
            .unwrap()
            .with_artifact(artifact.clone())
            .unwrap();

        assert_eq!(package.entry("verse"), Some("2:255"));
        assert_eq!(package.artifacts(), &[artifact]);
        assert!(!package.is_empty());
    }

    #[test]
    fn package_preserves_provenance_without_interpreting_it() {
        let provenance = ProvenanceContext::new().with_attribute("source", "quran-engine");

        let package = ContextPackage::new().with_provenance(provenance.clone());

        assert_eq!(package.provenance(), &provenance);
        assert_eq!(
            package.provenance().attribute("source"),
            Some("quran-engine")
        );
    }

    #[test]
    fn package_rejects_invalid_entries_and_artifacts() {
        let mut package = ContextPackage::new();

        assert!(package.insert(" ", "value").is_err());
        assert!(package.insert("key", " ").is_err());

        let invalid = ArtifactReference::new(ArtifactId::new("artifact").unwrap(), " ");

        assert_eq!(
            package.add_artifact(invalid).unwrap_err(),
            ContextError::InvalidArtifactReference
        );
    }

    #[test]
    fn accepted_context_has_no_missing_requirements() {
        let acceptance = ContextAcceptance::accepted();

        assert_eq!(acceptance.status(), ContextAcceptanceStatus::Accepted);
        assert!(acceptance.is_accepted());
        assert!(acceptance.missing_requirements().is_empty());
    }

    #[test]
    fn partial_context_records_missing_requirements() {
        let acceptance = ContextAcceptance::partial(["provenance", "canonical-text"]).unwrap();

        assert_eq!(acceptance.status(), ContextAcceptanceStatus::Partial);
        assert!(!acceptance.is_accepted());
        assert_eq!(
            acceptance.missing_requirements(),
            &["provenance".to_owned(), "canonical-text".to_owned()]
        );
    }

    #[test]
    fn partial_context_requires_missing_requirements() {
        assert_eq!(
            ContextAcceptance::partial(std::iter::empty::<&str>()).unwrap_err(),
            ContextError::EmptyMissingRequirements
        );
    }

    #[test]
    fn rejected_context_records_missing_requirements() {
        let acceptance = ContextAcceptance::rejected(["required-context"]).unwrap();

        assert_eq!(acceptance.status(), ContextAcceptanceStatus::Rejected);
        assert_eq!(
            acceptance.missing_requirements(),
            &["required-context".to_owned()]
        );
    }

    #[test]
    fn rejected_context_can_exist_without_missing_requirement_details() {
        let acceptance = ContextAcceptance::rejected(std::iter::empty::<&str>()).unwrap();

        assert_eq!(acceptance.status(), ContextAcceptanceStatus::Rejected);
        assert!(acceptance.missing_requirements().is_empty());
    }

    #[test]
    fn context_exchange_types_are_cloneable_and_comparable() {
        let request = ContextRequest::new(operation_context(), requirement());

        let package = ContextPackage::new().with_entry("key", "value").unwrap();

        assert_eq!(request.clone(), request);
        assert_eq!(package.clone(), package);
        assert_eq!(
            ContextAcceptance::accepted().clone(),
            ContextAcceptance::accepted()
        );
    }
}
