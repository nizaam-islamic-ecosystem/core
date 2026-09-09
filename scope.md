# Scope: Nizaam Core

`nizaam-core` is the shared Rust library for domain and infrastructure engines in the Nizaam Islamic Ecosystem. It supplies domain agnostic contracts and mechanisms. It does not own engine domain meaning, domain workflows, or engine specific storage.

## Architecture / Boundaries

Core owns approved shared contracts, identity, runtime mechanisms, and platform systems. Domain Engines and Infrastructure Engines consume Core while retaining their own semantics and behavior. Global Platform Core contains platform concerns such as the communication focused Control Plane. The Control Plane admits, validates, resolves, routes, propagates context, and reports communication failures. It does not become a domain workflow planner or execution engine.

Logging and Error are independent peer systems. They may communicate through typed references, but neither owns the other.

## Existing Project Baseline

Implementation began with an existing Cargo library skeleton at the repository root containing `Cargo.toml` and `src/lib.rs`. No binary target or `src/main.rs` exists. The repository root is the explicit single member Cargo workspace, and the complete private module scaffold is in place. The Cargo package remains `core`; its library target is `nizaam_core`, which avoids a downstream import collision with Rust's built in `core` crate.

## At a glance

| #  | Area                                         | Phase | Status      |
| -- | -------------------------------------------- | ----- | ----------- |
| 0  | Workspace and library foundation             | 0     | verified    |
| 1  | Identity, result primitives, operation model | 1     | verified    |
| 2  | Universal Contract Layer                     | 2     | verified    |
| 3  | Error System foundation                      | 3     | verified    |
| 4  | Logging System foundation                    | 4     | verified    |
| 5  | Context execution infrastructure             | 5     | verified    |
| 6  | Capability System                            | 6     | verified    |
| 7  | Transport and universal client/server        | 7     | not started |
| 8  | Engine Runtime                               | 8     | not started |
| 9  | Middleware and Security                      | 9     | not started |
| 10 | Artifact and Provenance                      | 10    | not started |
| 11 | Observability, Health, Configuration         | 11    | not started |
| 12 | Streaming, Concurrency, Background Tasks     | 12    | not started |
| 13 | Retry and Idempotency                        | 13    | not started |
| 14 | Internal Events                              | 14    | not started |
| 15 | Control Plane                                | 15    | not started |
| 16 | Engine SDK                                   | 16    | not started |
| 17 | Testing and Conformance hardening            | 17    | not started |

Testing is continuous. Phase 17 is the final integration and conformance hardening phase, not the first point at which tests are written.

## Project Structure

```text
Cargo.toml                         # package and explicit single member workspace
Cargo.lock
README.md
scope.md
src/
├── lib.rs                         # public library root
├── prelude.rs                     # small Phase 1 ergonomic surface
├── identity/                      # distinct Core IDs
├── operation/                     # operation and operation context
└── status.rs                      # result primitives and references
tests/
├── foundations.rs                 # public API integration test
└── contracts.rs                   # public contract integration tests
```

The phase modules for errors, logging, capability, transport, runtime, security, artifacts, provenance, observability, health, configuration, middleware, streaming, retry, idempotency, events, Control Plane, SDK, and conformance also exist as private scaffolding under `src/`. Their presence records the approved structure only. The contracts module is public because Phase 2 is implemented and verified. A scaffold's presence does not mark its phase as implemented or make its APIs public before the relevant phase has behavior and tests.

## Agent Implementation Rules

These rules are mandatory for any agent working on Nizaam Core.

### 1. Scope is authoritative

This scope is the authoritative implementation specification for Nizaam Core.

The agent must follow the architecture, boundaries, phase order, decisions,
constraints, and completion criteria defined in this document.

The agent must not silently reinterpret or replace architectural decisions
with its own preferred design.

### 2. Ask before proceeding when requirements are unclear

The agent MUST ask questions before implementing anything whenever:

* a requirement is ambiguous or incomplete;
* two requirements appear to conflict;
* the implementation requires an architectural decision that is not already
  defined in this scope;
* the agent needs to choose between multiple materially different designs;
* the agent believes an existing architectural decision should be changed;
* a previous phase must be modified in a way that could affect its contract
  or behavior;
* the agent needs to introduce a dependency, provider, runtime, transport,
  serialization format, storage mechanism, or other implementation choice
  that has not been authorized;
* the agent discovers that the current phase cannot be implemented correctly
  without changing the scope;
* the agent is unsure whether functionality belongs in Core or in an engine;
* the agent is unsure whether functionality belongs to the current phase
  or a later phase.

The agent MUST NOT guess in these situations.

The agent must explain the ambiguity, present the relevant options when
appropriate, and wait for explicit user direction.

### 3. Permission is required to work against these rules

If the agent determines that correct implementation requires violating,
bypassing, weakening, changing, or extending any rule, boundary, or
architectural decision in this scope, it MUST STOP and ask for explicit
permission before making that change.

The agent must clearly state:

1. Which rule, boundary, or decision would be affected.
2. Why the current implementation cannot proceed without changing it.
3. What change the agent proposes.
4. What possible consequences the change may have.

The agent must not proceed until the user explicitly approves the change.

### 4. Never silently change architecture

The agent must never silently:

* redesign an existing system;
* move responsibilities between Core and engines;
* introduce new Core responsibilities;
* change a verified phase's intended behavior;
* change public contracts;
* change dependency direction;
* introduce domain semantics into Core;
* turn a mechanism into a workflow or policy;
* implement functionality belonging to a later phase;
* remove an architectural boundary because it makes implementation easier.

