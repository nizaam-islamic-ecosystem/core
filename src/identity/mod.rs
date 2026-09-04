//! Distinct, validated identifiers used across Nizaam Core contracts.

use core::fmt;

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
            pub fn new(value: impl Into<String>) -> Result<Self, $crate::identity::InvalidIdentity> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err($crate::identity::InvalidIdentity);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::identity::InvalidIdentity;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

mod artifact;
mod capability;
mod contract;
mod engine;
mod message;
mod operation;
mod plan;

pub use artifact::ArtifactId;
pub use capability::CapabilityId;
pub use contract::ContractId;
pub use engine::{EngineId, EngineInstanceId};
pub use message::{CorrelationId, MessageId};
pub use operation::OperationId;
pub use plan::{AttemptId, NodeId, PlanId};

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
