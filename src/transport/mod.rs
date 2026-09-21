//! Phase 7 boundary for transport abstractions, connections, and streams.
//!
//! Transport is the provider-neutral abstraction for sending and receiving
//! byte-oriented messages between engines. Concrete transports (in-memory,
//! gRPC, HTTP, etc.) implement the `Transport` trait. Core supplies one
//! in-memory reference implementation; all other transports live outside Core.

pub mod connection;
pub mod framing;
pub mod in_memory;
pub mod stream;
pub mod transport_trait;

pub use connection::{Connection, ConnectionState};
pub use framing::{
    FLAG_ACK, FLAG_CONNECTION_FIN, FLAG_FINISH, FLAG_MORE, FLAG_RESTART, FLAG_SACK_PRESENT,
    FRAMING_VERSION, HEADER_LENGTH, MAX_FRAME_LENGTH_BYTES, MAX_PAYLOAD_LENGTH, MessageHeader,
    checksum, decode_frame, encode_frame, encode_message, validate_flags,
};
pub use in_memory::InMemoryTransport;
pub use stream::{ByteSink, ByteSource, MessageStream, StreamError};
pub use transport_trait::{BoxedFuture, Transport, TransportError, TransportResult};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::envelope::MessageEnvelope;
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::contracts::{UniversalRequest, UniversalResponse};
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::status::Status;

    fn request(
        target: &EngineId,
        target_instance: &EngineInstanceId,
        message_id: &str,
    ) -> UniversalRequest {
        let descriptor = ContractDescriptor::new(
            ContractId::new("transport.integration.request").unwrap(),
            CapabilityId::new("transport.integration").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            descriptor.clone(),
            Participants::new(EngineId::new("caller").unwrap(), target.clone())
                .with_target_instance(target_instance.clone()),
        );

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new(format!("operation-{message_id}")).unwrap(),
            CorrelationId::new(format!("correlation-{message_id}")).unwrap(),
        ));

        let envelope = MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"transport integration".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    fn response(request: UniversalRequest, status: Status) -> UniversalResponse {
        UniversalResponse::new(request.event.envelope, status)
    }

    #[test]
    fn transport_module_composes_instance_addressing_with_transport_trait() {
        let transport = InMemoryTransport::new();
        let engine = EngineId::new("quran-engine").unwrap();
        let first = EngineInstanceId::new("quran-engine-01").unwrap();
        let second = EngineInstanceId::new("quran-engine-02").unwrap();

        transport.register(engine.clone(), first.clone(), |request| {
            response(request, Status::Success)
        });
        transport.register(engine.clone(), second.clone(), |request| {
            response(request, Status::Failure)
        });

        let transport_api: &dyn Transport = &transport;

        assert!(transport_api.is_connected(&first));
        assert!(transport_api.is_connected(&second));
        assert_eq!(transport_api.connected_targets().len(), 2);

        let first_response = futures::executor::block_on(
            transport_api.call(&first, request(&engine, &first, "message-1")),
        )
        .unwrap();

        let second_response = futures::executor::block_on(
            transport_api.call(&second, request(&engine, &second, "message-2")),
        )
        .unwrap();

        assert_eq!(first_response.status, Status::Success);
        assert_eq!(second_response.status, Status::Failure);
        assert_eq!(
            first_response.event.envelope.message_id.as_str(),
            "message-1"
        );
        assert_eq!(
            second_response.event.envelope.message_id.as_str(),
            "message-2"
        );

        let unregistered = EngineInstanceId::new("quran-engine-missing").unwrap();
        let result = futures::executor::block_on(
            transport_api.call(&unregistered, request(&engine, &unregistered, "message-3")),
        );

        assert!(matches!(result, Err(TransportError::Disconnected)));
    }

    #[test]
    fn transport_module_reexports_binary_header_round_trip() {
        let header = MessageHeader::new(1, FLAG_MORE, 1024, 7, 3, u32::MAX, 0).unwrap();

        let encoded = header.serialize();
        let decoded = MessageHeader::deserialize(&encoded).unwrap();

        assert_eq!(decoded, header);
        assert_eq!(encoded.len(), HEADER_LENGTH);
    }

    struct MemorySink {
        bytes: std::sync::Mutex<Vec<u8>>,
    }

    impl MemorySink {
        fn new() -> Self {
            Self {
                bytes: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn bytes(&self) -> Vec<u8> {
            self.bytes.lock().unwrap().clone()
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
        bytes: std::sync::Mutex<Vec<u8>>,
        state: stream::ByteSourceState,
    }

    impl MemorySource {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes: std::sync::Mutex::new(bytes),
                state: stream::ByteSourceState::new(),
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

        fn stream_state(&self) -> &stream::ByteSourceState {
            &self.state
        }
    }

    #[test]
    fn transport_module_composes_byte_sink_source_and_message_stream() {
        let sink = MemorySink::new();
        let send_source = MemorySource::new(Vec::new());
        let send_stream = MessageStream::new(&sink, &send_source);

        send_stream.send(b"transport payload").unwrap();

        let encoded_frame = sink.bytes();
        assert!(encoded_frame.len() >= HEADER_LENGTH);
        let header = MessageHeader::deserialize(&encoded_frame[..HEADER_LENGTH]).unwrap();
        assert_eq!(header.payload_length() as usize, b"transport payload".len());
        assert!(header.is_finish());

        let source = MemorySource::new(encoded_frame);
        let receive_stream = MessageStream::new(&sink, &source);

        assert_eq!(
            receive_stream.recv().unwrap(),
            Some(b"transport payload".to_vec())
        );
    }

    #[test]
    fn connection_state_is_reexported_as_transport_lifecycle_state() {
        assert!(!ConnectionState::Connecting.is_open());
        assert!(ConnectionState::Open.is_open());
        assert!(!ConnectionState::Closing.is_open());
        assert!(!ConnectionState::Closed.is_open());
    }
}
