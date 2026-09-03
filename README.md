# Nizaam Core

> Shared Rust foundation of the Nizaam Islamic Ecosystem.

Nizaam Core is the shared Rust foundation of the Nizaam Islamic Ecosystem.

It provides the common mechanisms required by Domain Engines and Infrastructure Engines to communicate, execute safely, share runtime context, handle errors, observe operations, and follow consistent platform rules.

The Core provides **mechanics, contracts, and platform infrastructure**.

It does **not** provide domain meaning.

---

## Table of Contents

* [Purpose](#purpose)
* [Architecture](#architecture)
* [Core Responsibilities](#core-responsibilities)
* [Control Plane](#control-plane)
* [Error System](#error-system)
* [Logging System](#logging-system)
* [Artifacts and Provenance](#artifacts-and-provenance)
* [Observability](#observability)
* [Runtime](#runtime)
* [Security](#security)
* [Testing and Conformance](#testing-and-conformance)
* [Development Roadmap](#development-roadmap)
* [Design Principles](#design-principles)
* [Project Status](#project-status)
* [Source of Truth](#source-of-truth)

---

## Document Status

This README describes the finalized architectural direction of Nizaam Core.

The detailed implementation plan and development decisions are maintained separately in the Core architecture and scope documentation.

Changes to Core architecture should be made deliberately because Core is the shared foundation used by the Nizaam ecosystem.

---

## Versioning

**Current Architecture Version:** v1.0

The architecture version refers to the documented architectural baseline and does not necessarily represent a released software package version.

Future breaking architectural changes should be explicitly documented rather than silently introduced.

---

## Purpose

Nizaam contains many different engines, such as:

* Quran Engine
* Hadith Engine
* Arabic Engine
* Tafsir Engine
* Fiqh Engine
* Aqeedah Engine
* Knowledge Graph Engine
* Indexing Engine
* Storage and other Infrastructure Engines

These engines have different responsibilities and domain semantics, but they still need many of the same technical mechanisms.

Nizaam Core exists so those mechanisms are implemented once and consumed consistently across the ecosystem.

The fundamental dependency direction is:

```text
Domain Engine
      │
      ▼
 Nizaam Core
      ▲
      │
Infrastructure Engine
```

Core must remain independent of engine-specific semantics.

---

## Architecture

Nizaam Core has two major architectural areas:

```text
                         NIZAAM CORE
                              │
             ┌────────────────┴────────────────┐
             │                                 │
             ▼                                 ▼
      ENGINE COMMON CORE                GLOBAL PLATFORM CORE
             │                                 │
             │                                 └── Control Plane
             │
             ├── Identity
             ├── Contracts
             ├── Request / Response
             ├── Context
             ├── Transport
             ├── Client / Server
             ├── Capability Infrastructure
             ├── Lifecycle / Runtime
             ├── Security
             ├── Errors
             ├── Artifacts
             ├── Provenance
             ├── Observability
             ├── Health
             ├── Configuration
             ├── Middleware
             ├── Streaming
             ├── Concurrency
             ├── Cancellation
             ├── Deadlines
             ├── Retry
             ├── Idempotency
             ├── Events
             └── Testing / Conformance
```

### Engine Common Core

The Engine Common Core contains mechanisms shared by both Domain and Infrastructure Engines.

It defines the common language and runtime behavior that engines use to participate in the Nizaam ecosystem.

### Global Platform Core

The Global Platform Core contains Nizaam-wide platform functionality.

The primary component currently defined here is the **Control Plane**.

The Control Plane is not something every engine implements. It is a Nizaam-wide platform mechanism.

---

## Core Responsibilities

Nizaam Core provides the following major capabilities.

### Identity

Core defines strongly typed ecosystem identities including:

```text
MessageId
OperationId
CorrelationId
EngineId
EngineInstanceId
CapabilityId
ContractId
PlanId
NodeId
AttemptId
ArtifactId
```

These identities are intentionally distinct.

For example:

```text
OperationId
≠ MessageId
≠ NodeId
≠ AttemptId
≠ ArtifactId
```

---

### Universal Contracts

All engines communicate using a common contract model.

Core provides:

```text
Universal Request
Universal Response
Message Envelope
Contract Metadata
Schema / Version Metadata
Interaction Metadata
Requirements Metadata
Execution Metadata
Payload Descriptor
```

Core understands the structure and metadata of a message.

The actual domain payload remains engine-specific.

For example:

```text
QuranRequest
HadithRequest
ArabicNlpRequest
KGQueryRequest
```

do not become generic Core domain types.

---

### Operation and Context

Core provides a standardized operation context so engines do not need to reconstruct execution information from raw transport metadata.

An operation can carry:

```text
OperationId
CorrelationId
NodeId
AttemptId
Parent Operation
Deadline
Cancellation
Security Context
Provenance Context
```

The Engine Context can additionally provide access to:

```text
Universal Client
Artifact Access
Observability
Configuration
Capability Registry
```

---

### Capability System

Capabilities are the primary mechanism through which engines expose functionality.

Core provides the infrastructure for:

```text
Capability
Capability Definition
Capability Registration
Capability Registry
Capability Handler
Capability Dispatch
```

Conceptually:

```text
Capability Registry
        ↓
Capability Definition
        ↓
Capability Handler
        ↓
Engine-local execution
```

The capability itself belongs to the engine.

Core provides the mechanism, not the domain meaning.

---

## Runtime

Every engine uses the shared Core runtime instead of implementing its own independent runtime architecture.

The standard lifecycle is:

```text
START
  ↓
CONFIGURATION
  ↓
DEPENDENCIES
  ↓
CAPABILITIES
  ↓
REGISTRATION
  ↓
READY
  ↓
SERVING
  ↓
DRAINING
  ↓
STOPPED
```

The runtime provides common infrastructure for:

* Startup and shutdown
* Lifecycle management
* Request processing
* Capability dispatch
* Context propagation
* Cancellation
* Deadlines
* Transport integration
* Health
* Bounded concurrency
* Background tasks
* Retry and idempotency mechanisms

The engine provides the actual capability behavior.

---

## Universal Communication

Inter-engine communication follows a common architecture:

```text
Typed Client
     ↓
Universal Client
     ↓
Universal Request
     ↓
Transport
     ↓
Engine Runtime
     ↓
Capability
     ↓
Engine-specific Result
     ↓
Universal Response
```

The common communication layer carries things such as:

```text
Identity
Contract / Version
Interaction Metadata
Context
Requirements
Security
Provenance
Execution Metadata
Artifact References
```

The domain payload remains engine-specific.

---

## Transport

Core provides common transport infrastructure for inter-engine communication.

This includes:

```text
Transport
Connection
Request transmission
Response transmission
Stream handling
Transport errors
Transport lifecycle
```

The architecture supports RPC-style communication such as gRPC without making domain engines responsible for building their own transport architecture.

---

## Security

Security is a Core infrastructure concern.

Core provides mechanisms for:

```text
Service Identity
Security Context
Authentication Integration
Authorization Integration
Security Middleware
Trust / Integrity Information
Security Context Propagation
Mandatory Security Checks
```

The distinction is:

```text
Core Security
    → security mechanisms

Engine
    → capability/domain authorization rules
```

Core establishes the security boundary.

It does not define domain authority.

---

## Control Plane

The Control Plane is part of the **Global Platform Core**.

Its final responsibility is:

> The Control Plane provides the shared Nizaam-wide path through which engines communicate securely and consistently.

It handles infrastructure-level communication concerns such as:

```text
Request Admission
Contract Validation
Capability Resolution
Destination Resolution
Request Routing
Response Routing
Security Context Propagation
Context Propagation
Transport Lifecycle
Timeout / Deadline / Cancellation Propagation
Correlation / Tracing
Communication-level Retry
Streaming Communication
Artifact-reference Communication
Communication Errors
Communication Observability
```

### What the Control Plane does NOT do

The Control Plane must not become:

```text
❌ Domain Workflow Engine
❌ Domain Executor
❌ Semantic Orchestrator
❌ Domain Reasoning Engine
❌ Domain Dependency Manager
❌ Domain Governance Authority
❌ Domain Result Validator
❌ Model Inference Engine
❌ Engine-specific Execution Layer
```

The key boundary is:

```text
CONTROL PLANE
→ How do engines communicate safely and consistently?

ENGINE
→ What work should I perform and how?

GOVERNING ENGINE
→ Is this domain result acceptable according to my authority?
```

Each engine owns its own workflow and execution.

There is **no separate universal Execution Engine for normal cross-engine/domain execution**.

---

## Control Path vs Work Path

The Control Plane is primarily a control mechanism, not a proxy for every message.

Conceptually:

```text
CONTROL PATH

Application
    ↓
Control Plane
    ↓
Communication / capability resolution
```

while actual engine work happens through the participating engines:

```text
WORK / EXECUTION PATH

Engine
   ↔
Engine
   ↔
Engine
```

The Control Plane should not sit in the middle of every request and response merely to forward business data.

It facilitates and governs the communication infrastructure while keeping engines responsible for their own work.

---

## Engine Ownership

Each engine remains authoritative over its own:

```text
Capabilities
Workflows
Domain Logic
Infrastructure Logic
Domain Dependencies
Domain Decisions
Domain Validation
Domain Results
Domain Provenance
Domain Retry / Fallback Decisions
Domain Governance
Engine-specific State
Engine-specific Providers
Engine-specific Artifacts
```

For example:

```text
Hadith Engine
    → Hadith workflow

Quran Engine
    → Quran workflow

Arabic Engine
    → Arabic processing

Aqeedah Engine
    → Aqeedah domain governance
```

Core provides the mechanisms through which these engines communicate.

---

## Error System

The Error System is a first-class Core system.

It is independent from the Logging System.

```text
Nizaam Core
├── Logging System
├── Error System
└── Other Core Systems
```

The Error System provides:

```text
ErrorDefinition
ErrorEvent
GlobalError
ErrorReference
ErrorClass
Severity
Retryability
ErrorCode
ErrorContext
Error Catalog
Error Registration
Error Validation
```

An important distinction is:

```text
Error Definition
→ static description of an error

Error Event
→ runtime occurrence of an error
```

Engine-specific error definitions can exist, but they must conform to the global error contract.

The Error System owns error definitions and error events.

Future error-related subsystems may include:

```text
Error Catalog
Error Reporting
Diagnostics
Error Debugger
Auto-Fixer
```

The Debugger and Auto-Fixer are future extensions and are not required to be implemented as part of the initial foundation.

---

## Logging System

The Logging System is also a first-class Core system.

It is not owned by the Error System, and the Error System is not a Logging subsystem.

```text
Logging System
      ↕
Error System
```

Both systems can cooperate through stable contracts and references.

The Logging System provides standardized structured logging.

A common `LogEvent` concept contains information such as:

```text
Event Id
Timestamp
Level
Source
Scope
Component
Operation Context
Message
Event Type
Status
Error Reference
Metadata
Payload / Reference
```

Logging supports two scopes:

```text
GLOBAL
LOCAL
```

These are scopes/instances of the same Logging System, not separate logging implementations.

#### Global Logging

Global Logging is focused on Control Plane and global-operation activity.

Examples:

```text
request accepted
capability resolved
destination resolved
route established
request dispatched
response received
communication failed
global timeout
global cancellation
```

#### Local Logging

Local Logging belongs to individual engines and records their internal processing.

Examples:

```text
capability started
repository work
domain processing
model invocation
internal workflow steps
capability completed
```

Both scopes use the same standardized logging structure.

The Logging System is designed to support:

* Asynchronous logging
* Multiple consumers
* Fan-out
* Real-time delivery
* Persistent sinks
* Structured events
* Operation correlation
* Visibility/scoping
* Hierarchical context

Nizaam-visible operational events must use the standardized Core Logging interface.

---

## Artifacts and Provenance

Core provides common artifact infrastructure without defining what the artifact means to a particular engine.

Common concepts include:

```text
ArtifactId
ArtifactVersion
ArtifactReference
ArtifactAccess
Integrity Metadata
Provenance
```

Core can provide mechanisms for:

```text
Publish
Retrieve
Validate
Resolve
Version
Integrity
Lifecycle
```

The actual artifact content and semantic meaning remain engine-specific.

Core also provides common execution provenance such as:

```text
Operation
Attempt
Engine
Capability
Message
Timestamp
Source / Version
Execution Metadata
```

Domain-specific provenance remains owned by the engine.

---

## Observability

Core provides common operational observability mechanisms:

```text
Structured Logging
Metrics
Distributed Tracing
Correlation
Runtime Telemetry
Diagnostics Hooks
```

Common identifiers such as:

```text
OperationId
MessageId
CorrelationId
EngineId
CapabilityId
NodeId
AttemptId
```

can be propagated across:

```text
Logs
Traces
Metrics
Diagnostics
```

Engine developers should not have to implement distributed context propagation independently for every engine.

---

## Health and Readiness

Core provides a standardized health model including:

```text
Liveness
Readiness
Capability Readiness
Dependency Health
Startup Readiness
Draining State
```

The mechanism is common.

The actual health checks are engine-specific.

---

## Configuration

Core provides configuration plumbing for:

```text
Platform Configuration
Engine Configuration
Capability Configuration
Environment / Runtime Configuration
```

Common mechanisms include:

```text
Configuration Loading
Validation
Typed Access
Environment Integration
Runtime Propagation
```

Configuration values and engine-specific configuration schemas remain outside generic Core semantics.

---

## Middleware

Core provides a standardized middleware framework.

A typical processing pipeline is:

```text
Receive
  ↓
Security
  ↓
Tracing
  ↓
Metrics
  ↓
Validation
  ↓
Contract Resolution
  ↓
Capability Dispatch
  ↓
Handler
  ↓
Response Processing
```

Mandatory platform middleware cannot be bypassed by individual engines.

Engines may add domain-specific middleware where appropriate.

---

## Streaming

Streaming uses the same universal communication architecture.

Core provides:

```text
Stream Lifecycle
Stream Cancellation
Context Propagation
Correlation
Partial Results
Final Results
Artifact References
```

Nizaam does not create a completely separate communication protocol just for streaming.

---

## Concurrency and Background Tasks

Core provides reusable mechanisms for asynchronous execution:

```text
Task Spawning
Bounded Concurrency
Resource-aware Tasks
Cancellation-aware Tasks
Deadline-aware Tasks
Task Lifecycle
```

For background work, Core provides:

```text
Registration
Startup
Cancellation
Shutdown
Lifecycle Integration
Resource Accounting
Health Integration
```

The actual workloads and background jobs remain engine-specific.

---

## Retry and Idempotency

Core provides common retry and recovery mechanisms:

```text
Attempt Tracking
Retry Execution
Backoff
Retryability
Deadline Awareness
Cancellation Awareness
```

It also provides idempotency mechanisms such as:

```text
Idempotency Key
Duplicate Detection
Operation Identity
Attempt Identity
Safe Retry Support
```

Core provides the mechanisms.

The actual domain retry/fallback policy remains engine-owned.

---

## Internal Events

Core provides an optional reusable event mechanism:

```text
Event
EventId
Publisher
Subscriber
Scope
Delivery
Cancellation
Lifecycle
```

This is intentionally a mechanism, not a requirement to build a giant event-driven architecture.

Event definitions and meanings remain engine-specific.

---

## Extension Model

Nizaam Core should be reusable without becoming rigid.

Engines extend Core through explicit interfaces.

Conceptually:

```text
Engine
   ↓
Core Interface
   ├── Request
   ├── Response
   ├── Context
   └── Execution Boundary
```

An engine can provide capabilities such as:

```text
Arabic
Quran
Hadith
KG
Indexing
Storage
```

without modifying Core's internal implementation.

Core defines the **extension boundary**, not every provider or capability.

---

## Public vs Internal API

Not everything inside Core should become public.

### Public Core Surface

The intended engine-facing surface includes:

```text
Identity Types
Contract Types
Request / Response
EngineContext
OperationContext
Capability Interfaces
Capability Registration
Universal Client
Artifact References
Security Context
Error / Result Types
Lifecycle Interfaces
Observability Interfaces
```

### Internal Core Surface

Implementation details should normally remain private, including:

```text
Transport Internals
Connection Pools
Task Scheduling Internals
Middleware Implementation
Retry Internals
Serialization Internals
Registry Caching
Control Plane Internal Algorithms
Resource-management Internals
```

This keeps engine implementations decoupled from Core internals and allows the Core implementation to evolve without unnecessarily breaking engines.

---

## Dependency Rules

The most important architectural rule is:

```text
Domain Engine
      ↓
Nizaam Core
      ↑
Infrastructure Engine
```

Core must never depend on engine-specific semantics.

Core must not depend on:

```text
Arabic Semantics
Quran Semantics
Hadith Semantics
Tafsir Semantics
Fiqh Semantics
Aqeedah Methodology
Knowledge Graph Semantics
Indexing Semantics
Domain Models
Domain Algorithms
Engine-specific Workflows
Engine-specific Providers
Engine-specific Storage Schemas
```

The lower-level Core modules must not depend on higher-level engine semantics.

Conceptually:

```text
Types
  ↓
Contracts
  ↓
Context
  ↓
Runtime
  ↓
Common Services
  ↓
Control Plane
```

The Control Plane may consume lower-level Core facilities, but lower-level Core facilities must not depend on Control Plane planning logic.

---

## What Does NOT Belong in Core

Core must remain domain-agnostic.

The following remain engine-specific:

```text
Domain Entities
Domain Models
Domain Rules
Domain Invariants
Domain Services
Domain Algorithms
Domain Methodologies
Domain Policies

Arabic NLP
Tokenization
Sarf
Nahw
Arabic Semantics
Arabic Models

Knowledge Graph Entities
Claims
Relationships
Ontology
Traversal
Inference
Graph Reasoning

Index Definitions
Similarity Algorithms
Ranking Algorithms
Index Families
Physical Index Strategies

Engine-specific Repositories
Engine-specific Gateways
Engine-specific Providers
Provider Implementations
Storage Schemas
Domain Artifacts
Engine-specific Planners
Engine-specific Workflows
Engine-specific Execution Algorithms
```

The rule is simple:

> If something depends on domain meaning, it belongs to the engine rather than Core.

---

## Mechanism vs Semantics

Some concepts exist at both levels, but their responsibilities are different.

```text
Capability Validation
    Core → common validation mechanism
    Engine → domain validation rules

Result Construction
    Core → common result envelope
    Engine → result meaning

Provenance
    Core → execution/protocol provenance
    Engine → domain provenance

Configuration
    Core → configuration plumbing
    Engine → configuration schema/meaning

Caching
    Core → optional cache mechanism
    Engine → cached data and policy

Concurrency
    Core → task/concurrency primitives
    Engine → workload strategy

Background Work
    Core → lifecycle/resource mechanism
    Engine → actual jobs

Events
    Core → event mechanism
    Engine → event definitions

Health
    Core → health protocol
    Engine → health checks

Artifacts
    Core → artifact infrastructure
    Engine → artifact types and meaning
```

This distinction prevents Core from becoming a collection of domain abstractions simply because multiple engines have similar-looking code.

---

## Rust Architecture

The initial implementation should favor a strong internal module structure rather than prematurely splitting every logical component into its own Cargo crate.

A logical module does not automatically need to become a separate crate.

Crate boundaries should be decided based on:

```text
Dependency Direction
Public API Stability
Cyclic Dependency Avoidance
Compilation Boundaries
Independent Testing
Reuse
Future Engine Expansion
```

The Core should optimize for:

```text
Clear Dependency Direction
Minimal Cycles
Stable Public Boundaries
Fast Development
Independent Testing
Reusability
Future Expansion
```

The exact Rust traits, APIs, crate split, and implementation details remain implementation decisions unless explicitly frozen by the architecture.

---

## Testing and Conformance

Testing is part of the Core architecture.

Core should provide common infrastructure for:

```text
Unit Tests
Contract Tests
Capability Registration Tests
Request / Response Tests
Version Compatibility Tests
Error Mapping Tests
Context Propagation Tests
Security Tests
Artifact Tests
Cancellation Tests
Deadline Tests
Lifecycle Tests
Concurrency Tests
Inter-Engine Tests
```

A shared test environment can provide:

```text
Mock Runtime
Fake Context
Mock Capability Clients
Mock Artifacts
Synthetic Operations
Cancellation Tests
Contract Fixtures
```

Architecture-conformance checks should ensure that:

```text
Core does not depend on engines
Domain engines do not bypass Core
Infrastructure engines do not bypass Core
Generated wire types do not leak into domain layers
Mandatory runtime policies cannot be bypassed
Shared identity types are used consistently
```

Engines still provide their own domain-specific semantic tests.

---

## Development Roadmap

Core is designed to be implemented in dependency order.

```text
Phase 0
Workspace Foundation
        ↓
Phase 1
Identity + Foundational Types
        ↓
Phase 2
Universal Contract Layer
        ↓
Phase 3
Error System
        ↓
Phase 4
Logging System
        ↓
Phase 5
Context Execution Infrastructure
        ↓
Phase 6
Capability System
        ↓
Phase 7
Transport + Universal Client/Server
        ↓
Phase 8
Engine Runtime
        ↓
Phase 9
Middleware + Security
        ↓
Phase 10
Artifact + Provenance
        ↓
Phase 11
Observability + Health + Configuration
        ↓
Phase 12
Streaming + Concurrency + Background Tasks
        ↓
Phase 13
Retry + Idempotency
        ↓
Phase 14
Internal Events
        ↓
Phase 15
Control Plane
        ↓
Phase 16
Engine SDK
        ↓
Phase 17
Testing + Conformance
```

Testing itself is continuous throughout development. Phase 17 represents the final comprehensive testing and conformance pass.

---

## Major Milestones

### Milestone 1 — Core Foundations

```text
Identity
Contracts
Context
Errors
Logging
```

### Milestone 2 — Communication Core

```text
Contracts
Transport
Universal Client
Server
Capability System
```

### Milestone 3 — Minimal Engine Runtime

At this point a minimal engine should theoretically be able to:

```text
Receive Request
      ↓
Core Runtime
      ↓
Resolve Capability
      ↓
Execute Handler
      ↓
Return Response
```

### Milestone 4 — Production Runtime

```text
Security
Artifacts
Provenance
Observability
Health
Configuration
Streaming
Concurrency
Retry
Idempotency
```

### Milestone 5 — Platform Core

```text
Control Plane
+
Engine SDK
```

### Milestone 6 — Core v1 Foundation

Both Domain and Infrastructure Engines consume the same Nizaam Core mechanisms:

```text
                 NIZAAM CORE
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
   Domain Engines       Infrastructure Engines
          │                       │
    own semantics           own semantics
    own workflow            own workflow
    own execution           own execution
```

---

## Design Principles

The most important principles of Nizaam Core are:

1. **Core provides mechanisms, not domain meaning.**
2. **Domain and Infrastructure Engines depend on Core.**
3. **Core never depends on engine semantics.**
4. **Shared contracts are standardized.**
5. **Identity types are strongly typed and distinct.**
6. **Engine workflows remain engine-owned.**
7. **The Control Plane is shared communication/control infrastructure, not a universal executor.**
8. **Security, Logging, Error, Artifact, Observability, and other Core systems remain clearly separated by ownership.**
9. **Public APIs expose stable contracts; implementation details remain internal.**
10. **Core systems interact through explicit interfaces rather than direct access to internal state.**
11. **Common mechanisms may be shared without forcing common semantics.**
12. **Engine-specific functionality must not be promoted into Core merely because multiple engines have similar code.**
13. **Testing and architectural conformance are first-class concerns.**
14. **Concrete crate splitting should be justified by real dependency, compilation, versioning, or ownership needs.**
15. **Core should remain extensible as the Nizaam ecosystem grows.**

---

## Core in the Nizaam Ecosystem

The final conceptual architecture is:

```text
                         NIZAAM
                           │
                           ▼
                    Central API / Apps
                           │
                           ▼
                     NIZAAM CORE
                           │
             ┌─────────────┴─────────────┐
             │                           │
             ▼                           ▼
      Engine Common Core          Global Platform Core
             │                           │
             │                     Control Plane
             │                           │
      ┌──────┼────────┐                  │
      ▼      ▼        ▼                  │
    Domain  Arabic    KG                 │
    Engines Engine   Engine              │
      │      │        │                  │
      └──────┴────────┴──────────────────┘
                     │
                     ▼
              Infrastructure Engines
```

Every engine can evolve independently while using the same foundational Core mechanisms.

The Core remains the stable technical foundation underneath the ecosystem.

---

## Status of the Architecture

The Core architecture defines:

```text
Responsibilities
Boundaries
Dependency Direction
Public / Internal Philosophy
Engine Isolation
Control Plane Boundary
Core System Ownership
Extension Model
Testing / Conformance Model
Implementation Order
```

Concrete implementation details such as exact Cargo crate splitting, exact Rust trait signatures, serialization libraries, transport libraries, and internal implementation techniques should be decided during implementation unless explicitly frozen by the architecture.

---

## Source of Truth

The Core architecture plan is the authoritative source for mechanisms shared across Domain and Infrastructure Engines.

Engine implementation plans should reference the Core architecture rather than independently redefining shared mechanisms.

When an engine needs a mechanism that belongs to Core, the engine should consume the corresponding Core component rather than creating a parallel implementation.

The Core architecture should evolve carefully and deliberately because it forms the common foundation for the entire Nizaam ecosystem.

This version is intentionally **README-level**, while `scope.md` remains the detailed living implementation-progress document.

---

**Author:** Sharique Chaudhary <br/>
**Project:** Nizaam Islamic Ecosystem <br/>
**Component:** Nizaam Core <br/>
**Language:** Rust <br/>
**Status:** Architecture Frozen / Implementation in Progress <br/>
**Last Updated:** September 3, 2026 <br/>
**Documentation:** Core Architecture & Implementation Plan <br/>
