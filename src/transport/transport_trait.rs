//! The transport trait and error types for universal engine communication.
//!
//! Transport is the provider-neutral abstraction for sending and receiving
//! byte-oriented messages between engines. Concrete transports (in-memory,
//! gRPC, HTTP, etc.) implement the `Transport` trait. Core supplies one
//! in-memory reference implementation; all other transports live outside Core.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineId;
use core::pin::Pin;

/// A boxed future returned by transport operations.
pub type BoxedFuture<T, E> = Pin<Box<dyn futures::Future<Output = Result<T, E>> + Send>>;

/// Errors that can occur during transport operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportError {
    /// The transport is not connected to the target.
    Disconnected,
    /// The connection has been closed by the peer.
    Closed,
    /// Failed to encode the request payload.
    Encode(String),
    /// Failed to decode the response payload.
    Decode(String),
    /// The operation was cancelled.
    Cancelled,
    /// The operation deadline expired.
    DeadlineExpired,
    /// The operation timed out waiting for a response.
    Timeout,
    /// The peer returned an error.
    Peer(String),
}

impl TransportError {
    /// Returns true if this error indicates the transport should retry.
    pub fn is_retryable(&self) -> bool {
        matches!(self, TransportError::Timeout | TransportError::Disconnected)
    }
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::Disconnected => write!(f, "transport is not connected"),
            TransportError::Closed => write!(f, "connection closed by peer"),
            TransportError::Encode(msg) => write!(f, "encode error: {}", msg),
            TransportError::Decode(msg) => write!(f, "decode error: {}", msg),
            TransportError::Cancelled => write!(f, "operation was cancelled"),
            TransportError::DeadlineExpired => write!(f, "operation deadline expired"),
            TransportError::Timeout => write!(f, "operation timed out"),
            TransportError::Peer(msg) => write!(f, "peer error: {}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

/// Result type alias for transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

/// The transport trait for sending requests and receiving responses.
///
/// Implementors handle the concrete wire protocol (in-memory channel,
/// gRPC, HTTP, etc.). The trait is `Send + Sync` so it may be shared
/// across concurrent client calls.
pub trait Transport: Send + Sync {
    /// Sends a universal request and receives a universal response.
    fn call(
        &self,
        target: &EngineId,
        request: UniversalRequest,
    ) -> BoxedFuture<UniversalResponse, TransportError>;

    /// Returns true if the transport believes it is connected to the target.
    fn is_connected(&self, target: &EngineId) -> bool;

    /// Returns the list of engine instances this transport is connected to.
    fn connected_targets(&self) -> Vec<EngineId>;
}
