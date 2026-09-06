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
