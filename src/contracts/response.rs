use crate::contracts::descriptor::Interaction;
use crate::contracts::envelope::MessageEnvelope;
use crate::status::Status;

/// A universal response envelope carrying an opaque capability payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UniversalResponse {
    pub envelope: MessageEnvelope,
    pub status: Status,
}

impl UniversalResponse {
    pub fn new(envelope: MessageEnvelope, status: Status) -> Self {
        Self { envelope, status }
    }

    pub fn has_response_interaction(&self) -> bool {
        self.envelope.metadata.descriptor.interaction == Interaction::Response
    }
}
