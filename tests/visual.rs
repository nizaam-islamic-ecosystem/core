//! Phase 16 visual verification integration-test entrypoint.

#[path = "visual/artifacts_provenance.rs"]
mod artifacts_provenance;
#[path = "visual/concurrency_resources.rs"]
mod concurrency_resources;
#[path = "visual/configuration_context.rs"]
mod configuration_context;
#[path = "visual/control_plane.rs"]
mod control_plane;
#[path = "visual/engine_communication.rs"]
mod engine_communication;
#[path = "visual/events_observability.rs"]
mod events_observability;
#[path = "visual/fault_recovery.rs"]
mod fault_recovery;
#[path = "visual/full_system.rs"]
mod full_system;
#[path = "visual/lifecycle_runtime.rs"]
mod lifecycle_runtime;
#[path = "visual/message_framing.rs"]
mod message_framing;
#[path = "visual/retry_idempotency.rs"]
mod retry_idempotency;
#[path = "visual/security.rs"]
mod security;
#[path = "visual/streaming.rs"]
mod streaming;
#[path = "visual/support.rs"]
mod support;