If such a change appears necessary, the agent must stop and ask permission.

### 5. Protect previously verified phases

A previously verified phase is considered a stable foundation.

When implementing a new phase, the agent must preserve the behavior,
contracts, boundaries, and tests of all previously verified phases.

The agent must not rewrite, remove, weaken, or redesign previous-phase
functionality merely to simplify the current phase.

If modification of a previous phase is genuinely required by the approved
architecture, the agent must explain why and obtain permission before making
the change.

### 6. Implement only the current phase

The agent must implement the current phase being worked on.

The agent must not implement functionality from future phases merely because:

* the required files already exist as scaffolding;
* the functionality appears useful;
* the functionality makes the current implementation easier;
* the agent believes it should be implemented earlier.

Future-phase functionality remains deferred unless this scope explicitly
requires it for the current phase.

Existing scaffolding does not mean that a phase is implemented.

### 7. Preserve Core and engine boundaries

Core provides shared contracts and mechanisms.

Core must not acquire:

* domain entities;
* domain workflows;
* domain business rules;
* domain policies;
* domain algorithms;
* engine-specific semantics;
* engine-specific storage;
* engine-specific payload meaning;
* engine-specific methodologies.

If the agent believes something should be added to Core but it may contain
domain or engine semantics, the agent must stop and ask before implementing it.

### 8. Do not invent deferred implementation choices

When this scope deliberately leaves a choice open, the agent must not
silently choose a technology or provider unless the choice is necessary
and authorized.

This includes, where applicable:

* async runtime;
* transport implementation;
* serialization format;
* authentication provider;
* storage provider;
* persistence mechanism;
* observability vendor;
* external service;
* crate splitting;
* infrastructure provider.

If a deferred choice becomes necessary for the current phase, the agent
must explain the choice and ask for permission before committing to it.

### 9. Full regression testing is mandatory

After implementing or modifying any phase, the agent MUST run the complete
Core test suite from the repository root:

    cargo test

The agent must NOT test only the current phase.

Tests belonging to all previously implemented phases are mandatory
regression tests and must continue to pass.

A current phase must not be declared complete if previously passing tests
are failing.

If a regression occurs, the agent must investigate and resolve it before
declaring the current phase complete.

### 10. Do not modify tests merely to hide regressions

The agent must not modify, remove, weaken, skip, or delete an existing test
simply because the current implementation causes it to fail.

If an existing test conflicts with an approved architectural change, the
agent must stop and ask the user before changing the test or its expected
behavior.

#### 10.1. Unit and integration testing requirements

Every implementation source file must have unit tests covering the behavior
of each testable function, method, constructor, validation path, and relevant
error path defined in that file.

Unit tests should remain close to the implementation and may be placed in the
corresponding source module using Rust's `#[cfg(test)]` test modules.

The repository-level `tests/` directory must contain integration tests for
the public Core API and cross-module behavior.

Unit tests verify individual implementation behavior.

Integration tests verify that multiple Core modules and public APIs work
together correctly from the perspective of a downstream consumer.

Both unit and integration tests are mandatory. Neither replaces the other.

The agent must add or update appropriate unit and integration tests whenever
it adds or modifies behavior.

The complete test suite must be executed with:

    cargo test --workspace

This command must be treated as the standard verification command for both
unit and integration tests.

The agent must not consider a function, file, or phase adequately tested
merely because another unrelated integration test happens to pass.

### 10.1.1. Module-level testing through `mod.rs`

Every Core module folder that contains multiple implementation source files
must use its `mod.rs` as the module's shared internal test surface.

Each implementation source file must contain its own unit tests for its
testable behavior.

The corresponding `mod.rs` may additionally contain unit tests that exercise
the interaction between multiple implementation files within the same module.

These module-level tests are still unit tests because they verify behavior
within a single Core module and do not represent downstream consumer usage.

The `mod.rs` file must not become a replacement for source-file unit tests.
Source-file behavior must remain covered by tests close to its implementation.

Repository-level integration tests under `tests/` must be used to verify
public API behavior, cross-module interactions, feature-level behavior, and
end-to-end Core flows from the perspective of a downstream consumer.

### 11. Compilation is not completion

The agent must not consider a phase complete merely because:

* the project compiles;
* `cargo check` passes;
* the current phase's tests pass;
* the required files exist;
* the implementation appears logically correct.

Before declaring a phase complete, the agent must verify the phase against:

* Goal;
* Planned implementation;
* Files and folders;
* Boundary;
* Done when;
* Checklist;
* Existing regression tests.

### 12. Full verification after every implementation

The minimum completion verification for every implementation phase is:

    cargo test --workspace
    cargo test --workspace --all-targets
    cargo test --workspace --doc
    cargo check --workspace
    cargo build --workspace
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

These checks must be run from the repository root.

The agent must verify the complete existing test suite, formatting, and
Clippy checks after implementing or modifying any phase.

The agent must NOT test, format-check, or lint only the current phase.

All previously implemented phases are included in the required regression
verification.

A phase must not be declared complete if any of these checks fail.

The agent may run additional focused tests or verification commands when
appropriate, but focused checks do not replace these complete repository-wide
checks.

### 13. Ask questions instead of guessing

When uncertain, the default behavior is:

    STOP → EXPLAIN → ASK → WAIT → IMPLEMENT

The agent must not use:

    GUESS → IMPLEMENT → HOPE

This rule applies especially to architectural, API, ownership, dependency,
boundary, and phase-scope decisions.

### 14. Phase completion requires explicit verification

Before reporting that a phase is complete, the agent must verify that:

* the implementation matches this scope;
* no unauthorized architectural changes were introduced;
* previously verified phases remain intact;
* the complete `cargo test` suite passes;
* the current phase's requirements are satisfied;
* no future-phase functionality was accidentally implemented as part of
  the current phase.

If any of these conditions cannot be satisfied, the agent must not claim
the phase is complete.

### 15. User approval overrides implementation convenience

Implementation convenience is never sufficient justification for breaking
an architectural rule.

If following the scope makes implementation harder, the agent must follow
the scope.

If the agent believes the scope itself needs to change, it must ask the
user for permission before changing the scope or implementing against it.

### 16. Do not modify scope.md without permission

The agent must not modify `scope.md` as part of normal implementation.

If the agent believes the scope is incorrect, incomplete, contradictory,
or needs clarification, it must report the issue and ask the user for
permission before changing the scope.

### 17. Completion report

When a phase is completed, the agent should report:

* what was implemented;
* what files were changed;
* what tests were added or updated;
* the result of the full `cargo test`;
* any other verification performed;
* whether any previous-phase files or behavior were modified;
* whether any architectural decisions required user approval.

The agent must clearly state if anything remains unresolved.

# Implementation Phases

## Phase 0: Workspace foundation

### Goal

Establish one Core library crate at the center of the dependency graph without introducing engine dependencies or a binary target.

### Decided

Keep one `core` Cargo package with a `nizaam_core` library target in the existing `core/` directory. Concrete future crate splitting remains open and requires an architectural reason.

### What got built

Root Cargo workspace configuration, preserved `core/src/lib.rs` as the library root, the approved private module scaffold, and a concise crate README.

### Files and Folders

**Workspace / package files**

* `Cargo.toml` with the Core package and explicit workspace declaration
* `Cargo.lock`

**Core library files**

* `src/lib.rs`

**Documentation**

* `README.md`
* `scope.md`

**Structural requirement**

* No `core/src/main.rs`
* No binary target

### Verification

`cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all --check` pass from the repository root.

### Corrections

The Cargo package remains `core`. Its library target is `nizaam_core` so downstream Rust code does not collide with Rust's standard `core` crate.

### Remaining work

No binary target is planned. Future workspace members are deliberately deferred until their architecture exists.

### Checklist

* [x] Inspect and preserve the existing Cargo library project
* [x] Create explicit single member workspace configuration at the repository root
* [x] Keep `src/lib.rs` as crate root
* [x] Avoid `src/main.rs` and binary targets
* [x] Verify workspace checks, tests, Clippy, and formatting

---

## Phase 1: Absolute Core foundations

### Goal

Provide strongly typed identity, shared outcome primitives, and a minimal operation model on which later Core systems can depend.

### Decided

All Core identities are distinct newtypes around validated text. They are intentionally not interchangeable. Error and artifact primitives are references only: their owning systems will define their detailed semantics in later phases.

### What got built

`identity` defines `MessageId`, `OperationId`, `CorrelationId`, `EngineId`, `EngineInstanceId`, `CapabilityId`, `ContractId`, `PlanId`, `NodeId`, `AttemptId`, and `ArtifactId` in their dedicated identity modules. `status` defines `Status`, `Retryability`, `Compatibility`, `ErrorReference`, and `ArtifactReference`. `operation` defines `Operation` and `OperationContext` in their dedicated operation modules, including parent, plan, node, and attempt identity. `prelude` exposes the intentionally small Phase 1 public surface.

### Files and Folders

**Identity**

* `src/identity/mod.rs`
* `src/identity/message.rs`
* `src/identity/operation.rs`
* `src/identity/engine.rs`
* `src/identity/capability.rs`
* `src/identity/contract.rs`
* `src/identity/artifact.rs`
* `src/identity/plan.rs`

**Status / shared primitives**

* `src/status.rs`

**Operation foundation**

* `src/operation/mod.rs`
* `src/operation/context.rs`

**Public ergonomic surface**

* `src/prelude.rs`

**Tests**

* `tests/foundations.rs`

### Verification

The focused foundation integration tests and identity and operation unit tests pass. The complete workspace checks are rerun after Phase 2 because the Phase 2 public surface depends on these foundations.

### Corrections

None.

### Remaining work

Deadline, cancellation, security context, and provenance context receive their behavior in the later context, security, and provenance phases. No placeholder semantic systems were invented here.

### Checklist

* [x] Implement distinct identity types
* [x] Reject empty identity values
* [x] Implement outcome and compatibility primitives
* [x] Implement operation and attempt context foundations
* [x] Add unit and public API integration coverage
* [x] Verify foundation integration and unit tests

---

## Phase 2: Universal Contract Layer

**Status: verified**

### Goal

Establish the versioned, domain agnostic language used for communication between engines.

### Planned implementation

Add contract and payload descriptors; schema, version, interaction, requirements, and execution metadata; universal request, response, and message envelope types; and structural validation, compatibility, and payload encoding and decoding mechanisms.

### What got built

Implemented the public `contracts` module and exposed its deliberate Phase 2 surface through the prelude. Contract descriptors now carry validated contract and capability identities, contract and schema versions, interaction type, and payload media type. Shared metadata now carries sender and target engine identities, optional engine instance identities, capability and minimum version requirements, and execution hints.

Universal message envelopes now preserve message identity, the existing operation context, contract metadata, and an opaque encoded payload. Universal request and response wrappers distinguish request and response interactions, and responses carry the existing technical status primitive.

