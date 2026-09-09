use core::fmt;

use crate::identity::{CapabilityId, ContractId};

/// A validated semantic version for a contract or schema.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
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
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Interaction {
    Request,
    Response,
    Event,
}

/// Describes an encoded payload without interpreting its domain meaning.
///
/// Deserialization validates through [`PayloadDescriptor::new`] so that an
/// empty or whitespace-only `media_type` is rejected, matching the public
/// constructor. Serialization and equality behavior are unchanged.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PayloadDescriptor {
    media_type: String,
    schema_version: Version,
}

/// Intermediate representation used to validate a [`PayloadDescriptor`] while
/// deserializing from a transport such as JSON.
#[derive(Clone, Debug, serde::Deserialize)]
struct PayloadDescriptorIr {
    media_type: String,
    schema_version: Version,
}

impl<'de> serde::Deserialize<'de> for PayloadDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let intermediate = PayloadDescriptorIr::deserialize(deserializer)?;
        PayloadDescriptor::new(intermediate.media_type, intermediate.schema_version)
            .map_err(serde::de::Error::custom)
    }
}

/// An opaque payload owned and interpreted by an engine capability.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
    use std::error::Error;

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

    #[test]
    fn payload_descriptor_serde_rejects_whitespace_only_media_type() {
        let json = r#"{"media_type":"   ","schema_version":{"major":1,"minor":0,"patch":0}}"#;
        let result: Result<PayloadDescriptor, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn payload_descriptor_serde_rejects_empty_media_type() {
        let json = r#"{"media_type":"","schema_version":{"major":1,"minor":0,"patch":0}}"#;
        let result: Result<PayloadDescriptor, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn payload_descriptor_serde_round_trips_valid_media_type() {
        let json =
            r#"{"media_type":"application/json","schema_version":{"major":1,"minor":2,"patch":0}}"#;
        let descriptor: PayloadDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(descriptor.media_type(), "application/json");
        assert_eq!(descriptor.schema_version(), &Version::new(1, 2, 0));
    }

    #[test]
    fn version_derives_eq_ord_hash_and_implements_display() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 2, 3);
        let v3 = Version::new(1, 2, 4);
        let v4 = Version::new(2, 0, 0);

        assert_eq!(v1, v2);
        assert!(v1 < v3);
        assert!(v1 < v4);
        assert!(v3 < v4);
        assert_eq!(v1.to_string(), "1.2.3");
        assert_eq!(v4.to_string(), "2.0.0");

        use std::collections::HashSet;
        let mut set: HashSet<Version> = HashSet::new();
        set.insert(v1.clone());
        set.insert(v2.clone());
        assert_eq!(set.len(), 1);
        set.insert(v3.clone());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn version_accessors_return_components() {
        let v = Version::new(2, 5, 9);
        assert_eq!(v.major(), 2);
        assert_eq!(v.minor(), 5);
        assert_eq!(v.patch(), 9);
    }

    #[test]
    fn interaction_variants_are_distinct() {
        assert_ne!(Interaction::Request, Interaction::Response);
        assert_ne!(Interaction::Request, Interaction::Event);
        assert_ne!(Interaction::Response, Interaction::Event);
        assert_eq!(Interaction::Request, Interaction::Request);
    }

    #[test]
    fn encoded_payload_stores_descriptor_and_bytes() {
        let desc = PayloadDescriptor::new("text/plain", Version::new(1, 0, 0)).unwrap();
        let payload = EncodedPayload::new(desc.clone(), b"hello world");
        assert_eq!(payload.bytes(), b"hello world");
        assert_eq!(payload.descriptor(), &desc);
    }

    #[test]
    fn encoded_payload_accepts_string_as_bytes() {
        let desc = PayloadDescriptor::new("application/json", Version::new(1, 0, 0)).unwrap();
        let payload = EncodedPayload::new(desc, "json payload".to_string());
        assert_eq!(payload.bytes(), b"json payload");
    }

    #[test]
    fn raw_payload_codec_round_trips_any_bytes() {
        let codec = RawPayloadCodec;
        let original = b"arbitrary binary \x00 data";
        let encoded = codec.encode(original).unwrap();
        assert_eq!(encoded.as_slice(), original);
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(decoded.as_slice(), original);
    }

    #[test]
    fn raw_payload_codec_implements_default_and_clone() {
        let codec1 = RawPayloadCodec;
        let codec2 = RawPayloadCodec;
        assert_eq!(
            codec1.encode(b"test").unwrap(),
            codec2.encode(b"test").unwrap()
        );
        assert_eq!(
            codec1.clone().encode(b"test").unwrap(),
            codec1.encode(b"test").unwrap()
        );
    }

    #[test]
    fn encoding_error_display_and_error_trait() {
        let err = EncodingError::InvalidPayload;
        assert_eq!(err.to_string(), "the payload codec rejected the payload");
        assert!(err.source().is_none());
    }

    #[test]
    fn invalid_descriptor_display_and_error_trait() {
        let err = InvalidDescriptor::EmptyMediaType;
        assert_eq!(err.to_string(), "a payload media type must not be empty");
        assert!(err.source().is_none());
    }
}
