//! Phase 16 transport and framing conformance tests.
//!
//! The tests protect the actual 48-byte framing implementation and the
//! provider-neutral in-memory transport boundary.

use std::sync::{Arc, Mutex};

use nizaam_core::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, Interaction, MessageEnvelope,
    Participants, PayloadDescriptor, UniversalRequest, UniversalResponse,
};
use nizaam_core::identity::{
    CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};
use nizaam_core::operation::{Operation, OperationContext};
use nizaam_core::prelude::{Status, Version};
use nizaam_core::transport::framing::{
    self, FLAG_ACK, FLAG_CONNECTION_FIN, FLAG_FINISH, FLAG_MORE, FLAG_RESTART, FLAG_SACK_PRESENT,
    FRAMING_VERSION, HEADER_LENGTH, MAX_PAYLOAD_LENGTH, MessageHeader,
};
use nizaam_core::transport::stream::{
    ByteSink, ByteSource, ByteSourceState, MessageStream, StreamError,
};
use nizaam_core::transport::{InMemoryTransport, Transport, TransportError};

fn request(
    target_engine: &str,
    target_instance: &str,
    name: &str,
    payload: &[u8],
) -> UniversalRequest {
    let payload_descriptor =
        PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap();
    let descriptor = ContractDescriptor::new(
        ContractId::new("phase16.transport.contract").unwrap(),
        CapabilityId::new("phase16.transport.echo").unwrap(),
        Version::new(1, 0, 0),
        Interaction::Request,
        payload_descriptor.clone(),
    );
    let metadata = ContractMetadata::new(
        descriptor,
        Participants::new(
            EngineId::new("phase16-transport-caller").unwrap(),
            EngineId::new(target_engine).unwrap(),
        )
        .with_target_instance(EngineInstanceId::new(target_instance).unwrap()),
    );

    UniversalRequest::new(MessageEnvelope::new(
        MessageId::new(format!("phase16-transport-message-{name}")).unwrap(),
        OperationContext::new(Operation::new(
            OperationId::new(format!("phase16-transport-operation-{name}")).unwrap(),
            CorrelationId::new(format!("phase16-transport-correlation-{name}")).unwrap(),
        )),
        metadata,
        EncodedPayload::new(payload_descriptor, payload.to_vec()),
    ))
}

#[derive(Default)]
struct MemorySink {
    bytes: Mutex<Vec<u8>>,
}

impl MemorySink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.bytes.lock().unwrap())
    }
}

impl ByteSink for MemorySink {
    fn write(&self, buf: &[u8]) -> Result<(), StreamError> {
        self.bytes.lock().unwrap().extend_from_slice(buf);
        Ok(())
    }

    fn flush(&self) -> Result<(), StreamError> {
        Ok(())
    }

    fn close(&self) -> Result<(), StreamError> {
        Ok(())
    }
}

struct MemorySource {
    bytes: Mutex<Vec<u8>>,
    state: ByteSourceState,
}

impl MemorySource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Mutex::new(bytes),
            state: ByteSourceState::new(),
        }
    }
}

impl ByteSource for MemorySource {
    fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError> {
        let mut bytes = self.bytes.lock().unwrap();
        if bytes.is_empty() {
            return Ok(None);
        }

        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        bytes.drain(..count);
        Ok(Some(count))
    }

    fn stream_state(&self) -> &ByteSourceState {
        &self.state
    }
}

#[test]
fn framing_header_is_exactly_48_bytes() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 4, 10, 0, u32::MAX, 0).unwrap();

    assert_eq!(HEADER_LENGTH, 48);
    assert_eq!(header.header_length(), 48);
    assert_eq!(header.serialize().len(), 48);
}