Payload meaning remains outside Core. `EncodedPayload` stores the descriptor and bytes, while `PayloadCodec` provides the encoding and decoding boundary and `RawPayloadCodec` supports already encoded engine payloads without adding a serialization dependency. Compatibility checks compare contract identity, capability identity, interaction, media type, and version. Structural validation checks payload presence, capability requirements, descriptor consistency, and request or response interaction.

The standalone crate manifest was also corrected to remove invalid inherited workspace fields, allowing local Cargo validation.

### Files and Folders

**Contracts**

* `src/contracts/mod.rs`
* `src/contracts/descriptor.rs`
* `src/contracts/metadata.rs`
* `src/contracts/envelope.rs`
* `src/contracts/request.rs`
* `src/contracts/response.rs`
* `src/contracts/compatibility.rs`
* `src/contracts/validation.rs`

**Public surface**

* `src/prelude.rs`

**Tests**

* `tests/contracts.rs`
* `tests/foundations.rs`

### Verification

`cargo fmt --all --check`, `cargo test --workspace`, `cargo check --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass from the Core repository root. The test suite covers descriptor construction and rejection, metadata, compatibility, request validation, opaque payload round trips, and public prelude usage.

### Boundary

Core understands the envelope and contract metadata. Each engine continues to own its actual capability payload types and semantic validation.

### Done when

An engine can describe and structurally validate a versioned request or response without Core learning its domain meaning.

### Checklist

* [x] Implement contract and payload descriptors
* [x] Implement schema and contract version metadata
* [x] Implement interaction, requirements, and execution metadata
* [x] Implement universal message envelope, request, and response types
* [x] Implement opaque payload encoding and decoding boundary
* [x] Implement structural validation and compatibility checks
* [x] Preserve engine ownership of payload semantics
* [x] Add unit and public integration coverage in `tests/contracts.rs`
* [x] Verify formatting, tests, compilation, and Clippy

---

## Phase 3: Error System foundation

**Status: verified**

### Goal

Give every later Core component one strict technical error model.

### Planned implementation

Add error definitions and occurrences, global errors, error classes, severity, retryability, codes, context, references, catalog registration, and validation. Engine specific error definitions remain namespaced extensions of the global contract.

### What got built

Implemented the public, first-class `error` module with validated namespaced `ErrorCode`, `ErrorOwner`, and `ErrorDefinition` types; shared error classification and severity; and reuse of the existing `Retryability` and `ErrorReference` primitives. The Error System now validates definition ownership, registers definitions in an in-process catalog, rejects duplicates, and requires registration before reporting an occurrence.

`GlobalError` carries the definition's code, owner, version, class, severity, retryability, message, solution reference, operation context, structured diagnostic details, and cause reference. `ErrorEvent` separates a runtime occurrence from its static catalog definition. `ErrorSystem` and scoped `ErrorSystemInstance` provide synchronous registration and reporting without taking ownership of Logging, transport, persistence, or domain payload semantics.

### Files and Folders

**Error System**

* `src/error/mod.rs`
* `src/error/catalog.rs`
* `src/error/definition.rs`
* `src/error/event.rs`
* `src/error/reference.rs`
* `src/error/system.rs`
* `src/error/validation.rs`

**Shared dependencies**

* `src/status.rs`
* `src/identity/`

**Tests**

* `tests/errors.rs`
* `tests/errors/`

### Boundary

Error and Logging are peer systems. Logging does not own Error, and Error does not own Logging.

### Verification

The focused public API tests in `tests/errors.rs` cover registration, contextual reporting, cause preservation, structured diagnostics, and rejection of unregistered definitions. The complete Core test, formatting, compilation, and Clippy checks pass from `core/`.

### Done when

Later Core mechanisms can return validated, referenceable errors through a shared contract.

### Checklist

* [x] Implement validated namespaced error codes and ownership
* [x] Implement definitions, classes, severity, version, retryability, and guidance
* [x] Implement global errors, events, context, causes, and diagnostic details
* [x] Implement catalog registration, duplicate rejection, and lookup
* [x] Require registered definitions for occurrence reporting
* [x] Preserve existing `ErrorReference` and `Retryability` contracts
* [x] Add unit and public integration coverage
* [x] Verify formatting, tests, compilation, and Clippy

---

## Phase 4: Logging System foundation

**Status: verified**

### Goal

Provide structured, asynchronous, reusable logging for both global and engine local use.

### Planned implementation

Add log context, events, levels, event types, scopes, sources, producer validation, buffering, dispatch, subscribers, sinks, and logging instances. Global and local logging are scopes of the same system.

### What got built

Implemented the public `logging` module and its prelude surface. Structured `LogEvent` values carry Core identities, operation context, level, event type, source, scope, status, error references, artifact references, and metadata. Validation rejects empty fields, invalid scope and source combinations, missing local engine context, and mismatched engine sources.

`LoggingSystem` creates global and local `LoggingInstance` values over one shared dispatcher. `LogSink` supports multiple subscribers. Dispatch uses a bounded standard library channel and worker thread. Debug and info events may be dropped when the queue is full. Warning, error, and audit events wait for queue capacity. Shutdown is explicit and joins the worker.

### Files and Folders

**Logging System**

* `src/logging/mod.rs`
* `src/logging/context.rs`
* `src/logging/event.rs`
* `src/logging/instance.rs`
* `src/logging/system.rs`
* `src/logging/dispatch.rs`
* `src/logging/sink.rs`
* `src/logging/validation.rs`

**Public surface**

* `src/prelude.rs`

**Tests**

* `tests/logging.rs`
* `tests/logging/`

### Boundary

The logging mechanism is shared, while an engine retains ownership of its additional fields, consumers, and operational meaning.

### Done when

Core and engines can produce typed log events that flow through the same validated fan out mechanism.

### Checklist

* [x] Implement typed log context and event contract
* [x] Implement levels, event types, scopes, and sources
* [x] Implement event and producer validation
* [x] Implement bounded asynchronous buffering and dispatch
* [x] Implement subscribers and sinks
* [x] Implement global and local logging instances
* [x] Add public integration coverage
* [x] Verify formatting, tests, compilation, and Clippy

---

## Phase 5: Context execution infrastructure

**Status: verified**

### Goal

Make operation execution safe and consistent across engine boundaries.

### Planned implementation

Add cancellation tokens with parent, child, and shutdown propagation; deadlines and timeout handling; and the `EngineContext` composition of operation, correlation, cancellation, deadline, security, and provenance context.

### Boundary

Core propagates context. Engines decide what their domain work does when cancellation or expiration occurs.

### Done when

Downstream work receives context rather than reconstructing it from transport metadata.

### What got built

Implemented the public runtime context surface on the `audit-phases` branch. Core now provides thread safe cancellation tokens with parent and child propagation, absolute deadlines with expiration and remaining time checks, and provider neutral security and provenance context values. `EngineContext` composes these values with the existing `OperationContext` and derives child contexts without widening cancellation or deadlines.

The runtime boundaries now also provide lifecycle transitions, context checked execution pipelines, cancellable task scopes, background task ownership and shutdown joining, and a minimal engine runtime owner. Provenance contexts support immutable derived attributes without selecting a storage provider. Expired contexts can translate through the existing shared Error contract.

### Files and Folders

**Operation context**

* `src/operation/mod.rs`
* `src/operation/context.rs`
* `src/operation/cancellation.rs`
* `src/operation/deadline.rs`

**Security context**

* `src/security/mod.rs`
* `src/security/context.rs`

**Provenance context**

* `src/provenance/mod.rs`
* `src/provenance/context.rs`

**Runtime boundaries implemented in Phase 5**

* `src/runtime/mod.rs`
* `src/runtime/engine.rs`
* `src/runtime/lifecycle.rs`
* `src/runtime/pipeline.rs`
* `src/runtime/concurrency.rs`
* `src/runtime/background.rs`

**Shared error contract used by context**

* `src/error/`

**Public surface**

* `src/prelude.rs`

**Tests**

* `tests/context.rs`

### Verification

`cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo check --workspace` pass from the Core repository root. The suite covers 34 library tests and 21 integration tests, including cancellation propagation and isolation, deadline limiting, context composition, lifecycle transitions, pipeline ordering and cancellation, task scopes, background shutdown, engine shutdown, provenance derivation, and public consumer propagation.

### Checklist

* [x] Implement cancellation tokens and parent, child, and shutdown propagation
* [x] Implement absolute deadlines, expiration, and remaining time checks
* [x] Implement provider neutral security and provenance context values
* [x] Implement `EngineContext` composition and child derivation
* [x] Implement context checked execution pipeline behavior
* [x] Implement lifecycle, task scope, background task, and engine runtime boundaries
* [x] Translate expired contexts through the shared Error contract
* [x] Add unit and public integration coverage
* [x] Verify formatting, tests, compilation, and Clippy

The exact universal operation state machine remains intentionally deferred because the architecture plan leaves its transition table open. `src/operation/state.rs` is therefore not part of the verified Phase 5 implementation surface.

### Done

Downstream work receives trusted context from Core rather than reconstructing operation metadata from transport boundaries. Engines retain ownership of domain behavior after cancellation or expiration.

---

## Phase 6: Capability System

**Status: verified**

### Goal

Give engines a common way to expose, register, locate, and invoke capabilities.

### Planned implementation

Add capability definitions, registrations, a registry, handlers, and local dispatch.

### What got built

Implemented the public `capability` module and exposed its surface through the prelude. Capability definitions now carry validated metadata (`CapabilityId`, `EngineId`, name, description, `Version`) but not payload schemas; payload structure remains engine owned.

`CapabilityHandler` trait defines the invocation contract: takes `EngineContext` and `CapabilityInvocation`, returns `CapabilityOutcome`. `FunctionHandler<F>` adapter and `arc_handler()` helper allow plain functions to be registered as capability handlers without requiring explicit trait implementation.

`CapabilityRegistry` uses `RwLock<BTreeMap<CapabilityId, CapabilityEntry>>` for thread-safe registration and lookup, mirroring the `ErrorCatalog` pattern. Provides `register`, `unregister`, `get`, `contains`, `len`, `is_empty`, and `iter` operations.

`dispatch()` function takes `EngineContext`, `CapabilityInvocation`, and `&CapabilityRegistry`. Checks cancellation and deadline expiration before invoking handlers, returning `CapabilityDispatchResult` with `Outcome` or `Error` variants (`Unknown`, `Cancelled`, `DeadlineExpired`, `HandlerFailed`, `InvalidDefinition`).

### Files and Folders

**Capability System**

* `src/capability/mod.rs`
* `src/capability/definition.rs`
* `src/capability/registry.rs`
* `src/capability/handler.rs`
* `src/capability/dispatch.rs`

**Related identity**

* `src/identity/capability.rs`

**Public surface**

* `src/prelude.rs`

**Tests**

* `tests/communication/`
* `tests/contracts/`
* `tests/runtime/`

### Boundary

Core provides the capability mechanism only. Capability names, typed requests, workflows, and results remain engine owned.

### Verification

`cargo fmt --all --check`, `cargo test --workspace`, `cargo check --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass from the Core repository root. The suite covers 62 library unit tests (17 in mod.rs, 14 in definition.rs, 19 in registry.rs, 9 in handler.rs, 11 in dispatch.rs) and 27 integration tests in `tests/capability.rs`, including definition validation, registry CRUD, duplicate rejection, dispatch happy path, cancelled context, expired deadline, missing capability, handler failure translation, function adapter, end-to-end pipeline, multi-handler dispatch, dispatch precedence (cancellation/deadline before missing capability), and context metadata propagation.

