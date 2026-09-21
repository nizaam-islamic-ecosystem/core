//! Binary message framing protocol for the Phase 7 transport layer.
//!
//! The framing layer transports opaque payload bytes. It does not define the
//! application payload format.
//!
//! The checksum implementation requires the `xxh3` feature of the
//! `xxhash-rust` crate.
//!
//! Current wire header: 48 bytes, big-endian/network byte order.
//!
//! ```text
//! 0       1       2       4       8              16        20        24
//! +-------+-------+-------+-------+---------------+---------+---------+
//! |Version| Flags |Header |Payload| Transport     |Fragment |Cum. ACK |
//! |       |       |Length |Length | Stream ID     | Index   | Index   |
//! +-------+-------+-------+-------+---------------+---------+---------+
//! 24                      32                      40                 48
//! +-----------------------+-----------------------+------------------+
//! | SACK Bitmap (8 bytes) | XXH3-64 (8 bytes)    | Reserved (8 bytes)|
//! +-----------------------+-----------------------+------------------+
//! ```
//!
//! A frame is at most 20,000,000 bytes. With the current 48-byte header,
//! the maximum payload in one frame is 19,999,952 bytes.
//!
//! A logical message may exceed that size. The transport fragments it into
//! multiple frames using the same Transport Stream ID and monotonically
//! increasing Fragment Index values starting at zero.

use std::fmt;

/// Current transport framing protocol version.
pub const FRAMING_VERSION: u8 = 1;

/// Current fixed header length.
pub const HEADER_LENGTH: usize = 48;

/// Maximum complete frame size, in decimal bytes.
pub const MAX_FRAME_LENGTH_BYTES: usize = 20_000_000;

/// Maximum payload carried by one frame.
pub const MAX_PAYLOAD_LENGTH: usize = MAX_FRAME_LENGTH_BYTES - HEADER_LENGTH;

/// Bit 0: final frame of a Transport Stream.
pub const FLAG_FINISH: u8 = 1 << 0;
/// Bit 1: more fragments follow this frame.
pub const FLAG_MORE: u8 = 1 << 1;
/// Bit 2: frame carries acknowledgement information.
pub const FLAG_ACK: u8 = 1 << 2;
/// Bit 3: connection termination request.
pub const FLAG_CONNECTION_FIN: u8 = 1 << 3;
/// Bit 4: restart current transport connection/session.
pub const FLAG_RESTART: u8 = 1 << 4;
/// Bit 5: SACK bitmap is meaningful.
pub const FLAG_SACK_PRESENT: u8 = 1 << 5;
/// Bit 6: reserved for future protocol versions.
pub const FLAG_RESERVED_6: u8 = 1 << 6;
/// Bit 7: reserved for future protocol versions.
pub const FLAG_RESERVED_7: u8 = 1 << 7;

const KNOWN_FLAGS: u8 =
    FLAG_FINISH | FLAG_MORE | FLAG_ACK | FLAG_CONNECTION_FIN | FLAG_RESTART | FLAG_SACK_PRESENT;

/// Binary message header used by the transport framing protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    version: u8,
    flags: u8,
    header_length: u16,
    payload_length: u32,
    transport_stream_id: u64,
    fragment_index: u32,
    cumulative_ack_index: u32,
    sack_bitmap: u64,
    checksum: u64,
    reserved: u64,
}

impl MessageHeader {
    /// Creates a header. The checksum is initially zero and must be populated
    /// with `with_checksum` after the complete frame bytes are known.
    pub fn new(
        version: u8,
        flags: u8,
        payload_length: u32,
        transport_stream_id: u64,
        fragment_index: u32,
        cumulative_ack_index: u32,
        sack_bitmap: u64,
    ) -> Result<Self, String> {
        if usize::try_from(payload_length).map_err(|_| "invalid payload length")?
            > MAX_PAYLOAD_LENGTH
        {
            return Err(format!(
                "payload_length exceeds maximum frame payload ({MAX_PAYLOAD_LENGTH})"
            ));
        }

        validate_header_semantics(flags, payload_length as usize, sack_bitmap)?;

        Ok(Self {
            version,
            flags,
            header_length: HEADER_LENGTH as u16,
            payload_length,
            transport_stream_id,
            fragment_index,
            cumulative_ack_index,
            sack_bitmap,
            checksum: 0,
            reserved: 0,
        })
    }

    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn flags(&self) -> u8 {
        self.flags
    }
    pub fn header_length(&self) -> u16 {
        self.header_length
    }
    pub fn payload_length(&self) -> u32 {
        self.payload_length
    }
    pub fn transport_stream_id(&self) -> u64 {
        self.transport_stream_id
    }
    pub fn fragment_index(&self) -> u32 {
        self.fragment_index
    }
    pub fn cumulative_ack_index(&self) -> u32 {
        self.cumulative_ack_index
    }
    pub fn sack_bitmap(&self) -> u64 {
        self.sack_bitmap
    }
    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    pub fn is_finish(&self) -> bool {
        self.flags & FLAG_FINISH != 0
    }
    pub fn has_more(&self) -> bool {
        self.flags & FLAG_MORE != 0
    }
    pub fn is_ack(&self) -> bool {
        self.flags & FLAG_ACK != 0
    }
    pub fn is_connection_fin(&self) -> bool {
        self.flags & FLAG_CONNECTION_FIN != 0
    }
    pub fn is_restart(&self) -> bool {
        self.flags & FLAG_RESTART != 0
    }