#[test]
fn framing_header_round_trip_preserves_all_exposed_fields() {
    let header = MessageHeader::new(
        FRAMING_VERSION,
        FLAG_ACK | FLAG_SACK_PRESENT,
        0,
        0x0102_0304_0506_0708,
        9,
        11,
        0xFFEEDDCCBBAA9988,
    )
    .unwrap()
    .with_checksum(0x1122334455667788);

    let decoded = MessageHeader::deserialize(&header.serialize()).unwrap();

    assert_eq!(decoded, header);
}

#[test]
fn framing_numeric_fields_use_big_endian_network_order() {
    let header = MessageHeader::new(
        FRAMING_VERSION,
        FLAG_FINISH,
        0x0102_0304,
        0x0102_0304_0506_0708,
        0x0A0B_0C0D,
        0x0E0F_1011,
        0x1213_1415_1617_1819,
    )
    .unwrap()
    .with_checksum(0x2021_2223_2425_2627);

    let bytes = header.serialize();

    assert_eq!(&bytes[2..4], &48_u16.to_be_bytes());
    assert_eq!(&bytes[4..8], &0x0102_0304_u32.to_be_bytes());
    assert_eq!(&bytes[8..16], &0x0102_0304_0506_0708_u64.to_be_bytes());
    assert_eq!(&bytes[16..20], &0x0A0B_0C0D_u32.to_be_bytes());
    assert_eq!(&bytes[20..24], &0x0E0F_1011_u32.to_be_bytes());
    assert_eq!(&bytes[24..32], &0x1213_1415_1617_1819_u64.to_be_bytes());
    assert_eq!(&bytes[32..40], &0x2021_2223_2425_2627_u64.to_be_bytes());
    assert_eq!(&bytes[40..48], &[0; 8]);
}

#[test]
fn framing_rejects_wrong_header_sizes() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 1, 1, 0, u32::MAX, 0).unwrap();
    let bytes = header.serialize();

    let mut too_short = bytes.clone();
    too_short.truncate(47);
    assert!(MessageHeader::deserialize(&too_short).is_err());

    let mut too_long = bytes;
    too_long.push(0);
    assert!(MessageHeader::deserialize(&too_long).is_err());
}

#[test]
fn framing_rejects_unsupported_protocol_version() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 1, 1, 0, u32::MAX, 0).unwrap();
    let mut bytes = header.serialize();
    bytes[0] = FRAMING_VERSION + 1;

    assert!(MessageHeader::deserialize(&bytes).is_err());
}

#[test]
fn framing_rejects_invalid_header_length_field() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 1, 1, 0, u32::MAX, 0).unwrap();
    let mut bytes = header.serialize();
    bytes[2..4].copy_from_slice(&47_u16.to_be_bytes());

    assert!(MessageHeader::deserialize(&bytes).is_err());
}

#[test]
fn framing_rejects_reserved_header_bytes_and_flag_bits() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 1, 1, 0, u32::MAX, 0).unwrap();
    let mut bytes = header.serialize();
    bytes[40] = 1;

    assert!(MessageHeader::deserialize(&bytes).is_err());
    assert!(framing::validate_flags(1 << 6).is_err());
    assert!(framing::validate_flags(1 << 7).is_err());
    assert!(framing::validate_flags(FLAG_FINISH | FLAG_MORE).is_err());
}

#[test]
fn framing_validates_checksum_and_detects_payload_corruption() {
    let frame = framing::encode_message(1, b"checksum-payload")
        .unwrap()
        .remove(0);
    let (header, payload) = framing::decode_frame(&frame).unwrap();

    assert!(framing::verify_checksum(&header, &payload));

    let mut corrupted = frame;
    let last = corrupted.len() - 1;
    corrupted[last] ^= 0xFF;

    assert!(framing::decode_frame(&corrupted).is_err());
}

#[test]
fn empty_logical_payload_is_encoded_as_one_finish_frame() {
    let frames = framing::encode_message(7, b"").unwrap();

    assert_eq!(frames.len(), 1);
    let (header, payload) = framing::decode_frame(&frames[0]).unwrap();
    assert!(header.is_finish());
    assert!(!header.has_more());
    assert!(payload.is_empty());
}

