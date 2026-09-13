//! Phase 7 boundary for universal client connection mechanism.
//!
//! A client connection represents an outgoing connection from a client
//! to a server engine. It handles the transport-level details of
//! establishing and maintaining a connection.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineId;
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

/// A client connection to a peer engine.
///
/// A client connection is an outgoing connection from a client to a server.
pub trait ClientConnection: Send + Sync {
    /// Returns the engine id of the peer at the other end of this connection.
    fn peer(&self) -> &EngineId;

    /// Returns the current state of this connection.
    fn state(&self) -> ClientConnectionState;

    /// Sends a universal request and receives a universal response.
    fn call(&self, request: UniversalRequest) -> BoxedFuture<UniversalResponse, TransportError>;

    /// Closes the connection.
    fn close(&mut self);
}

/// A factory for creating client connections.
pub trait ClientConnectionFactory: Send + Sync {
    /// Creates a new client connection to the specified target.
    fn connect(&self, target: &EngineId) -> BoxedFuture<Box<dyn ClientConnection>, TransportError>;
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