    pub fn with_checksum(mut self, checksum: u64) -> Self {
        self.checksum = checksum;
        self
    }

    /// Serializes the complete fixed header.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_LENGTH);
        bytes.push(self.version);
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.header_length.to_be_bytes());
        bytes.extend_from_slice(&self.payload_length.to_be_bytes());
        bytes.extend_from_slice(&self.transport_stream_id.to_be_bytes());
        bytes.extend_from_slice(&self.fragment_index.to_be_bytes());
        bytes.extend_from_slice(&self.cumulative_ack_index.to_be_bytes());
        bytes.extend_from_slice(&self.sack_bitmap.to_be_bytes());
        bytes.extend_from_slice(&self.checksum.to_be_bytes());
        bytes.extend_from_slice(&self.reserved.to_be_bytes());
        bytes
    }

    /// Serializes the header with the checksum field zeroed.
    pub fn serialize_for_checksum(&self) -> Vec<u8> {
        let mut copy = self.clone();
        copy.checksum = 0;
        copy.serialize()
    }

    /// Parses a header using its Header Length field.
    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err("framing header is shorter than the 4-byte prefix".into());
        }

        let header_length = u16::from_be_bytes([data[2], data[3]]) as usize;
        if header_length != HEADER_LENGTH {
            return Err(format!(
                "unsupported framing header length: {header_length}"
            ));
        }
        if data.len() != header_length {
            return Err(format!(
                "expected {header_length}-byte header, got {} bytes",
                data.len()
            ));
        }

        let version = data[0];
        let flags = data[1];
        let payload_length = u32::from_be_bytes(data[4..8].try_into().unwrap());
        if payload_length as usize > MAX_PAYLOAD_LENGTH {
            return Err(format!(
                "payload_length exceeds maximum frame payload ({MAX_PAYLOAD_LENGTH})"
            ));
        }

        let header = Self {
            version,
            flags,
            header_length: header_length as u16,
            payload_length,
            transport_stream_id: u64::from_be_bytes(data[8..16].try_into().unwrap()),
            fragment_index: u32::from_be_bytes(data[16..20].try_into().unwrap()),
            cumulative_ack_index: u32::from_be_bytes(data[20..24].try_into().unwrap()),
            sack_bitmap: u64::from_be_bytes(data[24..32].try_into().unwrap()),
            checksum: u64::from_be_bytes(data[32..40].try_into().unwrap()),
            reserved: u64::from_be_bytes(data[40..48].try_into().unwrap()),
        };

        if header.reserved != 0 {
            return Err("reserved header bytes must be zero for framing version 1".into());
        }
        if version != FRAMING_VERSION {
            return Err(format!("unsupported framing protocol version: {version}"));
        }
        validate_header_semantics(
            header.flags,
            header.payload_length as usize,
            header.sack_bitmap,
        )?;

        Ok(header)
    }
}

/// Fragments one opaque logical payload into complete wire frames.
///
/// The caller supplies the Transport Stream ID so transport implementations
/// can preserve a stream identity across all fragments.
pub fn encode_message(transport_stream_id: u64, payload: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let fragment_count = if payload.is_empty() {
        1
    } else {
        payload.len().div_ceil(MAX_PAYLOAD_LENGTH)
    };

    let mut frames = Vec::with_capacity(fragment_count);
    for fragment_index in 0..fragment_count {
        let start = fragment_index * MAX_PAYLOAD_LENGTH;
        let end = payload.len().min(start + MAX_PAYLOAD_LENGTH);
        let fragment = &payload[start..end];
        let flags = if fragment_index + 1 == fragment_count {
            FLAG_FINISH
        } else {
            FLAG_MORE
        };

        let header = MessageHeader::new(
            FRAMING_VERSION,
            flags,
            u32::try_from(fragment.len()).map_err(|_| "fragment payload is too large")?,
            transport_stream_id,
            u32::try_from(fragment_index).map_err(|_| "fragment index overflow")?,
            NO_ACK_INDEX,
            0,
        )?;

        frames.push(encode_frame(header, fragment)?);
    }

    Ok(frames)
}