### Done when

An engine can register a capability and Core can resolve its handler without interpreting its domain semantics.

### Checklist

* [x] Implement capability definition with validation
* [x] Implement handler interface with function adapter
* [x] Implement thread-safe registry with register, unregister, get, iter
* [x] Implement dispatch with cancellation and deadline checks
* [x] Add unit tests for all capability modules
* [x] Add public integration coverage in `tests/capability.rs`
* [x] Verify formatting, tests, compilation, and Clippy

### Done

An engine can register a capability and Core can resolve its handler without interpreting its domain semantics.

---

## Phase 7: Transport and universal client/server

### Goal

Connect the universal contracts and capability system through a shared communication boundary.

### Planned implementation

Add transport, connection, stream, request and response transmission abstractions, then the universal client and engine server surfaces. Typed capability clients build on the universal client rather than creating separate transport stacks.

### Files and Folders

**Transport**

* `src/transport/mod.rs`
* `src/transport/transport.rs`
* `src/transport/connection.rs`
* `src/transport/stream.rs`

**Universal client**

* `src/client/mod.rs`
* `src/client/connection.rs`
* `src/client/universal.rs`

**Engine server**

* `src/server/mod.rs`
* `src/server/engine.rs`

**Related contracts**

* `src/contracts/`

**Related capabilities**

