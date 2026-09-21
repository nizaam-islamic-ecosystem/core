//! Control Plane communication boundary.
//!
//! This module provides the Control Plane-facing orchestration boundary for
//! communicating with concrete engine instances.
//!
//! The communication layer deliberately reuses the existing client and
//! transport abstractions. It does not implement another transport stack,
//! perform routing, own membership, execute capabilities, manage health,
//! implement authorization, or perform retry orchestration.
//!
//! The current implementation provides the concrete outbound path:
//!
//! ```text
//! Control Plane
//!       │
//!       ▼
//! ControlPlaneCommunication
//!       │
//!       ▼
//! UniversalClient
//!       │
//!       ▼
//! Transport
//!       │
//!       ▼
//! EngineInstanceId
//!       │
//!       ▼
//! Engine
//! ```
//!
//! The inbound Engine → Control Plane protocol will be integrated as the
//! request/response and Control Plane endpoint contracts are expanded. This
//! module therefore does not invent a second inbound transport mechanism.
//!
//! ## Identity invariant
//!
//! Communication is addressed to an `EngineInstanceId`, not merely an
//! `EngineId`. A logical engine may have multiple concrete runtime instances,
//! and the communication boundary must preserve the concrete instance selected
//! by the Control Plane routing layer.
//!
//! ```text
//! EngineId
//!     │
//!     └── logical engine identity
//!
//! EngineInstanceId
//!     │
//!     └── concrete runtime communication target
//! ```
//!
//! Routing is responsible for selecting the concrete instance. This module is
//! responsible for carrying communication to that already-selected target.

use crate::client::UniversalClient;
use crate::contracts::{UniversalRequest, UniversalResponse};
use crate::identity::EngineInstanceId;
use crate::transport::{BoxedFuture, Transport, TransportError};

/// Control Plane-facing communication boundary.
///
/// `ControlPlaneCommunication` provides the Control Plane with a semantic
/// communication surface over the existing universal client and transport
/// abstractions.
///
/// The generic transport remains responsible for the actual communication
/// mechanics. This type does not create or manage a second transport stack.
pub struct ControlPlaneCommunication<T>
where
    T: Transport,
{
    client: UniversalClient<T>,
}

