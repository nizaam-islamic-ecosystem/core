//! Phase 7 boundary for transport abstractions, connections, and streams.
//!
//! Transport is the provider-neutral abstraction for sending and receiving
//! byte-oriented messages between engines. Concrete transports (in-memory,
//! gRPC, HTTP, etc.) implement the `Transport` trait. Core supplies one
//! in-memory reference implementation; all other transports live outside Core.

pub mod connection;
pub mod in_memory;
pub mod stream;
pub mod transport_trait;

pub use connection::{Connection, ConnectionState};
pub use in_memory::InMemoryTransport;
pub use stream::{ByteSink, ByteSource, MessageStream, StreamError};
pub use transport_trait::{BoxedFuture, Transport, TransportError, TransportResult};
