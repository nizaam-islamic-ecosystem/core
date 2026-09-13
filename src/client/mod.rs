//! Phase 7 boundary for the universal engine client mechanism.
//!
//! The universal client provides a high-level interface for sending universal
//! requests through an abstract transport and receiving universal responses.
//! Typed capability clients build on the universal client rather than
//! creating separate transport stacks.

pub mod connection;
pub mod universal;

pub use universal::UniversalClient;
