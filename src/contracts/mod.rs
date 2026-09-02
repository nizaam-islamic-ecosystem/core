//! Phase 2 boundary for universal contracts, envelopes, and compatibility.

pub mod compatibility;
pub mod descriptor;
pub mod envelope;
pub mod metadata;
pub mod request;
pub mod response;
pub mod validation;

pub use descriptor::{
    ContractDescriptor, EncodedPayload, EncodingError, Interaction, InvalidDescriptor,
    PayloadCodec, PayloadDescriptor, RawPayloadCodec, Version,
};
pub use metadata::{ContractMetadata, ExecutionMetadata, Participants, RequirementsMetadata};