/// Sentinel used when a data frame carries no cumulative acknowledgement.
pub const NO_ACK_INDEX: u32 = u32::MAX;

/// Validates flag combinations defined by framing version 1.
pub fn validate_flags(flags: u8) -> Result<(), String> {
    if flags & (FLAG_RESERVED_6 | FLAG_RESERVED_7) != 0 {
        return Err("reserved flag bits are set".into());
    }
    if flags & FLAG_FINISH != 0 && flags & FLAG_MORE != 0 {
        return Err("FINISH and MORE cannot both be set".into());
    }
    if flags & FLAG_ACK != 0 && flags & FLAG_CONNECTION_FIN != 0 {
        return Err("ACK and CONNECTION_FIN cannot be combined".into());
    }
    if flags & FLAG_CONNECTION_FIN != 0 && flags & FLAG_MORE != 0 {
        return Err("CONNECTION_FIN cannot be combined with MORE".into());
    }
    if flags & FLAG_RESTART != 0 && flags & FLAG_FINISH != 0 {
        return Err("RESTART cannot be combined with FINISH".into());
    }
    if flags & !KNOWN_FLAGS != 0 {
        return Err("unknown framing flag bits are set".into());
    }
    Ok(())
}

fn validate_header_semantics(
    flags: u8,
    payload_length: usize,
    sack_bitmap: u64,
) -> Result<(), String> {
    validate_flags(flags)?;

    if flags & FLAG_ACK != 0 {
        if payload_length != 0 {
            return Err("ACK frame must have a zero-length payload".into());
        }
        if flags & (FLAG_FINISH | FLAG_MORE | FLAG_CONNECTION_FIN | FLAG_RESTART) != 0 {
            return Err("ACK frame contains incompatible flags".into());
        }
        if flags & FLAG_SACK_PRESENT == 0 && sack_bitmap != 0 {
            return Err("SACK bitmap is set without SACK_PRESENT".into());
        }
    }

    if flags & FLAG_SACK_PRESENT != 0 && flags & FLAG_ACK == 0 {
        return Err("SACK_PRESENT requires ACK".into());
    }

    if flags & FLAG_CONNECTION_FIN != 0
        && (payload_length != 0
            || flags & (FLAG_ACK | FLAG_MORE | FLAG_FINISH | FLAG_RESTART | FLAG_SACK_PRESENT) != 0)
    {
        return Err("CONNECTION_FIN frame contains incompatible fields".into());
    }

    if flags & FLAG_RESTART != 0
        && (payload_length != 0
            || flags
                & (FLAG_ACK | FLAG_MORE | FLAG_FINISH | FLAG_CONNECTION_FIN | FLAG_SACK_PRESENT)
                != 0)
    {
        return Err("RESTART frame contains incompatible fields".into());
    }
    Ok(())
}

/// Computes the XXH3-64 checksum over a header with its checksum field zeroed
/// followed by the opaque frame payload.
///
/// `xxhash-rust` is intentionally used only by the framing layer; checksum
/// semantics do not become an application payload concern.
pub fn checksum(header: &MessageHeader, payload: &[u8]) -> u64 {
    let header_bytes = header.serialize_for_checksum();
    let mut bytes = Vec::with_capacity(header_bytes.len() + payload.len());
    bytes.extend_from_slice(&header_bytes);
    bytes.extend_from_slice(payload);
    xxhash_rust::xxh3::xxh3_64(&bytes)
}

/// Validates the checksum stored in a complete frame.
pub fn verify_checksum(header: &MessageHeader, payload: &[u8]) -> bool {
    checksum(header, payload) == header.checksum()
}

