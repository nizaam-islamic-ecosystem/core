//! In-memory transport reference implementation for testing and local use.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::{EngineId, EngineInstanceId};
use crate::transport::{BoxedFuture, Transport, TransportError, framing};

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

type ChannelBuffer = (Vec<u8>, Vec<u8>);
type RegistrationMap = Arc<Mutex<HashMap<EngineInstanceId, InMemoryRegistration>>>;

struct InMemoryHandler {
    respond: Arc<dyn Fn(UniversalRequest) -> UniversalResponse + Send + Sync>,
}

impl Clone for InMemoryHandler {
    fn clone(&self) -> Self {
        Self {
            respond: Arc::clone(&self.respond),
        }
    }
}

#[derive(Clone)]
struct InMemoryRegistration {
    engine: EngineId,
    handler: InMemoryHandler,
    channel: Arc<Mutex<ChannelBuffer>>,
}

pub struct InMemoryTransport {
    registrations: RegistrationMap,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self {
            registrations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers one concrete engine instance.
    ///
    /// Multiple instances may belong to the same logical engine because
    /// registration is keyed by `EngineInstanceId`.
    pub fn register<F>(&self, engine: EngineId, instance: EngineInstanceId, handler: F)
    where
        F: Fn(UniversalRequest) -> UniversalResponse + Send + Sync + 'static,
    {
        let registration = InMemoryRegistration {
            engine,
            handler: InMemoryHandler {
                respond: Arc::new(handler),
            },
            channel: Arc::new(Mutex::new((Vec::new(), Vec::new()))),
        };

        // Publish the handler, channel, and call lock together under one
        // registry lock so calls cannot observe a partially registered target.
        self.registrations
            .lock()
            .unwrap()
            .insert(instance, registration);
    }

    /// Returns the concrete engine instances registered with this transport.
    pub fn registered_targets(&self) -> Vec<EngineInstanceId> {
        self.registrations.lock().unwrap().keys().cloned().collect()
    }

    /// Returns the logical engine id associated with a concrete instance.
    pub fn registered_engine(&self, instance: &EngineInstanceId) -> Option<EngineId> {
        self.registrations
            .lock()
            .unwrap()
            .get(instance)
            .map(|registration| registration.engine.clone())
    }
}

impl Default for InMemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for InMemoryTransport {
    fn call(
        &self,
        target: &EngineInstanceId,
        request: UniversalRequest,
    ) -> BoxedFuture<UniversalResponse, TransportError> {
        let registrations = Arc::clone(&self.registrations);
        let target = target.clone();

        Box::pin(async move {
            let registration = {
                let registrations = registrations.lock().unwrap();
                registrations.get(&target).cloned()
            };

            let registration = match registration {
                Some(registration) => registration,
                None => return Err(TransportError::Disconnected),
            };

            let encoded =
                serde_json::to_vec(&request).map_err(|e| TransportError::Encode(e.to_string()))?;

            let stream_id = next_transport_stream_id();
            let frames =
                framing::encode_message(stream_id, &encoded).map_err(TransportError::Encode)?;

            let payload = {
                let mut channel = registration
                    .channel
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());

                let (tx, _rx) = &mut *channel;
                for frame in frames {
                    tx.extend_from_slice(&frame);
                }

                let mut reassembled = Vec::new();

                loop {
                    if tx.len() < framing::HEADER_LENGTH {
                        break;
                    }

                    let header = framing::MessageHeader::deserialize(&tx[..framing::HEADER_LENGTH])
                        .map_err(TransportError::Decode)?;

                    let frame_len = framing::HEADER_LENGTH + header.payload_length() as usize;
                    if tx.len() < frame_len {
                        break;
                    }

                    let frame = tx.drain(..frame_len).collect::<Vec<u8>>();
                    let (header, fragment) =
                        framing::decode_frame(&frame).map_err(TransportError::Decode)?;

                    if header.transport_stream_id() != stream_id {
                        return Err(TransportError::Decode(
                            "transport stream id changed during one in-memory call".into(),
                        ));
                    }

                    reassembled.extend_from_slice(&fragment);

                    if header.is_finish() {
                        break;
                    }
                }

                reassembled
            };

            let decoded: UniversalRequest = serde_json::from_slice(&payload)
                .map_err(|e| TransportError::Decode(e.to_string()))?;

            let response = (registration.handler.respond)(decoded);

            Ok(response)
        })
    }

    fn is_connected(&self, target: &EngineInstanceId) -> bool {
        self.registrations.lock().unwrap().contains_key(target)
    }

    fn connected_targets(&self) -> Vec<EngineInstanceId> {
        self.registrations.lock().unwrap().keys().cloned().collect()
    }
}

fn next_transport_stream_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    if id == 0 {
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    } else {
        id
    }
}

