//! Shared, domain agnostic mechanisms and contracts for Nizaam engines.
//!
//! This crate intentionally contains no Quran, Hadith, Arabic, search, or
//! other engine specific semantics. It is a library crate consumed by domain
//! and infrastructure engines.

pub mod identity;
pub mod logging;
pub mod operation;
pub mod prelude;
pub mod status;

// These private module boundaries establish the approved Core structure. They
// become public only when their corresponding phase has a deliberate API.
pub(crate) mod artifacts;
pub(crate) mod capability;
pub(crate) mod client;
pub(crate) mod config;
pub(crate) mod conformance;
pub mod contracts;
pub(crate) mod control_plane;
pub mod error;
pub(crate) mod events;
pub(crate) mod health;
pub(crate) mod idempotency;
pub(crate) mod middleware;
pub(crate) mod observability;
pub mod provenance;
pub(crate) mod retry;
pub mod runtime;
pub(crate) mod sdk;
pub mod security;
pub(crate) mod server;
pub(crate) mod streaming;
pub(crate) mod transport;
