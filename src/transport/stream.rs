//! Stream abstractions for transport byte transfer.
//!
//! ByteSink and ByteSource are the primitive I/O traits used by transport
//! connections. They operate on raw bytes. `MessageStream` adds message
//! framing around those raw bytes, while `RawPayloadCodec` handles payload
//! encoding/decoding.

use crate::contracts::descriptor::{PayloadCodec, RawPayloadCodec};

/// Maximum permitted frame length before the payload buffer is allocated.
/// Frames claiming a larger length are rejected to avoid unbounded memory use.
pub const MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024;

/// Errors that can occur during stream operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamError {
    /// The stream is closed.
    Closed,
    /// Encoding failed.
    Encode(String),
    /// Decoding failed.
    Decode(String),
    /// The stream encountered an I/O error.
    Io(String),
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamError::Closed => write!(f, "stream is closed"),
            StreamError::Encode(msg) => write!(f, "encode error: {}", msg),
            StreamError::Decode(msg) => write!(f, "decode error: {}", msg),
            StreamError::Io(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<std::io::Error> for StreamError {
    fn from(e: std::io::Error) -> Self {
        StreamError::Io(e.to_string())
    }
}

/// A sink for writing bytes to a transport channel.
pub trait ByteSink: Send + Sync {
    /// Writes raw bytes to the channel.
    fn write(&self, buf: &[u8]) -> Result<(), StreamError>;

    /// Flushes any buffered writes.
    fn flush(&self) -> Result<(), StreamError>;

    /// Closes the sink for further writes.
    fn close(&self) -> Result<(), StreamError>;
}

/// A source for reading bytes from a transport channel.
pub trait ByteSource: Send + Sync {
    /// Reads bytes from the channel into the provided buffer.
    /// Returns the number of bytes read, or None if the channel is closed.
    fn read(&self, buf: &mut [u8]) -> Result<Option<usize>, StreamError>;
}

/// A framed message stream using `RawPayloadCodec` for payload encoding.
///
/// `MessageStream` wraps a raw `ByteSink`/`ByteSource` pair and owns the
/// message framing. Each message is encoded as a 4-byte big-endian payload
/// length followed by the encoded payload bytes.
pub struct MessageStream<'a> {
    sink: &'a dyn ByteSink,
    source: &'a dyn ByteSource,
    codec: RawPayloadCodec,
}

impl<'a> MessageStream<'a> {
    /// Creates a new `MessageStream` wrapping the provided sink and source.
    pub fn new(sink: &'a dyn ByteSink, source: &'a dyn ByteSource) -> Self {
        Self {
            sink,
            source,
            codec: RawPayloadCodec,
        }
    }

    /// Sends a single framed message.
    pub fn send(&self, message: &[u8]) -> Result<(), StreamError> {
        let encoded = self
            .codec
            .encode(message)
            .map_err(|e| StreamError::Encode(e.to_string()))?;

        if encoded.len() > MAX_FRAME_LENGTH {
            return Err(StreamError::Encode(
                "framed message exceeds the maximum length".into(),
            ));
        }

        let len = u32::try_from(encoded.len())
            .map_err(|_| StreamError::Encode("message is too large to frame".into()))?;

        let mut framed = Vec::with_capacity(4 + encoded.len());
        framed.extend_from_slice(&len.to_be_bytes());
        framed.extend_from_slice(&encoded);

        self.sink.write(&framed)?;
        self.sink.flush()
    }

    /// Receives a single framed message.
    pub fn recv(&self) -> Result<Option<Vec<u8>>, StreamError> {
        // Read the complete 4-byte, big-endian length prefix. A ByteSource
        // may legally return fewer bytes than requested, so do not assume
        // that one read fills the prefix.
        let mut len_buf = [0u8; 4];
        if !self.read_exact(&mut len_buf)? {
            return Ok(None);
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_FRAME_LENGTH {
            return Err(StreamError::Decode(
                "framed message exceeds the maximum length".into(),
            ));
        }
        // Read the complete encoded payload.
        let mut payload_buf = vec![0u8; len];
        if len > 0 && !self.read_exact(&mut payload_buf)? {
            return Err(StreamError::Closed);
        }

        let payload = self
            .codec
            .decode(&payload_buf)
            .map_err(|e: crate::contracts::EncodingError| StreamError::Decode(e.to_string()))?;
        Ok(Some(payload))
    }

    /// Reads exactly `buf.len()` bytes, returning `false` only when the stream
    /// closes before any bytes for this read are available.
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