#[test]
fn one_frame_message_does_not_create_artificial_fragmentation() {
    let payload = vec![0x5A; 1024];
    let frames = framing::encode_message(8, &payload).unwrap();

    assert_eq!(frames.len(), 1);
    let (header, decoded) = framing::decode_frame(&frames[0]).unwrap();
    assert_eq!(header.fragment_index(), 0);
    assert!(header.is_finish());
    assert_eq!(decoded, payload);
}

#[test]
fn maximum_frame_payload_fits_in_one_frame() {
    let payload = vec![0x5A; MAX_PAYLOAD_LENGTH];
    let frames = framing::encode_message(9, &payload).unwrap();

    assert_eq!(frames.len(), 1);
    let (header, decoded) = framing::decode_frame(&frames[0]).unwrap();
    assert_eq!(header.payload_length() as usize, MAX_PAYLOAD_LENGTH);
    assert_eq!(decoded, payload);
}

#[test]
fn payload_one_byte_above_frame_limit_creates_second_fragment() {
    let payload = vec![0x5A; MAX_PAYLOAD_LENGTH + 1];
    let frames = framing::encode_message(10, &payload).unwrap();

    assert_eq!(frames.len(), 2);

    let (first, first_payload) = framing::decode_frame(&frames[0]).unwrap();
    let (second, second_payload) = framing::decode_frame(&frames[1]).unwrap();

    assert!(first.has_more());
    assert!(!first.is_finish());
    assert_eq!(first.fragment_index(), 0);
    assert_eq!(first_payload.len(), MAX_PAYLOAD_LENGTH);
    assert!(second.is_finish());
    assert!(!second.has_more());
    assert_eq!(second.fragment_index(), 1);
    assert_eq!(second_payload.len(), 1);
}

#[test]
fn message_header_rejects_payload_above_the_frame_limit() {
    assert!(
        MessageHeader::new(
            FRAMING_VERSION,
            FLAG_FINISH,
            (MAX_PAYLOAD_LENGTH + 1) as u32,
            1,
            0,
            u32::MAX,
            0,
        )
        .is_err()
    );
}

#[test]
fn multi_fragment_message_preserves_fragment_order_and_stream_identity() {
    let payload = (0..(MAX_PAYLOAD_LENGTH + 1))
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    let frames = framing::encode_message(11, &payload).unwrap();
    let mut reconstructed = Vec::new();

    assert!(frames.len() > 1);

    for (index, frame) in frames.iter().enumerate() {
        let (header, fragment) = framing::decode_frame(frame).unwrap();
        assert_eq!(header.transport_stream_id(), 11);
        assert_eq!(header.fragment_index() as usize, index);
        reconstructed.extend_from_slice(&fragment);
    }

    assert_eq!(reconstructed, payload);
}

#[test]
fn missing_fragment_does_not_complete_a_logical_message() {
    let first = MessageHeader::new(FRAMING_VERSION, FLAG_MORE, 3, 12, 0, u32::MAX, 0).unwrap();
    let frame = framing::encode_frame(first, b"abc").unwrap();

    let sink = MemorySink::default();
    let source = MemorySource::new(frame);
    let stream = MessageStream::new(&sink, &source);

    assert!(matches!(
        stream.recv(),
        Err(StreamError::Closed | StreamError::Decode(_))
    ));
    assert!(matches!(stream.recv(), Err(StreamError::Decode(_))));
}

#[test]
fn duplicate_fragment_is_rejected_and_source_becomes_unusable() {
    let header = MessageHeader::new(FRAMING_VERSION, FLAG_MORE, 3, 13, 0, u32::MAX, 0).unwrap();
    let frame = framing::encode_frame(header, b"abc").unwrap();
    let mut bytes = frame.clone();
    bytes.extend_from_slice(&frame);

    let sink = MemorySink::default();
    let source = MemorySource::new(bytes);
    let stream = MessageStream::new(&sink, &source);

    assert!(matches!(stream.recv(), Err(StreamError::Decode(_))));
    assert!(matches!(stream.recv(), Err(StreamError::Decode(_))));
}

