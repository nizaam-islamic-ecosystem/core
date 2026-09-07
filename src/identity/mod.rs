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

    // -------------------------------------------------------------------------
    // Per-type tests: each identity type has its own test function.
    // -------------------------------------------------------------------------

    #[test]
    fn artifact_id_constructs_and_rejects_empty() {
        let id = ArtifactId::new("artifact-1").unwrap();
        assert_eq!(id.as_str(), "artifact-1");
        assert_eq!(id.to_string(), "artifact-1");
        assert!(ArtifactId::new("").is_err());
        assert!(ArtifactId::new("   ").is_err());
    }

    #[test]
    fn capability_id_constructs_and_rejects_empty() {
        let id = CapabilityId::new("engine.get-data").unwrap();
        assert_eq!(id.as_str(), "engine.get-data");
        assert_eq!(id.to_string(), "engine.get-data");
        assert!(CapabilityId::new("").is_err());
    }

    #[test]
    fn contract_id_constructs_and_rejects_empty() {
        let id = ContractId::new("v1.get-weather").unwrap();
        assert_eq!(id.as_str(), "v1.get-weather");
        assert_eq!(id.to_string(), "v1.get-weather");
        assert!(ContractId::new("").is_err());
    }

    #[test]
    fn engine_id_constructs_and_rejects_empty() {
        let id = EngineId::new("weather-engine").unwrap();
        assert_eq!(id.as_str(), "weather-engine");
        assert_eq!(id.to_string(), "weather-engine");
        assert!(EngineId::new("").is_err());
    }

    #[test]
    fn engine_instance_id_constructs_and_rejects_empty() {
        let id = EngineInstanceId::new("weather-engine-001").unwrap();
        assert_eq!(id.as_str(), "weather-engine-001");
        assert_eq!(id.to_string(), "weather-engine-001");
        assert!(EngineInstanceId::new("").is_err());
    }

    #[test]
    fn message_id_constructs_and_rejects_empty() {
        let id = MessageId::new("msg-abc-123").unwrap();
        assert_eq!(id.as_str(), "msg-abc-123");
        assert_eq!(id.to_string(), "msg-abc-123");
        assert!(MessageId::new("").is_err());
    }

    #[test]
    fn correlation_id_constructs_and_rejects_empty() {
        let id = CorrelationId::new("corr-batch-42").unwrap();
        assert_eq!(id.as_str(), "corr-batch-42");
        assert_eq!(id.to_string(), "corr-batch-42");
        assert!(CorrelationId::new("").is_err());
    }

    #[test]
    fn operation_id_constructs_and_rejects_empty() {
        let id = OperationId::new("op-12345").unwrap();
        assert_eq!(id.as_str(), "op-12345");
        assert_eq!(id.to_string(), "op-12345");
        assert!(OperationId::new("").is_err());
    }

    #[test]
    fn plan_id_constructs_and_rejects_empty() {
        let id = PlanId::new("plan-daily-batch").unwrap();
        assert_eq!(id.as_str(), "plan-daily-batch");
        assert_eq!(id.to_string(), "plan-daily-batch");
        assert!(PlanId::new("").is_err());
    }

    #[test]
    fn node_id_constructs_and_rejects_empty() {
        let id = NodeId::new("node-primary").unwrap();
        assert_eq!(id.as_str(), "node-primary");
        assert_eq!(id.to_string(), "node-primary");
        assert!(NodeId::new("").is_err());
    }

    #[test]
    fn attempt_id_constructs_and_rejects_empty() {
        let id = AttemptId::new("attempt-7").unwrap();
        assert_eq!(id.as_str(), "attempt-7");
        assert_eq!(id.to_string(), "attempt-7");
        assert!(AttemptId::new("").is_err());
    }

    // -------------------------------------------------------------------------
    // FromStr round-trip for every identity type
    // -------------------------------------------------------------------------

    #[test]
    fn all_identity_types_parse_from_str() {
        use std::str::FromStr;

        assert_eq!(ArtifactId::from_str("a1").unwrap().as_str(), "a1");
        assert_eq!(CapabilityId::from_str("c1").unwrap().as_str(), "c1");
        assert_eq!(ContractId::from_str("ct1").unwrap().as_str(), "ct1");
        assert_eq!(EngineId::from_str("e1").unwrap().as_str(), "e1");
        assert_eq!(EngineInstanceId::from_str("ei1").unwrap().as_str(), "ei1");
        assert_eq!(MessageId::from_str("m1").unwrap().as_str(), "m1");
        assert_eq!(CorrelationId::from_str("co1").unwrap().as_str(), "co1");
        assert_eq!(OperationId::from_str("op1").unwrap().as_str(), "op1");
        assert_eq!(PlanId::from_str("pl1").unwrap().as_str(), "pl1");
        assert_eq!(NodeId::from_str("n1").unwrap().as_str(), "n1");
        assert_eq!(AttemptId::from_str("at1").unwrap().as_str(), "at1");
    }

    // -------------------------------------------------------------------------
    // AsRef<str> for every identity type
    // -------------------------------------------------------------------------

    #[test]
    fn all_identity_types_implement_as_ref_str() {
        fn check<T: AsRef<str>>(id: T, expected: &str) {
            assert_eq!(id.as_ref(), expected);
        }
        check(ArtifactId::new("a").unwrap(), "a");
        check(CapabilityId::new("c").unwrap(), "c");
        check(ContractId::new("ct").unwrap(), "ct");
        check(EngineId::new("e").unwrap(), "e");
        check(EngineInstanceId::new("ei").unwrap(), "ei");
        check(MessageId::new("m").unwrap(), "m");
        check(CorrelationId::new("co").unwrap(), "co");
        check(OperationId::new("op").unwrap(), "op");
        check(PlanId::new("pl").unwrap(), "pl");
        check(NodeId::new("n").unwrap(), "n");
        check(AttemptId::new("at").unwrap(), "at");
    }

    // -------------------------------------------------------------------------
    // Clone, Eq, Hash for identity types
    // -------------------------------------------------------------------------

    #[test]
    fn identity_types_derive_clone_eq_hash() {
        use std::collections::HashMap;

        let id1 = ArtifactId::new("artifact-x").unwrap();
        let id2 = ArtifactId::new("artifact-x").unwrap();
        let id3 = ArtifactId::new("artifact-y").unwrap();

        assert_eq!(id1, id2);
        assert_eq!(id1.clone(), id2);

        let mut map: HashMap<ArtifactId, &str> = HashMap::new();
        map.insert(id1.clone(), "value1");
        assert_eq!(map.get(&id2), Some(&"value1"));
        assert!(!map.contains_key(&id3));
    }

    // -------------------------------------------------------------------------
    // Distinct types are not interchangeable
    // -------------------------------------------------------------------------

    #[test]
    fn distinct_identity_types_are_not_interchangeable() {
        use std::collections::HashSet;

        // CapabilityId and ContractId are different types even with the same string.
        let cap: CapabilityId = CapabilityId::new("shared").unwrap();
        let contract: ContractId = ContractId::new("shared").unwrap();

        // A composite tuple set holds both entries together.
        let mut combined: HashSet<(CapabilityId, ContractId)> = HashSet::new();
        combined.insert((cap, contract));
        assert_eq!(combined.len(), 1);
    }

    // -------------------------------------------------------------------------
    // InvalidIdentity error behaviors
    // -------------------------------------------------------------------------

    #[test]
    fn invalid_identity_error_behaviors() {
        let err = InvalidIdentity;
        assert_eq!(err.to_string(), "a Nizaam identity must not be empty");
        assert_eq!(err, InvalidIdentity);
        assert_eq!(err.clone(), InvalidIdentity);
    }
}
