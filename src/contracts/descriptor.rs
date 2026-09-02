use core::fmt;

use crate::identity::{CapabilityId, ContractId};

/// A validated semantic version for a contract or schema.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn major(&self) -> u32 {
        self.major
    }

    pub const fn minor(&self) -> u32 {
        self.minor
    }

    pub const fn patch(&self) -> u32 {
        self.patch
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Identifies the interaction represented by a contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Interaction {
    Request,
    Response,
    Event,
}

/// Describes an encoded payload without interpreting its domain meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadDescriptor {
    media_type: String,
    schema_version: Version,
}

/// An opaque payload owned and interpreted by an engine capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedPayload {
    descriptor: PayloadDescriptor,
    bytes: Vec<u8>,
}

/// Encodes and decodes opaque capability payload bytes.
pub trait PayloadCodec {
    fn encode(&self, bytes: &[u8]) -> Result<Vec<u8>, EncodingError>;
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u8>, EncodingError>;
}

/// A codec for payloads that are already encoded by the owning engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawPayloadCodec;

impl PayloadCodec for RawPayloadCodec {
    fn encode(&self, bytes: &[u8]) -> Result<Vec<u8>, EncodingError> {
        Ok(bytes.to_vec())
    }

    fn decode(&self, bytes: &[u8]) -> Result<Vec<u8>, EncodingError> {
        Ok(bytes.to_vec())
    }
}

/// An encoding or decoding failure supplied by a payload codec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodingError {
    InvalidPayload,
}

impl fmt::Display for EncodingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the payload codec rejected the payload")
    }
}

impl std::error::Error for EncodingError {}

impl EncodedPayload {
    pub fn new(descriptor: PayloadDescriptor, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            descriptor,
            bytes: bytes.into(),
        }
    }

    pub const fn descriptor(&self) -> &PayloadDescriptor {
        &self.descriptor
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl PayloadDescriptor {
    pub fn new(
        media_type: impl Into<String>,
        schema_version: Version,
    ) -> Result<Self, InvalidDescriptor> {
        let media_type = media_type.into();
        if media_type.trim().is_empty() {
            return Err(InvalidDescriptor::EmptyMediaType);
        }

        Ok(Self {
            media_type,
            schema_version,
        })
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub const fn schema_version(&self) -> &Version {
        &self.schema_version
    }
}

/// Describes a versioned contract and its payload shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractDescriptor {
    pub contract_id: ContractId,
    pub capability_id: CapabilityId,
    pub version: Version,
    pub interaction: Interaction,
    pub payload: PayloadDescriptor,
}

impl ContractDescriptor {
    pub fn new(
        contract_id: ContractId,
        capability_id: CapabilityId,
        version: Version,
        interaction: Interaction,
        payload: PayloadDescriptor,
    ) -> Self {
        Self {
            contract_id,
            capability_id,
            version,
            interaction,
            payload,
        }
    }
}

/// A structural error found while constructing a descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDescriptor {
    EmptyMediaType,
}

impl fmt::Display for InvalidDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMediaType => formatter.write_str("a payload media type must not be empty"),
        }
    }
}

impl std::error::Error for InvalidDescriptor {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_preserve_contract_and_payload_metadata() {
        let payload = PayloadDescriptor::new("application/json", Version::new(1, 2, 0)).unwrap();
        let descriptor = ContractDescriptor::new(
            ContractId::new("quran.lookup").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(2, 0, 1),
            Interaction::Request,
            payload,
        );

        assert_eq!(descriptor.version.to_string(), "2.0.1");
        assert_eq!(descriptor.payload.media_type(), "application/json");
        assert_eq!(descriptor.payload.schema_version(), &Version::new(1, 2, 0));
    }

    #[test]
    fn payload_descriptors_reject_empty_media_types() {
        assert_eq!(
            PayloadDescriptor::new("  ", Version::new(1, 0, 0)),
            Err(InvalidDescriptor::EmptyMediaType)
        );
    }
}
