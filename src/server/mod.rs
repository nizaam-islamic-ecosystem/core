//! Phase 7 boundary for the engine server boundary.
//!
//! The engine server handles incoming requests from clients and
//! dispatches them to capability handlers. It manages connection
//! state and request routing.

pub mod engine;

pub use engine::{EngineServer, RequestHandler, ServerState, handle_request};
