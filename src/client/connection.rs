//! Phase 7 boundary for universal client connection mechanism.
//!
//! A client connection represents an outgoing connection from a client
//! to a concrete server engine instance. It handles the transport-level
//! details of establishing and maintaining a connection.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::{EngineId, EngineInstanceId};
use crate::transport::{BoxedFuture, TransportError};

/// The lifecycle state of a client connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientConnectionState {
    /// The connection is being established.
    Connecting,
    /// The connection is open and ready for traffic.
    Open,
    /// The connection is in the process of being closed.
    Closing,
    /// The connection has been closed.
    Closed,
}

impl ClientConnectionState {
    /// Returns true if the connection is open and may be used.
    pub fn is_open(&self) -> bool {
        matches!(self, ClientConnectionState::Open)
    }
}

/// A client connection to a concrete engine instance.
///
/// A client connection is an outgoing connection from a client to a
/// particular server instance. Both the logical engine identity and the
/// concrete instance identity are exposed so callers cannot lose the
/// distinction between an engine and one of its runtime instances.
pub trait ClientConnection: Send + Sync {
    /// Returns the logical engine ID of the peer at the other end.
    fn peer_engine(&self) -> &EngineId;

    /// Returns the concrete engine instance ID of the peer at the other end.
    fn peer_instance(&self) -> &EngineInstanceId;

    /// Returns the current state of this connection.
    fn state(&self) -> ClientConnectionState;

    /// Sends a universal request and receives a universal response.
    fn call(&self, request: UniversalRequest) -> BoxedFuture<UniversalResponse, TransportError>;

    /// Closes the connection.
    fn close(&mut self);
}

/// A factory for creating client connections.
pub trait ClientConnectionFactory: Send + Sync {
    /// Creates a new client connection to the specified concrete engine
    /// instance.
    fn connect(
        &self,
        target: &EngineInstanceId,
    ) -> BoxedFuture<Box<dyn ClientConnection>, TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_connection_state_transitions_are_correct() {
        assert!(!ClientConnectionState::Connecting.is_open());
        assert!(ClientConnectionState::Open.is_open());
        assert!(!ClientConnectionState::Closing.is_open());
        assert!(!ClientConnectionState::Closed.is_open());
    }

    #[test]
    fn client_connection_state_is_open_only_when_open() {
        let open_state = ClientConnectionState::Open;
        let connecting_state = ClientConnectionState::Connecting;
        let closing_state = ClientConnectionState::Closing;
        let closed_state = ClientConnectionState::Closed;

        assert!(open_state.is_open());
        assert!(!connecting_state.is_open());
        assert!(!closing_state.is_open());
        assert!(!closed_state.is_open());
    }
}