#[test]
fn finish_and_more_flags_are_mutually_exclusive_and_track_completion() {
    let more = MessageHeader::new(FRAMING_VERSION, FLAG_MORE, 1, 14, 0, u32::MAX, 0).unwrap();
    let finish = MessageHeader::new(FRAMING_VERSION, FLAG_FINISH, 1, 14, 1, u32::MAX, 0).unwrap();

    assert!(more.has_more());
    assert!(!more.is_finish());
    assert!(finish.is_finish());
    assert!(!finish.has_more());
}

#[test]
fn ack_and_sack_metadata_remain_transport_level_metadata() {
    let header = MessageHeader::new(
        FRAMING_VERSION,
        FLAG_ACK | FLAG_SACK_PRESENT,
        0,
        42,
        0,
        7,
        0x55,
    )
    .unwrap();

    assert!(header.is_ack());
    assert_eq!(header.cumulative_ack_index(), 7);
    assert_eq!(header.sack_bitmap(), 0x55);
}

#[test]
fn connection_fin_and_restart_flags_remain_distinct() {
    let fin =
        MessageHeader::new(FRAMING_VERSION, FLAG_CONNECTION_FIN, 0, 1, 0, u32::MAX, 0).unwrap();
    let restart = MessageHeader::new(FRAMING_VERSION, FLAG_RESTART, 0, 1, 0, u32::MAX, 0).unwrap();

    assert!(fin.is_connection_fin());
    assert!(!fin.is_restart());
    assert!(restart.is_restart());
    assert!(!restart.is_connection_fin());
}

#[test]
fn malformed_complete_frame_length_is_rejected() {
    let frame = framing::encode_message(15, b"payload").unwrap().remove(0);
    let malformed = frame[..frame.len() - 1].to_vec();

    assert!(framing::decode_frame(&malformed).is_err());
}

#[test]
fn arbitrary_binary_payload_remains_opaque_to_framing() {
    let payload = (0..4096).map(|n| (n * 37 % 256) as u8).collect::<Vec<_>>();
    let frames = framing::encode_message(16, &payload).unwrap();
    let mut reconstructed = Vec::new();

    for frame in frames {
        let (_, fragment) = framing::decode_frame(&frame).unwrap();
        reconstructed.extend_from_slice(&fragment);
    }

    assert_eq!(reconstructed, payload);
}

#[test]
fn malformed_frame_makes_message_stream_source_unusable() {
    let frame = framing::encode_message(17, b"payload").unwrap().remove(0);
    let mut malformed = frame;
    malformed[32] ^= 0x01;

    let sink = MemorySink::default();
    let source = MemorySource::new(malformed);
    let stream = MessageStream::new(&sink, &source);

    assert!(matches!(stream.recv(), Err(StreamError::Decode(_))));
    assert!(matches!(stream.recv(), Err(StreamError::Decode(_))));
}

#[test]
fn message_stream_round_trip_preserves_a_logical_message_across_fragments() {
    let payload = b"phase16 transport logical message".to_vec();
    let sink = MemorySink::default();
    let input = MemorySource::new(Vec::new());
    let sender = MessageStream::new(&sink, &input);

    sender.send(&payload).unwrap();
    let encoded = sink.take();

    let receive_sink = MemorySink::default();
    let source = MemorySource::new(encoded);
    let receiver = MessageStream::new(&receive_sink, &source);

    assert_eq!(receiver.recv().unwrap(), Some(payload));
}