* `src/capability/`

**Tests**

* `tests/communication/`

### Boundary

Concrete transport and serialization choices remain implementation decisions. Engine payload meaning remains outside Core.

### Done when

A typed engine client can send a universal request through an abstract transport to an engine server and receive a universal response.

---

## Phase 8: Engine Runtime

### Goal

Provide the shared lifecycle and request execution infrastructure that every engine can use.

### Planned implementation

Add lifecycle progression from startup through configuration, dependencies, capabilities, registration, readiness, serving, draining, and stop. Integrate the request pipeline, capability dispatch, context, transport, cancellation, deadlines, health, and shutdown.

### Files and Folders

**Runtime**

* `src/runtime/mod.rs`
* `src/runtime/engine.rs`
* `src/runtime/lifecycle.rs`
* `src/runtime/pipeline.rs`
* `src/runtime/concurrency.rs`
* `src/runtime/background.rs`

**Capability dispatch**

* `src/capability/dispatch.rs`
* `src/capability/handler.rs`
* `src/capability/registry.rs`

**Communication**

* `src/client/`
* `src/server/`
* `src/transport/`

**Context**

* `src/operation/`
* `src/security/context.rs`
* `src/provenance/context.rs`

**Tests**

* `tests/runtime/`
* `tests/lifecycle/`

### Boundary

The runtime owns execution infrastructure. Each engine owns the behavior of its capability handlers and internal workflow.

### Done when

A minimal test engine can receive a request, dispatch a capability, and return a response through Core.

---

## Phase 9: Middleware and Security

### Goal

Add the mandatory processing and security boundary around runtime requests.

### Planned implementation

Add the middleware chain for security, tracing, metrics, validation, contract resolution, dispatch, and response processing. Add security context, service identity, authentication and authorization integration points, and security context propagation.

### Files and Folders

**Middleware**

* `src/middleware/mod.rs`
* `src/middleware/chain.rs`
* `src/middleware/stages.rs`

**Security**

* `src/security/mod.rs`
* `src/security/context.rs`
* `src/security/identity.rs`
* `src/security/authentication.rs`
* `src/security/authorization.rs`
* `src/security/middleware.rs`

**Runtime integration**

* `src/runtime/pipeline.rs`
* `src/runtime/engine.rs`

**Tests**

* `tests/security/`
* `tests/runtime/`
* `tests/conformance/`

### Boundary

Core supplies security mechanisms and enforced runtime boundaries. Engines retain capability and domain authorization rules.

### Done when

Every runtime request passes the mandatory processing boundary with trusted context available to its handler.

---

## Phase 10: Artifact and Provenance

### Goal

Make artifacts and execution history referenceable across engines.

### Planned implementation

Add artifact versions, references, access, publication, retrieval, validation, resolution, integrity, and lifecycle mechanisms. Add provenance for operations, attempts, engines, capabilities, messages, sources, versions, and execution.

### Files and Folders

**Artifacts**

* `src/artifacts/mod.rs`
* `src/artifacts/reference.rs`
* `src/artifacts/access.rs`
* `src/artifacts/integrity.rs`
* `src/artifacts/lifecycle.rs`

**Provenance**

* `src/provenance/mod.rs`
* `src/provenance/context.rs`

**Related identity**

* `src/identity/artifact.rs`
* `src/identity/operation.rs`
* `src/identity/engine.rs`
* `src/identity/capability.rs`
* `src/identity/message.rs`

**Tests**

* `tests/conformance/`
* `tests/communication/`

### Boundary

Core manages artifact and provenance mechanisms, never the meaning or content of an engine's artifacts.

### Done when

An engine can exchange artifact references and preserve execution provenance through Core contracts.

---

## Phase 11: Observability, Health, and Configuration

### Goal

Provide operational visibility, readiness, and typed runtime configuration.

### Planned implementation

