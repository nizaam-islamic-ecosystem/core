//! Capability definitions describe engine exposed capabilities.
//!
//! A `CapabilityDefinition` is a value type that describes what a capability
//! represents without learning its domain semantics. It is owned by an engine
//! and registered in the `CapabilityRegistry`.

use core::fmt;

use crate::{
    contracts::Version,
    identity::{CapabilityId, EngineId},
};

/// Error returned when a capability definition is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityDefinitionError {
    /// The capability name is empty.
    EmptyName,
    /// The description, if provided, is empty.
    EmptyDescription,
    /// The version is invalid.
    InvalidVersion,
}

impl fmt::Display for CapabilityDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilityDefinitionError::EmptyName => {
                formatter.write_str("a capability definition requires a non-empty name")
            }
            CapabilityDefinitionError::EmptyDescription => {
                formatter.write_str("a capability description, if provided, must not be empty")
            }
            CapabilityDefinitionError::InvalidVersion => {
                formatter.write_str("a capability definition requires a valid version")
            }
        }
    }
}

impl std::error::Error for CapabilityDefinitionError {}

/// A static description of a capability owned by an engine.
///
/// Capabilities are identified by `CapabilityId` and owned by an `EngineId`.
/// The definition carries no payload schema; payload meaning stays outside Core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityDefinition {
    capability_id: CapabilityId,
    owning_engine: EngineId,
    name: String,
    description: Option<String>,
    version: Option<Version>,
}

impl CapabilityDefinition {
    /// Creates a new capability definition.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityDefinitionError::EmptyName` if the name is empty or whitespace.
    /// Returns `CapabilityDefinitionError::EmptyDescription` if a description is provided but empty.
    /// Returns `CapabilityDefinitionError::InvalidVersion` if the version is invalid.
    pub fn new(
        capability_id: CapabilityId,
        owning_engine: EngineId,
        name: impl Into<String>,
    ) -> Result<Self, CapabilityDefinitionError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CapabilityDefinitionError::EmptyName);
        }
        Ok(Self {
            capability_id,
            owning_engine,
            name,
            description: None,
            version: None,
        })
    }

    /// Sets the optional description.
    ///
    /// # Errors
    ///
    /// Returns `CapabilityDefinitionError::EmptyDescription` if the description is empty after trim.
    pub fn with_description(
        mut self,
        description: impl Into<String>,
    ) -> Result<Self, CapabilityDefinitionError> {
        let description = description.into();
        if description.trim().is_empty() {
            return Err(CapabilityDefinitionError::EmptyDescription);
        }
        self.description = Some(description);
        Ok(self)
    }

    /// Sets the optional version.
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Returns the capability identifier.
    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }

    /// Returns the owning engine identifier.
    pub fn owning_engine(&self) -> &EngineId {
        &self.owning_engine
    }

    /// Returns the human-readable name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional description.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the optional version.
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Version;
    use crate::identity::{CapabilityId, EngineId};

    fn valid_definition() -> CapabilityDefinition {
        CapabilityDefinition::new(
            CapabilityId::new("test.capability").unwrap(),
            EngineId::new("test.engine").unwrap(),
            "Test Capability",
        )
        .unwrap()
    }

    #[test]
    fn definition_accepts_valid_inputs() {
        let def = valid_definition();
        assert_eq!(def.name(), "Test Capability");
        assert_eq!(def.description(), None);
        assert_eq!(def.version(), None);
    }

    #[test]
    fn definition_rejects_empty_name() {
        let result = CapabilityDefinition::new(
            CapabilityId::new("test.cap").unwrap(),
            EngineId::new("engine").unwrap(),
            "",
        );
        assert!(matches!(result, Err(CapabilityDefinitionError::EmptyName)));

        let result = CapabilityDefinition::new(
            CapabilityId::new("test.cap").unwrap(),
            EngineId::new("engine").unwrap(),
            "   ",
        );
        assert!(matches!(result, Err(CapabilityDefinitionError::EmptyName)));
    }

    #[test]
    fn definition_rejects_empty_description() {
        let result = valid_definition().with_description("");
        assert!(matches!(
            result,
            Err(CapabilityDefinitionError::EmptyDescription)
        ));

        let result = valid_definition().with_description("   ");
        assert!(matches!(
            result,
            Err(CapabilityDefinitionError::EmptyDescription)
        ));
    }

    #[test]
    fn definition_accepts_valid_description() {
        let def = valid_definition()
            .with_description("A test capability")
            .unwrap();
        assert_eq!(def.description(), Some("A test capability"));
    }

    #[test]
    fn definition_accepts_version() {
        let def = valid_definition().with_version(Version::new(1, 2, 3));
        assert_eq!(def.version().unwrap().major(), 1);
        assert_eq!(def.version().unwrap().minor(), 2);
        assert_eq!(def.version().unwrap().patch(), 3);
    }
}
