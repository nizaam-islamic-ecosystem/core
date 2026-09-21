//! Stream abstractions for transport byte transfer.
//!
//! `ByteSink` and `ByteSource` are raw byte I/O primitives. `MessageStream`
//! owns the Phase 7 message framing, fragmentation, checksum verification,
//! and logical-message reassembly. Payload bytes remain opaque to transport.

use crate::contracts::descriptor::{PayloadCodec, RawPayloadCodec};
use crate::transport::framing::{
    self, FLAG_ACK, FLAG_CONNECTION_FIN, FLAG_FINISH, FLAG_MORE, FLAG_RESTART, FLAG_SACK_PRESENT,
    FRAMING_VERSION, HEADER_LENGTH, MAX_FRAME_LENGTH_BYTES, MAX_PAYLOAD_LENGTH, MessageHeader,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Maximum complete wire-frame length, in decimal bytes.
pub const MAX_FRAME_LENGTH: usize = MAX_FRAME_LENGTH_BYTES;

/// A value used in `Cumulative ACK Index` when no fragment has yet been
/// cumulatively acknowledged.
pub const NO_CUMULATIVE_ACK: u32 = u32::MAX;

/// Errors that can occur during stream operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamError {
    Closed,
    Encode(String),
    Decode(String),
    Io(String),
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamError::Closed => write!(f, "stream is closed"),
            StreamError::Encode(msg) => write!(f, "encode error: {msg}"),
            StreamError::Decode(msg) => write!(f, "decode error: {msg}"),
            StreamError::Io(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<std::io::Error> for StreamError {
    fn from(e: std::io::Error) -> Self {
        StreamError::Io(e.to_string())
    }
}

/// A sink for writing raw bytes to a transport channel.
pub trait ByteSink: Send + Sync {
    fn write(&self, buf: &[u8]) -> Result<(), StreamError>;
    fn flush(&self) -> Result<(), StreamError>;
    fn close(&self) -> Result<(), StreamError>;
}

/// Shared receive state owned by a `ByteSource`.
#[derive(Debug)]
pub struct ByteSourceState {
    recv_lock: Mutex<()>,
    unusable: AtomicBool,
    reassembly: Mutex<ReassemblyState>,
}

impl ByteSourceState {
    pub fn new() -> Self {
        Self {
            recv_lock: Mutex::new(()),
            unusable: AtomicBool::new(false),
            reassembly: Mutex::new(ReassemblyState::default()),
        }
    }
}

impl Default for ByteSourceState {
    fn default() -> Self {
        Self::new()
    }
}

/// A source for reading raw bytes from a transport channel.
pub trait ByteSource: Send + Sync {
    fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError>;

    /// Returns source-owned state shared by every `MessageStream` wrapper.
    fn stream_state(&self) -> &ByteSourceState;
}

#[derive(Default, Debug)]
struct ReassemblyState {
    streams: HashMap<u64, IncomingStream>,
}

#[derive(Debug)]
struct IncomingStream {
    fragments: BTreeMap<u32, Vec<u8>>,
    final_index: Option<u32>,
    cumulative_ack_index: u32,
}

impl IncomingStream {
    fn new() -> Self {
        Self {
            fragments: BTreeMap::new(),
            final_index: None,
            cumulative_ack_index: NO_CUMULATIVE_ACK,
        }
    }

    fn insert(
        &mut self,
        header: &MessageHeader,
        payload: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, StreamError> {
        let index = header.fragment_index();

        if self.fragments.contains_key(&index) {
            return Err(StreamError::Decode(format!(
                "duplicate fragment index {index} for transport stream {}",
                header.transport_stream_id()
            )));
        }

        if header.is_finish()
            && self
                .final_index
                .replace(index)
                .is_some_and(|old| old != index)
        {
            return Err(StreamError::Decode(
                "multiple conflicting final fragments received".into(),
            ));
        }

        self.fragments.insert(index, payload);

        let mut next = match self.cumulative_ack_index {
            NO_CUMULATIVE_ACK => 0,
            value => value.saturating_add(1),
        };

        while self.fragments.contains_key(&next) {
            self.cumulative_ack_index = next;
            next = next.saturating_add(1);
            if next == u32::MAX {
                break;
            }
        }

        let Some(final_index) = self.final_index else {
            return Ok(None);
        };

        if self.cumulative_ack_index != final_index {
            return Ok(None);
        }

        let mut message = Vec::new();
        for index in 0..=final_index {
            let fragment = self.fragments.remove(&index).ok_or_else(|| {
                StreamError::Decode("final fragment observed with missing fragment".into())
            })?;
            message.extend_from_slice(&fragment);
        }

        Ok(Some(message))
    }
}

/// A framed transport message stream.
///
/// A call to `send` represents one logical message. Messages larger than the
/// per-frame payload limit are fragmented rather than rejected. `recv` emits a
/// logical message only after all fragments through its FINISH fragment have
/// arrived.
pub struct MessageStream<'a> {
    sink: &'a dyn ByteSink,
    source: &'a dyn ByteSource,
    codec: RawPayloadCodec,
    source_state: &'a ByteSourceState,
}

impl<'a> MessageStream<'a> {
    pub fn new(sink: &'a dyn ByteSink, source: &'a dyn ByteSource) -> Self {
        Self {
            sink,
            source,
            codec: RawPayloadCodec,
            source_state: source.stream_state(),
        }
    }

    /// Sends one logical message, fragmenting it into frames as necessary.
    pub fn send(&self, message: &[u8]) -> Result<(), StreamError> {
        let encoded = self
            .codec
            .encode(message)
            .map_err(|e| StreamError::Encode(e.to_string()))?;

        let stream_id = next_transport_stream_id();
        let fragment_count = if encoded.is_empty() {
            1
        } else {
            encoded.len().div_ceil(MAX_PAYLOAD_LENGTH)
        };

        for fragment_index in 0..fragment_count {
            let start = fragment_index * MAX_PAYLOAD_LENGTH;
            let end = encoded.len().min(start + MAX_PAYLOAD_LENGTH);
            let payload = &encoded[start..end];
            let is_final = fragment_index + 1 == fragment_count;

            let flags = if is_final { FLAG_FINISH } else { FLAG_MORE };
            let header = MessageHeader::new(
                FRAMING_VERSION,
                flags,
                u32::try_from(payload.len())
                    .map_err(|_| StreamError::Encode("frame payload is too large".into()))?,
                stream_id,
                u32::try_from(fragment_index)
                    .map_err(|_| StreamError::Encode("fragment index overflow".into()))?,
                NO_CUMULATIVE_ACK,
                0,
            )
            .map_err(StreamError::Encode)?;

            let frame = framing::encode_frame(header, payload).map_err(StreamError::Encode)?;
            if frame.len() > MAX_FRAME_LENGTH {
                return Err(StreamError::Encode(
                    "encoded frame exceeds MAX_FRAME_LENGTH".into(),
                ));
            }

            self.sink.write(&frame)?;
        }

        self.sink.flush()
    }

    /// Sends an acknowledgement frame for a transport stream.
    ///
    /// `cumulative_ack_index` is the highest contiguous fragment received.
    /// `sack_bitmap` uses bit 0 for the fragment immediately after that index.
    pub fn send_ack(
        &self,
        transport_stream_id: u64,
        cumulative_ack_index: u32,
        sack_bitmap: u64,
    ) -> Result<(), StreamError> {
        let mut flags = FLAG_ACK;
        if sack_bitmap != 0 {
            flags |= FLAG_SACK_PRESENT;
        }

        let header = MessageHeader::new(
            FRAMING_VERSION,
            flags,
            0,
            transport_stream_id,
            0,
            cumulative_ack_index,
            sack_bitmap,
        )
        .map_err(StreamError::Encode)?;

        let frame = framing::encode_frame(header, &[]).map_err(StreamError::Encode)?;
        self.sink.write(&frame)?;
        self.sink.flush()
    }

    /// Receives one complete logical message.
    ///
    /// ACK frames are transport control frames and are consumed here rather
    /// than exposed as application payloads. Connection FIN and RESTART are
    /// likewise handled as transport state and are not application messages.
    pub fn recv(&self) -> Result<Option<Vec<u8>>, StreamError> {
        let _guard = self
            .source_state
            .recv_lock
            .lock()
            .map_err(|_| StreamError::Io("message stream receive lock is poisoned".into()))?;

        if self.source_state.unusable.load(Ordering::Acquire) {
            return Err(StreamError::Decode(
                "stream is unusable after a framing protocol violation".into(),
            ));
        }

        loop {
            let frame = self.read_frame()?;
            let Some(frame) = frame else {
                if self.has_incomplete_streams()? {
                    self.source_state.unusable.store(true, Ordering::Release);
                    return Err(StreamError::Decode(
                        "stream closed before all fragmented messages were completed".into(),
                    ));
                }
                return Ok(None);
            };

            let (header, payload) = framing::decode_frame(&frame).map_err(StreamError::Decode)?;

            if header.flags() & FLAG_RESTART != 0 {
                self.source_state.unusable.store(true, Ordering::Release);
                return Err(StreamError::Decode(
                    "transport restart requested; current stream is unusable".into(),
                ));
            }

            if header.flags() & FLAG_CONNECTION_FIN != 0 {
                if self.has_incomplete_streams()? {
                    self.source_state.unusable.store(true, Ordering::Release);
                    return Err(StreamError::Decode(
                        "CONNECTION_FIN received while a transport stream is incomplete".into(),
                    ));
                }
                return Ok(None);
            }

            if header.is_ack() {
                if !payload.is_empty() {
                    return Err(StreamError::Decode(
                        "ACK frame must have a zero-length payload".into(),
                    ));
                }
                continue;
            }

            if header.flags() & FLAG_MORE == 0 && !header.is_finish() {
                return Err(StreamError::Decode(
                    "data frame must carry FINISH or MORE".into(),
                ));
            }

            let completed = {
                let mut state = self
                    .source_state
                    .reassembly
                    .lock()
                    .map_err(|_| StreamError::Io("reassembly lock is poisoned".into()))?;

                let stream = state
                    .streams
                    .entry(header.transport_stream_id())
                    .or_insert_with(IncomingStream::new);

                stream.insert(&header, payload)?
            };

            if let Some(message) = completed {
                self.source_state
                    .reassembly
                    .lock()
                    .map_err(|_| StreamError::Io("reassembly lock is poisoned".into()))?
                    .streams
                    .remove(&header.transport_stream_id());

                let decoded =
                    self.codec
                        .decode(&message)
                        .map_err(|e: crate::contracts::EncodingError| {
                            StreamError::Decode(e.to_string())
                        })?;

                return Ok(Some(decoded));
            }
        }
    }

    fn read_frame(&self) -> Result<Option<Vec<u8>>, StreamError> {
        let mut fixed_header = [0u8; HEADER_LENGTH];
        if !self.read_exact(&mut fixed_header)? {
            return Ok(None);
        }

        let header = match MessageHeader::deserialize(&fixed_header) {
            Ok(header) => header,
            Err(error) => {
                self.source_state.unusable.store(true, Ordering::Release);
                return Err(StreamError::Decode(error));
            }
        };
        let payload_len = header.payload_length() as usize;
        if payload_len > MAX_PAYLOAD_LENGTH {
            self.source_state.unusable.store(true, Ordering::Release);
            return Err(StreamError::Decode(
                "frame payload exceeds the configured maximum".into(),
            ));
        }

        let mut frame = Vec::with_capacity(HEADER_LENGTH + payload_len);
        frame.extend_from_slice(&fixed_header);

        if payload_len > 0 {
            let mut payload = vec![0u8; payload_len];
            if !self.read_exact(&mut payload)? {
                self.source_state.unusable.store(true, Ordering::Release);
                return Err(StreamError::Closed);
            }
            frame.extend_from_slice(&payload);
        }

        Ok(Some(frame))
    }

    fn has_incomplete_streams(&self) -> Result<bool, StreamError> {
        let state = self
            .source_state
            .reassembly
            .lock()
            .map_err(|_| StreamError::Io("reassembly lock is poisoned".into()))?;
        Ok(!state.streams.is_empty())
    }

    fn read_exact(&self, buf: &mut [u8]) -> Result<bool, StreamError> {
        let mut offset = 0;
        while offset < buf.len() {
            match self.source.read(&mut buf[offset..])? {
                Some(0) => return Err(StreamError::Closed),
                None if offset == 0 => return Ok(false),
                None => return Err(StreamError::Closed),
                Some(n) => offset += n,
            }
        }
        Ok(true)
    }
}

fn next_transport_stream_id() -> u64 {
    static NEXT_ID: OnceLock<AtomicU64> = OnceLock::new();
    let counter = NEXT_ID.get_or_init(|| AtomicU64::new(1));
    let id = counter.fetch_add(1, Ordering::Relaxed);
    if id == 0 {
        counter.fetch_add(1, Ordering::Relaxed)
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Condvar, Mutex};

    /// A synchronized byte channel using a single mutex for buffer and closed state.
    struct TestChannel {
        state: Arc<Mutex<TestState>>,
        signal: Arc<Condvar>,
    }

    struct TestState {
        buffer: Vec<u8>,
        closed: bool,
    }

    impl TestChannel {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(TestState {
                    buffer: Vec::new(),
                    closed: false,
                })),
                signal: Arc::new(Condvar::new()),
            }
        }
        fn split(&self) -> (TestWriteEnd, TestReadEnd) {
            let s1 = Arc::clone(&self.state);
            let s2 = Arc::clone(&self.state);
            let c1 = Arc::clone(&self.signal);
            let c2 = Arc::clone(&self.signal);
            (
                TestWriteEnd {
                    state: s1,
                    signal: c1,
                },
                TestReadEnd {
                    state: s2,
                    signal: c2,
                    stream_state: ByteSourceState::new(),
                },
            )
        }
    }

    struct TestWriteEnd {
        state: Arc<Mutex<TestState>>,
        signal: Arc<Condvar>,
    }

    struct TestReadEnd {
        state: Arc<Mutex<TestState>>,
        signal: Arc<Condvar>,
        stream_state: ByteSourceState,
    }

    impl ByteSink for TestWriteEnd {
        fn write(&self, buf: &[u8]) -> Result<(), StreamError> {
            let mut state = self.state.lock().unwrap();
            if state.closed {
                return Err(StreamError::Closed);
            }
            state.buffer.extend_from_slice(buf);
            drop(state);
            self.signal.notify_one();
            Ok(())
        }
        fn flush(&self) -> Result<(), StreamError> {
            Ok(())
        }
        fn close(&self) -> Result<(), StreamError> {
            let mut state = self.state.lock().unwrap();
            state.closed = true;
            drop(state);
            self.signal.notify_one();
            Ok(())
        }
    }

    impl ByteSource for TestReadEnd {
        fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError> {
            let mut state = self.state.lock().unwrap();
            while state.buffer.is_empty() && !state.closed {
                state = self
                    .signal
                    .wait(state)
                    .map_err(|e| StreamError::Io(e.to_string()))?;
            }
            if state.buffer.is_empty() {
                return Ok(None);
            }
            let n = state.buffer.len().min(buf.len());
            buf[..n].copy_from_slice(&state.buffer[..n]);
            state.buffer.drain(..n);
            Ok(Some(n))
        }

        fn stream_state(&self) -> &ByteSourceState {
            &self.stream_state
        }
    }

    #[test]
    fn message_stream_send_and_recv() {
        let channel = TestChannel::new();
        let (write_end, read_end) = channel.split();
        let sink_arc = Arc::new(write_end);
        let source_arc = Arc::new(read_end);
        let stream = MessageStream::new(&*sink_arc, &*source_arc);
        stream.send(b"hello world").unwrap();
        let received = stream.recv().unwrap();
        assert_eq!(received, Some(b"hello world".to_vec()));
    }

    #[test]
    fn message_stream_recv_rejects_oversized_frame_and_invalidates_shared_source() {
        let channel = TestChannel::new();
        let (write_end, read_end) = channel.split();
        let sink_arc = Arc::new(write_end);
        let source_arc = Arc::new(read_end);

        {
            let stream = MessageStream::new(&*sink_arc, &*source_arc);

            let mut invalid_header = vec![0u8; HEADER_LENGTH];
            invalid_header[0] = FRAMING_VERSION;
            invalid_header[2..4].copy_from_slice(&(HEADER_LENGTH as u16).to_be_bytes());
            invalid_header[4..8]
                .copy_from_slice(&(u32::try_from(MAX_PAYLOAD_LENGTH + 1).unwrap()).to_be_bytes());
            sink_arc.write(&invalid_header).unwrap();

            let error = stream.recv().unwrap_err();
            assert_eq!(
                error,
                StreamError::Decode(format!(
                    "payload_length exceeds maximum frame payload ({MAX_PAYLOAD_LENGTH})"
                ))
            );
        }

        // A new wrapper over the same ByteSource must observe the same
        // terminal state rather than attempting to parse the rejected bytes.
        let recreated = MessageStream::new(&*sink_arc, &*source_arc);
        let error = recreated.recv().unwrap_err();
        assert_eq!(
            error,
            StreamError::Decode("stream is unusable after a framing protocol violation".into())
        );
    }

    #[test]
    fn message_stream_wrappers_share_receive_serialization() {
        let channel = TestChannel::new();
        let (write_end, read_end) = channel.split();
        let sink_arc = Arc::new(write_end);
        let source_arc = Arc::new(read_end);

        let stream = MessageStream::new(&*sink_arc, &*source_arc);
        stream.send(b"first").unwrap();
        stream.send(b"second").unwrap();

        let source_for_a = Arc::clone(&source_arc);
        let source_for_b = Arc::clone(&source_arc);
        let sink_for_a = Arc::clone(&sink_arc);
        let sink_for_b = Arc::clone(&sink_arc);

        let handle_a = std::thread::spawn(move || {
            let stream = MessageStream::new(&*sink_for_a, &*source_for_a);
            stream.recv().unwrap().unwrap()
        });

        let handle_b = std::thread::spawn(move || {
            let stream = MessageStream::new(&*sink_for_b, &*source_for_b);
            stream.recv().unwrap().unwrap()
        });

        let mut messages = vec![handle_a.join().unwrap(), handle_b.join().unwrap()];
        messages.sort();

        assert_eq!(messages, vec![b"first".to_vec(), b"second".to_vec()]);
    }

    struct TestReadProxy<'a> {
        source: &'a TestReadEnd,
    }

    impl<'a> ByteSource for TestReadProxy<'a> {
        fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError> {
            self.source.read(buf)
        }

        fn stream_state(&self) -> &ByteSourceState {
            self.source.stream_state()
        }
    }

    #[test]
    fn message_stream_proxy_preserves_source_state() {
        let channel = TestChannel::new();
        let (write_end, read_end) = channel.split();
        let sink_arc = Arc::new(write_end);
        let source_arc = Arc::new(read_end);

        {
            let stream = MessageStream::new(&*sink_arc, &*source_arc);
            let mut invalid_header = vec![0u8; HEADER_LENGTH];
            invalid_header[0] = FRAMING_VERSION;
            invalid_header[2..4].copy_from_slice(&(HEADER_LENGTH as u16).to_be_bytes());
            invalid_header[4..8]
                .copy_from_slice(&(u32::try_from(MAX_PAYLOAD_LENGTH + 1).unwrap()).to_be_bytes());
            sink_arc.write(&invalid_header).unwrap();

            let error = stream.recv().unwrap_err();
            assert_eq!(
                error,
                StreamError::Decode(format!(
                    "payload_length exceeds maximum frame payload ({MAX_PAYLOAD_LENGTH})"
                ))
            );
        }

        let proxy = TestReadProxy {
            source: &source_arc,
        };
        let stream = MessageStream::new(&*sink_arc, &proxy);

        let error = stream.recv().unwrap_err();
        assert_eq!(
            error,
            StreamError::Decode("stream is unusable after a framing protocol violation".into())
        );
    }

    #[test]
    fn message_stream_recv_none_when_closed() {
        let channel = TestChannel::new();
        let (write_end, read_end) = channel.split();
        let sink_arc = Arc::new(write_end);
        let source_arc = Arc::new(read_end);
        sink_arc.close().unwrap();
        let stream = MessageStream::new(&*sink_arc, &*source_arc);
        let result = stream.recv();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
