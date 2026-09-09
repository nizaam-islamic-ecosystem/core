//! Connection trait and connection state for engine transport.
//!
//! A connection is an open duplex channel to one peer engine. Connections
//! are produced by a `ConnectionFactory` and consumed by transport
//! implementations. Core supplies the abstraction; concrete connection
//! types live alongside their transport.

use crate::identity::EngineId;
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

/// A connection to a peer engine.
///
/// A connection is a duplex pair of byte streams: a sink for outgoing
/// bytes and a source for incoming bytes. Each transport implementation
/// defines its own concrete connection type.
pub trait Connection: Send + Sync {
    /// Returns the engine id of the peer at the other end of this connection.
    fn peer(&self) -> &EngineId;

    /// Returns the current state of this connection.
    fn state(&self) -> ConnectionState;

    /// Returns a reference to the outgoing byte sink.
    fn sink(&self) -> &dyn ByteSink;

    /// Returns a reference to the incoming byte source.
    fn source(&self) -> &dyn ByteSource;

    /// Closes the connection.
    fn close(&mut self);
}
