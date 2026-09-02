use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;

/// A universal request envelope carrying an opaque capability payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniversalRequest {
    pub envelope: MessageEnvelope,
}

impl UniversalRequest {
    pub fn new(envelope: MessageEnvelope) -> Self {
        Self { envelope }
    }

    pub fn has_request_interaction(&self) -> bool {
        self.envelope.metadata.descriptor.interaction == Interaction::Request
    }
}