impl<T> ControlPlaneCommunication<T>
where
    T: Transport,
{
    /// Creates a new Control Plane communication boundary over the given
    /// transport.
    pub fn new(transport: T) -> Self {
        Self {
            client: UniversalClient::new(transport),
        }
    }

    /// Sends a universal request to a concrete engine instance.
    ///
    /// The target is an `EngineInstanceId` because the Control Plane must
    /// communicate with the concrete runtime instance selected by resolution
    /// and routing.
    ///
    /// `UniversalClient` performs the canonical request-target validation,
    /// including ensuring that the request metadata identifies the same
    /// concrete target instance.
    ///
    /// This method does not perform routing, retry, capability execution,
    /// membership lookup, health evaluation, or authorization.
    pub fn send_to_engine(
        &self,
        target: &EngineInstanceId,
        request: UniversalRequest,
    ) -> BoxedFuture<UniversalResponse, TransportError> {
        self.client.send(target, request)
    }

    /// Returns whether the transport currently considers the concrete engine
    /// instance connected.
    ///
    /// This is a transport connectivity observation. It does not imply that
    /// the engine is healthy, ready, authorized, or routable.
    pub fn is_connected(&self, target: &EngineInstanceId) -> bool {
        self.client.is_connected(target)
    }

    /// Returns the concrete engine instances currently known as connected by
    /// the underlying transport.
    ///
    /// The returned list is a transport-level observation and must not be
    /// interpreted as the Control Plane membership or routing view.
    pub fn connected_targets(&self) -> Vec<EngineInstanceId> {
        self.client.connected_targets()
    }

    /// Returns the underlying universal client.
    ///
    /// This accessor allows later Control Plane composition to reuse the
    /// existing client abstraction without exposing transport implementation
    /// details through this module.
    pub fn client(&self) -> &UniversalClient<T> {
        &self.client
    }

    /// Returns the underlying transport.
    ///
    /// This is intentionally exposed only as a reference. The communication
    /// boundary does not take ownership of transport operations beyond the
    /// universal client it owns.
    pub fn transport(&self) -> &T {
        self.client.transport()
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
    use crate::transport::InMemoryTransport;

    fn request_for(
        sender: &EngineId,
        target: &EngineId,
        target_instance: Option<EngineInstanceId>,
        message_id: &str,
    ) -> UniversalRequest {
        let descriptor = ContractDescriptor::new(
            ContractId::new("communication.test").unwrap(),
            CapabilityId::new("communication.test-capability").unwrap(),
            Version::new(1, 0, 0),
            Interaction::Request,
            PayloadDescriptor::new("application/octet-stream", Version::new(1, 0, 0)).unwrap(),
        );

        let mut participants = Participants::new(sender.clone(), target.clone());

        if let Some(instance) = target_instance {
            participants = participants.with_target_instance(instance);
        }

        let metadata = ContractMetadata::new(descriptor.clone(), participants);

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("communication-operation").unwrap(),
            CorrelationId::new("communication-correlation").unwrap(),
        ));

        let envelope = MessageEnvelope::new(
            MessageId::new(message_id).unwrap(),
            operation_context,
            metadata,
            EncodedPayload::new(descriptor.payload, b"communication payload"),
        );

        UniversalRequest::new(envelope)
    }

    fn response_for(request: UniversalRequest, status: Status) -> UniversalResponse {
        UniversalResponse::new(request.event.envelope, status)
    }

    #[test]
    fn communication_can_send_to_a_concrete_engine_instance() {
        let transport = InMemoryTransport::new();

        let sender = EngineId::new("control-plane-test").unwrap();
        let target_engine = EngineId::new("target-engine").unwrap();
        let target_instance = EngineInstanceId::new("target-engine-01").unwrap();

        transport.register(target_engine.clone(), target_instance.clone(), |request| {
            response_for(request, Status::Success)
        });

        let communication = ControlPlaneCommunication::new(transport);

        let request = request_for(
            &sender,
            &target_engine,
            Some(target_instance.clone()),
            "communication-message-01",
        );

        let response =
            futures::executor::block_on(communication.send_to_engine(&target_instance, request))
                .unwrap();

        assert_eq!(response.status, Status::Success);
        assert_eq!(
            response.event.envelope.message_id.as_str(),
            "communication-message-01"
        );
    }

    #[test]
    fn communication_uses_the_concrete_instance_as_the_target() {
        let transport = InMemoryTransport::new();

        let sender = EngineId::new("control-plane-test").unwrap();
        let target_engine = EngineId::new("target-engine").unwrap();

        let instance_one = EngineInstanceId::new("target-engine-01").unwrap();
        let instance_two = EngineInstanceId::new("target-engine-02").unwrap();

        transport.register(target_engine.clone(), instance_one.clone(), |request| {
            response_for(request, Status::Success)
        });

        transport.register(target_engine.clone(), instance_two.clone(), |request| {
            response_for(request, Status::TimedOut)
        });

        let communication = ControlPlaneCommunication::new(transport);

        let request = request_for(
            &sender,
            &target_engine,
            Some(instance_two.clone()),
            "instance-specific-message",
        );

        let response =
            futures::executor::block_on(communication.send_to_engine(&instance_two, request))
                .unwrap();

        assert_eq!(response.status, Status::TimedOut);
    }

    #[test]
    fn communication_rejects_a_mismatched_request_target() {
        let transport = InMemoryTransport::new();

        let sender = EngineId::new("control-plane-test").unwrap();
        let target_engine = EngineId::new("target-engine").unwrap();

        let transport_instance = EngineInstanceId::new("target-engine-01").unwrap();
        let request_instance = EngineInstanceId::new("target-engine-02").unwrap();

        transport.register(
            target_engine.clone(),
            transport_instance.clone(),
            |_request| {
                panic!("mismatched target must not reach the transport handler");
            },
        );

        let communication = ControlPlaneCommunication::new(transport);

        let request = request_for(
            &sender,
            &target_engine,
            Some(request_instance),
            "mismatched-target",
        );

        let result =
            futures::executor::block_on(communication.send_to_engine(&transport_instance, request));

        assert_eq!(
            result,
            Err(TransportError::Peer(
                "request target instance does not match the client target".into()
            ))
        );
    }

    #[test]
    fn communication_rejects_a_request_without_a_concrete_target() {
        let transport = InMemoryTransport::new();

        let sender = EngineId::new("control-plane-test").unwrap();
        let target_engine = EngineId::new("target-engine").unwrap();
        let target_instance = EngineInstanceId::new("target-engine-01").unwrap();

        transport.register(target_engine.clone(), target_instance.clone(), |_request| {
            panic!("request without a concrete target must not reach the transport handler");
        });

        let communication = ControlPlaneCommunication::new(transport);

        let request = request_for(&sender, &target_engine, None, "missing-target-instance");

        let result =
            futures::executor::block_on(communication.send_to_engine(&target_instance, request));

        assert_eq!(
            result,
            Err(TransportError::Peer(
                "request does not specify a concrete target instance".into()
            ))
        );
    }

    #[test]
    fn communication_reports_transport_connectivity() {
        let transport = InMemoryTransport::new();

        let engine = EngineId::new("target-engine").unwrap();
        let instance = EngineInstanceId::new("target-engine-01").unwrap();

        transport.register(engine, instance.clone(), |request| {
            response_for(request, Status::Success)
        });

        let communication = ControlPlaneCommunication::new(transport);

        assert!(communication.is_connected(&instance));
        assert_eq!(communication.connected_targets(), vec![instance],);
    }

    #[test]
    fn communication_exposes_the_underlying_client_without_replacing_it() {
        let transport = InMemoryTransport::new();
        let communication = ControlPlaneCommunication::new(transport);

        let _: &UniversalClient<InMemoryTransport> = communication.client();
    }
}
