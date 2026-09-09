//! In-memory transport reference implementation for testing and local use.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineId;
use crate::transport::{BoxedFuture, Transport, TransportError};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type ChannelBuffer = (Vec<u8>, Vec<u8>);
type RegistrationMap = Arc<Mutex<HashMap<EngineId, InMemoryRegistration>>>;

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
    handler: InMemoryHandler,
    channel: Arc<Mutex<ChannelBuffer>>,
    call_lock: Arc<Mutex<()>>,
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

    pub fn register<F>(&self, target: EngineId, handler: F)
    where
        F: Fn(UniversalRequest) -> UniversalResponse + Send + Sync + 'static,
    {
        let registration = InMemoryRegistration {
            handler: InMemoryHandler {
                respond: Arc::new(handler),
            },
            channel: Arc::new(Mutex::new((Vec::new(), Vec::new()))),
            call_lock: Arc::new(Mutex::new(())),
        };

        // Publish the handler, channel, and call lock together under one
        // registry lock so calls cannot observe a partially registered target.
        self.registrations
            .lock()
            .unwrap()
            .insert(target, registration);
    }

    pub fn registered_targets(&self) -> Vec<EngineId> {
        self.registrations.lock().unwrap().keys().cloned().collect()
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
        target: &EngineId,
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

            let len = u32::try_from(encoded.len())
                .map_err(|_| TransportError::Encode("message is too large to frame".into()))?;

            let mut framed = Vec::with_capacity(4 + encoded.len());
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&encoded);

            // A target has a shared in-memory request buffer. The entire
            // request exchange must therefore be serialized per target so
            // that concurrent calls cannot consume each other's frames.
            let _call_guard = registration
                .call_lock
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            {
                let mut channel = registration
                    .channel
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());

                let (tx, _rx) = &mut *channel;
                tx.extend_from_slice(&framed);
            }

            let payload = {
                let mut channel = registration
                    .channel
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());

                let (tx, _rx) = &mut *channel;

                if tx.len() < 4 {
                    return Err(TransportError::Decode("incomplete request".into()));
                }

                let len_bytes: [u8; 4] = tx[..4].try_into().unwrap();
                let len = u32::from_be_bytes(len_bytes) as usize;

                if tx.len() < 4 + len {
                    return Err(TransportError::Decode("incomplete payload".into()));
                }

                tx.drain(..4);
                tx.drain(..len).collect::<Vec<u8>>()
            };

            let decoded: UniversalRequest = serde_json::from_slice(&payload)
                .map_err(|e| TransportError::Decode(e.to_string()))?;

            let response = (registration.handler.respond)(decoded);

            Ok(response)
        })
    }

    fn is_connected(&self, target: &EngineId) -> bool {
        self.registrations.lock().unwrap().contains_key(target)
    }

    fn connected_targets(&self) -> Vec<EngineId> {
        self.registrations.lock().unwrap().keys().cloned().collect()
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
    use crate::identity::{
        CapabilityId, ContractId, CorrelationId, EngineId, MessageId, OperationId,
    };
    use crate::operation::{Operation, OperationContext};
    use crate::status::Status;

    fn make_request(target: &EngineId, message_id: &str) -> UniversalRequest {
        let desc = ContractDescriptor::new(
            ContractId::new("lookup.request").unwrap(),
            CapabilityId::new("lookup").unwrap(),
            Version::new(1, 2, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let metadata = ContractMetadata::new(
            desc.clone(),
            Participants::new(EngineId::new("caller").unwrap(), target.clone()),
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

    #[test]
    fn in_memory_call_round_trips_handler_response() {
        let transport = InMemoryTransport::new();
        let target = EngineId::new("echo").unwrap();

        transport.register(target.clone(), |request| {
            UniversalResponse::new(request.envelope, Status::Success)
        });

        let request = make_request(&target, "msg-1");

        let response = futures::executor::block_on(transport.call(&target, request)).unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(response.envelope.message_id.as_str(), "msg-1");
    }

    #[test]
    fn in_memory_call_returns_disconnected_for_unregistered_target() {
        let transport = InMemoryTransport::new();
        let target = EngineId::new("missing").unwrap();
        let request = make_request(&target, "msg-1");

        let result = futures::executor::block_on(transport.call(&target, request));

        assert!(matches!(result, Err(TransportError::Disconnected)));
    }

    #[test]
    fn concurrent_calls_to_same_target_keep_request_response_pairs() {
        let transport = InMemoryTransport::new();
        let target = EngineId::new("echo").unwrap();

        transport.register(target.clone(), |request| {
            let message_id = request.envelope.message_id.as_str().to_owned();
            let status = if message_id == "msg-1" || message_id == "msg-2" {
                Status::Success
            } else {
                Status::Failure
            };

            UniversalResponse::new(request.envelope, status)
        });

        let request_1 = make_request(&target, "msg-1");
        let request_2 = make_request(&target, "msg-2");

        let transport_1 = transport.clone();
        let transport_2 = transport.clone();
        let target_1 = target.clone();
        let target_2 = target.clone();

        let thread_1 = std::thread::spawn(move || {
            futures::executor::block_on(transport_1.call(&target_1, request_1)).unwrap()
        });

        let thread_2 = std::thread::spawn(move || {
            futures::executor::block_on(transport_2.call(&target_2, request_2)).unwrap()
        });

        let response_1 = thread_1.join().unwrap();
        let response_2 = thread_2.join().unwrap();

        assert_eq!(response_1.status, Status::Success);
        assert_eq!(response_2.status, Status::Success);

        let ids = [
            response_1.envelope.message_id.as_str(),
            response_2.envelope.message_id.as_str(),
        ];

        assert!(ids.contains(&"msg-1"));
        assert!(ids.contains(&"msg-2"));
    }
}
