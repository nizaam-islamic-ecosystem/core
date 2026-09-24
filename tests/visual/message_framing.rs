use crate::support::{section, separator, show_bytes, show_frame, step, success};
use nizaam_core::transport::framing::{
    FLAG_ACK, FLAG_FINISH, FLAG_MORE, FLAG_SACK_PRESENT, FRAMING_VERSION, HEADER_LENGTH,
    MAX_PAYLOAD_LENGTH, MessageHeader, NO_ACK_INDEX, decode_frame, encode_frame, encode_message,
};

#[test]
fn visual_message_framing_and_reassembly() {
    section("NIZAAM CORE — MESSAGE FRAMING");
    step(1, "48-byte header");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("visual")
        .join("dummy.json");
    let payload = std::fs::read(&fixture)
        .expect("tests/visual/dummy.json must exist under the Cargo manifest directory");
    let header = MessageHeader::new(
        FRAMING_VERSION,
        FLAG_FINISH,
        payload.len() as u32,
        77,
        0,
        NO_ACK_INDEX,
        0,
    )
    .unwrap();
    let frame = encode_frame(header, &payload).unwrap();
    show_bytes("logical payload", &payload);
    println!("  header length     : {HEADER_LENGTH} bytes");
    println!("  frame length      : {} bytes", frame.len());
    assert_eq!(frame.len(), HEADER_LENGTH + payload.len());

    step(2, "decode and verify checksum");
    let (decoded, decoded_payload) = decode_frame(&frame).unwrap();
    assert_eq!(decoded_payload, payload);
    assert!(decoded.is_finish());
    println!("  stream id         : {}", decoded.transport_stream_id());
    println!("  fragment index    : {}", decoded.fragment_index());
    println!("  checksum          : 0x{:016x}", decoded.checksum());
    success("frame decoded and checksum verified");

    step(3, "logical JSON message and fragmentation boundary");
    let frames = encode_message(1234, &payload).unwrap();
    println!(
        "  dummy.json size    : {} bytes ({:.2} MiB)",
        payload.len(),
        payload.len() as f64 / (1024.0 * 1024.0)
    );
    println!("  max frame size     : 20,000,000 bytes");
    println!("  encoded fragments : {}", frames.len());
    assert!(payload.len() < MAX_PAYLOAD_LENGTH);
    assert_eq!(frames.len(), 1);
    let mut rebuilt = Vec::new();
    for (index, raw) in frames.iter().enumerate() {
        let (header, bytes) = decode_frame(raw).unwrap();
        show_frame(
            index,
            raw.len(),
            header.transport_stream_id(),
            header.flags(),
        );
        assert_eq!(header.transport_stream_id(), 1234);
        assert_eq!(header.fragment_index(), index as u32);
        assert_eq!(header.has_more(), index + 1 < frames.len());
        assert_eq!(header.is_finish(), index + 1 == frames.len());
        rebuilt.extend_from_slice(&bytes);
    }
    assert_eq!(rebuilt, payload);
    success("dummy.json payload preserved without unnecessary fragmentation");

    step(4, "invalid flags and checksum corruption");
    assert!(
        MessageHeader::new(
            FRAMING_VERSION,
            FLAG_FINISH | FLAG_MORE,
            0,
            1,
            0,
            NO_ACK_INDEX,
            0
        )
        .is_err()
    );
    assert!(
        MessageHeader::new(FRAMING_VERSION, FLAG_SACK_PRESENT, 0, 1, 0, NO_ACK_INDEX, 1).is_err()
    );
    let ack = MessageHeader::new(
        FRAMING_VERSION,
        FLAG_ACK | FLAG_SACK_PRESENT,
        0,
        9,
        0,
        3,
        0b101,
    )
    .unwrap();
    let ack_frame = encode_frame(ack, &[]).unwrap();
    assert!(decode_frame(&ack_frame).unwrap().0.is_ack());
    let mut corrupt = frame.clone();
    corrupt[HEADER_LENGTH] ^= 1;
    assert!(decode_frame(&corrupt).is_err());
    separator();
    success("48-byte framing rules, fragmentation, checksum, and invalid-frame rejection verified");
}