#[test]
fn in_memory_transport_addresses_the_registered_concrete_instance() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase16-transport-engine").unwrap();
    let first = EngineInstanceId::new("phase16-transport-instance-a").unwrap();
    let second = EngineInstanceId::new("phase16-transport-instance-b").unwrap();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));

    let first_seen = Arc::clone(&seen);
    transport.register(engine.clone(), first.clone(), move |incoming| {
        first_seen.lock().unwrap().push("a".to_owned());
        UniversalResponse::new(incoming.event.envelope, Status::Success)
    });

    let second_seen = Arc::clone(&seen);
    transport.register(engine.clone(), second.clone(), move |incoming| {
        second_seen.lock().unwrap().push("b".to_owned());
        UniversalResponse::new(incoming.event.envelope, Status::Failure)
    });

    let response_a = futures::executor::block_on(transport.call(
        &first,
        request(
            "phase16-transport-engine",
            "phase16-transport-instance-a",
            "call-a",
            b"opaque-a",
        ),
    ))
    .unwrap();
    let response_b = futures::executor::block_on(transport.call(
        &second,
        request(
            "phase16-transport-engine",
            "phase16-transport-instance-b",
            "call-b",
            b"opaque-b",
        ),
    ))
    .unwrap();

    assert_eq!(response_a.status, Status::Success);
    assert_eq!(response_b.status, Status::Failure);
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["a".to_owned(), "b".to_owned()]
    );
}

#[test]
fn unregistered_in_memory_transport_target_returns_disconnected() {
    let transport = InMemoryTransport::new();
    let instance = EngineInstanceId::new("phase16-transport-missing-instance").unwrap();

    let result = futures::executor::block_on(transport.call(
        &instance,
        request(
            "phase16-transport-engine",
            "phase16-transport-missing-instance",
            "missing",
            b"payload",
        ),
    ));

    assert!(matches!(result, Err(TransportError::Disconnected)));
}

#[test]
fn large_logical_request_round_trips_through_in_memory_transport_framing() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase16-large-engine").unwrap();
    let instance = EngineInstanceId::new("phase16-large-instance").unwrap();
    let payload = vec![0xAB; MAX_PAYLOAD_LENGTH + 1024];

    transport.register(engine.clone(), instance.clone(), |request| {
        UniversalResponse::new(request.event.envelope, Status::Success)
    });

    let request = request(engine.as_str(), instance.as_str(), "large", &payload);
    let response = futures::executor::block_on(transport.call(&instance, request)).unwrap();

    assert_eq!(response.event.envelope.payload.bytes(), payload.as_slice());
}

#[test]
fn transport_fragmentation_is_not_application_streaming() {
    let logical_payload = vec![0x44; MAX_PAYLOAD_LENGTH + 1];
    let frames = framing::encode_message(21, &logical_payload).unwrap();
    let mut reconstructed = Vec::new();

    assert_eq!(frames.len(), 2);
    for frame in frames {
        let (_, fragment) = framing::decode_frame(&frame).unwrap();
        reconstructed.extend_from_slice(&fragment);
    }

    assert_eq!(reconstructed, logical_payload);
    assert_eq!(reconstructed.len(), logical_payload.len());
}

#[test]
fn transport_failure_remains_distinct_from_engine_level_failure() {
    let transport = InMemoryTransport::new();
    let engine = EngineId::new("phase16-error-engine").unwrap();
    let instance = EngineInstanceId::new("phase16-error-instance").unwrap();

    transport.register(engine.clone(), instance.clone(), |request| {
        UniversalResponse::new(request.event.envelope, Status::Failure)
    });

    let engine_failure = futures::executor::block_on(transport.call(
        &instance,
        request(
            engine.as_str(),
            instance.as_str(),
            "engine-failure",
            b"payload",
        ),
    ))
    .unwrap();

    assert_eq!(engine_failure.status, Status::Failure);

    let missing = EngineInstanceId::new("phase16-error-missing").unwrap();
    let transport_failure = futures::executor::block_on(transport.call(
        &missing,
        request(
            engine.as_str(),
            missing.as_str(),
            "transport-failure",
            b"payload",
        ),
    ));

    assert!(matches!(
        transport_failure,
        Err(TransportError::Disconnected)
    ));
}