Add metrics, tracing, correlation, telemetry, diagnostics hooks, liveness, readiness, capability readiness, dependency health, draining state, configuration loading, validation, environment integration, typed access, and runtime propagation.

### Files and Folders

**Observability**

* `src/observability/mod.rs`
* `src/observability/metrics.rs`
* `src/observability/tracing.rs`
* `src/observability/diagnostics.rs`

**Health**

* `src/health/mod.rs`
* `src/health/liveness.rs`
* `src/health/readiness.rs`
* `src/health/dependency.rs`

**Configuration**

* `src/config/mod.rs`
* `src/config/loader.rs`
* `src/config/validation.rs`
* `src/config/environment.rs`

**Runtime integration**

* `src/runtime/`

**Tests**

* `tests/runtime/`
* `tests/conformance/`

### Boundary

Core provides shared protocols and plumbing. Engine specific health checks and configuration schemas remain engine owned.

### Done when

An engine can expose its operating state and receive validated runtime configuration through common interfaces.

---

## Phase 12: Streaming, Concurrency, and Background Tasks

### Goal

Add the heavier runtime mechanisms after the base runtime is established.

### Planned implementation

Add streaming lifecycle, partial and final results, cancellation, and context propagation; bounded and resource aware task execution; and background task registration, startup, cancellation, shutdown, health participation, and resource accounting.

### Files and Folders

**Streaming**

* `src/streaming/mod.rs`
* `src/streaming/lifecycle.rs`
* `src/streaming/messages.rs`

**Runtime concurrency**

* `src/runtime/concurrency.rs`

**Background tasks**

* `src/runtime/background.rs`

**Runtime lifecycle**

* `src/runtime/lifecycle.rs`
* `src/runtime/engine.rs`

**Context integration**

* `src/operation/`
* `src/security/context.rs`
* `src/provenance/context.rs`

**Tests**

* `tests/runtime/`
* `tests/lifecycle/`
* `tests/communication/`

### Boundary

Core provides lifecycle safe mechanisms. Workload strategy and engine specific jobs remain local to each engine.

### Done when

Streaming and background work participate in the same context, lifecycle, health, cancellation, and deadline rules as requests.

---

## Phase 13: Retry and Idempotency

### Goal

Make safe repeat execution available once operation, error, and runtime foundations exist.

### Planned implementation

Add attempt tracking, retry execution, backoff, retryability, deadline and cancellation awareness, idempotency keys, duplicate detection, and safe retry support.

### Files and Folders

**Retry**

* `src/retry/mod.rs`
* `src/retry/policy.rs`
* `src/retry/attempt.rs`
* `src/retry/backoff.rs`

**Idempotency**

* `src/idempotency/mod.rs`
* `src/idempotency/key.rs`

**Related identity**

* `src/identity/operation.rs`
* `src/identity/plan.rs`

**Related status/error**

* `src/status.rs`
* `src/error/`

**Runtime integration**

* `src/runtime/`

**Tests**

* `tests/runtime/`
* `tests/communication/`
* `tests/conformance/`

### Boundary

Core supplies the execution mechanism. Retry policy remains owned by the operation, plan node, or capability that requires it.

### Done when

Core can distinguish duplicate work and execute an authorized retry without losing operation context.

---

## Phase 14: Internal Events

### Goal

Provide a small reusable mechanism for internal engine events.

### Planned implementation

Add event identity, publishing, subscription, scope, delivery, cancellation, and lifecycle support.

### Files and Folders

**Events**

* `src/events/mod.rs`
* `src/events/event.rs`
* `src/events/publisher.rs`
* `src/events/subscriber.rs`

**Context / lifecycle integration**

* `src/operation/`
* `src/runtime/`
* `src/security/context.rs`

**Tests**

* `tests/runtime/`
* `tests/conformance/`

### Boundary

This is optional infrastructure, not a required event driven architecture. Event definitions remain engine owned.

### Done when

An engine can publish and consume scoped internal events through a lifecycle aware Core mechanism.

---

## Phase 15: Control Plane

### Goal

Implement the frozen communication focused Global Platform Core after its required foundations are available.

### Planned implementation

Add request admission, protocol validation, contract, capability, and destination resolution, context propagation, request and response routing, communication lifecycle handling, and communication failure reporting.

### Files and Folders

**Control Plane**

* `src/control_plane/mod.rs`
* `src/control_plane/admission.rs`
* `src/control_plane/contract.rs`
* `src/control_plane/capability.rs`
* `src/control_plane/destination.rs`
* `src/control_plane/routing.rs`
* `src/control_plane/lifecycle.rs`
* `src/control_plane/failure.rs`

**Related communication**

* `src/client/`
* `src/server/`
* `src/transport/`

**Related contracts**

* `src/contracts/`

**Related capabilities**

* `src/capability/`

**Related context**

* `src/operation/`
* `src/security/`
* `src/provenance/`

**Tests**

* `tests/communication/`
* `tests/conformance/`

### Boundary

The Control Plane is not a domain planner, reasoning engine, global workflow executor, model inference engine, or an engine's internal execution layer.

### Done when

It can govern secure, compatible inter engine communication while engines continue to own workflow and domain completion.

---

## Phase 16: Engine SDK

### Goal

Offer engine developers an ergonomic public surface over stable Core mechanisms.

### Planned implementation

Add SDK entry points for engine setup, capabilities, contexts, and the supported runtime and client surfaces.

### Files and Folders

**SDK**

* `src/sdk/mod.rs`
* `src/sdk/engine.rs`
* `src/sdk/capability.rs`
* `src/sdk/context.rs`

**Underlying Core surfaces**

