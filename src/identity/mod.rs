//! Distinct, validated identifiers used across Nizaam Core contracts.

use core::fmt;
use core::str::FromStr;

/// Error returned when a Core identity is empty or consists only of whitespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidIdentity;

impl fmt::Display for InvalidIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Nizaam identity must not be empty")
    }
}

impl std::error::Error for InvalidIdentity {}

macro_rules! identity {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates an identity from a nonempty, non-whitespace value.
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentity> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(InvalidIdentity);
                }
                Ok(Self(value))
            }

            /// Returns the stable textual representation of this identity.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = InvalidIdentity;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

identity!(/// Identifies a protocol message. Message identity is distinct from operation identity.
    MessageId);
identity!(/// Identifies a platform operation across its attempts and messages.
    OperationId);
identity!(/// Links related messages and operations for correlation.
    CorrelationId);
identity!(/// Identifies an engine type.
    EngineId);
identity!(/// Identifies a running instance of an engine.
    EngineInstanceId);
identity!(/// Identifies an engine exposed capability.
    CapabilityId);
identity!(/// Identifies a versioned contract.
    ContractId);
identity!(/// Identifies a platform plan.
    PlanId);
identity!(/// Identifies a node within an operation or plan.
    NodeId);
identity!(/// Identifies an individual execution attempt.
    AttemptId);
identity!(/// Identifies an artifact independently of messages and operations.
    ArtifactId);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identities_preserve_their_distinct_types() {
        let operation = OperationId::new("operation-1").unwrap();
        let message = MessageId::new("message-1").unwrap();
        let mut ids = HashSet::new();
        ids.insert(operation);

        assert_eq!(message.as_str(), "message-1");
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn identities_reject_empty_values() {
        assert!(EngineId::new("").is_err());
        assert!(CapabilityId::new("   ").is_err());
    }
}
