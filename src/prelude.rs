//! Curated engine-facing Core types used across common Nizaam engine code.
//!
//! The prelude is an ergonomic surface, not a mirror of every public Core
//! module. It contains stable identity, contract, capability, execution
//! context, security-context, status, provenance, and core runtime types that
//! engine implementations commonly use together.
//!
//! Subsystem-specific APIs such as Control Plane coordination, transport,
//! retry/idempotency policy, health reporting, streaming, configuration,
//! observability, artifact storage, and event delivery remain available from
//! their owning modules so their boundaries stay explicit.

pub use crate::capability::{
    CapabilityDefinition, CapabilityDefinitionError, CapabilityDispatchResult, CapabilityEntry,
    CapabilityError, CapabilityHandler, CapabilityInvocation, CapabilityOutcome,
    CapabilityRegistry, RegistryError, dispatch,
};

pub use crate::contracts::{
    ContractDescriptor, ContractMetadata, EncodedPayload, EncodingError, ExecutionMetadata,
    Interaction, InvalidDescriptor, MessageEnvelope, Participants, PayloadCodec, PayloadDescriptor,
    RawPayloadCodec, RequirementsMetadata, UniversalEvent, UniversalEventError, UniversalRequest,
    UniversalResponse, Version,
};

pub use crate::error::{ErrorClass, ErrorCode, ErrorDefinition, ErrorOwner, Severity};

pub use crate::identity::{
    ArtifactId, AttemptId, CapabilityId, ContractId, CorrelationId, EngineId, EngineInstanceId,
    EventId, MessageId, NodeId, OperationId, PlanId,
};

pub use crate::logging::{
    DispatchError, DispatchOutcome, InstanceError, LogContext, LogEvent, LogEventType, LogLevel,
    LogMetadata, LogScope, LogSink, LogSource, LogValidationError, LoggingInstance, LoggingSystem,
};

pub use crate::operation::{CancellationToken, Deadline, Operation, OperationContext};

pub use crate::provenance::ProvenanceContext;

pub use crate::runtime::{EngineContext, EngineRuntime, LifecycleState, RequestAdmissionError};

pub use crate::security::{PrincipalId, PrincipalIdentity, PrincipalType, SecurityContext};

pub use crate::status::{ArtifactReference, Compatibility, ErrorReference, Retryability, Status};
