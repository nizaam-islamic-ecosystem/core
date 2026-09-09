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
            let mut framed = Vec::with_capacity(4 + encoded.len());
            framed.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
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
                    let mut ch = channels.lock().unwrap();
                    let (rx, _tx) = ch.get_mut(&target).ok_or(TransportError::Disconnected)?;
                    if rx.len() < 4 {
                        return Err(TransportError::Decode("incomplete request".into()));
                    }
                    let len_bytes: [u8; 4] = rx[..4].try_into().unwrap();
                    let len = u32::from_be_bytes(len_bytes) as usize;
                    rx.drain(..4);
                    if rx.len() < len {
                        return Err(TransportError::Decode("incomplete payload".into()));
                    }
                    let payload: Vec<u8> = rx.drain(..len).collect();
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