* `src/runtime/`
* `src/capability/`
* `src/client/`
* `src/server/`
* `src/operation/`

**Public surface**

* `src/prelude.rs`

**Tests**

* `tests/conformance/`
* `tests/runtime/`
* `tests/communication/`

### Boundary

The SDK simplifies Core use; it does not create a second runtime or expose Core internal implementation details as a public contract.

### Done when

An engine developer can build against the supported SDK surface without manually composing every internal Core module.

---

## Phase 17: Testing and Conformance hardening

### Goal

Complete the integrated verification and architecture conformance layer after the major runtime surfaces exist.

### Planned implementation

Add shared unit, contract, capability registration, request and response, compatibility, error mapping, context, security, artifact, cancellation, deadline, lifecycle, concurrency, and inter engine tests. Add checks for dependency direction, Core bypasses, generated type leakage, runtime boundaries, and mandatory middleware.

### Files and Folders

**Conformance**

* `src/conformance/mod.rs`
* `src/conformance/architecture.rs`
* `src/conformance/contracts.rs`
* `src/conformance/communication.rs`
* `src/conformance/lifecycle.rs`
* `src/conformance/security.rs`

**Test suites**

* `tests/foundations.rs`
* `tests/context.rs`
* `tests/errors.rs`
* `tests/logging.rs`
* `tests/contracts/`
* `tests/communication/`
* `tests/conformance/`
* `tests/lifecycle/`
* `tests/runtime/`
* `tests/security/`
* `tests/logging/`
* `tests/errors/`

**Cross-system coverage**

* Identity and contracts
* Capability registration and dispatch
* Request / response communication
* Error mapping
* Logging
* Context propagation
* Security
* Artifacts and provenance
* Cancellation and deadlines
* Runtime lifecycle
* Concurrency
* Inter-engine communication
* Middleware enforcement
* Architecture boundaries

### Boundary

Tests are written throughout every phase. This phase finalizes the complete cross system and conformance coverage.

### Done when

Both engine categories can demonstrate use of Core without violating the frozen architectural boundaries.

---

# Architectural Decisions

* Core remains a library crate with no `main.rs`.
* One crate with internal modules is the current implementation choice. Future crate splitting is not authorized without a demonstrated architectural reason.
* Core contains mechanisms and contracts, never Nizaam engine domain semantics.
* The Control Plane will be communication focused and implemented only in Phase 15.
* The Error System is a first class Core system and remains independent from Logging, transport, persistence, and domain payload semantics.
* Error codes use validated namespaces with explicit ownership, so Core and each engine can define distinct error families without collisions.
* Error definitions must be registered in the Error Catalog before an error occurrence can be reported.
* The global error structure is strict and shared, while engines may define their own error codes and messages within that structure.
* Error definitions describe error meaning, while Error Events record runtime occurrences with their execution context.
* Logging uses one structured event contract. Global and local logging are scoped instances of one shared system.
* Logging dispatch is bounded and asynchronous. Debug and info events may be dropped under pressure, while warning, error, and audit events wait for queue capacity.
* Logging consumers implement the shared `LogSink` contract. Core does not select a persistence provider, user interface, or observability vendor.
* Core owns context propagation mechanics, while engines own cancellation and timeout behavior in their domain workflows.
* Cancellation uses parent and child propagation. Child cancellation is isolated from its parent and sibling contexts.
* Engine shutdown uses the same cancellation mechanism as operation and task contexts.
* Deadlines are absolute execution boundaries. A child context inherits the earliest applicable deadline and cannot extend its parent deadline.
* `EngineContext` is the shared composition boundary for operation, cancellation, deadline, security, and provenance context.
* Phase 5 remains provider neutral. It does not select an async runtime, transport, authentication provider, storage system, or serialization format.
* Logging and Error remain independent peer systems. Context infrastructure may reference their contracts but does not own them.
* The Capability System provides the mechanism for capability registration and dispatch. Core does not prescribe typed request/response structures; those remain engine owned.
* Capability handlers are invoked via `CapabilityHandler` trait taking `EngineContext` and `CapabilityInvocation`, returning `CapabilityOutcome`.
* `CapabilityRegistry` uses `RwLock<BTreeMap<CapabilityId, CapabilityEntry>>` for thread-safe registration and lookup, mirroring the `ErrorCatalog` pattern.
* `CapabilityDispatchResult` separates successful `Outcome` from `Error` variants (`Unknown`, `Cancelled`, `DeadlineExpired`, `HandlerFailed`, `InvalidDefinition`).
* The dispatch function checks cancellation and deadline expiration before invoking handlers, ensuring safe execution boundaries.
* `FunctionHandler<F>` adapter and `arc_handler()` helper allow plain functions to be registered as capability handlers without requiring explicit trait implementation.
* Capability definitions carry metadata (`CapabilityId`, `EngineId`, name, description, `Version`) but not payload schemas; those remain engine owned.

# Corrections / Changes

* The Cargo package is `core` and its library target is `nizaam_core`; the existing directory, library root, and private module scaffold remain in place.

# Open Questions

None currently. Concrete trait signatures, provider choices, serialization, async runtime, transport implementation, and eventual crate splitting are deliberately deferred by the plan rather than unresolved architecture.

# Current State

Phase 6, Capability System, is implemented and verified on the `phase-6-capability-system` branch. Capability definitions, registration, thread-safe registry, handler interface with function adapter, and local dispatch with cancellation and deadline checks all pass Core checks. Phase 5, Context execution infrastructure, remains verified. Phase 4, the Logging System foundation, remains verified. Error and Logging remain independent peer systems.

# Next Step

Phase 7: Transport and universal client/server.
