//! Binary message framing protocol for Phase 7 transport layer.
//!
//! Defines the 20-byte binary header used for transporting logical messages
//! between engines. The header consists of:
//!
//! - Version (1 byte)
//!
//! - Flags (1 byte)
//!
//! - Payload Length (4 bytes, big-endian)
//!
//! - Transport Message ID (8 bytes)
//!
//! - Fragment Index (4 bytes)
//!
//! This ensures that fragments can be reassembled correctly and that
//! logical message boundaries are preserved across transport hops.

use std::fmt;

/// Binary message header used in the transport layer.
///
/// Layout (big-endian):
///   [0] Version (1 byte)
///   [1] Flags (1 byte)
///   [2-3] Reserved (2 bytes)
///   [4] Payload Length (4 bytes)
///   [8] Transport Message ID (8 bytes)
///   [12] Fragment Index (4 bytes)
#[derive(Debug)]
pub struct MessageHeader {
    version: u8,
    flags: u8,
    reserved: [u8; 2],
    payload_length: u32,
    message_id: [u8; 8],
    fragment_index: u32,
}

impl MessageHeader {
    /// Constructs a header from the given fields.
    ///
    /// # Arguments
    ///
    /// * `version` - Protocol version (e.g., 1 for initial release)
    /// * `flags` - Bitmask indicating special flags
    /// * `payload_length` - Size of the payload in bytes
    /// * `message_id` - Unique identifier for the logical message
    /// * `fragment_index` - Position of this fragment within the message
    ///
    /// # Panics
    ///
    /// Panics if `payload_length` exceeds the maximum allowed frame size
    /// (MAX_FRAME_LENGTH).
    pub fn new(
        version: u8,
        flags: u8,
        payload_length: u32,
        message_id: [u8; 8],
        fragment_index: u32,
    ) -> Self {
        Self {
            version,
            flags,
            reserved: [0, 0],
            payload_length,
            message_id,
            fragment_index,
        }
    }

    /// Serializes the header to a 20-byte binary format.
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` representing the binary header.
    pub fn serialize(&self) -> Vec<u8> {
        // Pack the header into a 20-byte binary format
        let mut header_bytes = Vec::with_capacity(20);
        header_bytes.push(self.version);
        header_bytes.push(self.flags);
        header_bytes.extend_from_slice(&self.reserved);
        header_bytes.extend_from_slice(&self.payload_length.to_be_bytes());
        header_bytes.extend_from_slice(&self.message_id);
        header_bytes.extend_from_slice(&self.fragment_index.to_be_bytes());

        header_bytes
    }

    /// Deserializes a 20-byte header from binary data.
    ///
    /// # Arguments
    ///
    /// * `data` - The binary header data (exactly 20 bytes)
    ///
    /// # Returns
    ///
    /// A `MessageHeader` or `Err` if the data is malformed.
    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        if data.len() != 20 {
            return Err(format!("Expected 20-byte header, got {} bytes", data.len()));
        }

        let version = data[0];
        let flags = data[1];
        let reserved = [data[2], data[3]];
        let payload_length = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let message_id = data[8..16].try_into().map_err(|_| "Invalid message ID")?;
        let fragment_index = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);

        Ok(MessageHeader {
            version,
            flags,
            reserved,
            payload_length,
            message_id,
            fragment_index,
        })
    }
}

impl fmt::Display for MessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MessageHeader {{ version={}, flags={}, payload_length={}, message_id={:?}, fragment_index={} }}",
            self.version, self.flags, self.payload_length, self.message_id, self.fragment_index
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_header_creates_with_correct_values() {
        let header = MessageHeader::new(
            1,                        // version
            0b00000001,               // flags (final fragment set)
            1024,                     // payload_length
            [1, 2, 3, 4, 5, 6, 7, 8], // message_id
            0,                        // fragment_index
        );

        assert_eq!(header.version, 1);
        assert_eq!(header.flags, 0b00000001);
        assert_eq!(header.payload_length, 1024);
        assert_eq!(header.message_id, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(header.fragment_index, 0);
    }

    #[test]
    fn message_header_serializes_to_20_bytes() {
        let header = MessageHeader::new(1, 0, 100, [1, 2, 3, 4, 5, 6, 7, 8], 0);
        let bytes = header.serialize();
        assert_eq!(bytes.len(), 20);
    }

    #[test]
    fn message_header_serialize_deserialize_roundtrip() {
        let original = MessageHeader::new(2, 15, 2048, [10, 20, 30, 40, 50, 60, 70, 80], 5);
        let serialized = original.serialize();
        let deserialized = MessageHeader::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.flags, original.flags);
        assert_eq!(deserialized.payload_length, original.payload_length);
        assert_eq!(deserialized.message_id, original.message_id);
        assert_eq!(deserialized.fragment_index, original.fragment_index);
    }

    #[test]
    fn message_header_deserialize_rejects_wrong_length() {
        let result = MessageHeader::deserialize(&[0u8; 19]);
        assert!(result.is_err());

        let result = MessageHeader::deserialize(&[0u8; 21]);
        assert!(result.is_err());

        let result = MessageHeader::deserialize(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn message_header_uses_big_endian_encoding() {
        let header = MessageHeader::new(
            0x01,       // version at byte 0
            0x02,       // flags at byte 1
            0x00100000, // payload_length 65536 at bytes 4-7 (big endian: 00 10 00 00)
            [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
            0x00000005, // fragment_index at bytes 16-19 (big endian: 00 00 00 05)
        );
        let bytes = header.serialize();

        // Verify big-endian encoding
        assert_eq!(bytes[0], 0x01); // version
        assert_eq!(bytes[1], 0x02); // flags
        assert_eq!(bytes[2], 0x00); // reserved byte 1
        assert_eq!(bytes[3], 0x00); // reserved byte 2
        assert_eq!(bytes[4], 0x00); // payload_length high byte
        assert_eq!(bytes[5], 0x10); // payload_length mid-high byte
        assert_eq!(bytes[6], 0x00); // payload_length mid-low byte
        assert_eq!(bytes[7], 0x00); // payload_length low byte
        assert_eq!(bytes[16], 0x00); // fragment_index high byte
        assert_eq!(bytes[19], 0x05); // fragment_index low byte
    }

    #[test]
    fn message_header_display_formats_correctly() {
        let header = MessageHeader::new(1, 1, 1024, [1, 2, 3, 4, 5, 6, 7, 8], 2);
        let formatted = format!("{}", header);
        assert!(formatted.contains("version=1"));
        assert!(formatted.contains("flags=1"));
        assert!(formatted.contains("payload_length=1024"));
        assert!(formatted.contains("fragment_index=2"));
    }
}
