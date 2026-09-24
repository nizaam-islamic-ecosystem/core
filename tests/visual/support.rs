use nizaam_core::identity::{
    CapabilityId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
};

pub fn section(title: &str) {
    println!("\n============================================================");
    println!("{title}");
    println!("============================================================");
}

pub fn step(number: usize, label: &str) {
    println!("STEP {number:02} — {label}");
}
pub fn success(message: &str) {
    println!("  ✓ {message}");
}
pub fn failure(message: &str) {
    println!("  ✗ {message}");
}
pub fn separator() {
    println!("------------------------------------------------------------");
}
pub fn show_arrow(from: &str, to: &str) {
    println!("  {from}  →  {to}");
}
pub fn show_bytes(label: &str, bytes: &[u8]) {
    println!("  {label:<18}: {} bytes", bytes.len());
}
pub fn show_frame(index: usize, frame_len: usize, stream_id: u64, flags: u8) {
    println!("  frame {index:<2} length={frame_len:<8} stream={stream_id:<4} flags=0x{flags:02x}");
}
pub fn show_identity(
    operation: &OperationId,
    message: &MessageId,
    correlation: &CorrelationId,
    engine: &EngineId,
    instance: &EngineInstanceId,
    capability: &CapabilityId,
) {
    println!("  OperationId       : {operation}");
    println!("  MessageId         : {message}");
    println!("  CorrelationId     : {correlation}");
    println!("  EngineId          : {engine}");
    println!("  EngineInstanceId  : {instance}");
    println!("  CapabilityId      : {capability}");
}