/// Builds one complete wire frame.
pub fn encode_frame(mut header: MessageHeader, payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() > MAX_PAYLOAD_LENGTH {
        return Err(format!(
            "payload exceeds maximum frame payload ({MAX_PAYLOAD_LENGTH})"
        ));
    }
    if payload.len() != header.payload_length as usize {
        return Err("payload length does not match header".into());
    }

    header.checksum = checksum(&header, payload);
    let mut frame = header.serialize();
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Parses and validates one complete frame.
pub fn decode_frame(frame: &[u8]) -> Result<(MessageHeader, Vec<u8>), String> {
    if frame.len() < HEADER_LENGTH {
        return Err("frame is shorter than the fixed header".into());
    }

    let header = MessageHeader::deserialize(&frame[..HEADER_LENGTH])?;
    let expected = HEADER_LENGTH + header.payload_length as usize;
    if frame.len() != expected {
        return Err(format!(
            "frame length does not match header payload length: expected {expected}, got {}",
            frame.len()
        ));
    }

    let payload = frame[HEADER_LENGTH..].to_vec();
    if !verify_checksum(&header, &payload) {
        return Err("frame checksum verification failed".into());
    }

    Ok((header, payload))
}

impl fmt::Display for MessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MessageHeader {{ version={}, flags={}, header_length={}, payload_length={}, transport_stream_id={}, fragment_index={}, cumulative_ack_index={}, sack_bitmap=0x{:016x}, checksum=0x{:016x} }}",
            self.version,
            self.flags,
            self.header_length,
            self.payload_length,
            self.transport_stream_id,
            self.fragment_index,
            self.cumulative_ack_index,
            self.sack_bitmap,
            self.checksum
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_48_bytes_and_round_trips() {
        let header = MessageHeader::new(
            FRAMING_VERSION,
            FLAG_MORE,
            1024,
            0x0102_0304_0506_0708,
            5,
            NO_ACK_INDEX,
            0,
        )
        .unwrap();

        let bytes = header.serialize();
        assert_eq!(bytes.len(), HEADER_LENGTH);

        let decoded = MessageHeader::deserialize(&bytes).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn header_uses_big_endian_encoding() {
        let header = MessageHeader::new(
            FRAMING_VERSION,
            FLAG_FINISH,
            0x0010_0000,
            0x0102_0304_0506_0708,
            0x0000_0005,
            NO_ACK_INDEX,
            0,
        )
        .unwrap();

        let bytes = header.serialize();

        assert_eq!(bytes[0], FRAMING_VERSION);
        assert_eq!(bytes[1], FLAG_FINISH);
        assert_eq!(&bytes[2..4], &(HEADER_LENGTH as u16).to_be_bytes());
        assert_eq!(&bytes[4..8], &0x0010_0000u32.to_be_bytes());
        assert_eq!(&bytes[8..16], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&bytes[16..20], &5u32.to_be_bytes());
    }

    #[test]
    fn message_fragmentation_does_not_reject_large_logical_messages() {
        let payload = vec![0x5a; MAX_PAYLOAD_LENGTH + 17];
        let frames = encode_message(42, &payload).unwrap();

        assert_eq!(frames.len(), 2);

        let (first, first_payload) = decode_frame(&frames[0]).unwrap();
        let (last, last_payload) = decode_frame(&frames[1]).unwrap();

        assert_eq!(first.transport_stream_id(), 42);
        assert_eq!(last.transport_stream_id(), 42);
        assert_eq!(first.fragment_index(), 0);
        assert_eq!(last.fragment_index(), 1);
        assert!(first.has_more());
        assert!(!first.is_finish());
        assert!(!last.has_more());
        assert!(last.is_finish());
        assert_eq!(first_payload.len(), MAX_PAYLOAD_LENGTH);
        assert_eq!(last_payload.len(), 17);
    }

    #[test]
    fn checksum_detects_payload_corruption() {
        let payload = b"transport payload";
        let header = MessageHeader::new(
            FRAMING_VERSION,
            FLAG_FINISH,
            payload.len() as u32,
            7,
            0,
            NO_ACK_INDEX,
            0,
        )
        .unwrap();

        let mut frame = encode_frame(header, payload).unwrap();
        frame[HEADER_LENGTH] ^= 0x01;

        assert!(decode_frame(&frame).is_err());
    }

    #[test]
    fn ack_frame_has_zero_payload_and_sack_semantics() {
        let header = MessageHeader::new(
            FRAMING_VERSION,
            FLAG_ACK | FLAG_SACK_PRESENT,
            0,
            9,
            0,
            3,
            0b101,
        )
        .unwrap();

        let frame = encode_frame(header.clone(), &[]).unwrap();
        let (decoded, payload) = decode_frame(&frame).unwrap();

        assert!(decoded.is_ack());
        assert_eq!(decoded.cumulative_ack_index(), 3);
        assert_eq!(decoded.sack_bitmap(), 0b101);
        assert!(payload.is_empty());
    }

    #[test]
    fn invalid_flag_combinations_are_rejected() {
        assert!(
            MessageHeader::new(
                FRAMING_VERSION,
                FLAG_FINISH | FLAG_MORE,
                0,
                1,
                0,
                NO_ACK_INDEX,
                0,
            )
            .is_err()
        );

        assert!(
            MessageHeader::new(FRAMING_VERSION, FLAG_SACK_PRESENT, 0, 1, 0, NO_ACK_INDEX, 1,)
                .is_err()
        );
    }

    #[test]
    fn reserved_header_bytes_must_be_zero() {
        let header =
            MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 0, 1, 0, NO_ACK_INDEX, 0).unwrap();

        let mut bytes = header.serialize();
        bytes[40] = 1;

        assert!(MessageHeader::deserialize(&bytes).is_err());
    }
}
