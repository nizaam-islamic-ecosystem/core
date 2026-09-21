//! Connection trait and connection state for engine transport.
//!
//! A connection is an open duplex channel to one concrete engine instance.
//! Connections are produced by a `ConnectionFactory` and consumed by transport
//! implementations. Core supplies the abstraction; concrete connection
//! types live alongside their transport.

use crate::identity::{EngineId, EngineInstanceId};
use crate::transport::stream::{ByteSink, ByteSource};

/// The lifecycle state of a connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    /// The connection is being established.
    Connecting,
    /// The connection is open and ready for traffic.
    Open,
    /// The connection is in the process of being closed.
    Closing,
    /// The connection has been closed.
    Closed,
}

impl ConnectionState {
    /// Returns true if the connection is open and may be used.
    pub fn is_open(&self) -> bool {
        matches!(self, ConnectionState::Open)
    }
}

/// A connection to a concrete engine instance.
///
/// A connection is a duplex pair of byte streams: a sink for outgoing
/// bytes and a source for incoming bytes. Each transport implementation
/// defines its own concrete connection type.
///
/// The logical [`EngineId`] identifies the engine represented by the peer,
/// while [`EngineInstanceId`] identifies the concrete runtime instance at
/// the other end of this connection.
pub trait Connection: Send + Sync {
    /// Returns the logical engine id of the peer at the other end.
    fn peer_engine(&self) -> &EngineId;

    /// Returns the concrete engine instance id of the peer at the other end.
    fn peer_instance(&self) -> &EngineInstanceId;

    /// Returns the current state of this connection.
    fn state(&self) -> ConnectionState;

    /// Returns a reference to the outgoing byte sink.
    fn sink(&self) -> &dyn ByteSink;

    /// Returns a reference to the incoming byte source.
    fn source(&self) -> &dyn ByteSource;

    /// Closes the connection.
    fn close(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_transitions_are_correct() {
        assert!(!ConnectionState::Connecting.is_open());
        assert!(ConnectionState::Open.is_open());
        assert!(!ConnectionState::Closing.is_open());
        assert!(!ConnectionState::Closed.is_open());
    }

    #[test]
    fn connection_state_is_open_only_when_open() {
        let open_state = ConnectionState::Open;
        let connecting_state = ConnectionState::Connecting;
        let closing_state = ConnectionState::Closing;
        let closed_state = ConnectionState::Closed;

        assert!(open_state.is_open());
        assert!(!connecting_state.is_open());
        assert!(!closing_state.is_open());
        assert!(!closed_state.is_open());
    }
}
