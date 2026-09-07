//! Capability System: engine owned capability registration and dispatch mechanisms.
//!
//! This module provides the Core mechanism for engines to expose, register,
//! locate, and invoke capabilities. Core provides the capability mechanism only;
//! capability names, typed requests, workflows, and results remain engine owned.

use core::fmt;

use crate::identity::{CapabilityId, ContractId};

pub mod definition;
pub mod dispatch;
pub mod handler;
pub mod registry;

pub use definition::{CapabilityDefinition, CapabilityDefinitionError};
pub use dispatch::{CapabilityDispatchResult, dispatch};
pub use handler::{BoxedCapabilityHandler, CapabilityHandler, CapabilityResponse, arc_handler};
pub use registry::{CapabilityEntry, CapabilityRegistry, RegistryError};

/// The input presented to a capability handler during dispatch.
///
/// This struct carries the engine-agnostic inputs that Core uses to invoke
/// a capability. The payload_bytes are opaque to Core; their meaning belongs
/// to the engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityInvocation {
    capability_id: CapabilityId,
    contract_id: Option<ContractId>,
    payload_bytes: Vec<u8>,
}

impl CapabilityInvocation {
    /// Creates a new capability invocation.
    pub fn new(
        capability_id: CapabilityId,
        contract_id: ContractId,
        payload_bytes: Vec<u8>,
    ) -> Self {
        Self {
            capability_id,
            contract_id: Some(contract_id),
            payload_bytes,
        }
    }

    /// Returns the target capability identifier.
    pub fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }

    /// Returns the optional contract identifier.
    pub fn contract_id(&self) -> Option<&ContractId> {
        self.contract_id.as_ref()
    }

    /// Returns the request payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload_bytes
    }
}

/// The result produced by a capability handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityOutcome {
    response: Vec<u8>,
}

impl CapabilityOutcome {
    /// Creates a new capability outcome from the response bytes.
    pub fn new(response: impl Into<Vec<u8>>) -> Self {
        Self {
            response: response.into(),
        }
    }

    /// Consumes self and returns the response bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.response
    }

    /// Returns a reference to the response bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.response
    }
}

/// Errors that can occur during capability operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    /// The capability is not registered in the registry.
    Unknown,
    /// The execution was cancelled.
    Cancelled,
    /// The execution deadline expired.
    DeadlineExpired,
    /// The handler returned an error.
    HandlerFailed(String),
    /// The capability definition is invalid.
    InvalidDefinition,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilityError::Unknown => formatter.write_str("capability not found"),
            CapabilityError::Cancelled => formatter.write_str("capability execution was cancelled"),
            CapabilityError::DeadlineExpired => {
                formatter.write_str("capability execution deadline expired")
            }
            CapabilityError::HandlerFailed(msg) => {
                write!(formatter, "capability handler failed: {msg}")
            }
            CapabilityError::InvalidDefinition => {
                formatter.write_str("capability definition is invalid")
            }
        }
    }
}

impl std::error::Error for CapabilityError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{CapabilityId, ContractId};

    fn make_capability_id() -> CapabilityId {
        CapabilityId::new("test.capability").unwrap()
    }

    fn make_contract_id() -> ContractId {
        ContractId::new("test.contract").unwrap()
    }

    #[test]
    fn invocation_new_stores_all_fields() {
        let cap_id = make_capability_id();
        let contract_id = make_contract_id();
        let payload = b"some payload".to_vec();

        let invocation =
            CapabilityInvocation::new(cap_id.clone(), contract_id.clone(), payload.clone());

        assert_eq!(invocation.capability_id(), &cap_id);
        assert_eq!(invocation.contract_id(), Some(&contract_id));
        assert_eq!(invocation.payload_bytes(), payload.as_slice());
    }

    #[test]
    fn invocation_payload_bytes_is_empty_when_empty() {
        let invocation =
            CapabilityInvocation::new(make_capability_id(), make_contract_id(), Vec::new());
        assert!(invocation.payload_bytes().is_empty());
    }

    #[test]
    fn invocation_clone_preserves_all_fields() {
        let invocation = CapabilityInvocation::new(
            make_capability_id(),
            make_contract_id(),
            b"clone me".to_vec(),
        );
        let cloned = invocation.clone();
        assert_eq!(invocation, cloned);
    }

    #[test]
    fn outcome_new_stores_response_bytes() {
        let bytes = b"response data".to_vec();
        let outcome = CapabilityOutcome::new(bytes.clone());
        assert_eq!(outcome.as_bytes(), bytes.as_slice());
    }

    #[test]
    fn outcome_into_bytes_consumes_self() {
        let bytes = b"consume me".to_vec();
        let outcome = CapabilityOutcome::new(bytes.clone());
        let extracted = outcome.into_bytes();
        assert_eq!(extracted, bytes);
    }

    #[test]
    fn outcome_new_accepts_string_via_into() {
        let outcome = CapabilityOutcome::new("hello world".to_string());
        assert_eq!(outcome.as_bytes(), b"hello world");
    }

    #[test]
    fn outcome_new_accepts_bytes_slice_via_into() {
        let outcome = CapabilityOutcome::new(&b"slice"[..]);
        assert_eq!(outcome.as_bytes(), b"slice");
    }

    #[test]
    fn outcome_as_bytes_returns_empty_for_empty_response() {
        let outcome = CapabilityOutcome::new(Vec::<u8>::new());
        assert!(outcome.as_bytes().is_empty());
    }

    #[test]
    fn error_display_unknown() {
        let error = CapabilityError::Unknown;
        assert_eq!(error.to_string(), "capability not found");
    }

    #[test]
    fn error_display_cancelled() {
        let error = CapabilityError::Cancelled;
        assert_eq!(error.to_string(), "capability execution was cancelled");
    }

    #[test]
    fn error_display_deadline_expired() {
        let error = CapabilityError::DeadlineExpired;
        assert_eq!(error.to_string(), "capability execution deadline expired");
    }

    #[test]
    fn error_display_handler_failed_includes_message() {
        let error = CapabilityError::HandlerFailed("disk full".to_string());
        assert_eq!(error.to_string(), "capability handler failed: disk full");
    }

    #[test]
    fn error_display_handler_failed_with_empty_message() {
        let error = CapabilityError::HandlerFailed(String::new());
        assert_eq!(error.to_string(), "capability handler failed: ");
    }

    #[test]
    fn error_display_invalid_definition() {
        let error = CapabilityError::InvalidDefinition;
        assert_eq!(error.to_string(), "capability definition is invalid");
    }

    #[test]
    fn error_implements_std_error_trait() {
        fn assert_error<E: std::error::Error>() {}
        assert_error::<CapabilityError>();
    }

    #[test]
    fn invocation_and_outcome_equality() {
        let invocation1 =
            CapabilityInvocation::new(make_capability_id(), make_contract_id(), b"same".to_vec());
        let invocation2 =
            CapabilityInvocation::new(make_capability_id(), make_contract_id(), b"same".to_vec());
        let outcome1 = CapabilityOutcome::new(b"same".to_vec());
        let outcome2 = CapabilityOutcome::new(b"same".to_vec());
        assert_eq!(invocation1, invocation2);
        assert_eq!(outcome1, outcome2);
    }
}
