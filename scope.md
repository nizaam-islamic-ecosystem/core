# Scope: Nizaam Core

`nizaam-core` is the shared Rust library for domain and infrastructure engines in the Nizaam Islamic Ecosystem. It supplies domain agnostic contracts and mechanisms. It does not own engine domain meaning, domain workflows, or engine specific storage.

## Architecture / Boundaries

Core owns approved shared contracts, identity, runtime mechanisms, and platform systems. Domain Engines and Infrastructure Engines consume Core while retaining their own semantics and behavior. Global Platform Core contains platform concerns such as the communication focused Control Plane. The Control Plane admits, validates, resolves, routes, propagates context, and reports communication failures. It does not become a domain workflow planner or execution engine.

Logging and Error are independent peer systems. They may communicate through typed references, but neither owns the other.

## Existing Project Baseline

Implementation began with an existing Cargo library skeleton at `core/` containing `Cargo.toml` and `src/lib.rs`. No binary target or `src/main.rs` exists. The root workspace and the complete private module scaffold are now in place. The Cargo package remains `core`; its library target is `nizaam_core`, which avoids a downstream import collision with Rust's built in `core` crate.

## At a glance

| # | Area | Phase | Status |
| - | ---- | ----- | ------ |
| 0 | Workspace and library foundation | 0 | verified |
| 1 | Identity, result primitives, operation model | 1 | verified |
| 2 | Universal Contract Layer | 2 | verified |
| 3 | Error System foundation | 3 | not started |
| 4 | Logging System foundation | 4 | not started |
| 5 | Context execution infrastructure | 5 | not started |
| 6 | Capability System | 6 | not started |
| 7 | Transport and universal client/server | 7 | not started |
| 8 | Engine Runtime | 8 | not started |
| 9 | Middleware and Security | 9 | not started |
| 10 | Artifact and Provenance | 10 | not started |
| 11 | Observability, Health, Configuration | 11 | not started |
| 12 | Streaming, Concurrency, Background Tasks | 12 | not started |
| 13 | Retry and Idempotency | 13 | not started |
| 14 | Internal Events | 14 | not started |
| 15 | Control Plane | 15 | not started |
| 16 | Engine SDK | 16 | not started |
| 17 | Testing and Conformance hardening | 17 | not started |

Testing is continuous. Phase 17 is the final integration and conformance hardening phase, not the first point at which tests are written.

## Project Structure

```text
Cargo.toml                         # workspace, member: core
core/
├── Cargo.toml                     # core package, nizaam_core library target
├── README.md
├── scope.md
├── src/
│   ├── lib.rs                     # public library root
│   ├── prelude.rs                 # small Phase 1 ergonomic surface
│   ├── identity/                  # distinct Core IDs
│   ├── operation/                 # operation and operation context
│   └── status.rs                  # result primitives and references
└── tests/
    └── foundations.rs             # public API integration test
```

The phase modules for contracts, errors, logging, capability, transport, runtime, security, artifacts, provenance, observability, health, configuration, middleware, streaming, retry, idempotency, events, Control Plane, SDK, and conformance also exist as private scaffolding under `src/`. Their presence records the approved structure only. It does not mark their phases as implemented or make their APIs public before the relevant phase has behavior and tests.

## Implementation Phases

### Phase 0: Workspace foundation

#### Goal

Establish one Core library crate at the center of the dependency graph without introducing engine dependencies or a binary target.

#### Decided

Keep one `core` Cargo package with a `nizaam_core` library target in the existing `core/` directory. Concrete future crate splitting remains open and requires an architectural reason.

#### What got built

Root Cargo workspace configuration, preserved `core/src/lib.rs` as the library root, the approved private module scaffold, and a concise crate README.

#### Verification

`cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass. `cargo fmt --all --check` currently reports formatting in the private module scaffold and is still to be corrected.

#### Corrections

The Cargo package remains `core`. Its library target is `nizaam_core` so downstream Rust code does not collide with Rust's standard `core` crate.

#### Remaining work

No binary target is planned. Future workspace members are deliberately deferred until their architecture exists.

#### Checklist

- [x] Inspect and preserve the existing Cargo library project
- [x] Create root workspace configuration
- [x] Keep `src/lib.rs` as crate root
- [x] Avoid `src/main.rs` and binary targets
- [x] Verify workspace checks

### Phase 1: Absolute Core foundations

#### Goal

Provide strongly typed identity, shared outcome primitives, and a minimal operation model on which later Core systems can depend.

#### Decided

All Core identities are distinct newtypes around validated text. They are intentionally not interchangeable. Error and artifact primitives are references only: their owning systems will define their detailed semantics in later phases.

#### What got built

`identity` defines `MessageId`, `OperationId`, `CorrelationId`, `EngineId`, `EngineInstanceId`, `CapabilityId`, `ContractId`, `PlanId`, `NodeId`, `AttemptId`, and `ArtifactId`. `status` defines `Status`, `Retryability`, `Compatibility`, `ErrorReference`, and `ArtifactReference`. `operation` defines `Operation` and `OperationContext`, including parent, plan, node, and attempt identity. `prelude` exposes the intentionally small Phase 1 public surface.

#### Verification

`cargo check --workspace`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass. Five tests pass: four unit tests and one integration test. `cargo fmt --all --check` remains outstanding for the private module scaffold.

#### Corrections

None.

#### Remaining work

Deadline, cancellation, security context, and provenance context receive their behavior in the later context, security, and provenance phases. No placeholder semantic systems were invented here.

#### Checklist

- [x] Implement distinct identity types
- [x] Reject empty identity values
- [x] Implement outcome and compatibility primitives
- [x] Implement operation and attempt context foundations
- [x] Add unit and public API integration coverage
- [x] Verify Rust checks

### Phase 2: Universal Contract Layer

**Status: verified**

#### Goal

Establish the versioned, domain agnostic language used for communication between engines.

#### Planned implementation

Add contract and payload descriptors; schema, version, interaction, requirements, and execution metadata; universal request, response, and message envelope types; and structural validation, compatibility, and payload encoding and decoding mechanisms.

#### What got built

Implemented the public `contracts` module and exposed its deliberate Phase 2 surface through the prelude. Contract descriptors now carry validated contract and capability identities, contract and schema versions, interaction type, and payload media type. Shared metadata now carries sender and target engine identities, optional engine instance identities, capability and minimum version requirements, and execution hints.

Universal message envelopes now preserve message identity, the existing operation context, contract metadata, and an opaque encoded payload. Universal request and response wrappers distinguish request and response interactions, and responses carry the existing technical status primitive.

Payload meaning remains outside Core. `EncodedPayload` stores the descriptor and bytes, while `PayloadCodec` provides the encoding and decoding boundary and `RawPayloadCodec` supports already encoded engine payloads without adding a serialization dependency. Compatibility checks compare contract identity, capability identity, interaction, media type, and version. Structural validation checks payload presence, capability requirements, descriptor consistency, and request or response interaction.

The standalone crate manifest was also corrected to remove invalid inherited workspace fields, allowing local Cargo validation.

#### Verification

`cargo fmt --all`, `cargo test`, and `cargo clippy --all-targets -- -D warnings` pass from `core/`. The test suite covers descriptor construction and rejection, metadata, compatibility, request validation, opaque payload round trips, and public prelude usage.

#### Boundary

Core understands the envelope and contract metadata. Each engine continues to own its actual capability payload types and semantic validation.

#### Done when

An engine can describe and structurally validate a versioned request or response without Core learning its domain meaning.

#### Checklist

- [x] Implement contract and payload descriptors
- [x] Implement schema and contract version metadata
- [x] Implement interaction, requirements, and execution metadata
- [x] Implement universal message envelope, request, and response types
- [x] Implement opaque payload encoding and decoding boundary
- [x] Implement structural validation and compatibility checks
- [x] Preserve engine ownership of payload semantics
- [x] Add unit and public integration coverage
- [x] Verify formatting, tests, and Clippy

### Phase 3: Error System foundation

#### Goal

Give every later Core component one strict technical error model.

#### Planned implementation

Add error definitions and occurrences, global errors, error classes, severity, retryability, codes, context, references, catalog registration, and validation. Engine specific error definitions remain namespaced extensions of the global contract.

#### Boundary

Error and Logging are peer systems. Logging does not own Error, and Error does not own Logging.

#### Done when

Later Core mechanisms can return validated, referenceable errors through a shared contract.

### Phase 4: Logging System foundation

#### Goal

Provide structured, asynchronous, reusable logging for both global and engine local use.

#### Planned implementation

Add log context, events, levels, event types, scopes, sources, producer validation, buffering, dispatch, subscribers, sinks, and logging instances. Global and local logging are scopes of the same system.

#### Boundary

The logging mechanism is shared, while an engine retains ownership of its additional fields, consumers, and operational meaning.

#### Done when

Core and engines can produce typed log events that flow through the same validated fan out mechanism.

### Phase 5: Context execution infrastructure

#### Goal

Make operation execution safe and consistent across engine boundaries.

#### Planned implementation

Add cancellation tokens with parent, child, and shutdown propagation; deadlines and timeout handling; and the `EngineContext` composition of operation, correlation, cancellation, deadline, security, and provenance context.

#### Boundary

Core propagates context. Engines decide what their domain work does when cancellation or expiration occurs.

#### Done when

Downstream work receives context rather than reconstructing it from transport metadata.

### Phase 6: Capability System

#### Goal

Give engines a common way to expose, register, locate, and invoke capabilities.

#### Planned implementation

Add capability definitions, registrations, a registry, handlers, and local dispatch.

#### Boundary

Core provides the capability mechanism only. Capability names, typed requests, workflows, and results remain engine owned.

#### Done when

An engine can register a capability and Core can resolve its handler without interpreting its domain semantics.

### Phase 7: Transport and universal client/server

#### Goal

Connect the universal contracts and capability system through a shared communication boundary.

#### Planned implementation

Add transport, connection, stream, request and response transmission abstractions, then the universal client and engine server surfaces. Typed capability clients build on the universal client rather than creating separate transport stacks.

#### Boundary

Concrete transport and serialization choices remain implementation decisions. Engine payload meaning remains outside Core.

#### Done when

A typed engine client can send a universal request through an abstract transport to an engine server and receive a universal response.

### Phase 8: Engine Runtime

#### Goal

Provide the shared lifecycle and request execution infrastructure that every engine can use.

#### Planned implementation

Add lifecycle progression from startup through configuration, dependencies, capabilities, registration, readiness, serving, draining, and stop. Integrate the request pipeline, capability dispatch, context, transport, cancellation, deadlines, health, and shutdown.

#### Boundary

The runtime owns execution infrastructure. Each engine owns the behavior of its capability handlers and internal workflow.

#### Done when

A minimal test engine can receive a request, dispatch a capability, and return a response through Core.

### Phase 9: Middleware and Security

#### Goal

Add the mandatory processing and security boundary around runtime requests.

#### Planned implementation

Add the middleware chain for security, tracing, metrics, validation, contract resolution, dispatch, and response processing. Add security context, service identity, authentication and authorization integration points, and security context propagation.

#### Boundary

Core supplies security mechanisms and enforced runtime boundaries. Engines retain capability and domain authorization rules.

#### Done when

Every runtime request passes the mandatory processing boundary with trusted context available to its handler.

### Phase 10: Artifact and Provenance

#### Goal

Make artifacts and execution history referenceable across engines.

#### Planned implementation

Add artifact versions, references, access, publication, retrieval, validation, resolution, integrity, and lifecycle mechanisms. Add provenance for operations, attempts, engines, capabilities, messages, sources, versions, and execution.

#### Boundary

Core manages artifact and provenance mechanisms, never the meaning or content of an engine's artifacts.

#### Done when

An engine can exchange artifact references and preserve execution provenance through Core contracts.

### Phase 11: Observability, Health, and Configuration

#### Goal

Provide operational visibility, readiness, and typed runtime configuration.

#### Planned implementation

Add metrics, tracing, correlation, telemetry, diagnostics hooks, liveness, readiness, capability readiness, dependency health, draining state, configuration loading, validation, environment integration, typed access, and runtime propagation.

#### Boundary

Core provides shared protocols and plumbing. Engine specific health checks and configuration schemas remain engine owned.

#### Done when

An engine can expose its operating state and receive validated runtime configuration through common interfaces.

### Phase 12: Streaming, Concurrency, and Background Tasks

#### Goal

Add the heavier runtime mechanisms after the base runtime is established.

#### Planned implementation

Add streaming lifecycle, partial and final results, cancellation, and context propagation; bounded and resource aware task execution; and background task registration, startup, cancellation, shutdown, health participation, and resource accounting.

#### Boundary

Core provides lifecycle safe mechanisms. Workload strategy and engine specific jobs remain local to each engine.

#### Done when

Streaming and background work participate in the same context, lifecycle, health, cancellation, and deadline rules as requests.

### Phase 13: Retry and Idempotency

#### Goal

Make safe repeat execution available once operation, error, and runtime foundations exist.

#### Planned implementation

Add attempt tracking, retry execution, backoff, retryability, deadline and cancellation awareness, idempotency keys, duplicate detection, and safe retry support.

#### Boundary

Core supplies the execution mechanism. Retry policy remains owned by the operation, plan node, or capability that requires it.

#### Done when

Core can distinguish duplicate work and execute an authorized retry without losing operation context.

### Phase 14: Internal Events

#### Goal

Provide a small reusable mechanism for internal engine events.

#### Planned implementation

Add event identity, publishing, subscription, scope, delivery, cancellation, and lifecycle support.

#### Boundary

This is optional infrastructure, not a required event driven architecture. Event definitions remain engine owned.

#### Done when

An engine can publish and consume scoped internal events through a lifecycle aware Core mechanism.

### Phase 15: Control Plane

#### Goal

Implement the frozen communication focused Global Platform Core after its required foundations are available.

#### Planned implementation

Add request admission, protocol validation, contract, capability, and destination resolution, context propagation, request and response routing, communication lifecycle handling, and communication failure reporting.

#### Boundary

The Control Plane is not a domain planner, reasoning engine, global workflow executor, model inference engine, or an engine's internal execution layer.

#### Done when

It can govern secure, compatible inter engine communication while engines continue to own workflow and domain completion.

### Phase 16: Engine SDK

#### Goal

Offer engine developers an ergonomic public surface over stable Core mechanisms.

#### Planned implementation

Add SDK entry points for engine setup, capabilities, contexts, and the supported runtime and client surfaces.

#### Boundary

The SDK simplifies Core use; it does not create a second runtime or expose Core internal implementation details as a public contract.

#### Done when

An engine developer can build against the supported SDK surface without manually composing every internal Core module.

### Phase 17: Testing and Conformance hardening

#### Goal

Complete the integrated verification and architecture conformance layer after the major runtime surfaces exist.

#### Planned implementation

Add shared unit, contract, capability registration, request and response, compatibility, error mapping, context, security, artifact, cancellation, deadline, lifecycle, concurrency, and inter engine tests. Add checks for dependency direction, Core bypasses, generated type leakage, runtime boundaries, and mandatory middleware.

#### Boundary

Tests are written throughout every phase. This phase finalizes the complete cross system and conformance coverage.

#### Done when

Both engine categories can demonstrate use of Core without violating the frozen architectural boundaries.

## Architectural Decisions

- Core remains a library crate with no `main.rs`.
- One crate with internal modules is the current implementation choice. Future crate splitting is not authorized without a demonstrated architectural reason.
- Core contains mechanisms and contracts, never Nizaam engine domain semantics.
- The Control Plane will be communication focused and implemented only in Phase 15.

## Corrections / Changes

- The Cargo package is `core` and its library target is `nizaam_core`; the existing directory, library root, and private module scaffold remain in place.

## Open Questions

None currently. Concrete trait signatures, provider choices, serialization, async runtime, transport implementation, and eventual crate splitting are deliberately deferred by the plan rather than unresolved architecture.

## Current State

Phase 0, Phase 1, and Phase 2 are implemented. The crate has no binary target or `src/main.rs`; it has no dependencies on Nizaam engines or domain semantics. Compilation, tests, and Clippy pass.

## Next Step

Implement Phase 3, the Error System foundation. Preserve the separation between technical error handling and the already implemented universal contract layer.
