//! Shared, domain agnostic mechanisms and contracts for Nizaam engines.
//!
//! This crate intentionally contains no Quran, Hadith, Arabic, search, or
//! other engine specific semantics. It is a library crate consumed by domain
//! and infrastructure engines.

pub mod capability;
pub mod identity;
pub mod logging;
pub mod operation;
pub mod prelude;
pub mod status;

pub mod artifact;
pub mod client;
pub mod config;
pub mod contracts;
pub mod control_plane;
pub mod error;
pub mod events;
pub mod health;
pub mod idempotency;
pub mod middleware;
pub mod observability;
pub mod provenance;
pub mod retry;
pub mod runtime;
pub mod sdk;
pub mod security;
pub mod server;
pub mod streaming;
pub mod transport;