impl Clone for InMemoryTransport {
    fn clone(&self) -> Self {
        Self {
            registrations: Arc::clone(&self.registrations),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::descriptor::{
        ContractDescriptor, EncodedPayload, Interaction, PayloadDescriptor, Version,
    };
    use crate::contracts::envelope::MessageEnvelope;
    use crate::contracts::metadata::{ContractMetadata, Participants};
    use crate::identity::{CapabilityId, ContractId, CorrelationId, MessageId, OperationId};
    use crate::operation::{Operation, OperationContext};
    use crate::status::Status;

    fn make_request(
        target: &EngineId,
        target_instance: &EngineInstanceId,
        message_id: &str,
    ) -> UniversalRequest {
        let desc = ContractDescriptor::new(
            ContractId::new("lookup.request").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(1, 2, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            desc.clone(),
            Participants::new(EngineId::new("caller").unwrap(), target.clone())
                .with_target_instance(target_instance.clone()),
        );

        let op_ctx = OperationContext::new(Operation::new(
            OperationId::new("op-1").unwrap(),
            CorrelationId::new("corr-1").unwrap(),
        ));

        let envelope = MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            op_ctx,
            metadata,
            EncodedPayload::new(desc.payload, b"opaque payload".to_vec()),
        );

        UniversalRequest::new(envelope)
    }

    fn target(engine: &str, instance: &str) -> (EngineId, EngineInstanceId) {
        (
            EngineId::new(engine).unwrap(),
            EngineInstanceId::new(instance).unwrap(),
        )
    }

    #[test]
    fn in_memory_call_round_trips_handler_response() {
        let transport = InMemoryTransport::new();
        let (engine, instance) = target("echo", "echo-1");

        transport.register(engine.clone(), instance.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        let request = make_request(&engine, &instance, "msg-1");

        let response = futures::executor::block_on(transport.call(&instance, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.event.envelope.message_id.as_str(), "msg-1");
    }

    #[test]
    fn in_memory_call_returns_disconnected_for_unregistered_target() {
        let transport = InMemoryTransport::new();
        let (engine, instance) = target("missing", "missing-1");
        let request = make_request(&engine, &instance, "msg-1");

        let result = futures::executor::block_on(transport.call(&instance, request));

        assert!(matches!(result, Err(TransportError::Disconnected)));
    }

    #[test]
    fn multiple_instances_of_same_engine_are_independently_addressable() {
        let transport = InMemoryTransport::new();
        let engine = EngineId::new("echo").unwrap();
        let instance_1 = EngineInstanceId::new("echo-1").unwrap();
        let instance_2 = EngineInstanceId::new("echo-2").unwrap();

        transport.register(engine.clone(), instance_1.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        transport.register(engine.clone(), instance_2.clone(), |request| {
            UniversalResponse::new(request.event.envelope, Status::Failure)
        });

        let request_1 = make_request(&engine, &instance_1, "msg-1");
        let request_2 = make_request(&engine, &instance_2, "msg-2");

        let response_1 =
            futures::executor::block_on(transport.call(&instance_1, request_1)).unwrap();
        let response_2 =
            futures::executor::block_on(transport.call(&instance_2, request_2)).unwrap();

        assert_eq!(response_1.status, Status::Success);
        assert_eq!(response_2.status, Status::Failure);

        assert_eq!(
            transport.registered_engine(&instance_1),
            Some(engine.clone())
        );
        assert_eq!(transport.registered_engine(&instance_2), Some(engine));
    }

    #[test]
    fn concurrent_calls_to_same_target_keep_request_response_pairs() {
        let transport = InMemoryTransport::new();
        let (engine, instance) = target("echo", "echo-1");

        transport.register(engine.clone(), instance.clone(), |request| {
            let message_id = request.event.envelope.message_id.as_str().to_owned();
            let status = if message_id == "msg-1" || message_id == "msg-2" {
                Status::Success
            } else {
                Status::Failure
            };

            UniversalResponse::new(request.event.envelope, status)
        });

        let request_1 = make_request(&engine, &instance, "msg-1");
        let request_2 = make_request(&engine, &instance, "msg-2");

        let transport_1 = transport.clone();
        let transport_2 = transport.clone();
        let instance_1 = instance.clone();
        let instance_2 = instance.clone();

        let thread_1 = std::thread::spawn(move || {
            futures::executor::block_on(transport_1.call(&instance_1, request_1)).unwrap()
        });

        let thread_2 = std::thread::spawn(move || {
            futures::executor::block_on(transport_2.call(&instance_2, request_2)).unwrap()
        });

        let response_1 = thread_1.join().unwrap();
        let response_2 = thread_2.join().unwrap();

        assert_eq!(response_1.status, Status::Success);
        assert_eq!(response_2.status, Status::Success);

        let ids = [
            response_1.event.envelope.message_id.as_str(),
            response_2.event.envelope.message_id.as_str(),
        ];

        assert!(ids.contains(&"msg-1"));
        assert!(ids.contains(&"msg-2"));
    }

    #[test]
    fn reentrant_handler_can_call_same_target_without_deadlock() {
        use std::future::Future;
        use std::task::{Context, Poll};

        fn poll_ready<F: Future>(future: F) -> F::Output {
            let waker = futures::task::noop_waker();
            let mut context = Context::from_waker(&waker);
            let mut future = Box::pin(future);

            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => output,
                Poll::Pending => panic!("in-memory call unexpectedly yielded"),
            }
        }

        let transport = InMemoryTransport::new();
        let (engine, instance) = target("echo", "echo-1");
        let request = make_request(&engine, &instance, "msg-1");

        let nested_transport = transport.clone();
        let nested_instance = instance.clone();

        transport.register(engine.clone(), instance.clone(), move |request| {
            let message_id = request.event.envelope.message_id.as_str().to_owned();

            // Only the outer request re-enters, preventing infinite recursion.
            if message_id == "msg-1" {
                let nested_request = make_request(&engine, &nested_instance, "msg-2");
                let nested_response =
                    poll_ready(nested_transport.call(&nested_instance, nested_request))
                        .expect("reentrant call should succeed");

                assert_eq!(nested_response.event.envelope.message_id.as_str(), "msg-2");
                assert_eq!(nested_response.status, Status::Success);
            }

            UniversalResponse::new(request.event.envelope, Status::Success)
        });

        let response = futures::executor::block_on(transport.call(&instance, request)).unwrap();
        assert_eq!(response.status, Status::Success);
        assert_eq!(response.event.envelope.message_id.as_str(), "msg-1");
    }
}
