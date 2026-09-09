//! In-memory transport reference implementation for testing and local use.

use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineId;
use crate::status::Status;
use crate::transport::{BoxedFuture, Transport, TransportError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type ChannelBuffer = (Vec<u8>, Vec<u8>);
type SharedChannels = Arc<Mutex<HashMap<EngineId, ChannelBuffer>>>;

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

unsafe impl Send for InMemoryHandler {}
unsafe impl Sync for InMemoryHandler {}

pub struct InMemoryTransport {
    channels: SharedChannels,
    handlers: Arc<Mutex<HashMap<EngineId, InMemoryHandler>>>,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register<F>(&self, target: EngineId, handler: F)
    where
        F: Fn(UniversalRequest) -> UniversalResponse + Send + Sync + 'static,
    {
        self.handlers.lock().unwrap().insert(
            target.clone(),
            InMemoryHandler {
                respond: Arc::new(handler),
            },
        );
        self.channels
            .lock()
            .unwrap()
            .insert(target, (Vec::new(), Vec::new()));
    }

    pub fn registered_targets(&self) -> Vec<EngineId> {
        self.handlers.lock().unwrap().keys().cloned().collect()
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
        let handlers = Arc::clone(&self.handlers);
        let channels = Arc::clone(&self.channels);
        let target = target.clone();

        Box::pin(async move {
            let encoded =
                serde_json::to_vec(&request).map_err(|e| TransportError::Encode(e.to_string()))?;
            let len = u32::try_from(encoded.len())
                .map_err(|_| TransportError::Encode("message is too large to frame".into()))?;
            let mut framed = Vec::with_capacity(4 + encoded.len());
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&encoded);

            {
                let mut ch = channels.lock().unwrap();
                if let Some((tx, _rx)) = ch.get_mut(&target) {
                    tx.extend_from_slice(&framed);
                }
            }

            let handler = {
                let h = handlers.lock().unwrap();
                h.get(&target).cloned()
            };

            match handler {
                Some(h) => {
                    let payload = {
                        let mut ch = channels.lock().unwrap();
                        let (tx, _rx) = ch.get_mut(&target).ok_or(TransportError::Disconnected)?;
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
                    let respond = Arc::clone(&h.respond);
                    let response = respond(decoded);
                    Ok(response)
                }
                None => {
                    let envelope = request.envelope.clone();
                    Ok(UniversalResponse::new(envelope, Status::Failure))
                }
            }
        })
    }

    fn is_connected(&self, target: &EngineId) -> bool {
        self.handlers.lock().unwrap().contains_key(target)
    }

    fn connected_targets(&self) -> Vec<EngineId> {
        self.handlers.lock().unwrap().keys().cloned().collect()
    }
}

impl Clone for InMemoryTransport {
    fn clone(&self) -> Self {
        Self {
            channels: Arc::clone(&self.channels),
            handlers: Arc::clone(&self.handlers),
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

    fn make_request(target: &EngineId) -> UniversalRequest {
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
            MessageId::new("msg-1").unwrap(),
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

        let request = make_request(&target);
        let response = futures::executor::block_on(transport.call(&target, request)).unwrap();
        assert_eq!(response.status, Status::Success);
        assert_eq!(response.envelope.message_id.as_str(), "msg-1");
    }

    #[test]
    fn in_memory_call_returns_failure_for_unregistered_target() {
        let transport = InMemoryTransport::new();
        let target = EngineId::new("missing").unwrap();
        let request = make_request(&target);
        let response = futures::executor::block_on(transport.call(&target, request)).unwrap();
        assert_eq!(response.status, Status::Failure);
    }
}
