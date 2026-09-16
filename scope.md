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
| 7  | Transport and universal client/server        | 7     | verified    |
| 8  | Engine Runtime                               | 8     | verified    |
| 9  | Middleware and Security                      | 9     | verified    |
| 10 | Artifact and Provenance                      | 10    | verified    |
| 11 | Observability, Health, Configuration         | 11    | in progress |
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

   set -o pipefail
   {
      cargo fmt --all --check &&
      cargo clippy --workspace --all-targets -- -D warnings &&
      cargo build --workspace &&
      cargo check --workspace &&
      cargo test --workspace --all-targets &&
      cargo test --workspace --doc
   } 2>&1 | tee cargo-check.log

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

* `tests/communication.rs`
* `tests/contracts.rs`
* `tests/runtime.rs`

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

**Status: verified**

### Goal

Connect the universal contracts and capability system through a shared communication boundary.

### Planned implementation

Add transport, connection, stream, request and response transmission abstractions, then the universal client and engine server surfaces. Typed capability clients build on the universal client rather than creating separate transport stacks.

### What got built

Implemented the Phase 7 communication boundary across transport, client, server, and capability integration. The implementation includes:

* transport abstractions with `Transport`, `TransportError`, and `TransportResult`;
* connection abstractions with `Connection`, `ConnectionState`, `ClientConnection`, `ClientConnectionFactory`, and client connection state handling;
* binary `MessageHeader` framing with the required fixed 20-byte header and big-endian numeric fields;
* bounded `MessageStream` framing over `ByteSink` and `ByteSource`;
* bounded frame handling with `MAX_FRAME_LENGTH`;
* logical-message fragmentation and reassembly for payloads larger than one transport frame;
* fragment index and final-fragment validation, including rejection of invalid or unexpected frame sequences;
* a provider-neutral `InMemoryTransport` implementation used to exercise the transport contract;
* `UniversalClient<T: Transport>` as the shared client surface over an abstract transport;
* `EngineServer` with handler registration, capability-based request routing, and serving/draining/stopped request admission behavior;
* integration with the existing Phase 6 capability registry/handler/dispatch boundary without introducing a second capability system;
* preservation of universal message identity, operation context, contract metadata, payload descriptors, participants, and engine instance identities across the transport boundary;
* opaque byte payload handling without Core interpreting engine-specific payload semantics; and
* reuse of the same underlying transport by higher-level clients rather than introducing separate transport stacks.

The implementation remains within the Phase 7 boundary: transport owns framing, fragmentation, reassembly, ordering, and transport-level message identification, while logical message semantics and engine-specific payload meaning remain outside the transport layer.

### Message Framing Protocol

The transport layer uses a domain agnostic binary message framing protocol.

A logical request, response, or event is a logical message. A logical message may be transmitted as one or more transport frames.

Transport framing metadata is encoded in binary form. The framing protocol is transport focused and does not interpret engine specific payload semantics.

Each transport frame contains a fixed 20-byte binary header followed by the frame payload:

```text
┌──────────────────────────────────────────────────────┐
│ Version            │ 1 byte                         │
├────────────────────┼─────────────────────────────────┤
│ Flags              │ 1 byte                         │
├────────────────────┼─────────────────────────────────┤
│ Payload Length     │ 4 bytes                        │
├────────────────────┴─────────────────────────────────┤
│ Transport Message ID │ 8 bytes                      │
├──────────────────────┼───────────────────────────────┤
│ Fragment Index       │ 4 bytes                      │
└──────────────────────┴───────────────────────────────┘
│                                                      │
│ Frame Payload                                        │
│                                                      │
└──────────────────────────────────────────────────────┘
```

All multi-byte numeric framing fields use big-endian encoding.

The 20-byte header is transport metadata and is not part of the logical payload.

`Version` identifies the framing protocol version.

`Flags` contains transport framing flags. The final-fragment indicator is represented by a flag.

`Payload Length` stores the size of the payload carried by that individual frame.

`Transport Message ID` identifies the logical message at the transport layer. It is a transport-level identifier and is distinct from the Nizaam `MessageId` contained in the logical message envelope.

`Fragment Index` identifies the zero-based position of the frame within the logical message.

The transport header must remain minimal and transport focused. Information already represented by the Nizaam Universal Contract, Envelope, operation context, contract metadata, or other established Core contracts must not be redundantly duplicated in the transport header unless required by the framing protocol itself.

The frame payload remains opaque bytes. Core must not interpret, modify, or depend on the semantic meaning of engine payloads.

`MAX_FRAME_LENGTH` limits the payload size of an individual transport frame, not the size of a complete logical message.

A logical message whose payload is larger than `MAX_FRAME_LENGTH` MUST be fragmented into multiple transport frames and reassembled by the receiving side.

The transport protocol MUST NOT require a logical message to fit within a single transport frame.

Fragmentation and reassembly MUST:

* preserve the complete logical payload;
* preserve logical message boundaries;
* preserve frame ordering;
* avoid dropping payload data;
* remain independent of engine-specific payload meaning.

Fragments belonging to one logical message MUST be processed in contiguous fragment-index order beginning at fragment index `0`.

The final fragment MUST be identified by the final-fragment flag.

A duplicate, missing, or unexpected fragment index MUST result in a transport/protocol error rather than silently producing an incomplete or corrupted logical message.

Phase 7 does not define retransmission or retry of missing fragments. Recovery and retry mechanisms remain the responsibility of later Core phases.

The transport layer MUST enforce a bounded maximum for a complete reassembled logical message. The exact `MAX_MESSAGE_LENGTH` value and its configuration remain implementation decisions unless explicitly frozen elsewhere in this scope.

The transport layer owns:

* frame construction;
* frame parsing;
* binary framing metadata;
* fragmentation;
* reassembly;
* fragment ordering;
* transport-level message identification.

The transport layer does not own logical message semantics.

If implementation requires a materially different framing model, changes the binary transport requirement, duplicates higher-level contract semantics in the transport header, removes fragmentation or reassembly, or changes any of the guarantees defined above, the agent MUST stop and ask for explicit permission before proceeding.

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

* `tests/communication.rs`

### Boundary

Concrete transport implementation choices remain implementation decisions except for the message framing guarantees defined above.

The transport layer uses binary framing metadata and carries opaque byte payloads. Higher-level message identity, operation context, contract metadata, and payload semantics remain owned by the established Core contracts and by engines.

Engine payload meaning remains outside Core.

### Done when

A typed engine client can send a universal request through an abstract transport to an engine server and receive a universal response.

The transport can transmit both small and large logical messages without losing payload data by using bounded frames and fragmentation/reassembly.

The framing implementation preserves logical message boundaries while keeping transport metadata separate from the logical message contract.

### Verification

Phase 7 was verified through the complete Core test/format/check pipeline and the dedicated communication integration suite. The verification covers the public communication surface and the byte-level transport contract, including:

* [x] universal client to engine server request/response flow;
* [x] abstract `Transport` usage and transport trait-object behavior;
* [x] capability handler invocation through the engine server;
* [x] engine routing to the addressed target;
* [x] rejection of requests before serving and after draining/stopping;
* [x] successful and rejected client connections;
* [x] closed-connection call rejection;
* [x] binary message-header serialization/deserialization and fixed header sizing;
* [x] big-endian framing field encoding;
* [x] transport message and fragment metadata;
* [x] logical-message fragmentation and exact payload reassembly;
* [x] large logical payload round trips;
* [x] opaque binary payload preservation, including non-UTF-8 bytes;
* [x] operation-context and contract metadata preservation;
* [x] sender/target and engine-instance identity preservation; and
* [x] stream/source/sink and connection abstraction behavior.

The dedicated Phase 7 integration suite passes with `33 passed; 0 failed`. The broader Core regression suite also remains green, including the previously verified capability, contract, context, error, foundation, and logging tests.

The full verification command used for the Core repository is:

```text
cargo build --workspace && cargo fmt --all && cargo clippy --all && cargo check --all && cargo test
```

No Phase 7 test failure remains unresolved.

---

## Phase 8: Engine Runtime

### Goal

Provide the shared lifecycle and request execution infrastructure that every
engine can use.

The Engine Runtime coordinates engine lifecycle, request admission, execution,
context propagation, capability dispatch, concurrency, dependency readiness,
and shutdown without owning engine-specific semantics or workflows.

### Planned implementation

Add lifecycle progression from startup through configuration, dependencies,
capabilities, registration, readiness, serving, draining, and stop. Integrate
the request pipeline, capability dispatch, context, transport, cancellation,
deadlines, readiness, concurrency, and shutdown.

The runtime provides execution mechanisms and coordination. Each engine retains
ownership of its capability handlers, domain workflows, domain state, payload
meaning, business rules, and engine-specific synchronization.

---

### Engine Lifecycle

The Engine Runtime owns the legal lifecycle transitions.

The canonical lifecycle is:

```text
STARTING
   ↓
CONFIGURING
   ↓
DEPENDENCIES
   ↓
CAPABILITIES
   ↓
REGISTERING
   ↓
READY
   ↓
SERVING
   ↓
DRAINING
   ↓
STOPPED
````

The lifecycle states have the following meaning:

* `STARTING` establishes the engine runtime instance.
* `CONFIGURING` loads and validates the engine configuration required by the
  engine.
* `DEPENDENCIES` resolves and initializes the dependencies required by the
  engine.
* `CAPABILITIES` initializes the capabilities that the engine intends to
  provide.
* `REGISTERING` publishes the engine and its successfully initialized
  capabilities through the appropriate Core registration mechanisms.
* `READY` means the engine has completed required initialization and is
  eligible to begin serving, but normal request admission has not yet begun.
* `SERVING` means the runtime is actively accepting new normal requests.
* `DRAINING` means no new requests may be admitted while previously admitted
  work is allowed to complete according to its execution context.
* `STOPPED` is the terminal lifecycle state.

Lifecycle transitions are explicit and controlled by the Engine Runtime.

Normal lifecycle states must not be silently skipped. A transition may omit a
stage only when that stage is explicitly not applicable to the engine and the
runtime's lifecycle model permits that omission.

`DRAINING` cannot transition back to `SERVING`.

`STOPPED` is terminal and cannot transition back into an active lifecycle
state.

Capability handlers must not directly control the engine lifecycle.

An engine may participate in lifecycle transitions through the runtime's
supported lifecycle mechanisms, but the runtime owns transition validity.

### Lifecycle Failure

A failure during startup or lifecycle initialization must not leave the engine
in an ambiguous or indefinitely waiting state.

A lifecycle failure is reported through the existing Core error mechanisms and
causes the engine to enter a failed startup condition before reaching
`READY`.

`FAILED` is treated as a lifecycle failure condition rather than a normal
serving lifecycle stage. The engine must not become `SERVING` after an
unresolved startup failure.

After a terminal lifecycle failure, the runtime transitions the engine to
`STOPPED` through the defined shutdown path.

Phase 8 does not implement retry loops for failed startup operations. Retry and
recovery behavior remain part of later Core mechanisms.

### READY and SERVING

`READY` and `SERVING` are intentionally distinct.

`READY` means:

```text
required initialization complete
engine eligible to serve
not yet admitting normal work
```

`SERVING` means:

```text
engine actively admits normal requests
```

The transition from `READY` to `SERVING` is an explicit Engine Runtime
transition.

An engine must not admit normal requests while it remains in `READY`.

This distinction allows readiness to be established before request admission
begins and prevents external work from reaching an engine that has not yet
entered its serving state.

---

### Request Admission

The Engine Runtime controls request admission according to lifecycle state.

Only an engine in `SERVING` state may accept new normal requests.

Requests received while the engine is in `STARTING`, `CONFIGURING`,
`DEPENDENCIES`, `CAPABILITIES`, `REGISTERING`, `READY`, or `STOPPED` state must
not be dispatched to capability handlers.

When the engine enters `DRAINING`, no new requests may be admitted.

Requests that were already admitted before `DRAINING` began remain runtime-owned
work and may continue according to their existing execution context,
cancellation, and deadline rules.

Admission is the point at which the runtime accepts responsibility for a
request's execution.

Lifecycle admission checks must occur before capability handler invocation.

Capability handlers must never be responsible for determining whether the
engine is currently accepting work.

The Control Plane, introduced in a later phase, determines communication
routing and destination behavior. Engine Runtime determines whether the target
engine can accept work once the request reaches that engine.

---

### Startup Responsibilities

Startup establishes the engine before normal serving begins.

The startup sequence is conceptually:

```text
STARTING
   ↓
configuration
   ↓
dependency initialization
   ↓
capability initialization
   ↓
engine/capability registration
   ↓
READY
   ↓
SERVING
```

Startup is establishment and validation.

Serving is execution.

The runtime must not turn startup into a monolithic operation that also owns
engine-specific business workflows.

---

### Request Execution Pipeline

An admitted request follows the Core request execution path:

```text
Transport
   ↓
Frame/message reconstruction
   ↓
Lifecycle admission
   ↓
Universal Request
   ↓
Request structural validation
   ↓
Execution context association
   ↓
Cancellation / deadline checks
   ↓
Capability resolution
   ↓
Capability dispatch
   ↓
Engine capability handler
   ↓
Outcome / Error
   ↓
Universal Response
   ↓
Transport
```

The runtime must not resolve capabilities for requests that have already been
rejected by lifecycle admission.

Request validation must occur before capability handler invocation.

The runtime must preserve the cancellation and deadline precedence already
defined by the Capability System. Cancellation or deadline expiration detected
before handler invocation must prevent the handler from being invoked.

The runtime must not create a second competing context, cancellation, or
deadline mechanism when an existing Core mechanism already provides it.

Message reconstruction is provided by the transport layer and its framing,
fragmentation, and reassembly protocol defined by Phase 7.

---

### Error Handling

Errors occurring before capability execution remain runtime or communication
errors and must not invoke the capability handler.

Where an error occurs during processing of a valid universal request and a
universal response can still be produced, the runtime should represent the
failure through the established Universal Response and Core error/status
mechanisms rather than bypassing the response contract.

The runtime must preserve the distinction between:

* transport or framing failures;
* lifecycle admission failures;
* request validation failures;
* cancellation;
* deadline expiration;
* capability resolution failures;
* capability dispatch failures;
* engine handler failures.

The runtime must not collapse every failure into a generic engine failure when
the existing Core error model can preserve the actual failure category.

Transport-level failures remain transport failures when a valid universal
response cannot be produced.

Capability handlers remain responsible for their own domain-specific failure
meaning.

Retry behavior is not implemented by Phase 8.

---

### Concurrency

The Engine Runtime permits independent admitted requests to execute
concurrently.

Concurrency is a runtime capability, not a requirement that every engine
operation execute in parallel.

Each admitted request receives an isolated request execution context.

Operation, cancellation, deadline, security, and provenance context must not be
shared mutably between unrelated requests.

Core must not impose global serialization on engine capability handlers.

Engine-specific shared state and synchronization remain engine owned.

An engine may serialize, limit, or parallelize its own operations according to
its internal requirements.

Core does not require a specific thread model, executor, async runtime, worker
pool, or scheduler unless explicitly authorized by a later architectural
decision.

Request execution order is not guaranteed for unrelated requests unless a
later Core mechanism or engine-owned workflow establishes an ordering
requirement.

The runtime must not introduce global synchronization that prevents valid
reentrant Core or engine operations.

Cancellation and deadline behavior use the existing Phase 5 context mechanisms.
Core provides the state and propagation mechanism; engines remain responsible
for how their work responds to cancellation or expiration.

Phase 8 does not establish a mandatory global concurrency limit.

Resource-aware bounded concurrency and heavier task scheduling remain deferred
to the later Streaming, Concurrency, and Background Tasks phase.

---

### Shutdown and Draining

A normal shutdown follows:

```text
SERVING
   ↓
DRAINING
   ↓
stop admitting new requests
   ↓
signal runtime-owned background work
   ↓
allow already-admitted work to finish
   ↓
join runtime-owned work
   ↓
STOPPED
```

`DRAINING` is not immediate cancellation.

New requests are rejected once draining begins.

Already-admitted requests remain runtime-owned work and may finish according to
their existing context, cancellation, and deadline rules.

Runtime-owned background work receives the appropriate shutdown signal and is
joined before the runtime reaches `STOPPED`.

Phase 8 does not define an elaborate configurable shutdown grace-period policy.

A later mechanism may explicitly cancel remaining work when a shutdown policy
requires it, but the basic lifecycle contract remains:

```text
drain first
then stop
```

`STOPPED` is terminal.

---

### Readiness

Phase 8 establishes the runtime concept of readiness but does not implement the
complete health subsystem planned for Phase 11.

`READY` means the engine has completed all required initialization necessary to
be eligible for serving.

Detailed health concerns such as:

* liveness;
* dependency health;
* capability readiness;
* operational diagnostics;

remain part of the later Health and Observability phase.

Phase 8 consumes readiness as an execution boundary rather than becoming the
complete health system.

---

### Dependencies

Engine dependencies are evaluated during the startup lifecycle before the
engine can become `READY`.

A dependency may be declared required or optional by the engine.

A required dependency must be available and successfully initialized before
the engine may reach `READY`.

An optional dependency must not prevent the engine from reaching `READY` when
its absence does not prevent the engine from providing its required behavior.

Dependency semantics remain engine owned. Core Runtime manages dependency
lifecycle participation and readiness, but does not interpret why a particular
engine requires a particular dependency.

A required dependency initialization failure prevents readiness.

An optional dependency failure may leave the engine `READY` while the affected
engine-owned capability remains unavailable or degraded according to engine
logic.

Phase 8 does not introduce automatic retry loops for dependency initialization.
Retry and recovery remain later concerns.

Dependency cycles must not result in indefinite startup waiting. A dependency
cycle must be detected and reported as a startup failure.

If a dependency becomes unavailable after the engine is already serving, the
engine is not automatically forced to `STOPPED` by Core Runtime solely because
of that loss.

Runtime and later health mechanisms expose the dependency state, while the
engine determines how its affected capabilities respond.

Core must not automatically reject every request to an engine merely because
one dependency is unavailable unless the dependency state makes the engine
itself unable to satisfy the runtime contract.

---

### Capability Visibility

Capability registration and capability availability are distinct concepts.

```text
Registered
= capability exists in the engine's registered capability set

Available
= engine is in SERVING state and the capability may accept normal work
```

Capabilities may be initialized and registered during the startup lifecycle,
but normal external invocation must not occur until the engine reaches
`SERVING`.

The runtime must not expose a capability as normally callable while its engine
is still starting.

Whether an engine capability is required for engine readiness is engine-owned
metadata.

If a required capability fails initialization, the engine cannot become
`READY`.

If an optional capability fails initialization, the engine may become `READY`
provided its required behavior remains valid.

The Core Runtime does not interpret the domain meaning of required or optional
capabilities.

---

### Engine Registration

Engine registration is the runtime's mechanism for making the engine
available to the Core communication/runtime infrastructure.

Engine registration is distinct from capability registration.

Capability registration establishes the capabilities provided by the engine.

Engine registration establishes that the engine itself has completed the
required startup stages and can participate in the shared runtime.

The runtime must not introduce an independent global routing or destination
system merely to perform Phase 8 engine registration.

Detailed destination resolution and inter-engine routing remain part of the
later Control Plane phase.

An engine must complete required registration before it can reach `READY`.

External request admission remains blocked until the engine transitions from
`READY` to `SERVING`.

---

### Dependency Between Runtime and Engine Responsibilities

The Core Runtime owns coordination and shared mechanisms:

```text
Core Runtime
├── lifecycle
├── lifecycle transition validity
├── request admission
├── request execution coordination
├── context propagation
├── cancellation/deadline integration
├── capability resolution and dispatch
├── concurrency mechanism
├── dependency lifecycle participation
├── readiness boundary
└── shutdown coordination
```

The engine owns semantic behavior:

```text
Engine
├── domain state
├── capability implementation
├── domain workflows
├── business rules
├── payload semantics
├── domain-specific synchronization
├── domain-specific storage
└── capability-specific response meaning
```

The runtime must never directly manipulate engine domain state.

The runtime may invoke an engine-owned capability handler, but it must not
implement or interpret that handler's domain workflow.

For example:

```text
Runtime
   ↓
Capability Handler
   ↓
Engine-owned logic
```

not:

```text
Runtime
   ↓
Engine database
   ↓
modify domain data
```

---

### Background Tasks

Phase 8 establishes basic ownership and lifecycle integration for runtime-owned
background tasks.

Runtime-owned background tasks participate in engine shutdown.

During draining/shutdown:

```text
Engine Runtime
    ↓
signal owned background tasks
    ↓
allow shutdown
    ↓
join owned tasks
```

Phase 8 does not define the complete resource-aware background-task scheduler,
advanced task accounting, or sophisticated execution policy. Those remain part
of Phase 12.

---

### Phase 8 Explicit Non-Goals

Phase 8 must not silently implement functionality belonging to later phases.

The following remain deferred:

* retry policy;
* idempotency;
* advanced streaming;
* security middleware and authorization policy;
* artifact persistence;
* observability and detailed health systems;
* Internal Events;
* Control Plane routing;
* Engine SDK;
* domain workflows;
* domain-specific business logic;
* engine-specific storage;
* a specific async runtime or executor;
* a mandatory global concurrency policy.

Existing Phase 8 behavior must not be changed merely to make implementation of
a later phase more convenient.

---

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

* `tests/runtime.rs`
* `tests/lifecycle.rs`

### Boundary

The Engine Runtime owns shared engine lifecycle, request admission, execution
coordination, context propagation, capability dispatch, dependency lifecycle
participation, readiness, concurrency mechanisms, and shutdown coordination.

Engines retain ownership of domain state, capability semantics, workflows,
business rules, payload meaning, domain-specific synchronization, and
engine-specific storage.

The runtime must not become a workflow engine, domain planner, reasoning
engine, model inference system, or domain authorization policy engine.

The Control Plane remains a later communication-focused system and is not
implemented as part of the Phase 8 runtime.

### Done when

A minimal test engine can:

1. start through the defined lifecycle;
2. initialize required dependencies and capabilities;
3. register itself and its capabilities;
4. reach `READY`;
5. transition explicitly to `SERVING`;
6. accept a universal request only while serving;
7. construct or propagate the appropriate execution context;
8. validate and dispatch the request to the correct capability;
9. execute the engine-owned handler;
10. produce a universal response;
11. support independent admitted requests concurrently;
12. reject new work during `DRAINING`;
13. allow already-admitted work to complete according to its context;
14. shut down owned work cleanly and reach `STOPPED`;
15. preserve all previously verified Core phase behavior.

The complete runtime lifecycle, request pipeline, dependency behavior,
capability visibility, concurrency behavior, and shutdown semantics are covered
by unit and integration tests.

This incorporates the full discussion rather than just the original Phase 8 description. The original scope establishes Phase 8 as the lifecycle/request-execution integration point, while our discussion fills in the missing behavioral contracts without pulling later-phase responsibilities forward. :contentReference[oaicite:1]{index=1}

One deliberate choice here is that **`FAILED` is a failure condition, not another normal lifecycle stage**. That keeps your actual state machine clean:

```text
STARTING
   ↓
CONFIGURING
   ↓
DEPENDENCIES
   ↓
CAPABILITIES
   ↓
REGISTERING
   ↓
READY
   ↓
SERVING
   ↓
DRAINING
   ↓
STOPPED
```

with failure able to occur from the startup states and ultimately lead to `STOPPED`.

---

### Phase 8 Completion Record

The following completion record is appended to the original Phase 8 specification. Existing Phase 8 requirements, boundaries, non-goals, and decisions above remain unchanged.

### Done When

* [x] Start a runtime through the defined lifecycle.
* [x] Progress through `STARTING`, `CONFIGURING`, `DEPENDENCIES`, `CAPABILITIES`, `REGISTERING`, `READY`, `SERVING`, `DRAINING`, and `STOPPED` according to the legal lifecycle model.
* [x] Keep `READY` distinct from `SERVING` so readiness does not itself admit normal work.
* [x] Reject normal work outside `SERVING` and prevent capability dispatch after lifecycle admission rejection.
* [x] Preserve the request execution boundary using universal request validation, execution context association, cancellation/deadline checks, capability resolution, dispatch, handler execution, and universal response construction.
* [x] Preserve cancellation-before-deadline precedence and prevent handler execution when the request context is already cancelled or expired.
* [x] Propagate `EngineContext` through runtime execution and capability dispatch without introducing a competing context mechanism.
* [x] Integrate the existing Capability System for capability registration, lookup, and dispatch without introducing a second capability mechanism.
* [x] Allow independent admitted work to execute concurrently with isolated task scopes and without a mandatory global concurrency limit.
* [x] Provide runtime-owned background task lifecycle integration with cancellation and joining during shutdown.
* [x] Implement draining semantics so new work is stopped while already-admitted work can complete according to its execution context.
* [x] Ensure runtime-owned background work is signalled and joined before the runtime reaches `STOPPED`.
* [x] Coordinate concurrent shutdown callers and protect shutdown completion from lifecycle races.
* [x] Handle runtime-owned reentrant shutdown without deadlocking the shutdown/join path.
* [x] Preserve all previously verified Core phase behavior.

Phase 8 completion is supported by unit and integration coverage across lifecycle, runtime context, pipeline, task scopes, background task ownership, capability dispatch, shutdown coordination, concurrent shutdown, reentrant shutdown, request routing, and regression behavior.

### Architectural Decisions

* The Engine Runtime remains the owner of lifecycle validity, request admission, execution coordination, context propagation, capability dispatch, concurrency mechanisms, readiness boundaries, dependency lifecycle participation, and shutdown coordination. Engine domain state, workflows, business rules, payload semantics, storage, and domain-specific synchronization remain engine-owned.
* `FAILED` remains a lifecycle failure condition rather than a normal lifecycle state.
* `READY` and `SERVING` remain separate states. Reaching `READY` does not begin normal request admission.
* `DRAINING` is a shutdown/drain state, not immediate cancellation. New normal work is rejected while already-admitted work remains eligible to finish according to its context.
* `STOPPED` remains terminal and is reached only through the runtime shutdown/cleanup path.
* Runtime shutdown is coordinated as a complete transaction so concurrent callers cannot race lifecycle transitions or observe an intermediate shutdown state as completed.
* Runtime-owned background tasks are cancelled/signalled and joined before shutdown completion. Reentrant shutdown from a runtime-owned task must not deadlock against the join path.
* Core continues to use the existing `EngineContext`, cancellation, deadline, capability registry, and dispatch mechanisms rather than introducing competing implementations.
* Core does not commit Phase 8 to a specific async runtime, executor, worker pool, scheduler, or mandatory global concurrency policy.
* `InvalidTransition` is owned by the Nizaam Error System and is exposed from `nizaam_core::error`; implementing Rust's standard `std::error::Error` interoperability does not transfer ownership to Rust's standard library error model.
* Phase 7 `EngineServer` remains a communication boundary and is not merged into the Phase 8 Engine Runtime lifecycle state machine.
* Capability registration/dispatch remains distinct from engine registration/lifecycle ownership. Control Plane routing remains deferred to Phase 15.
* Capability request routing tests must derive the invocation target from the request descriptor so that a mismatch between the request and registry cannot accidentally be hidden by constructing the invocation from a registry lookup.

### Open Questions

None currently for Phase 8.

Production transport, async runtime/executor, advanced resource-aware concurrency, sophisticated background scheduling, retry/recovery, middleware/security policy, observability/health, Control Plane routing, and Engine SDK design remain intentionally deferred to their respective phases rather than being unresolved Phase 8 architecture.

### Corrections / Changes

* Added the Nizaam-owned technical `InvalidTransition` error to the Error System and moved lifecycle transition errors away from runtime-owned error definitions without coupling the Error System back to the runtime lifecycle.
* Updated lifecycle/runtime tests to use the Error System's `InvalidTransition` representation and preserve the legal lifecycle transition contract.
* Corrected shutdown behavior so runtime-owned background cleanup is not bypassed when the runtime has already reached `STOPPED`.
* Added per-runtime shutdown coordination so concurrent shutdown callers cannot race `SERVING`/`DRAINING`/`STOPPED` transitions or repeat cleanup.
* Added explicit shutdown completion coordination so shutdown completion is not reported before runtime-owned background work has been joined.
* Corrected the reentrant background-task shutdown path so a runtime-owned task cannot deadlock by attempting to reacquire shutdown coordination while the shutdown caller is joining owned work.
* Added regression coverage for concurrent shutdown callers waiting for cleanup completion and for reentrant shutdown from runtime-owned background work.
* Corrected the runtime integration capability-routing test so the invocation capability is derived from the request descriptor and distinct capability IDs/handlers can expose routing mismatches.
* Preserved the existing Phase 5, Phase 6, and Phase 7 boundaries while integrating their context, capability, and communication mechanisms into the Phase 8 runtime foundation.

### Current State

Phase 8, Engine Runtime, is implemented and verified. The runtime foundation now provides the shared lifecycle owner, lifecycle admission boundary, execution context integration, cancellation/deadline checks, execution pipeline, capability dispatch integration, concurrent task scopes, runtime-owned background task lifecycle, and coordinated shutdown behavior. The lifecycle remains explicit from `Created` through the startup stages to `READY`, `SERVING`, `DRAINING`, and terminal `STOPPED`. The implementation preserves the separation between Core runtime mechanisms and engine-owned semantics.

The Phase 8 work also includes shutdown correctness for concurrent callers and runtime-owned reentrant callers, with cleanup completion coordinated before `STOPPED` is reported. Capability routing integration tests exercise request-derived capability selection rather than masking routing mismatches with registry-derived invocation data.

Phase 8 has been reviewed against automated CodeRabbit and Greptile findings. Genuine correctness and test-quality findings were fixed, including shutdown cleanup, concurrent shutdown coordination, reentrant shutdown behavior, lifecycle transition protection, and capability routing coverage. Additional speculative edge-case hardening is intentionally not being pursued without a concrete engine requirement or demonstrated correctness issue.

The complete Core verification was run successfully before merge preparation, with the workspace build/check/test/lint/format pipeline passing and all unit and integration suites green, including the Phase 8 runtime and lifecycle suites.

### Next Step

Phase 8 is complete and ready to remain merged as the shared runtime foundation.

Proceed to **Phase 9: Middleware and Security**, while preserving the Phase 8 lifecycle, admission, context, capability-dispatch, concurrency, and shutdown boundaries. Any future Phase 8 change should be driven by a concrete requirement or an actual correctness issue discovered while implementing or operating later phases, rather than by pursuing theoretical completeness.

---

## Phase 9: Middleware and Security

### Goal

Add the mandatory processing and security boundary around runtime requests.

Phase 9 provides Core-controlled middleware, service identity, authentication,
authorization, security context propagation, and cross-cutting request/response
processing.

Middleware remains a cross-cutting mechanism. It must not own engine domain
work, domain workflows, engine-specific payload semantics, or domain business
rules.

### Planned implementation

Add the mandatory middleware chain for security, tracing, metrics, validation,
contract validation and compatibility, capability identification and
resolution, dispatch integration, and response processing.

Add security identity, authentication, authorization, security middleware, and
trusted security context propagation across Core-supported engine boundaries.

Build on the existing Phase 5 `EngineContext`, Phase 6 capability system, and
Phase 8 Engine Runtime rather than replacing or duplicating their mechanisms.

---

### Middleware Definition

Middleware is a Core-controlled processing stage that may inspect, validate,
enrich, observe, authorize, reject, or pass a request to the next processing
stage without owning the request's domain work.

Middleware is a cross-cutting mechanism.

Middleware is distinct from:

```text
Middleware
≠ Capability
≠ Capability Handler
≠ Engine Runtime
≠ Engine Domain Workflow
````

Examples of middleware concerns include:

```text
security
authentication
authorization
tracing
metrics
request validation
contract validation
compatibility checks
response processing
```

Domain operations such as:

```text
Quran lookup
Hadith search
Arabic grammar analysis
Knowledge Graph reasoning
Fiqh workflows
```

remain engine-owned and must not be implemented by Core middleware.

---

### Request and Response Middleware

Middleware may operate on incoming requests, outgoing responses, or both.

Conceptually:

```text
                    REQUEST
                       ↓
              Request Middleware
                       ↓
                Capability
                       ↓
                  Handler
                       ↓
             Response Middleware
                       ↓
                    RESPONSE
```

A request-only middleware performs work before capability execution.

A response-only middleware performs work after capability execution.

A middleware that needs both request and response processing may establish
request state before execution and finalize or observe the result afterward.

For example:

```text
Tracing
→ establish trace context
→ observe execution
→ record response outcome
```

while:

```text
Validation
→ validate request
→ continue or reject
```

---

### Mandatory Middleware

Every externally admitted runtime request MUST pass through the configured
mandatory middleware chain.

A capability handler MUST NOT be directly invoked through the normal runtime
path in a way that bypasses mandatory middleware.

The normal runtime path must not provide an application-level shortcut around
security, validation, context propagation, or other mandatory processing.

Explicit internal Core operations may use narrower internal mechanisms when
such paths are explicitly defined by the architecture. Such internal paths
must not become an externally exposed mechanism for bypassing mandatory
security or runtime processing.

---

### Middleware Ordering

Middleware ordering MUST be explicit and deterministic.

The normal request path is conceptually:

```text
Incoming Request
      ↓
Engine Runtime Admission
      ↓
Authentication / Identity
      ↓
Trusted Security Context
      ↓
Request Validation
      ↓
Contract Validation / Compatibility
      ↓
Capability Identification
      ↓
Generic Core Authorization
      ↓
Capability Resolution
      ↓
Capability / Domain Authorization
      ↓
Capability Dispatch
      ↓
Engine Handler
      ↓
Response Processing
      ↓
Transport
```

Tracing, metrics, and other observability middleware participate in the same
processing boundary. Their exact placement relative to one another may remain
an implementation detail provided that security and mandatory processing
guarantees are not weakened.

Security checks MUST occur before final capability resolution and handler
invocation.

Capability identification and capability resolution are distinct:

```text
Capability Identification
= determine which capability the request asks for

Capability Resolution
= resolve the executable capability/handler associated with that request
```

The runtime may identify the requested capability before generic authorization
because generic authorization may need the requested capability as an input.
Actual handler resolution must not expose or execute unauthorized capability
behavior.

---

### Middleware Result

A middleware stage may conceptually produce one of three outcomes:

```text
Continue
Reject
Fail
```

```text
Continue
→ processing proceeds to the next stage

Reject
→ downstream processing is not executed and a rejection response is produced
  when the communication contract permits it

Fail
→ processing terminates and the failure is handled through the established
  Core error/response mechanisms
```

A middleware rejection or failure MUST prevent downstream capability handler
execution.

---

### Middleware and Context

Middleware receives the same trusted `EngineContext` used by the runtime and
capability execution path.

Request-specific security, operation, cancellation, deadline, and provenance
state must remain request-scoped.

Unrelated concurrent requests must not share mutable request-specific context.

For example:

```text
Request A → EngineContext A
Request B → EngineContext B
```

not:

```text
global current request
global current user
global mutable request security state
```

Middleware may maintain shared operational state only through appropriate
synchronization and ownership mechanisms.

---

### Authentication, Identity, and Authorization

These are distinct concepts.

```text
Identity
= representation of an actor, service, engine, or principal

Authentication
= establishing and validating the identity of the caller

Authorization
= determining whether the authenticated identity is allowed to perform an
  operation
```

Authentication answers:

```text
Who is making this request?
```

Authorization answers:

```text
Is this identity allowed to perform this operation?
```

The security model must support both:

```text
human / user principals
service / engine principals
```

A request does not have to originate from a human user.

The architecture MUST remain provider neutral.

Phase 9 does not select or require a specific authentication provider,
credential mechanism, or external identity provider.

Examples such as JWT, OAuth, API keys, mTLS, or OIDC remain implementation
choices unless explicitly authorized by a later architectural decision.

---

### Security Identity

The security identity boundary represents a trusted principal or service
identity after authentication.

Conceptually:

```text
Principal Identity
├── principal type
├── principal identifier
└── trusted identity metadata
```

The exact structure remains an implementation decision provided that it
preserves the distinction between identities and authentication state.

Authentication establishes trusted identity.

Authorization consumes trusted identity and the appropriate request/security
context to make an allow/deny decision.

---

### Security Context

Phase 9 builds on the security context mechanisms established by Phase 5.

The request flow is:

```text
Authenticated Request
       ↓
Trusted Security Identity
       ↓
Security Context
       ↓
EngineContext
       ↓
Middleware / Capability Handler
```

Handlers MUST NOT reconstruct authentication state from raw transport metadata
when a trusted Core security context is already available.

Core must provide trusted security information through the established context
boundary rather than requiring engines to repeatedly interpret authentication
metadata themselves.

---

### Authorization Ownership

Authorization is divided into two layers.

#### Generic Core Authorization

Core may provide authorization mechanisms that evaluate generic security
information such as:

```text
principal identity
requested capability
calling service / engine identity
generic security scopes
generic resource or tenant identifiers
other explicitly defined security metadata
```

The generic Core authorization layer determines whether the caller is permitted
to access or invoke the requested capability under the generic security rules.

#### Engine / Domain Authorization

Engines retain ownership of authorization decisions that require knowledge of
engine-specific semantics or interpretation of engine-owned payloads.

For example:

```text
Core
→ "Is this principal allowed to invoke capability X?"

Engine
→ "Given the semantics of this capability and its payload, is this specific
   operation permitted?"
```

Core MUST NOT inspect engine-specific payload meaning merely to perform
authorization.

Core must not learn domain semantics such as the meaning of Quran-specific,
Hadith-specific, Arabic-specific, Fiqh-specific, or other engine-owned fields
merely to implement generic authorization.

Generic security metadata may be used by Core only when that metadata has been
defined as a Core security concept rather than as an engine domain concept.

---

### Authorization Processing

The authorization flow is:

```text
Request
   ↓
Authentication
   ↓
Trusted Principal / Service Identity
   ↓
Request Validation
   ↓
Capability Identification
   ↓
Generic Core Authorization
   ↓
Capability Resolution
   ↓
Capability / Domain Authorization
   ↓
Capability Dispatch
   ↓
Engine Handler
```

Generic authorization must not require fully resolving an executable handler
before the authorization decision.

A failed authorization check MUST stop downstream processing.

An unauthorized request MUST NOT invoke the capability handler.

When the communication contract remains usable, authorization failures should
be represented through the existing universal response and Core error/status
mechanisms.

An authorization failure is not automatically a transport failure.

A transport failure remains a transport failure when the transport itself is
unable to carry or produce the required response.

---

### Security Context Across Engine Boundaries

Trusted security context MUST propagate across Core-supported engine boundaries
without being silently lost or downgraded.

For example:

```text
External Caller
      ↓
Quran Engine
      ↓
Arabic Engine
```

The downstream security context should preserve the relevant trusted
relationship from the original request.

The security context must be capable of representing, where applicable:

```text
Original Principal
Calling Service / Engine Identity
```

These identities MUST remain distinguishable when both are relevant.

An engine calling another engine must not silently replace the original
principal with its own identity.

Likewise, a downstream engine must not automatically assume that a calling
engine is authorized to perform every operation that the original principal
could perform.

---

### Delegation and Impersonation

Engine-to-engine calls MUST NOT implicitly create identity impersonation.

If delegated identity, impersonation, or acting-on-behalf-of behavior is
introduced later, it must be an explicit security mechanism with explicit
authorization semantics.

The existence of an original principal in the security context does not by
itself grant a downstream engine unlimited authority to act as that principal.

---

### Security Failure

Authentication and authorization failures are security processing failures.

The normal behavior is:

```text
Security Failure
      ↓
Stop processing
      ↓
No capability resolution for execution
      ↓
No capability dispatch
      ↓
No engine handler
```

When possible, the runtime produces a universal response carrying the
appropriate Core error/status information.

Security failures must remain distinguishable from transport failures,
capability failures, and engine handler failures.

---

### Tracing

Tracing middleware provides request execution tracing across the Core runtime
and engine boundaries.

Conceptually:

```text
Request
   ↓
Trace Context
   ↓
Engine A
   ↓
Engine B
```

Tracing may establish or propagate trace information and observe request and
response execution.

Phase 9 establishes middleware-level tracing integration points.

The complete observability and telemetry system belongs to Phase 11.

---

### Metrics

Metrics middleware may record generic operational measurements such as:

```text
request count
success / failure count
latency
```

Metrics must remain generic and cross-cutting.

Phase 9 does not become the complete observability platform.

Detailed telemetry, diagnostics, metric systems, and operational observability
remain part of Phase 11.

---

### Middleware and Concurrency

Middleware must support concurrent request execution established by Phase 8.

Request-specific middleware state must remain isolated between unrelated
requests.

Core must not introduce global mutable security or middleware state that makes
concurrent requests interfere with one another.

Shared middleware state is permitted only where it is explicitly synchronized
and does not violate request isolation.

---

### Middleware and Cancellation / Deadlines

Middleware uses the existing `EngineContext` and therefore receives the
request's cancellation and deadline state.

Middleware may observe cancellation and deadline expiration.

If a request is already cancelled or expired, middleware must not unnecessarily
continue into expensive downstream processing when the existing Core contract
requires the request to terminate.

Phase 9 does not replace the Phase 5 cancellation or deadline mechanisms.

---

### Phase 9 and Phase 11 Boundary

Phase 9 provides:

```text
mandatory middleware chain
security mechanisms
authentication boundary
authorization boundary
security context propagation
basic tracing middleware integration
basic metrics middleware integration
```

Phase 11 provides:

```text
full observability
telemetry
diagnostics
health
liveness
readiness
dependency health
configuration
```

Phase 9 must not silently implement the complete observability or health
subsystems before their dedicated phase.

---

### Explicit Non-Goals

Phase 9 must not implement:

* domain authorization rules;
* Quran, Hadith, Arabic, Fiqh, or other domain-specific policies;
* engine-specific payload interpretation for generic Core authorization;
* a specific authentication provider;
* an external identity provider;
* implicit identity impersonation;
* Control Plane routing;
* retry policy;
* idempotency;
* artifact persistence or artifact security storage;
* the complete observability platform;
* the complete health subsystem;
* domain workflows;
* engine-specific business logic.

Future-phase functionality must not be pulled into Phase 9 merely because it
would make implementation more convenient.

---

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

**Related systems**

* `src/operation/`
* `src/provenance/`
* `src/capability/`
* `src/contracts/`
* `src/client/`
* `src/server/`
* `src/transport/`

**Tests**

* `tests/security.rs`
* `tests/runtime.rs`
* `tests/conformance.rs`

---

### Boundary

Core provides shared middleware and security mechanisms.

Core controls the mandatory runtime processing boundary, trusted identity
representation, authentication integration boundary, generic authorization
mechanism, security context propagation, and cross-cutting middleware
behavior.

Engines retain ownership of:

```text
domain semantics
domain workflows
domain authorization rules
engine-specific payload meaning
engine-specific business rules
engine-specific state
engine-specific synchronization
engine-specific storage
```

Core must not become a domain policy engine merely because authorization can
inspect generic security metadata.

The Control Plane remains a later communication-focused system and is not
implemented as part of Phase 9.

---

### Done when

A runtime request:

1. enters through the mandatory middleware boundary;
2. receives trusted security identity/context when authentication succeeds;
3. passes the deterministic middleware processing path;
4. can be rejected by security or validation middleware before handler
   execution;
5. distinguishes capability identification from capability resolution;
6. undergoes generic Core authorization before executable capability resolution;
7. can undergo engine/domain authorization without Core interpreting domain
   payload semantics;
8. reaches the capability handler only after all mandatory processing succeeds;
9. propagates trusted security context across supported engine boundaries;
10. preserves original principal and calling service identity when both are
    relevant;
11. supports concurrent request processing without sharing mutable
    request-specific security state;
12. produces an appropriate universal response for failures whenever the
    communication contract remains usable;
13. preserves the distinction between security failures and transport,
    capability, and handler failures;
14. provides basic tracing and metrics middleware integration;
15. preserves all previously verified Core behavior.

Unit tests and integration tests must cover middleware ordering, mandatory
middleware enforcement, authentication and authorization behavior, security
context propagation, identity separation, rejection/failure handling,
concurrent request isolation, and integration with the Phase 8 runtime.

* [x] Implement mandatory request middleware enforcement.
* [x] Integrate mandatory middleware into the Engine Runtime request boundary.
* [x] Preserve deterministic request/response middleware ordering.
* [x] Stop downstream execution on middleware rejection or failure.
* [x] Support provider-neutral user, service, and engine identities.
* [x] Establish trusted `SecurityContext` after successful authentication.
* [x] Preserve calling-service identity where applicable.
* [x] Propagate security context through child execution contexts.
* [x] Perform capability identification separately from executable capability resolution.
* [x] Perform generic Core authorization before executable capability resolution.
* [x] Prevent unauthorized requests from reaching capability handlers.
* [x] Preserve request-scoped context isolation for concurrent requests.
* [x] Preserve cancellation and deadline precedence after middleware processing.
* [x] Prevent capability identity mutation between authorization and dispatch.
* [x] Produce universal failure responses when the communication contract remains usable.
* [x] Preserve distinctions between security, pipeline, transport, capability, and handler failures.
* [x] Add basic tracing and metrics middleware integration points.
* [x] Redact authentication credentials from debug output.
* [x] Log pipeline failures through the existing Core Logging System without credential leakage.
* [x] Add or update unit, integration, and conformance coverage.
* [x] Preserve previously verified Phase 0–8 behavior.
* [x] Complete implementation review and resolve genuine findings before merge.

---

## Phase 10: Artifact and Provenance

### Goal

Provide the shared Core mechanisms for artifact identity, versioning, references,
validation, integrity, publication, retrieval, resolution, lifecycle, access,
and provenance.

Core manages artifact mechanisms and historical relationships. Engines and
platform components retain ownership of artifact meaning, domain semantics,
domain-specific validation, creation logic, transformation logic, and
engine-specific content interpretation.

### Planned implementation

Add artifact and provenance infrastructure that allows engines and Core
operations to create, reference, version, validate, publish, resolve, retrieve,
and track artifacts without requiring Core to understand the domain semantics
of those artifacts.

Artifact content remains provider-neutral and must not require the complete
artifact to be loaded into Core memory.

---

### Artifact Definition

An `Artifact` is a uniquely identifiable logical piece of content or produced
output that can be referenced, versioned, validated, accessed, and associated
with provenance.

An artifact is not necessarily a file or a physical storage object.

Examples may include:

```text
dataset
trained model
embedding index
compiled engine package
generated document
evaluation result
configuration bundle
binary
large generated output
````

These examples are illustrative. Core must not interpret their domain meaning.

The conceptual boundary is:

```text
Artifact
= identifiable managed logical content/output

Core
= artifact mechanism and lifecycle infrastructure

Engine
= artifact meaning and domain semantics
```

---

### Logical Artifact vs Physical Content

An artifact is a logical object and is not required to directly contain its
physical content bytes.

Conceptually:

```text
Artifact
   ↓
ArtifactVersion
   ↓
ContentReference
   ↓
physical representation / content provider
   ↓
actual bytes
```

The physical representation may reside in:

```text
local storage
database
object storage
distributed storage
remote service
other provider implementations
```

The concrete storage provider remains an implementation detail.

Core must not become an object-storage implementation merely because it provides
artifact lifecycle and access mechanisms.

---

### Artifact, ArtifactVersion, and ArtifactReference

These are distinct concepts and MUST NOT be collapsed into one type.

#### Artifact

An `Artifact` provides the stable logical identity of the managed artifact.

Conceptually:

```text
Artifact
└── ArtifactId
```

The same `ArtifactId` remains stable across versions.

#### ArtifactVersion

An `ArtifactVersion` represents one exact immutable logical content state.

Conceptually:

```text
ArtifactVersion
├── ArtifactId
├── Version
├── ContentReference
├── ContentDigest
├── ContentSize
├── metadata
└── lifecycle state
```

For one artifact:

```text
Artifact A
├── v1
├── v2
└── v3
```

Each version may coexist with the others and represents a distinct immutable
state.

#### ArtifactReference

An `ArtifactReference` is a lightweight reference or selector used by Core
objects, operations, messages, engines, or provenance records.

Conceptually:

```text
ArtifactReference
├── ArtifactId
└── version selector
```

A selector may identify:

```text
exact version
mutable alias
```

The exact URI, string, binary, or other wire representation remains an
implementation choice.

An `ArtifactReference` is not the artifact itself.

---

### Version Identity

Each version identifier must uniquely identify one immutable version within its
artifact.

The scope does not require a specific version-numbering scheme.

Core must not assume that version identifiers necessarily imply ordering or
semantic version relationships.

In particular:

```text
version identity
≠
version ordering
```

The architecture does not require Semantic Versioning unless explicitly
authorized later.

---

### Immutability

A published artifact version is immutable.

Its:

```text
content
content identity
version identity
integrity information
```

must not silently change.

If content changes, a new version must be created.

For example:

```text
A:v5
   ↓
new content
   ↓
A:v6
```

A published version must never silently become a different content state.

Operational or descriptive metadata may have controlled updates where permitted,
provided that those updates do not change the meaning or integrity identity of
the version's content.

Changing descriptive metadata such as a display label or description must not
alter the content digest when that metadata is not part of the content being
integrity-verified.

---

### Physical Representations

One logical artifact version may have multiple physical representations without
requiring multiple logical artifact versions.

For example:

```text
Artifact: quran-dataset
Version: v5

Logical content
├── compressed representation
├── uncompressed representation
└── cached representation
```

An `ArtifactVersion` represents one logical content state while its physical
storage representation may vary.

The content reference identifies the representation used by the retrieval
contract.

Core must not treat different storage encodings as different logical artifact
versions solely because the physical representation differs.

---

### Integrity

Artifact integrity provides a mechanism for verifying that retrieved content
matches the content recorded for the artifact version.

Conceptually:

```text
content bytes
     ↓
content digest
```

During retrieval:

```text
retrieved content
      ↓
calculate digest
      ↓
compare with recorded integrity information
      ↓
match / mismatch
```

A mismatch results in an integrity failure.

Artifact versions MUST contain verifiable integrity information before they are
considered successfully publishable.

A specific hashing or digest algorithm is not frozen by Phase 10. The concrete
algorithm/provider remains an implementation decision.

`ArtifactId`, version identity, and content digest are distinct concepts:

```text
ArtifactId
→ which logical artifact?

Version
→ which exact version?

ContentDigest
→ which exact content representation?
```

The digest does not replace artifact identity.

---

### Integrity vs Validation

Integrity and validation are separate concerns.

#### Integrity

Answers:

```text
"Are these bytes the expected bytes?"
```

#### Generic structural validation

Answers:

```text
"Does this artifact satisfy the generic structural or contract requirements
expected by Core?"
```

#### Domain validation

Answers:

```text
"Does this artifact satisfy its engine/domain-specific semantic requirements?"
```

Core may provide generic structural validation and integrity mechanisms.

Engines remain responsible for domain-specific semantic validation.

Core must not acquire domain semantics merely to validate a domain artifact.

---

### Artifact Ownership

Core and engines have different responsibilities.

Core owns or provides mechanisms for:

```text
artifact identity
version mechanics
references
integrity mechanisms
lifecycle mechanics
access mechanisms
publication
resolution
retrieval abstractions
provenance linkage
```

The engine or platform component that creates the artifact owns:

```text
artifact meaning
domain metadata semantics
artifact creation logic
artifact transformation logic
domain validation
domain-specific interpretation
```

Core must not directly manipulate engine domain state or decide what engine
content means.

---

### Metadata

Artifact metadata is conceptually divided into Core metadata and domain
metadata.

Core metadata may include:

```text
ArtifactId
version
content size
integrity information
creation information
lifecycle state
```

Domain metadata may include information such as:

```text
model architecture
dataset language
domain revision
embedding dimensions
other engine-specific metadata
```

Core may preserve generic metadata but must not interpret engine-specific
metadata semantics.

---

### Content Reference

`ContentReference` identifies how the physical content associated with an
artifact version can be accessed.

The content reference does not imply that Core owns or materializes the actual
content.

Core must remain independent of the underlying content-storage provider.

The provider may support physical representations such as compressed,
uncompressed, cached, streamed, or otherwise stored forms.

---

### Publication

Publication is the process of making a specific artifact version available
through an authoritative reference or access mechanism.

Creation and publication are distinct:

```text
created
≠
published
```

Unpublished versions may exist internally or under controlled access.

A successfully published version is:

```text
immutable
resolvable
integrity-verifiable
available through the normal publication/reference mechanism
```

Publication must conceptually behave as an atomic externally visible
transition:

```text
VALIDATED
   ↓
PUBLISH
   ↓
PUBLISHED
```

The system must not expose an artifact as successfully published while its
required content, reference, integrity information, or publication invariants
are incomplete.

The concrete transaction/storage mechanism is an implementation detail.

---

### Publication Visibility

Unpublished versions must not automatically become available through the normal
published-artifact resolution path.

Conceptually:

```text
unpublished
→ internal / controlled access

published
→ normal artifact resolution path
```

Both are subject to the Phase 9 security model.

---

### Resolution

Resolution identifies the exact artifact version represented by an
`ArtifactReference`.

A reference may contain an exact version:

```text
A:v7
```

or a mutable selector:

```text
A:latest
```

A mutable selector may resolve to one exact version at a particular resolution
point:

```text
A:latest
   ↓
A:v14
```

A resolved execution reference MUST identify exactly one version.

An execution must not repeatedly resolve a mutable alias and silently switch
artifact versions during its lifetime.

---

### Aliases

Mutable aliases such as `latest` are permitted.

Aliases are resolution metadata and are not artifact content.

Changing an alias does not modify any artifact version.

For example:

```text
latest → v8

publish v9

latest → v9
```

does not modify `v8`.

The exact version remains immutable and independently addressable.

---

### Reproducibility

Exact artifact versions are canonical for execution and provenance.

An alias may be used at submission or resolution time, but once resolved, the
operation must retain the exact artifact version.

For example:

```text
Operation X
uses:
quran-dataset/latest
```

may resolve to:

```text
quran-dataset:v14
```

The operation and its provenance must record:

```text
quran-dataset:v14
```

rather than relying on `latest` as the final historical reference.

A running operation must not dynamically switch to a newly published version.

For example:

```text
Operation starts
    ↓
Model:v8
    ↓
Model:v9 published
```

The operation continues using:

```text
Model:v8
```

---

### Retrieval

Resolution and retrieval are separate operations.

```text
Resolution
= identify the artifact/version

Retrieval
= obtain the artifact content

Integrity Verification
= verify the retrieved content
```

The normal retrieval flow is:

```text
ArtifactReference
      ↓
Resolution
      ↓
exact ArtifactVersion
      ↓
ContentReference
      ↓
retrieve content
      ↓
integrity verification
      ↓
trusted content
```

Content failing integrity verification must not be returned as trusted content.

A retrieval operation must return an appropriate integrity failure when the
recorded content cannot be verified.

---

### Large Artifact Handling

Artifact size is independent of transport frame size.

```text
Artifact size
≠
transport frame size
```

An artifact may be:

```text
16 MB
100 MB
10 GB
1 TB
or larger
```

The artifact system must not require the complete artifact to be materialized
in Core memory.

Retrieval must support incremental content access through an implementation
appropriate to the provider, such as:

```text
streaming
chunks
provider handles
range retrieval
```

The exact mechanism remains an implementation decision.

Phase 7's bounded transport frames and fragmentation/reassembly remain
transport concerns.

Phase 10's artifact model remains independent of the number and size of
individual transport frames.

---

### Content Integrity and Physical Representation

An artifact version represents logical content while physical representations
may vary.

The recorded integrity information applies to the content representation
returned by the artifact retrieval contract.

Core must not assume that every physical representation is semantically
identical merely because it belongs to the same logical artifact unless the
retrieval contract defines the representation relationship.

The exact transformation/compression mechanism remains outside the Phase 10
artifact semantics.

---

### Deduplication

Physical deduplication is optional.

Two logical artifact versions may have identical content digests:

```text
A:v1 → digest D
B:v7 → digest D
```

This does not make the logical artifact versions identical.

```text
same content
≠
same logical artifact
```

A storage provider may reuse physical content, but Core must preserve the
logical artifact identities and version relationships.

---

### Artifact Lifecycle

The semantic lifecycle is intentionally restrained:

```text
CREATED
   ↓
VALIDATING
   ↓
VALIDATED
   ↓
PUBLISHED
   ↓
SUPERSEDED
   ↓
ARCHIVED
```

`REVOKED` represents an exceptional trust/usage state for a published version.

Not every artifact must pass through every state.

For example, an internal artifact may never be published.

#### `SUPERSEDED`

Means that a newer version exists.

A superseded version is not automatically invalid.

An exact reference to a superseded version remains valid unless the version is
explicitly revoked or access policy otherwise prevents its use.

#### `ARCHIVED`

Means the version is retained but is no longer active or preferred.

An archived version may remain valid and retrievable by exact reference,
subject to security and retention policy.

#### `REVOKED`

Means the version is explicitly considered unsafe, invalid, or untrusted under
normal usage policy.

Revocation does not erase the artifact's identity or historical existence.

---

### Deletion

`DELETED` is not a semantic artifact lifecycle state frozen by Phase 10.

Physical deletion is a storage and retention concern.

The artifact record may remain while physical content is removed later according
to a future retention/storage policy.

Core must not couple semantic artifact identity directly to storage-provider
garbage-collection behavior.

---

### Revocation

Revocation and supersession are distinct.

```text
SUPERSEDED
= newer version exists

REVOKED
= version must no longer be trusted/used normally
```

For example:

```text
v5 → SUPERSEDED
```

does not invalidate its historical truth.

Whereas:

```text
v6 → REVOKED
```

means normal consumption should be prevented according to the applicable
security and artifact policy.

Historical provenance may continue to reference revoked versions.

---

### Artifact Access and Security

Artifact operations reuse the Phase 9 security system.

The artifact system provides access operations.

The security system determines whether the caller is authorized.

Examples:

```text
retrieve A:v2
    ↓
authorization
    ↓
retrieve
```

and:

```text
publish A:v3
    ↓
authorization
    ↓
publish
```

Phase 10 must not create an independent artifact authorization framework.

---

### Source vs Artifact

`Source`, `Artifact`, `ArtifactVersion`, and `ArtifactReference` are distinct
concepts.

```text
Source
= origin of content or information

Artifact
= managed logical content/output

ArtifactVersion
= exact immutable state

ArtifactReference
= reference or selector for an artifact/version
```

A conceptual provenance flow may be:

```text
Source
   ↓
processing operation
   ↓
Artifact
   ↓
ArtifactVersion
   ↓
ArtifactReference
```

Core does not need to understand the semantic meaning of the source or
artifact.

---

### Provenance

Artifacts represent managed objects and versions.

Provenance records historical events and relationships involving those
artifacts.

A provenance record may conceptually contain:

```text
ProvenanceRecord
├── operation/event identity
├── actor/engine identity
├── input artifact references
├── output artifact references
├── source references
├── execution context
├── timestamps
└── relevant historical relationships
```

Provenance must reference artifacts rather than duplicate their content.

For example:

```text
Operation X
├── input  → Dataset:v4
├── input  → Model:v2
└── output → EmbeddingIndex:v7
```

The provenance system is not the artifact content store.

---

### Provenance and Historical Truth

Provenance is append-oriented historical information.

Historical provenance facts must not be silently rewritten because artifact
state changes later.

For example:

```text
Operation X
used Model:v2
```

remains historically true even if:

```text
Model:v2 → REVOKED
```

or:

```text
Model:v2 → SUPERSEDED by v3
```

Later artifact lifecycle changes are additional facts and must not rewrite the
original historical relationship.

---

### Provenance and Exact Versions

Final provenance for executable operations should identify exact artifact
versions.

For example:

```text
Operation X
uses quran-dataset:v14
```

rather than leaving a mutable selector such as:

```text
quran-dataset:latest
```

as the final execution reference.

The originally supplied alias may remain available as request/history metadata
when useful, but the resolved exact version establishes reproducibility.

---

### Provenance and Attempts

Phase 10 does not implement retry behavior.

However, the provenance model must be extensible enough to associate artifact
relationships with operation attempts introduced by the later Retry and
Idempotency phase.

Conceptually:

```text
Operation X
├── Attempt 1
├── Attempt 2
└── Attempt 3
```

Artifact provenance must be able to associate inputs and outputs with the
appropriate operation/attempt without implementing retry semantics itself.

Retry policy remains part of Phase 13.

---

### Provenance Input and Output Relationships

The artifact/provenance relationship must support explicit inputs and outputs.

Conceptually:

```text
Operation
├── consumes
│    ├── Artifact A:v1
│    └── Artifact B:v7
│
└── produces
     └── Artifact C:v2
```

This supports chains such as:

```text
Source
   ↓
normalization
   ↓
dataset:v5
   ↓
model:v8
   ↓
embedding-index:v2
```

without requiring Core to understand the domain semantics of those artifacts.

---

### Artifact Retrieval and Memory

Artifact retrieval must not imply whole-artifact loading into Core memory.

Large artifacts should be accessible incrementally.

The artifact API must therefore support or permit a provider implementation in
which content is consumed incrementally rather than requiring:

```text
entire artifact
→ Core memory
```

This requirement is independent of the transport framing protocol.

---

### Content Addressing

A content digest may be used by a storage provider as a physical content key,
but content-addressable storage is not required by Core.

Therefore:

```text
Digest
→ integrity evidence

Storage provider
→ may optionally use digest as content identifier
```

Core remains provider-neutral.

---

### Failure Semantics

The artifact system must preserve meaningful failure categories.

Conceptual failure classes include:

```text
ArtifactNotFound
InvalidReference
VersionNotFound
AccessDenied
IntegrityFailure
ValidationFailure
ResolutionFailure
PublicationFailure
RetrievalFailure
RevokedArtifact
```

The exact Rust error representation may vary, but semantically distinct
failure conditions must not all be collapsed into one generic artifact error.

A failed integrity check must remain distinguishable from:

```text
not found
access denied
invalid reference
validation failure
transport failure
```

---

### Artifact and Transport Boundary

Phase 7 owns:

```text
transport framing
bounded frames
fragmentation
reassembly
```

Phase 10 owns:

```text
artifact identity
artifact versions
artifact references
content access
artifact lifecycle
integrity
provenance
```

A large artifact may therefore be transported through multiple bounded
transport frames without the artifact system treating those frames as separate
artifact versions.

Conceptually:

```text
10 GB Artifact
      ↓
retrieval stream
      ↓
bounded transport frames
      ↓
transport
```

Artifact size and transport frame size remain independent concerns.

---

### Explicit Non-Goals

Phase 10 must not become:

* a database engine;
* an object-storage vendor integration;
* an S3-specific implementation;
* a filesystem-specific artifact system;
* a domain-specific artifact semantics engine;
* a domain-specific file-format engine;
* a workflow engine;
* a provenance workflow planner;
* a security policy engine;
* a retry engine;
* an idempotency engine.

Core must provide artifact mechanisms without taking ownership of Quran,
Hadith, Arabic, Fiqh, ML, Knowledge Graph, or other engine-specific artifact
semantics.

---

### Files and Folders

**Artifact**

* `src/artifact/mod.rs`
* `src/artifact/artifact.rs`
* `src/artifact/version.rs`
* `src/artifact/reference.rs`
* `src/artifact/content.rs`
* `src/artifact/integrity.rs`
* `src/artifact/lifecycle.rs`
* `src/artifact/store.rs`
* `src/artifact/resolution.rs`
* `src/artifact/publication.rs`

**Provenance**

* `src/provenance/mod.rs`
* `src/provenance/record.rs`
* `src/provenance/context.rs`
* `src/provenance/relation.rs`

**Related systems**

* `src/security/`
* `src/operation/`
* `src/contracts/`
* `src/transport/`
* `src/runtime/`

**Tests**

* `tests/artifact.rs`
* `tests/provenance.rs`
* `tests/integration.rs`

Exact filenames may be adjusted if the existing repository structure already
provides equivalent modules; the architectural boundaries above must remain.

---

### Boundary

Core provides the shared mechanisms required to identify, version, reference,
validate, publish, resolve, retrieve, verify, and track artifacts.

The artifact system does not own artifact domain semantics or require a specific
storage provider.

Security and access decisions reuse the Phase 9 security boundary.

Provenance records historical relationships involving artifacts, operations,
engines, capabilities, sources, messages, and execution without duplicating
artifact content.

The artifact system must remain compatible with large content and bounded
transport framing.

---

### Done when

A test engine or Core test fixture can:

1. create a logical artifact;
2. create distinct versions under one stable `ArtifactId`;
3. create lightweight artifact references;
4. distinguish exact versions from mutable aliases;
5. resolve aliases deterministically to an exact version;
6. prevent execution from silently changing artifact versions;
7. maintain immutable published versions;
8. validate generic artifact structure;
9. verify artifact content integrity;
10. publish a validated version atomically;
11. keep unpublished versions outside the normal published resolution path;
12. retrieve artifact content through a provider-neutral content mechanism;
13. retrieve large artifacts without requiring complete materialization in Core
    memory;
14. distinguish archival, supersession, and revocation;
15. preserve exact-version references after supersession unless explicitly
    revoked;
16. prevent normal trusted use of revoked versions according to artifact and
    security policy;
17. reuse Phase 9 authorization rather than creating a second security system;
18. represent input, output, source, operation, engine, and execution
    relationships through provenance;
19. preserve historical provenance facts without silently rewriting them;
20. remain extensible for operation attempts introduced in Phase 13;
21. keep artifact identity and content independent from individual transport
    frames;
22. preserve all previously verified Core behavior.

Unit tests and integration tests must cover artifact identity and versioning,
reference resolution, immutability, publication, retrieval, integrity
verification, lifecycle transitions, security enforcement, provenance
relationships, historical provenance behavior, large-content access, and
Phase 7 transport integration.

* [x] Implement artifact identity and stable `ArtifactId` handling
* [x] Implement distinct `Artifact`, `ArtifactVersion`, and `ArtifactReference` concepts
* [x] Implement immutable artifact version mechanics
* [x] Implement exact-version and mutable-alias references
* [x] Implement deterministic artifact resolution
* [x] Implement provider-neutral `ContentReference`
* [x] Implement generic artifact structural validation
* [x] Implement content digest and integrity verification
* [x] Require verified integrity evidence for publication
* [x] Implement artifact lifecycle transitions
* [x] Implement publication through the dedicated validated-to-published transition
* [x] Prevent unpublished versions from normal published resolution
* [x] Implement provider-neutral artifact storage and retrieval abstractions
* [x] Preserve support for large artifacts without requiring complete Core-memory materialization
* [x] Implement alias management without mutating exact artifact versions
* [x] Implement superseded, archived, and revoked lifecycle semantics
* [x] Reuse the Phase 9 security boundary for artifact access
* [x] Implement provenance records and provenance relationships
* [x] Preserve historical provenance across later artifact lifecycle changes
* [x] Preserve exact artifact versions for reproducible execution and provenance
* [x] Keep artifact/content identity independent from transport frame boundaries
* [x] Preserve distinct artifact failure categories
* [x] Add unit tests for artifact and provenance modules
* [x] Add public integration coverage in `tests/artifact.rs`, `tests/provenance.rs`, and `tests/integration.rs`
* [x] Preserve previously verified Phase 0–9 behavior
* [x] Complete implementation review and resolve genuine findings before merge
* [x] Complete full workspace verification before merge

---

## Phase 11: Observability, Health, and Configuration

### Goal

Provide the shared Core mechanisms for observability, operational health, and
configuration management.

Phase 11 provides generic logging/observability integration, metrics,
tracing, diagnostics, health reporting, readiness and dependency visibility,
configuration loading and validation, resolved configuration snapshots, and
controlled configuration updates.

These systems remain distinct from one another and must not become hidden
lifecycle controllers, domain-logic systems, or correctness dependencies.

### Planned implementation

Add observability infrastructure for generic logs, metrics, tracing,
diagnostics, correlation, and operational signals.

Add structured health reporting for liveness, readiness, lifecycle visibility,
capability readiness, dependency health, and overall operational condition.

Add configuration loading, parsing, validation, resolution, construction, and
immutable-by-default runtime configuration with explicitly supported dynamic
updates.

Build on the existing Phase 5 context, Phase 8 Engine Runtime, Phase 9
security/middleware, and Phase 10 artifact/provenance mechanisms without
replacing or duplicating them.

---

### System Boundaries

Phase 11 contains three related but independent Core systems:

```text
Configuration
→ defines how a component should operate

Health
→ reports the current operational condition

Observability
→ records behavior, measurements, events, and execution signals
````

These concepts must remain distinct:

```text
Configuration
≠ Runtime State

Health
≠ Lifecycle

Health
≠ Observability

Logging
≠ Metrics
≠ Tracing

Diagnostics
≠ Provenance

Configuration
≠ Secrets
```

Phase 11 integrates these systems with the runtime but does not merge them into
a single subsystem.

---

### Observability

Observability provides cross-cutting mechanisms for understanding runtime
behavior and operational condition.

The observability model includes:

```text
Logging
Metrics
Tracing
Diagnostics
Correlation
Telemetry
```

Telemetry is the umbrella concept for operational signals. It must not become a
replacement for the existing Logging system.

The existing Error and Logging systems remain independent Core peer systems.

Observability mechanisms must not interpret engine-specific domain semantics
unless an engine explicitly supplies domain-specific observability data.

---

### Logging

Logging records discrete runtime or operational events.

Examples include:

```text
engine started
dependency unavailable
request rejected
configuration update failed
lifecycle transition
artifact retrieval failure
```

Logging remains distinct from metrics, tracing, diagnostics, and provenance.

Logging mechanisms must preserve structured error information where
appropriate without forcing all failures into logging-only representations.

Sensitive information MUST NOT be automatically emitted into logs.

This includes, where applicable:

```text
credentials
authentication material
secrets
private security metadata
raw sensitive payloads
```

Explicitly approved safe logging policies may expose selected metadata, but the
default observability boundary must protect sensitive values.

---

### Metrics

Metrics represent numerical or aggregatable operational measurements.

Generic Core metrics may include:

```text
request count
success/failure count
request latency
active requests
capability invocation count
lifecycle transition count
dependency status
runtime activity
```

Engines may provide domain-specific metrics without requiring Core to interpret
their meaning.

For example:

```text
Arabic parser throughput
embedding generation rate
Knowledge Graph query latency
```

Core must not require domain-specific semantics in order to collect such
measurements.

Metrics should use controlled, bounded dimensions.

High-cardinality request-specific values such as:

```text
MessageId
OperationId
arbitrary user input
large artifact references
raw payload values
```

must not be automatically used as unbounded metric labels.

Such information belongs in more appropriate observability mechanisms such as
traces, structured logs, or diagnostics when safe to expose.

---

### Tracing

Tracing represents the execution path of an operation through Core and engine
boundaries.

Conceptually:

```text
External Request
      ↓
Engine A
      ↓
Capability
      ↓
Engine B
      ↓
Engine C
```

may appear as one connected trace with multiple spans.

Tracing should support:

```text
trace context
span relationships
parent/child relationships
request execution timing
selected execution attributes/events
```

Tracing context must propagate across Core-supported engine boundaries.

Tracing remains distinct from:

```text
MessageId
CorrelationId
OperationId
```

These identifiers must not be silently merged.

Conceptually:

```text
MessageId
→ identifies a logical message

CorrelationId
→ correlates related messages/activities

OperationId
→ identifies a logical operation

TraceId
→ identifies a tracing execution graph

SpanId
→ identifies one tracing segment
```

The exact relationship between these identifiers remains an implementation
decision unless explicitly frozen elsewhere.

The exact tracing provider, protocol, exporter, or backend remains an
implementation choice.

---

### Correlation

Correlation mechanisms allow related activity to be connected across runtime
boundaries.

Correlation must preserve existing Core identifiers rather than replacing
them with a single generic identifier.

Relevant context may propagate across:

```text
client
→ engine
→ capability
→ downstream engine
```

without collapsing distinct identity concepts.

Correlation context must remain request/operation scoped and safe for concurrent
execution.

---

### Diagnostics

Diagnostics provide information useful for understanding the current or recent
operational condition of a Core component, engine, capability, dependency, or
configuration state.

Diagnostics are distinct from ordinary event logging and from historical
provenance.

Conceptually:

```text
Logging
→ "Something happened."

Diagnostics
→ "This is the current/recent operational condition."

Provenance
→ "This is what historically happened involving an operation/artifact."
```

Diagnostics may expose information such as:

```text
engine lifecycle state
dependency condition
runtime condition
capability availability
configuration condition
operational failures
```

Diagnostics must not require domain-specific interpretation by Core.

---

### Observability Failure Behavior

Observability is normally best-effort and must not become a correctness
dependency.

Failure to emit optional logs, metrics, traces, or diagnostics must not
normally cause an otherwise valid engine request or capability operation to
fail.

For example:

```text
request
  ↓
handler succeeds
  ↓
metrics exporter unavailable
```

must not automatically become:

```text
business request failed
```

Instead:

```text
business result
→ remains successful

observability failure
→ may be recorded as an operational diagnostic
```

An observability backend failure may itself become observable through available
health or diagnostic mechanisms.

---

### Observability and Context

Observability mechanisms use the existing Core context boundaries.

Request-specific information must remain request-scoped.

For example:

```text
Request A → context A
Request B → context B
```

Shared observability infrastructure must support concurrent requests without
mixing request-specific state.

Observability code must not introduce synchronization that blocks valid
concurrent or reentrant engine operations.

---

### Health

Health provides structured operational status for Core-managed runtime
components and engines.

Health is observational and does not replace or redefine the Phase 8 lifecycle
state machine.

Conceptually:

```text
Lifecycle
→ controls runtime state

Health
→ reports operational state
```

Health MUST NOT silently become a second lifecycle controller.

A health result of `UNHEALTHY` does not automatically imply:

```text
STOPPED
```

unless an explicitly defined later policy mechanism authorizes such behavior.

---

### Liveness

Liveness indicates whether the runtime/component is alive enough to be
considered operational.

Liveness is independent from readiness.

An engine may be:

```text
liveness = healthy
readiness = false
```

For example, the engine process may still be functioning while a required
dependency is unavailable.

A concrete liveness enum or transport representation remains an
implementation choice as long as the semantics remain explicit.

---

### Readiness

Readiness reports whether the engine can currently accept normal work.

Readiness follows the Phase 8 runtime boundary.

The runtime state:

```text
SERVING
```

is the state in which normal requests may be admitted.

Health should expose that fact rather than inventing a conflicting readiness
model.

Conceptually:

```text
SERVING
→ ready to accept normal work

DRAINING
→ not ready for new work

STOPPED
→ not ready
```

Health must not directly redefine request admission.

---

### Lifecycle Visibility

Health should expose relevant lifecycle information where useful.

For example, while draining:

```text
liveness = healthy
readiness = false
lifecycle = DRAINING
```

This allows consumers to distinguish:

```text
alive but draining
```

from:

```text
unhealthy
```

Draining is an operational lifecycle mode, not automatically a health failure.

---

### Capability Readiness

Health may report readiness/availability at capability granularity.

For example:

```text
Engine
├── capability A → ready
├── capability B → ready
├── capability C → unavailable
└── capability D → degraded
```

Engine-level readiness does not automatically require every capability to be
healthy.

Whether a capability is required for engine readiness remains engine-owned,
consistent with Phase 8.

Capability health must not be interpreted by Core as domain semantics.

---

### Dependency Health

Health should expose dependency conditions separately from engine lifecycle.

Conceptually:

```text
Dependency A → healthy
Dependency B → degraded
Dependency C → unavailable
```

Loss of a dependency after startup does not automatically transition the
engine to `STOPPED`.

The runtime reports the condition, while engines determine how affected
capabilities behave according to their own semantics.

Detailed dependency health belongs to the Health system; dependency lifecycle
participation remains governed by Phase 8.

---

### Health Status

Health reporting should be structured rather than reduced to a single boolean.

A conceptual health report may contain:

```text
HealthReport
├── lifecycle state
├── liveness
├── readiness
├── overall assessment
├── dependency statuses
└── capability statuses
```

The exact Rust representation remains an implementation choice.

The model should support at least the conceptual overall assessments:

```text
HEALTHY
DEGRADED
UNHEALTHY
UNKNOWN
```

These values are derived operational assessments and must not replace the
underlying detailed status information.

---

### Health Aggregation

Overall health should be derived from component-level observations according
to deterministic rules.

For example:

```text
liveness = healthy
readiness = ready
required dependencies = healthy
required capabilities = healthy

→ overall = HEALTHY
```

A system may be:

```text
liveness = healthy
readiness = ready
optional dependency = unavailable
optional capability = unavailable

→ overall = DEGRADED
```

An inability to establish the health state may produce:

```text
UNKNOWN
```

rather than falsely reporting `HEALTHY` or `UNHEALTHY`.

The exact aggregation algorithm remains an implementation choice unless later
frozen explicitly, but its behavior must be deterministic.

---

### Health Checks

Health checks should be lightweight and observational.

Health checks must not execute arbitrary domain workflows merely to determine
health.

A health check should prefer:

```text
current state
cached state
lightweight dependency probes
runtime state
capability state
```

over expensive domain execution.

Health checks must not unnecessarily create significant workload or mutate
engine/domain state.

Health observation must safely coexist with concurrent request execution.

---

### Health and Lifecycle

Phase 8 owns lifecycle transitions.

Phase 11 owns health reporting.

Therefore:

```text
Phase 8
→ decides lifecycle

Phase 11
→ observes/reports lifecycle
```

Health may report:

```text
STARTING
READY
SERVING
DRAINING
STOPPED
```

where lifecycle visibility is useful, but health must not create a competing
state machine.

---

### Health and Shutdown

During draining, health should be capable of exposing:

```text
liveness = healthy
readiness = false
lifecycle = DRAINING
```

assuming the runtime remains operational.

After `STOPPED`, the component must no longer be considered ready or serving.

Health reporting must remain consistent with the Phase 8 lifecycle without
taking ownership of lifecycle transitions.

---

### Health Failure Behavior

Failure to report health must not normally cause valid business execution to
fail.

For example:

```text
health provider unavailable
```

does not automatically imply:

```text
request failed
```

Health infrastructure may report itself as degraded or unavailable through
other operational mechanisms where appropriate.

---

### Configuration

Configuration provides the shared Core mechanism for loading, parsing,
validating, resolving, constructing, and managing runtime configuration.

The conceptual configuration pipeline is:

```text
Raw Configuration
      ↓
Load
      ↓
Parse
      ↓
Structural Validation
      ↓
Semantic Validation
      ↓
Resolution
      ↓
Resolved Configuration
      ↓
Runtime
```

Configuration is a Core mechanism.

Engine-specific configuration semantics remain engine-owned.

---

### Configuration Parsing

Parsing determines whether raw configuration data can be converted into the
expected configuration representation.

For example:

```text
port = "hello"
```

may be a parsing/type failure.

Parsing errors must remain distinct from semantic validation failures where
the distinction is meaningful.

---

### Configuration Validation

Validation determines whether parsed configuration satisfies required
structural and semantic constraints.

For example:

```text
port = 99999
```

may parse successfully but fail validation.

Core may provide generic validation mechanisms.

Engines provide engine-specific configuration schemas and semantic validation
where required.

Core must not interpret domain-specific configuration semantics.

---

### Configuration Resolution

Resolution produces a configuration representation suitable for runtime
construction.

Resolution may include deterministic processing of supported configuration
sources, references, defaults, or other required generic mechanisms.

When multiple configuration sources are supported, their precedence must be
deterministic and explicitly defined.

The exact source types and precedence hierarchy remain implementation choices
until explicitly frozen.

---

### Core vs Engine Configuration

Core configuration may include generic settings such as:

```text
runtime
transport
middleware
security
observability
generic resource settings
```

Engine configuration may include:

```text
model settings
dataset settings
domain-specific parameters
Arabic engine settings
Knowledge Graph settings
other engine-owned configuration
```

Core provides the configuration mechanism.

Engines define the meaning of their domain-specific configuration.

---

### Configuration Immutability

Configuration is immutable by default for an active runtime instance.

A configuration field may support runtime mutation only when that capability is
explicitly defined for that field.

The runtime must not assume that every configuration value can change while an
engine is serving.

Conceptually:

```text
normal configuration
→ immutable after construction

explicitly mutable configuration
→ controlled runtime updates
```

---

### Configuration Snapshots

The runtime should operate against a resolved configuration snapshot.

Conceptually:

```text
Configuration Snapshot N
        ↓
Runtime
```

If a valid dynamic update is accepted:

```text
Snapshot N
    ↓
validated update
    ↓
Snapshot N+1
```

This avoids rebuilding configuration independently for every request.

---

### Running Operations and Configuration Updates

A running operation must not silently switch configuration semantics because a
new configuration snapshot becomes active.

By default:

```text
Request A → configuration snapshot N

configuration update

Request B → configuration snapshot N+1
```

An explicitly runtime-observable setting may follow its documented dynamic
semantics, but such behavior must be intentionally defined rather than assumed.

Configuration update behavior must not compromise already-admitted operation
execution.

---

### Configuration Updates

When runtime configuration mutation is supported, updates must follow:

```text
Current Valid Configuration
        ↓
Proposed Update
        ↓
Parse
        ↓
Validate
        ↓
Resolve
        ↓
Apply Atomically
```

If validation or application fails:

```text
Current Valid Configuration
→ remains active
```

The system must not expose a partially applied configuration.

The exact implementation/transaction mechanism remains an implementation
choice.

---

### Secrets

Configuration should support secret references without requiring raw
credentials to be embedded directly into ordinary configuration documents.

Conceptually:

```text
Configuration
     ↓
Secret Reference
     ↓
Secret Provider
     ↓
Resolved Secret
```

The exact secret-provider technology remains an implementation/provider choice.

Core must not become a secret-management product merely because configuration
can reference secrets.

Resolved secrets must not be casually emitted to:

```text
logs
metrics
traces
diagnostics
errors
```

Sensitive configuration values must be appropriately redacted.

---

### Configuration Failure Semantics

Configuration failures should preserve meaningful categories where applicable,
including:

```text
load failure
parse failure
validation failure
resolution failure
secret resolution failure
incompatible configuration
runtime application failure
```

A required startup configuration failure must prevent the engine from reaching
`READY` under the Phase 8 lifecycle rules.

A failed runtime configuration update must preserve the last valid active
configuration.

An invalid proposed update must not partially affect the running configuration.

---

### Configuration and Health

Configuration state may influence health and diagnostics, but these systems
remain independent.

For example:

```text
configuration update
      ↓
validation failure
      ↓
previous configuration remains active
      ↓
diagnostic may be emitted
```

A failed configuration proposal does not automatically make the engine
unhealthy.

Conversely, an active configuration problem that genuinely prevents required
operation may affect readiness or health according to the established runtime
and health rules.

---

### Configuration and Observability

Configuration changes and failures may produce safe observability signals.

Observability MUST NOT automatically expose sensitive configuration values.

Safe signals may include:

```text
configuration update attempted
configuration update succeeded/failed
configuration snapshot changed
configuration validation failure
```

without exposing secret contents.

---

### Phase 11 and Earlier Phases

Phase 11 builds on previous Core mechanisms:

```text
Phase 5
→ EngineContext and cancellation/deadline propagation

Phase 8
→ lifecycle, admission, runtime concurrency, shutdown

Phase 9
→ security identity/context and mandatory middleware

Phase 10
→ artifacts, integrity, provenance, and historical execution relationships
```

Phase 11 must not replace or duplicate those mechanisms.

---

### Phase 11 and Later Phases

Phase 11 provides the observability, health, and configuration foundations
required by later phases.

It does not implement:

```text
retry policy
idempotency
advanced streaming
background-task scheduling policy
Control Plane routing
Engine SDK behavior
domain workflows
domain-specific business logic
```

Later phases may consume Phase 11 mechanisms without redefining their
semantics.

---

### Explicit Non-Goals

Phase 11 must not become:

* a centralized logging vendor/platform;
* a cloud monitoring vendor integration;
* an external telemetry backend;
* a deployment manager;
* an infrastructure manager;
* an automatic self-healing system;
* a hidden lifecycle controller;
* a dynamic configuration control plane;
* a secret-management platform;
* a domain observability engine;
* a domain health engine;
* a domain business-logic system.

Provider-specific integrations remain implementation or later-platform concerns.

---

### Files and Folders

**Observability**

* `src/observability/mod.rs`
* `src/observability/logging.rs`
* `src/observability/metrics.rs`
* `src/observability/tracing.rs`
* `src/observability/diagnostics.rs`
* `src/observability/correlation.rs`

**Health**

* `src/health/mod.rs`
* `src/health/status.rs`
* `src/health/liveness.rs`
* `src/health/readiness.rs`
* `src/health/dependencies.rs`
* `src/health/capabilities.rs`
* `src/health/report.rs`

**Configuration**

* `src/config/mod.rs`
* `src/config/loader.rs`
* `src/config/parser.rs`
* `src/config/validation.rs`
* `src/config/resolution.rs`
* `src/config/snapshot.rs`
* `src/config/update.rs`
* `src/config/secrets.rs`

**Runtime integration**

* `src/runtime/`
* `src/middleware/`
* `src/security/`
* `src/operation/`
* `src/provenance/`

**Tests**

* `tests/observability.rs`
* `tests/health.rs`
* `tests/configuration.rs`
* `tests/phase11_end_to_end.rs`

Exact filenames may be adjusted if the repository already provides equivalent
modules, but the architectural boundaries defined in this phase must remain.

---

### Boundary

Core provides generic mechanisms for:

```text
observability
health reporting
configuration management
```

Core controls:

```text
observability integration boundaries
health reporting semantics
configuration parsing/validation/resolution mechanisms
configuration snapshot management
```

Engines retain ownership of:

```text
domain-specific configuration semantics
domain-specific metrics
domain-specific operational interpretation
domain-specific health meaning
domain-specific workflows
domain-specific business rules
domain-specific payload semantics
```

Health reports lifecycle but does not own lifecycle.

Observability records behavior but does not become a correctness dependency.

Configuration defines runtime behavior but does not become an implicit workflow
or deployment system.

Security remains owned by the Phase 9 security boundary.

Artifact history remains owned by the Phase 10 artifact/provenance boundary.

---

### Done when

A test engine or Core fixture can:

1. load configuration through the defined configuration pipeline;
2. parse and structurally validate configuration;
3. apply semantic validation where required;
4. construct a resolved configuration snapshot;
5. distinguish Core configuration from engine-owned configuration semantics;
6. operate with immutable configuration by default;
7. accept an explicitly supported runtime configuration update;
8. validate a proposed configuration update before applying it;
9. apply valid runtime configuration changes atomically;
10. preserve the previous valid configuration when an update fails;
11. resolve supported secret references without requiring raw secrets in ordinary
    configuration data;
12. prevent sensitive configuration values from being emitted through normal
    observability mechanisms;
13. report liveness independently from readiness;
14. expose readiness consistently with the Phase 8 runtime lifecycle;
15. expose draining state without treating draining as automatic health failure;
16. report capability-level readiness/availability;
17. report dependency health independently from lifecycle state;
18. provide structured overall health assessments including healthy, degraded,
    unhealthy, and unknown conditions;
19. aggregate component health deterministically;
20. perform lightweight observational health checks;
21. expose lifecycle state without creating a competing lifecycle controller;
22. provide generic metrics and metrics integration points;
23. provide tracing and trace-context propagation across Core-supported engine
    boundaries;
24. preserve the distinction between MessageId, CorrelationId, OperationId,
    TraceId, and SpanId;
25. provide structured diagnostics distinct from historical provenance;
26. allow observability failures without normally failing valid business
    execution;
27. support concurrent observability and health operations without violating
    request isolation;
28. preserve all previously verified Core behavior.

Unit tests and integration tests must cover configuration loading/parsing/
validation, configuration snapshots and updates, secret redaction, liveness,
readiness, capability/dependency health, lifecycle visibility, health
aggregation, middleware/runtime observability integration, tracing/correlation
propagation, metrics, diagnostics, observability failure isolation, and
concurrent access.

* [x] Implement generic observability integration boundaries.
* [x] Implement metrics with bounded dimensions, descriptor validation, gauges, counters, histograms, snapshots, and concurrent recorder access.
* [x] Implement tracing with validated `TraceId`/`SpanId`, span attributes/events, parent-child relationships, completed spans, serialization/deserialization validation, and self-parent rejection.
* [x] Implement structured diagnostics with bounded subjects/details and validation-preserving deserialization.
* [x] Implement correlation context integration without replacing existing logging context.
* [x] Implement structured health status, liveness, readiness, dependency health, capability health, lifecycle visibility, and aggregate health reporting.
* [x] Implement deterministic health component normalization and aggregation.
* [x] Implement configuration loading from the process environment with owned values and source identity.
* [x] Implement source-neutral configuration fixtures for deterministic tests.
* [x] Implement typed configuration parsing for string, boolean, integer, and float values.
* [x] Implement structural validation, required-key validation, deterministic error collection, and semantic-validation extension points.
* [x] Implement deterministic reference resolution and defaults.
* [x] Implement immutable resolved configuration snapshots with monotonically increasing snapshot identifiers.
* [x] Implement controlled runtime configuration preparation and atomic activation.
* [x] Implement configuration conflict detection and updater lineage protection.
* [x] Implement semantic-validator lineage invalidation so previously prepared updates cannot bypass a changed validation policy.
* [x] Preserve sensitive-value redaction across configuration and observability boundaries.
* [x] Add Phase 11 unit and cross-module integration coverage.
* [x] Preserve previously verified Phase 0–10 behavior.

---

## Phase 12: Streaming, Concurrency, and Background Tasks

### Goal

Provide the shared Core mechanisms for application-level streaming, bounded
concurrency, resource-controlled execution, task ownership, cancellation
propagation, and runtime-managed background work.

Phase 12 defines how logical application results and runtime-managed tasks live,
execute, consume resources, propagate cancellation, and terminate.

Phase 7 defines how logical messages and transport bytes are framed,
fragmented, transmitted, and reassembled.

These responsibilities MUST remain separate.

---

### Planned implementation

Add application-level stream abstractions, stream lifecycle management,
logical result delivery, ordered publication, bounded buffering and
backpressure, stream cancellation and deadline propagation, bounded task
execution, background task ownership and lifecycle management, and integration
with the existing Core operation, context, runtime, security, artifact,
provenance, health, and configuration systems.

Phase 12 builds on:

```text
Phase 5
→ context, cancellation, and deadline mechanisms

Phase 8
→ engine lifecycle, admission, concurrency, and shutdown

Phase 9
→ security identity/context and mandatory security boundaries

Phase 10
→ artifact and provenance mechanisms

Phase 11
→ health and configuration mechanisms
````

Phase 12 must not replace or create competing versions of those mechanisms.

---

### Application Streaming vs Transport Fragmentation

Application-level streaming and transport fragmentation are distinct
mechanisms.

Transport fragmentation means:

```text
one logical message
    ↓
multiple bounded transport frames
    ↓
one reconstructed logical message
```

Application-level streaming means:

```text
one logical operation
    ↓
multiple logical stream items
    ↓
stream termination
```

For example:

```text
100 MB response
    ↓
7 transport frames
    ↓
ONE logical response
```

is not application-level streaming.

Whereas:

```text
search operation
    ↓
result 1
    ↓
result 2
    ↓
result 3
    ↓
...
    ↓
final result
```

is application-level streaming.

Phase 12 MUST NOT treat transport-frame boundaries as application stream
boundaries.

Phase 7 owns:

```text
binary transport framing
fragmentation
reassembly
transport-frame ordering
```

Phase 12 owns:

```text
logical streaming
stream lifecycle
logical-item ordering
buffering
backpressure
stream cancellation
stream resource behavior
```

---

### Stream Definition

A `Stream` represents an ordered sequence of logical application-level items
belonging to an operation or an explicitly defined independent interaction.

Conceptually:

```text
Stream
├── stream identity
├── owner
├── operation/context relationship
├── logical items
├── lifecycle
└── termination state
```

A stream carries logical application-level items, not arbitrary transport
byte fragments.

Logical stream items may represent:

```text
partial results
final results
progress updates
events where explicitly permitted by the stream contract
other operation-defined application items
```

The semantic type and meaning of a stream item are defined by the applicable
operation or capability contract.

Core MUST NOT interpret engine-specific result semantics.

---

### Stream Creation

A stream may only be created as part of an admitted operation or through an
explicitly defined independent stream contract.

Normal stream creation MUST remain subject to the established:

```text
runtime admission
security
validation
context
capability
```

boundaries.

An unauthorized, rejected, or non-admitted request MUST NOT allocate or expose
a normal application stream as a mechanism for bypassing those boundaries.

Stream creation is part of admitted operation execution.

---

### Stream Ownership

Streams are operation-scoped by default.

Conceptually:

```text
Operation
   ↓
Stream
```

A stream may have an independent lifetime only when its contract explicitly
establishes:

```text
independent ownership
independent lifecycle
cancellation semantics
resource management
cleanup behavior
```

An active stream MUST always have an explicit owner.

A stream MUST NOT become ownerless or implicitly detached.

Possible ownership models include:

```text
operation-owned stream
explicit independent stream owner
```

The default remains operation ownership.

---

### Stream and Operation Lifetime

An operation-owned stream remains associated with the operation that created
it unless an explicit stream contract establishes independent ownership.

By default:

```text
operation starts
    ↓
stream created
    ↓
stream produces items
    ↓
stream terminates
    ↓
operation completes
```

A normal operation-owned stream MUST NOT silently become an unowned
long-lived stream after its operation terminates.

---

### Stream and Operation Cancellation

Operation cancellation propagates to operation-owned streams by default.

Conceptually:

```text
Operation cancelled
      ↓
Stream cancelled
      ↓
Producer notified
      ↓
Producer stops / cleans up
```

Cancelling an operation-owned stream normally propagates cancellation to the
producing operation when that stream represents the operation's primary
output.

An operation may explicitly define a stream as optional or independently
cancellable.

In that case the operation may continue according to its explicit contract.

Child cancellation MUST NOT implicitly cancel unrelated sibling operations.

---

### Stream Context

Streams reuse established Core context mechanisms.

Conceptually:

```text
OperationContext
      ↓
Stream
      ↓
Producer / Consumer
```

A stream inherits or references the relevant context for:

```text
operation identity
cancellation
deadline
security context
provenance context
correlation/tracing context
```

Phase 12 MUST NOT create unrelated parallel context systems merely for
streaming.

Request-specific state MUST remain isolated between unrelated concurrent
streams.

---

### Stream Security

A stream does not bypass the Phase 9 security boundary.

The authorization boundary occurs before normal stream creation and execution.

Conceptually:

```text
Request
   ↓
Security
   ↓
Stream creation
   ↓
Stream execution
```

The stream producer and consumer MUST NOT independently reinterpret
authentication state or bypass established security context.

---

### Stream and Provenance

Phase 12 integrates with the Phase 10 provenance system.

Streams may participate in relationships such as:

```text
Operation
   ↓
Stream
   ↓
logical results / artifact references
```

Phase 12 MUST NOT create a second provenance mechanism.

Existing provenance relationships remain associated with the relevant
operation, attempt, artifact, engine, and other established Core identities.

---

### Stream Lifecycle

A stream uses a non-reopenable lifecycle.

Conceptually:

```text
CREATED
   ↓
OPEN
   ↓
   ├── COMPLETED
   ├── CANCELLED
   └── FAILED
```

`COMPLETED`, `CANCELLED`, and `FAILED` are terminal states.

A stream MUST NOT transition from a terminal state back into an active state.

Once terminal:

```text
no more logical items
no further lifecycle transitions
```

Stream termination MUST be idempotent.

Multiple independent shutdown, cancellation, deadline, consumer, or failure
paths MUST NOT corrupt stream state after a terminal state has already been
established.

---

### Stream Completion

`COMPLETED` means:

```text
producer finished normally
final logical result was emitted according to the stream contract
stream closed successfully
```

No further items may be produced after completion.

---

### Stream Cancellation

`CANCELLED` means the stream terminated because cancellation or expiration
ended its execution.

Possible causes include:

```text
consumer cancellation
operation cancellation
engine shutdown
deadline expiration
owner cancellation
```

The actual cancellation reason may remain available through the applicable
Core context or error information.

Once cancellation is terminal:

```text
no new logical item may be published
```

---

### Stream Failure

`FAILED` means the stream terminated abnormally.

Possible causes include:

```text
producer failure
dependency failure
runtime failure
resource failure
protocol violation
other execution failure
```

A stream failure MUST terminate the stream rather than leave it indefinitely
open.

A stream failure does not automatically imply that the entire engine has
failed.

Whether the associated operation also fails depends on whether the stream is
required for that operation's successful completion.

---

### Logical Stream Items

A stream contains logical application-level items.

The logical item type is determined by the operation or capability contract.

A logical stream item may be small enough for one logical message:

```text
Stream Item
    ↓
Logical Message
    ↓
one transport frame
```

or large enough to require fragmentation:

```text
Stream Item
    ↓
Logical Message
    ↓
Phase 7 fragmentation
    ↓
multiple transport frames
```

Transport frames MUST NOT become application stream items merely because they
are transmitted separately.

---

### Partial and Final Results

Stream items may be classified as:

```text
PARTIAL
FINAL
```

`PARTIAL` means more logical stream items may follow.

`FINAL` means no further logical stream items will be successfully produced.

Example:

```text
item 1 → PARTIAL
item 2 → PARTIAL
item 3 → FINAL
```

The final item MUST be logically distinguishable from partial items.

`FINAL` is an application-level stream signal.

It is not a transport-frame termination signal and MUST NOT be confused with
Phase 7 framing metadata.

---

### Empty Streams

An operation may legitimately produce zero logical result items.

An empty stream is not automatically a failure.

Conceptually:

```text
OPEN
   ↓
empty successful completion
   ↓
COMPLETED
```

The exact representation of an empty final result remains operation-contract
specific.

---

### Ordering

Logical stream items MUST be delivered in their defined logical order.

For the default ordered stream model:

```text
item 0
item 1
item 2
item 3
```

must be observed in that same logical order.

Internal computation may be concurrent.

For example:

```text
worker A ──┐
worker B ──┼──→ ordered publication path → Stream
worker C ──┘
```

One logical producer does not require one physical worker.

If multiple workers participate in producing results, the stream publication
mechanism MUST preserve the defined logical order.

---

### Producer Model

A stream has one logical ordered producer by default.

The implementation may use multiple workers, threads, or tasks internally.

Internal parallel computation MUST be separated from logical stream
publication.

Multiple independent producers may publish into one stream only when the
stream contract explicitly defines their ordering, ownership, and lifecycle
semantics.

The default stream abstraction MUST NOT introduce ambiguous multi-producer
ordering.

---

### Consumer Model

The default stream abstraction has one logical consumer.

This provides unambiguous consumption ownership for operations such as:

```text
next_item()
```

Fan-out is not a default stream behavior.

Broadcast, pub/sub, and multi-consumer distribution are separate abstractions
and MUST NOT be implicitly introduced into the normal stream mechanism.

---

### Stream Delivery

Core MUST NOT intentionally duplicate logical stream items as a consequence of
buffering, scheduling, backpressure, cancellation, stream management, or
internal concurrency.

The normal stream abstraction therefore provides one logical publication of
each successfully accepted item.

This is not a distributed exactly-once execution guarantee.

Retry, duplicate execution, and idempotency semantics remain outside Phase 12
and belong to the later Retry and Idempotency phase.

---

### Backpressure

Streams MUST use bounded buffering or another explicit bounded resource
control mechanism.

Unbounded stream-buffer growth is not a supported default.

Conceptually:

```text
Producer
   ↓
bounded buffer
   ↓
Consumer
```

If the consumer is slower than the producer:

```text
consumer slower
    ↓
backpressure
    ↓
producer waits / slows / remains bounded
```

A slow consumer is not automatically a failed consumer.

Backpressure MUST preserve logical stream-item ordering.

---

### No Silent Stream Item Dropping

Core MUST NOT silently drop logical stream items merely because a stream
buffer or resource limit has been reached.

When backpressure cannot be respected, the system must use an explicit
contract-defined outcome such as:

```text
wait
reject
explicit stream failure
explicit cancellation
```

rather than silently discarding logical data.

---

### Consumer Disappearance

If the consumer disconnects, drops the stream, or otherwise terminates
consumption:

```text
consumer gone
    ↓
stream cancellation
    ↓
producer notified
    ↓
producer stops / cleans up
```

An operation-owned stream MUST NOT allow a producer to continue generating
unconsumed work indefinitely after consumer termination.

The cancellation path must use the existing Core cancellation mechanisms.

---

### Stream Cancellation Wins Over Further Production

If cancellation occurs while a producer is preparing an item:

```text
producer preparing item N
        ↓
stream cancelled
```

the item MUST NOT be published after terminal cancellation.

Once the stream reaches a terminal cancellation state:

```text
no additional logical items
```

may be published.

---

### Stream Deadline

Operation-owned streams inherit the operation deadline by default.

For example:

```text
operation deadline = 30 seconds
        ↓
stream starts
        ↓
deadline reached
        ↓
stream expiration / cancellation
```

A stream MUST NOT silently convert a bounded operation into indefinite
execution.

An independently owned long-lived stream must explicitly define its own
lifetime and deadline semantics.

---

### Stream Shutdown

During normal engine draining:

```text
DRAINING
   ↓
no new streams admitted
   ↓
existing streams may continue
```

During termination that cancels remaining work:

```text
shutdown cancellation
   ↓
active streams cancelled
   ↓
producers notified
   ↓
cleanup
```

Phase 12 integrates with the Phase 8 shutdown model and does not replace it.

---

### Stream Resource Management

At minimum, stream resource controls must account for:

```text
active streams
buffered stream items
buffer/resource memory
per-operation stream resources
per-engine stream resources
```

Exact numerical limits remain configuration or implementation choices.

The runtime MUST prevent accidental unbounded resource growth.

---

### Background Tasks

A background task is runtime-managed work that may execute independently of an
active request.

A streaming producer is not automatically a background task merely because
it runs asynchronously.

Conceptually:

```text
Operation Task
→ tied to one operation

Stream Producer
→ tied to a stream/operation

Background Task
→ longer-lived work owned by an engine or runtime subsystem
```

A background task may outlive an individual request only when its ownership
and lifecycle explicitly permit that behavior.

---

### Background Task Ownership

Every runtime-managed task MUST have an explicit owner.

Possible owners include:

```text
engine
operation
stream
runtime subsystem
```

A runtime-managed task MUST NOT be ownerless.

Task submission and ownership registration conceptually follow:

```text
task submission
   ↓
resource/admission check
   ↓
owner assigned
   ↓
task registered
   ↓
queued or running
```

A task must have a stable owner before it is considered runtime-managed.

There must be no lifecycle interval in which a managed task exists without a
known owner.

---

### Background Task Lifecycle

A runtime-managed background task follows:

```text
CREATED
   ↓
RUNNING
   ↓
   ├── COMPLETED
   ├── CANCELLED
   └── FAILED
```

Terminal task states are terminal.

The exact internal representation remains an implementation detail, but task
lifecycle behavior must preserve these semantics.

---

### Task Admission

Task submission and task execution are distinct.

A task request does not imply successful task acceptance.

Conceptually:

```text
Task Submission
      ↓
Resource Admission
      ↓
Accepted
      ↓
Queued or Running
      ↓
Execution
```

When task capacity is exhausted, the runtime may:

```text
wait
reject
return an explicit failure
```

according to the task contract and resource policy.

A task MUST NOT be reported as successfully accepted when it was silently
discarded.

---

### Bounded Task Execution

Runtime-managed background work MUST have explicit resource bounds.

Potential controls include:

```text
maximum active tasks
maximum queued tasks
per-engine task limits
per-operation task limits
resource quotas
```

Exact numbers and the concrete scheduler remain implementation/configuration
choices.

Unbounded runtime-managed task creation is not a supported default.

---

### Task Queue Backpressure

When task capacity is exhausted:

```text
capacity full
```

the runtime may use:

```text
wait
reject
explicit failure
```

as appropriate to the task contract.

Required or accepted work MUST NOT be silently discarded merely because the
task queue is full.

---

### Background Task Cancellation

Every runtime-managed task MUST have a cancellation path.

Conceptually:

```text
Task
├── owner
├── context
├── cancellation
├── lifecycle
└── completion/failure state
```

Tasks reuse established Core cancellation mechanisms.

Examples:

```text
Engine shutdown
   ↓
engine-owned task cancellation

Operation cancellation
   ↓
operation task cancellation

Stream cancellation
   ↓
stream producer cancellation
```

Phase 12 MUST NOT create a separate cancellation framework for tasks.

---

### Cancellation Hierarchy

Cancellation propagates downward through established ownership and context
relationships.

Conceptually:

```text
Engine Context
      │
      ├── Operation A
      │      ├── Stream A
      │      └── Task A
      │
      └── Operation B
             └── Stream B
```

If the engine is cancelled:

```text
Engine
   ↓
Operations
   ↓
Streams / Tasks
```

If Operation A is cancelled:

```text
Operation A
   ↓
Stream A
   ↓
Task A
```

A child MUST NOT automatically cancel an unrelated sibling.

---

### Owner Termination

Owned streams and tasks MUST NOT become immortal when their owner terminates.

By default:

```text
owner terminated
   ↓
owned stream/task receives cancellation
   ↓
cleanup
```

Independent lifetime is permitted only when an explicit contract establishes
independent ownership and cleanup behavior.

---

### Required and Optional Background Tasks

Background tasks may be explicitly classified as required or optional.

A required task's failure may affect:

```text
engine readiness
health
runtime availability
```

according to explicit runtime or engine policy.

An optional task's failure normally results in:

```text
failure recorded
failure observable
task terminated
```

without automatically terminating the entire engine.

Task criticality MUST NOT be inferred implicitly by the runtime.

---

### Task Failure

A task failure must be recorded and exposed through the appropriate Core
mechanisms.

Task failure does not automatically imply:

```text
engine failure
```

unless the task is explicitly required for engine/runtime operation.

A background-task failure must remain scoped to its task and associated
operation/owner unless an explicit larger-scope contract states otherwise.

---

### Task Restart and Retry Boundary

Phase 12 provides:

```text
task lifecycle
task admission
task failure detection
task failure reporting
task ownership
task cancellation
task shutdown integration
```

Phase 12 does not provide:

```text
retry policy
retry count
backoff
automatic restart policy
idempotency
duplicate detection
```

Automatic task restart/retry belongs to the later Retry and Idempotency
mechanisms.

---

### Configuration Interaction

Background tasks use the established Phase 11 configuration model.

By default, a long-lived task operates against the configuration snapshot
applicable when the task is established.

A task MUST NOT silently begin observing arbitrary configuration changes.

If a configuration field is explicitly runtime-mutable and explicitly declared
observable by the task, the task may follow that documented dynamic behavior.

Configuration updates must not create ambiguous task semantics.

---

### Concurrency

Phase 8 establishes that independent runtime work may execute concurrently.

Phase 12 adds bounded concurrency and resource controls for streams and tasks.

The runtime may execute:

```text
multiple operations
multiple stream producers
multiple background tasks
```

concurrently subject to applicable resource bounds.

Engine-specific state synchronization remains engine-owned.

Phase 12 does not require a specific:

```text
thread model
executor
async runtime
worker pool
scheduler
```

unless explicitly defined elsewhere.

---

### Per-Engine and Global Resource Bounds

The architecture may support both:

```text
global runtime limits
+
engine-specific limits
```

For example:

```text
global maximum active tasks = 1000
engine A maximum active tasks = 100
```

The effective resource capacity must respect both applicable global and
engine-specific bounds.

The exact limit hierarchy and implementation mechanism remain open unless
explicitly frozen.

---

### Fairness

Resource controls should avoid designs that inherently permit unbounded
starvation between engines or operations.

A specific scheduling or fairness algorithm is not frozen by Phase 12.

The implementation may choose an appropriate fairness strategy provided that
resource bounds, ownership, and cancellation guarantees remain intact.

---

### Stream and Task Failure Scope

A stream or task failure is scoped to the affected stream/task and its
associated operation/owner unless an explicit contract makes that work
required for larger runtime availability.

For example:

```text
optional stream failure
→ stream FAILED

optional task failure
→ task FAILED

required runtime task failure
→ may affect health/readiness
```

Neither stream nor task failure automatically crashes the entire engine.

---

### Explicit Non-Goals

Phase 12 must not become:

* transport framing infrastructure;
* message fragmentation/reassembly infrastructure;
* transport retransmission infrastructure;
* retry infrastructure;
* idempotency infrastructure;
* a distributed exactly-once execution system;
* an event bus;
* a pub/sub platform;
* a distributed workflow engine;
* a scheduler product;
* a model inference scheduler;
* a distributed task-orchestration platform;
* a domain-specific task system;
* an engine-specific business workflow system.

Phase 12 provides runtime-level streaming, concurrency, resource control,
task lifecycle, ownership, cancellation, and shutdown integration.

---

### Files and Folders

**Streaming**

* `src/streaming/mod.rs`
* `src/streaming/stream.rs`
* `src/streaming/item.rs`
* `src/streaming/lifecycle.rs`
* `src/streaming/backpressure.rs`
* `src/streaming/context.rs`

**Runtime concurrency and tasks**

* `src/runtime/concurrency.rs`
* `src/runtime/task.rs`
* `src/runtime/background.rs`

**Context and cancellation**

* `src/operation/`
* `src/runtime/`

**Related systems**

* `src/middleware/`
* `src/security/`
* `src/provenance/`
* `src/artifact/`
* `src/health/`
* `src/config/`
* `src/transport/`

**Tests**

* `tests/streaming.rs`
* `tests/concurrency.rs`
* `tests/tasks.rs`
* `tests/phase12_end_to_end`

Exact filenames may be adjusted if the repository already provides equivalent
modules. The architectural boundaries defined in this phase must remain.

---

### Boundary

Phase 7 owns:

```text
binary transport framing
frame headers
fragmentation
reassembly
transport-frame ordering
```

Phase 12 owns:

```text
logical application streams
logical stream-item ordering
stream lifecycle
partial/final semantics
bounded buffering
backpressure
stream cancellation
stream ownership
bounded task execution
task ownership
task lifecycle
task cancellation
resource control
shutdown integration
```

The final hierarchy is:

```text
Application Stream
      ↓
Logical Stream Item
      ↓
Logical Message
      ↓
Phase 7 Fragmentation
      ↓
Transport Frames
```

Transport frame boundaries MUST NOT define application stream boundaries.

Phase 7 MUST remain unaware of engine-specific stream semantics.

Phase 12 MUST remain unaware of the meaning of engine-specific payloads.

---

### Done when

A test engine or Core fixture can:

1. create a stream only from admitted and authorized execution or through an
   explicitly defined independent stream contract;
2. associate every stream with an explicit owner;
3. associate operation-owned streams with the appropriate operation context;
4. produce ordered logical stream items;
5. support partial and final result semantics;
6. represent empty successful streams;
7. enforce the defined stream lifecycle;
8. enforce terminal stream states;
9. make stream termination idempotent;
10. prevent publication after terminal cancellation or completion;
11. prevent Core from intentionally duplicating successfully accepted logical
    stream items;
12. apply bounded stream buffering;
13. apply backpressure when consumers are slower than producers;
14. preserve logical item ordering during backpressure;
15. avoid silently dropping logical stream items;
16. produce an explicit failure, rejection, wait, or cancellation outcome when
    bounded streaming cannot continue;
17. detect consumer disappearance and propagate cancellation to the producer;
18. propagate operation cancellation to operation-owned streams;
19. propagate deadlines through the established Core context;
20. preserve security, provenance, correlation, and operation context;
21. allow concurrent internal computation without violating logical publication
    order;
22. maintain one logical producer and one logical consumer by default;
23. prevent default stream fan-out;
24. distinguish stream items from Phase 7 transport frames;
25. allow one logical stream item to span multiple transport frames;
26. integrate with the Phase 9 security boundary;
27. integrate with Phase 10 provenance without duplicating its mechanisms;
28. create runtime-managed background tasks with explicit ownership;
29. assign ownership before a task is considered runtime-managed;
30. enforce bounded task admission and execution;
31. track task lifecycle and terminal outcomes;
32. provide cancellation for every runtime-managed task;
33. propagate owner cancellation to owned tasks;
34. prevent ownerless/detached runtime tasks;
35. distinguish required and optional background tasks;
36. report task failures without automatically crashing the engine;
37. preserve applicable configuration snapshot semantics for long-lived tasks;
38. integrate streams and tasks with Phase 8 engine shutdown;
39. preserve all previously verified Core behavior.

Unit tests and integration tests must cover:

```text
stream creation and admission
stream ownership
stream lifecycle
terminal-state enforcement
idempotent termination
logical item ordering
partial/final results
empty streams
consumer cancellation
producer cancellation
deadline propagation
bounded buffering
backpressure
no silent stream-item loss
no unintended item duplication
concurrent internal producers
stream/context/security propagation
logical-item vs transport-frame separation
background-task ownership
task admission
task lifecycle
bounded task execution
task cancellation
task failure
required/optional task behavior
owner termination
configuration interaction
engine draining/shutdown
resource limits
integration with the Phase 8 runtime
```

---

### Architectural Invariant

The central invariant of Phase 12 is:

```text
Phase 7
→ defines how logical-message bytes move.

Phase 12
→ defines how logical application work lives over time.
```

Therefore:

```text
Transport fragmentation
≠
Application streaming

Logical stream item
≠
Transport frame

Operation
≠
Background task

Cancellation
≠
Failure

Slow consumer
≠
Failed consumer

Stream ownership
≠
Transport connection ownership
```

Phase 12 must preserve these distinctions throughout implementation.

---

## Phase 13: Retry and Idempotency

### Goal

Make safe repeat execution available once operation, error, runtime,
streaming, artifact, security, configuration, and observability foundations
exist.

Phase 13 provides the shared Core mechanisms for attempt tracking, retry
execution, retryability evaluation, backoff, idempotency, duplicate detection,
unknown-outcome handling, and safe repeat execution.

A retry is a new attempt belonging to the same logical operation.

Phase 13 MUST NOT treat a retry as a new logical operation.

---

### Planned implementation

Add:

```text
attempt tracking
retry execution
retryability classification and evaluation
retry policy enforcement
backoff and jitter
retry limits and budgets
idempotency keys
idempotency state
duplicate detection
in-flight duplicate handling
completed duplicate handling
unknown-outcome handling
retry safety evaluation
operation/runtime integration
artifact/provenance integration
streaming integration
security/context propagation
configuration snapshot propagation
observability integration
````

Phase 13 builds on the existing:

```text
OperationId
Engine Runtime
Error / Status
Security Context
Configuration Snapshot
Streaming
Artifact / Provenance
Observability
Concurrency / Resource limits
```

It MUST NOT replace or create competing versions of those mechanisms.

---

### Operation vs Attempt

An `Operation` represents one logical unit of work requested or established
within Core.

An `Attempt` represents one concrete execution of that logical operation.

Conceptually:

```text
Operation X
├── Attempt 1
├── Attempt 2
├── Attempt 3
└── ...
```

The operation identity remains stable across retries.

Each attempt represents one actual execution and has its own attempt identity
and execution lifecycle.

Retries create additional attempts under the same logical operation.

A retry MUST NOT create a new logical `OperationId` merely because another
attempt is required.

---

### Attempt Identity

Attempt identity and attempt ordering are distinct.

Conceptually:

```text
AttemptId
= unique identity of one concrete execution attempt

AttemptNumber
= ordinal number of the attempt within the logical operation
```

For example:

```text
AttemptId = unique attempt identity
AttemptNumber = 2
```

The exact identifier representation remains an implementation decision.

The existence of an attempt number does not require a specific versioning or
numbering scheme for other Core identities.

---

### Attempt Lifecycle

An attempt follows a non-reopenable lifecycle:

```text
CREATED
   ↓
RUNNING
   ↓
   ├── SUCCEEDED
   ├── FAILED
   └── CANCELLED
```

Terminal states are terminal.

An attempt MUST NOT transition from:

```text
SUCCEEDED → RUNNING
FAILED    → RUNNING
CANCELLED → RUNNING
```

An attempt itself is never "retried".

Instead:

```text
Attempt 1
   ↓
FAILED
   ↓
retry decision
   ↓
Attempt 2
   ↓
CREATED
```

This preserves one execution identity per attempt.

---

### Retryability

Retryability determines whether another attempt is permitted after an attempt
reaches a retry-relevant outcome.

Retryability is not equivalent to generic failure detection.

Conceptually:

```text
Attempt outcome
      ↓
failure/outcome classification
      ↓
retryability evaluation
```

Possible outcome/failure classifications include:

```text
TRANSIENT
PERMANENT
CANCELLED
DEADLINE
AUTHENTICATION
AUTHORIZATION
VALIDATION
RESOURCE_EXHAUSTED
DEPENDENCY
TRANSPORT
ENGINE
UNKNOWN
```

These categories are conceptual.

A category does not automatically imply retryability.

For example:

```text
transient dependency failure
→ may be retryable

permanent dependency configuration failure
→ may be non-retryable
```

Therefore:

```text
Failure Category
      +
Retry Policy / Retryability Metadata
      ↓
Retry Decision
```

---

### Retryability vs Idempotency vs Retry Safety

These are three separate concepts.

```text
Retryability
= whether another attempt may be performed.

Idempotency
= whether repeated submissions of the same logical action can be recognized
  and handled without unintended additional side effects.

Retry Safety
= whether creating another attempt is safe under the operation's current
  execution state and known external effects.
```

For example:

```text
retryable = YES
idempotent = NO
safe to retry = NO
```

This is valid.

Authorization to perform an operation does not automatically make another
attempt safe.

---

### Retry Requested vs Retry Permitted

A request to retry does not imply that a retry is permitted.

```text
Retry Requested
≠
Retry Permitted
```

A retry may be requested by:

```text
caller
operation
runtime policy
capability policy
plan-node policy
```

but Core MUST evaluate all applicable constraints before creating another
attempt.

Conceptually:

```text
failure
 ↓
retry requested?
 ↓
retry policy permits?
 ↓
retry safety permits?
 ↓
retry budget available?
 ↓
deadline permits?
 ↓
cancellation permits?
 ↓
resource capacity available?
 ↓
create next attempt
```

---

### Retry Policy Ownership

Retry policy remains owned by the operation, plan node, or capability that
requires the retry behavior.

Core supplies the execution mechanism and enforces the resulting constraints.

Applicable policies MUST be combined conservatively.

A less restrictive policy MUST NOT override a more restrictive safety
constraint.

Conceptually:

```text
Operation policy
      +
Plan-node constraints
      +
Capability constraints
      ↓
Effective retry policy
```

Where applicable:

```text
Operation: max retries = 5
Plan node: max retries = 3
Capability: max retries = 1
```

results in:

```text
Effective max retries = 1
```

Likewise:

```text
Operation → retry allowed
Capability → retry forbidden
```

results in:

```text
Retry → forbidden
```

The exact policy representation remains an implementation decision.

---

### Retry Safety

A retry MUST consider whether repeating the logical operation can create an
unintended additional effect.

Semantic categories may include:

```text
read-only
pure computation
state-changing
external side effect
```

These categories are descriptive only.

Core MUST NOT infer retry safety solely from an operation name such as
`read`, `create`, `update`, or `delete`.

The actual operation contract determines retry safety.

A potentially side-effecting operation MUST NOT be automatically retried unless
its retry contract provides sufficient idempotency or equivalent safe-repeat
semantics.

Therefore:

```text
retry allowed
+
idempotency/safe-repeat semantics
=
potentially safe
```

while:

```text
retry allowed
+
no protection for side effects
=
do not automatically retry
```

---

### External Effects

An external effect may include:

```text
persistent state mutation
database write
artifact creation/publication
external service call
notification
event emission
observable stream output
other operation-defined side effect
```

The presence of any externally observable effect must be considered when
evaluating whether another attempt can safely execute.

---

### Idempotency

Idempotency allows repeated submissions of the same logical action to be
recognized and handled without unintended additional side effects.

Idempotency MUST NOT be interpreted as requiring every repeated execution to
produce byte-for-byte identical responses.

Instead, it provides safe recognition and handling of repeated logical action
submissions.

For example:

```text
create operation
IdempotencyKey = K
```

If the first attempt succeeds but its response is lost:

```text
Attempt 1
→ side effect succeeded
→ response unavailable
```

a later submission with:

```text
IdempotencyKey = K
```

MUST NOT blindly create a second side effect when the operation's idempotency
contract identifies it as the same logical action.

---

### OperationId vs IdempotencyKey

`OperationId` and `IdempotencyKey` are distinct.

```text
OperationId
= identity of the logical Core operation.

IdempotencyKey
= identity used to recognize repeated submissions of the same intended
  logical action.
```

A retry within one known operation normally remains under the same
`OperationId`.

A repeated client submission may use the same `IdempotencyKey` to associate
it with an existing logical action.

The architecture MUST NOT assume that:

```text
OperationId == IdempotencyKey
```

---

### Idempotency Key Source

An idempotency key may be:

```text
caller supplied
operation supplied
Core/client generated
```

when the applicable contract permits it.

The exact generation mechanism remains an implementation decision.

When an operation relies on idempotency, its idempotency scope and identity
rules MUST be explicit.

---

### Idempotency Scope

An idempotency key is not required to be globally unique across the entire
Nizaam ecosystem.

Conceptually:

```text
Idempotency Identity
=
scope + key
```

The scope may include appropriate operation, capability, service, or other
contract-defined identity.

The exact scope model remains operation-contract specific.

The same key value may therefore be independently valid in two different
scopes.

---

### Idempotency Record

An idempotency record must contain enough information to recognize and safely
handle repeated submissions.

Conceptually:

```text
IdempotencyRecord
├── scope
├── key
├── logical operation identity
├── operation/capability identity
├── current state
├── recorded outcome where available
├── relevant result/reference
└── retention/expiry metadata
```

A record MUST NOT require indefinite in-memory retention of arbitrary large
responses.

Large results may instead be represented by durable references such as
artifact or result references.

---

### Idempotency Retention

Idempotency records MUST have an explicit retention/expiry policy.

The architecture does not freeze a universal retention duration.

The retention scope and duration must be deterministic for the applicable
operation/idempotency contract.

Expired idempotency state MUST NOT silently be treated as proof that an earlier
logical action never occurred.

---

### Duplicate Detection

Duplicate detection MUST primarily use explicit logical action identity,
including the applicable:

```text
IdempotencyKey
+
scope
+
operation/capability identity
+
other required contract identity
```

Core MUST NOT use raw payload equality as the universal duplicate mechanism.

These are not equivalent:

```text
same payload
≠
same logical action
```

and:

```text
different payload
≠
necessarily different logical action
```

`MessageId` and `OperationId` remain semantically distinct from idempotency
identity.

---

### Idempotency Conflict

Reusing an idempotency key for a different logical action within the same
idempotency scope MUST NOT silently map the new request to an older operation.

For example:

```text
K = 123
```

was previously used for:

```text
create user Bob
```

and is later reused for:

```text
delete user Alice
```

The system MUST detect an idempotency conflict or equivalent explicit failure.

Conceptually:

```text
same key
+
different logical action
=
IdempotencyConflict
```

---

### In-Flight Duplicate Handling

If a request arrives using an idempotency identity already associated with an
in-flight logical operation:

```text
Request A
K = 123
→ Attempt 1 running

Request B
K = 123
→ arrives
```

Core MUST detect the existing logical action before creating an unsafe
competing side-effecting execution.

Depending on the operation contract, the duplicate may:

```text
join the existing operation
wait for the existing outcome
observe the existing operation
return an in-progress/duplicate result
```

The exact interaction remains operation-contract specific.

The system MUST NOT blindly start a second side-effecting execution merely
because a duplicate submission arrived.

---

### Completed Idempotent Duplicate Handling

If an idempotent logical action has already completed:

```text
Request A
K = 123
→ SUCCEEDED
```

and a duplicate arrives:

```text
Request B
K = 123
```

the system MUST NOT blindly perform the side effect again.

The duplicate must resolve to the previously established logical outcome
according to the operation's idempotency contract.

Possible mechanisms include:

```text
replay retained result
return durable result/artifact reference
return completion record
return another contract-defined representation of the same outcome
```

Core MUST NOT require indefinite in-memory retention of arbitrary result
bodies.

---

### Failed Idempotent Duplicate Handling

A duplicate of a known completed failure may return the previously recorded
failure when the operation's idempotency contract permits it.

However:

```text
known failure
≠
unknown outcome
```

A failed attempt must not automatically be treated as proof that no side
effect occurred.

---

### Unknown Outcome

The system MUST distinguish:

```text
known success
known failure
unknown outcome
```

An unknown outcome occurs when Core cannot determine whether an operation's
effect actually completed.

Examples include:

```text
response lost
network timeout after remote execution
process failure after an external call
connection failure after side effect completion
```

An unknown outcome MUST NOT automatically be converted to a clean failure
merely to enable retry.

Conceptually:

```text
UNKNOWN
   ↓
reconciliation / idempotency handling
   ↓
determine whether safe continuation is possible
```

---

### Unknown Outcome Safety

For a non-idempotent side effect:

```text
UNKNOWN
   ↓
do not automatically retry
```

For an idempotent operation:

```text
UNKNOWN
   ↓
inspect idempotency state
   ↓
existing completed outcome?
   ├── yes → use existing outcome
   └── no  → operation-specific reconciliation
```

Another attempt may only be created when the applicable contract establishes
that continued execution is safe.

Unknown outcome is therefore a condition requiring reconciliation or an
explicit safe-repeat mechanism.

---

### Retry Decision Algorithm

The normal retry decision flow is conceptually:

```text
Attempt fails
     ↓
Classify outcome
     ↓
Known failure?
     ├── NO → Unknown-outcome handling
     │          ↓
     │      reconciliation / idempotency
     │          ↓
     │      safe continuation?
     │
     └── YES
           ↓
      Retryability check
           ↓
      Retry policy permits?
           ├── NO → stop
           │
           └── YES
                 ↓
           Cancellation active?
                 ├── YES → stop
                 │
                 └── NO
                       ↓
                  Deadline expired?
                       ├── YES → stop
                       │
                       └── NO
                             ↓
                       Retry budget available?
                             ├── NO → stop
                             │
                             └── YES
                                   ↓
                              External effects?
                                   ├── NO → backoff
                                   │         ↓
                                   │      next attempt
                                   │
                                   └── YES
                                         ↓
                                   idempotency / equivalent
                                   safe-repeat semantics available?
                                         ├── NO → stop
                                         │
                                         └── YES
                                               ↓
                                           backoff + jitter
                                               ↓
                                           next attempt
```

Retry MUST NOT be implemented as:

```text
error
 ↓
try again
```

---

### Attempt Limits

The architecture must distinguish:

```text
maximum retries
maximum total attempts
```

The relationship is:

```text
1 original attempt
+
N retries
=
N + 1 total attempts
```

The effective limit must remain bounded.

The exact numerical limits are configuration/policy decisions.

---

### Sequential Retry

Retries are sequential by default.

The normal model is:

```text
Attempt 1
   ↓
terminal attempt outcome
   ↓
retry decision
   ↓
Attempt 2
```

Phase 13 does not introduce concurrent speculative attempts by default.

The following is NOT a default behavior:

```text
Attempt 1 ──┐
            ├── simultaneous execution
Attempt 2 ──┘
```

Hedged or speculative execution remains a future explicit mechanism.

---

### Retry and Concurrent In-Flight Execution

If an earlier attempt may still be running when a duplicate submission arrives,
the idempotency system must check existing state before launching another
side-effecting execution.

An idempotency-aware retry MUST NOT blindly create competing attempts merely
because the caller believes the previous attempt failed.

---

### Retry and Cancellation

Cancellation has higher authority than ordinary retry policy.

Conceptually:

```text
Operation cancelled
   ↓
current attempt cancelled
   ↓
no new automatic retry
```

A caller cancellation MUST NOT automatically trigger another attempt.

Child cancellation must continue to use the established Core cancellation
hierarchy.

---

### Retry and Deadlines

All attempts belonging to one operation normally share the same operation
deadline.

Conceptually:

```text
Operation
   ↓
Operation deadline = T
   ├── Attempt 1
   ├── Attempt 2
   └── Attempt 3
```

A retry does not receive a fresh independent operation lifetime by default.

When the operation deadline expires:

```text
deadline reached
   ↓
no new retry
```

even when retry budget remains.

Deadline therefore takes precedence over further retries.

---

### Retry and Security

Every attempt must retain the appropriate security/delegation context of the
logical operation.

For example:

```text
Attempt 1 → Principal X
Attempt 2 → Principal X
Attempt 3 → Principal X
```

unless an explicitly authorized delegation mechanism defines otherwise.

A retry MUST NOT silently become:

```text
anonymous
different principal
different authorization context
```

Authorization to perform an operation does not automatically imply unlimited
retry authority.

---

### Retry and Configuration

Attempts belonging to the same logical operation normally remain associated
with the same configuration snapshot.

Conceptually:

```text
Operation X
   ↓
Configuration Snapshot N
   ├── Attempt 1
   ├── Attempt 2
   └── Attempt 3
```

A retry must not silently change the operation's configuration semantics merely
because a new runtime configuration snapshot becomes active.

Explicit operation contracts may define another behavior for genuinely
runtime-observable configuration, but this is not the default.

---

### Retry and Streaming

Retry semantics must respect Phase 12 stream semantics.

If an attempt fails before producing externally observable stream output:

```text
no external stream output
   ↓
automatic retry may be possible
```

subject to all other retry-safety conditions.

If an attempt has already emitted externally observable stream items:

```text
partial output exists
   ↓
automatic retry of the same output stream is prohibited by default
```

A retry after partial stream output requires explicit operation/stream
semantics such as:

```text
resume
new stream with explicit retry lineage
deduplication
replacement semantics
other explicitly defined behavior
```

Core MUST NOT blindly restart a partially observed stream and cause duplicate
output.

For example, this is not an acceptable default:

```text
Attempt 1
→ A
→ B
→ C
→ FAILED

Attempt 2
→ A
→ B
→ C
```

The resulting consumer-visible:

```text
A
B
C
A
B
C
```

must not occur merely because Core automatically retried the failed attempt.

---

### Retry and Artifacts

Retry semantics must respect the Phase 10 artifact model.

If an attempt creates or publishes an artifact before the attempt outcome
becomes uncertain:

```text
Attempt 1
→ Artifact A:v1
→ response unavailable
```

a retry MUST NOT simply create another logical artifact version merely because
another attempt is being executed.

The operation's idempotency contract determines whether the retry should:

```text
reuse the existing artifact
return an existing artifact reference
reconcile publication
perform another explicitly safe action
```

Artifact identity and version rules remain owned by the Artifact system.

Phase 13 ensures that retry behavior does not violate those rules.

---

### Retry and Provenance

Retry MUST extend the existing provenance lineage rather than create a new
logical operation lineage.

Conceptually:

```text
Operation X
├── Attempt 1 → FAILED
└── Attempt 2 → SUCCEEDED
```

Where applicable, each attempt may be associated with:

```text
inputs
outputs
engine
capability
execution context
outcome
failure information
```

Provenance must preserve the distinction between:

```text
one logical operation
multiple concrete attempts
```

Retry MUST NOT create unrelated operations merely because multiple attempts
occurred.

---

### Retry and Observability

Observability must preserve retry lineage.

A retry should appear as:

```text
one Operation
   ↓
multiple Attempts
```

rather than multiple unrelated operations.

Conceptually:

```text
OperationId
   ↓
Attempts
   ↓
Trace / spans
```

Metrics and diagnostics may distinguish:

```text
operation count
attempt count
retry count
```

according to the Phase 11 observability model.

The exact exporter/provider remains outside Phase 13.

---

### Retry and Resource Limits

Retries consume ordinary runtime resources.

A retry MUST NOT bypass Phase 12 limits such as:

```text
task limits
concurrency limits
engine limits
stream limits
resource quotas
```

A retry is another actual execution attempt and must be subject to the same
applicable runtime resource controls as other execution.

---

### Retry Budgets

Retry must support bounded aggregate additional work.

Conceptually:

```text
per-operation retry limit
per-engine retry/resource limit
global runtime retry/resource budget
```

The architecture may support a retry budget that limits aggregate retry work.

Exact budget algorithms and numerical values remain configuration/policy
decisions.

The invariant is:

> Retry must not create unbounded additional work.

---

### Backoff

Retries should not normally execute immediately after every retryable
failure.

The backoff mechanism should support:

```text
base delay
attempt-dependent increase
maximum delay
randomized jitter
```

The exact formula remains an implementation/configuration choice.

Randomized jitter should be supported to reduce synchronized retry storms.

---

### Retry Storm Protection

The runtime must protect against retry storms.

For example:

```text
10,000 operations
     ↓
shared dependency failure
     ↓
all retry immediately
     ↓
dependency overloaded further
     ↓
more failures
```

Retry protection should combine appropriate mechanisms such as:

```text
bounded retry counts
backoff
jitter
per-operation limits
per-engine limits
global retry/resource budgets
```

Exact numerical limits are configuration decisions.

Retry MUST NOT bypass existing runtime resource controls.

---

### Side-Effect Safety and Duplicate Prevention

The strongest Phase 13 invariant is:

> A potentially side-effecting operation must not be automatically repeated unless the operation's retry contract establishes sufficient idempotency or equivalent safe-repeat semantics.

This means:

```text
retry authorization
≠
retry safety
```

and:

```text
idempotency
≠
retry policy
```

Both must independently be satisfied where necessary.

---

### Failure and Cancellation Precedence

The runtime must preserve the distinction between:

```text
failure
cancellation
deadline expiration
unknown outcome
```

A caller cancellation must not be converted into a retry request.

A deadline expiration must not produce a fresh retry lifetime.

An unknown outcome must not be converted into a clean failure merely to enable
retry.

The retry mechanism must preserve the meaning of the original outcome.

---

### Idempotency and Large Results

The idempotency system MUST NOT require indefinite in-memory buffering of
large responses or streams.

Large logical outcomes may be represented through:

```text
artifact references
durable result references
completion records
other operation-defined persistent references
```

This keeps Phase 13 compatible with Phase 10 large-artifact handling and
Phase 12 streaming.

---

### Idempotency and Streaming

Idempotency for streaming operations must explicitly account for externally
observable partial output.

A completed duplicate may only replay or reproduce a streaming outcome when
the operation's stream contract provides a safe replay/resume mechanism.

Core MUST NOT imply that all streams are replayable.

An operation that does not define safe stream replay/resume semantics must not
be automatically retried after externally observable stream output.

---

### Idempotency and Artifact Outputs

For artifact-producing operations, idempotency may use the existing Artifact
system to preserve or return the logical artifact identity/reference created
by the original execution.

Retry MUST NOT infer that:

```text
Attempt 2
```

automatically means:

```text
new ArtifactVersion
```

Artifact reuse, deduplication, or new-version creation must follow the
operation's explicit artifact contract.

---

### Storage and Persistence Boundary

Phase 13 defines idempotency and retry semantics but does not require a
specific persistence implementation.

Idempotency state may eventually reside in:

```text
memory
database
distributed store
other provider
```

subject to the operation's required durability and retention guarantees.

The concrete storage provider remains an implementation choice.

---

### Explicit Non-Goals

Phase 13 must not become:

* a distributed transaction system;
* a consensus protocol;
* a distributed exactly-once execution system;
* a workflow engine;
* an automatic side-effect compensation engine;
* a distributed saga/orchestration system;
* a speculative or hedged execution system by default;
* a replacement for the Phase 10 artifact system;
* a replacement for the Phase 12 streaming system;
* a replacement for the Phase 11 observability system;
* a replacement for the Phase 9 security system.

Phase 13 provides retry and idempotency mechanisms without taking ownership of
domain workflow semantics.

---

### Files and Folders

**Retry**

* `src/retry/mod.rs`
* `src/retry/policy.rs`
* `src/retry/attempt.rs`
* `src/retry/backoff.rs`

**Idempotency**

* `src/idempotency/mod.rs`
* `src/idempotency/key.rs`
* `src/idempotency/record.rs`
* `src/idempotency/state.rs`

**Related identity**

* `src/identity/operation.rs`
* `src/identity/plan.rs`

**Related status/error**

* `src/status.rs`
* `src/error/`

**Runtime integration**

* `src/runtime/`
* `src/streaming/`
* `src/artifact/`
* `src/provenance/`
* `src/security/`
* `src/config/`
* `src/observability/`

**Tests**

* `tests/runtime.rs`
* `tests/communication.rs`
* `tests/retry.rs`
* `tests/idempotency.rs`
* `tests/conformance.rs`

Exact filenames may be adjusted if the existing repository already provides
equivalent modules. The architectural boundaries defined in this phase must
remain.

---

### Boundary

Core supplies:

```text
attempt lifecycle
retry execution mechanism
retryability evaluation
backoff
retry limits
retry budgets
idempotency mechanisms
duplicate detection
unknown-outcome handling
retry/runtime integration
```

Retry policy remains owned by the:

```text
operation
plan node
capability
```

that requires it.

The Artifact system remains responsible for artifact identity and version
semantics.

The Streaming system remains responsible for stream lifecycle and logical
stream-item semantics.

The Security system remains responsible for identity and authorization.

The Configuration system remains responsible for configuration snapshots and
runtime configuration semantics.

The Observability system remains responsible for telemetry and diagnostics.

Phase 13 coordinates with these systems but does not replace them.

---

### Done when

A test engine or Core fixture can:

1. represent one logical operation with multiple concrete attempts;
2. assign each attempt its own identity and attempt number;
3. enforce attempt lifecycle and terminal-state behavior;
4. classify attempt outcomes;
5. determine retryability from the applicable failure and policy information;
6. distinguish retryability from retry safety;
7. distinguish retry permission from authorization;
8. combine operation, plan-node, and capability retry constraints
   conservatively;
9. enforce maximum retry/attempt limits;
10. execute retries sequentially by default;
11. apply bounded backoff;
12. apply randomized jitter;
13. enforce retry budgets/resource limits;
14. prevent retry from bypassing Phase 12 concurrency/resource controls;
15. create and validate idempotency identities;
16. distinguish `OperationId` from `IdempotencyKey`;
17. scope idempotency keys deterministically;
18. detect in-flight duplicate submissions;
19. prevent unsafe competing side-effecting executions;
20. handle completed idempotent duplicates without blindly re-executing the
    side effect;
21. resolve completed duplicates to the previously established logical outcome
    according to the operation contract;
22. distinguish known success, known failure, and unknown outcome;
23. prevent blind retry after unknown side effects;
24. perform reconciliation/idempotency handling for unknown outcomes where
    supported;
25. detect idempotency conflicts when the same key is reused for a different
    logical action in the same scope;
26. propagate cancellation through the established Core context;
27. prevent new retries after operation cancellation;
28. preserve one operation-level deadline across all attempts by default;
29. prevent retries after the operation deadline expires;
30. preserve the original security/delegation context across attempts;
31. preserve the applicable configuration snapshot across attempts;
32. preserve retry lineage in observability/provenance;
33. integrate safely with artifact creation/publication and artifact identity;
34. prevent automatic replay of the same stream after externally observable
    partial output;
35. permit retry after partial stream output only when explicit safe
    resume/replay semantics exist;
36. allow idempotency outcomes to use durable result/artifact references rather
    than requiring indefinite in-memory response retention;
37. prevent concurrent speculative attempts by default;
38. preserve all previously verified Core behavior.

Unit tests and integration tests must cover:

```text
operation/attempt relationships
attempt lifecycle
attempt identity
failure classification
retryability
retry policy precedence
maximum retries
maximum attempts
sequential retries
backoff
jitter
retry budgets
cancellation
deadline expiration
security-context preservation
configuration-snapshot preservation
idempotency key handling
idempotency scope
idempotency conflicts
in-flight duplicates
completed duplicates
known failures
unknown outcomes
side-effect safety
artifact interaction
provenance lineage
stream retry protection
partial-output retry behavior
resource-limit enforcement
retry storm protection
observability lineage
```

Integration tests must verify that retry behavior remains correct across the
existing Runtime, Security, Streaming, Artifact/Provenance, Configuration,
and Observability boundaries.

````

The key invariant to keep at the top of your mind while implementing this phase is:

```text
Retry = new attempt
        of the same logical operation

NOT

Retry = new operation
````

And the safety gate is:

```text
failure/outcome
    ↓
retryability
    ↓
policy
    ↓
cancellation/deadline
    ↓
resource budget
    ↓
external effects
    ↓
idempotency / safe-repeat semantics
    ↓
backoff + jitter
    ↓
new attempt
```

---

## Phase 14: Internal Events

### Goal

Provide a small, reusable, lifecycle-aware mechanism for publishing and
consuming internal engine/runtime events.

Phase 14 provides event identity, event publication, subscription, scope,
delivery, cancellation, ownership, ordering, and lifecycle support.

Internal events are optional infrastructure.

Phase 14 MUST NOT require the ecosystem to adopt an event-driven architecture.

---

### Planned implementation

Add:

```text
event identity
event type
event scope
event payload
event publication
event subscription
delivery semantics
subscriber isolation
ordering
bounded event buffering
publisher lifecycle
subscription lifecycle
cancellation
ownership
security integration
context/correlation propagation
runtime shutdown integration
````

The event mechanism builds on the existing:

```text
Phase 5
→ operation/context mechanisms

Phase 8
→ runtime lifecycle and shutdown

Phase 9
→ security and authorization

Phase 10
→ artifact and provenance mechanisms

Phase 11
→ observability and diagnostics

Phase 12
→ bounded task/concurrency mechanisms

Phase 13
→ retry/idempotency mechanisms where explicitly required
```

Phase 14 MUST NOT replace or create competing versions of these mechanisms.

---

### Internal Event Definition

An internal event is a discrete, one-way notification representing that
something happened or became true within an engine or runtime boundary.

Conceptually:

```text
Event
= notification of an occurrence
```

An event is not:

```text
Request
Response
Stream Item
Transport Frame
Log
Metric
Provenance Record
```

The distinction is:

```text
Request / Response
→ command or direct interaction

Stream
→ ordered results belonging to an operation

Event
→ notification that something happened
```

An event does not request the subscriber to perform the original action.

For example:

```text
Request:
IndexDocument

Event:
DocumentIndexed
```

The request asks for work.

The event announces that work or a related occurrence happened.

---

### Event vs Streaming

An event and a stream are separate abstractions.

Streaming:

```text
Operation
   ↓
ordered logical items
   ↓
completion
```

Event:

```text
occurrence
   ↓
published notification
```

Therefore:

```text
Stream Item
≠
Event
```

A stream is associated with an operation and has stream lifecycle semantics.

An internal event is an independent notification occurrence within its
declared scope.

---

### Event vs Transport Message

An internal event may be represented as a logical message when it crosses an
appropriate communication boundary, but the event abstraction is independent
from transport framing.

Conceptually:

```text
Internal Event
     ↓
event message representation
     ↓
Phase 7 framing
     ↓
Transport Frames
```

A purely local event may remain entirely in-process:

```text
Internal Event
     ↓
local publisher/subscriber
```

Phase 14 MUST NOT require every event to use transport.

Phase 7 remains responsible for transport framing, fragmentation, and
reassembly.

---

### Event Ownership and Semantics

Core provides event infrastructure.

Engines and runtime components define:

```text
event meaning
event type
event payload schema
event emission conditions
domain-specific semantics
```

For example:

```text
Arabic Engine
→ ArabicAnalysisCompleted

Knowledge Engine
→ KnowledgeGraphRebuilt
```

Core MUST NOT interpret engine-specific event semantics.

The architectural boundary is:

```text
Core
→ mechanism

Engine
→ semantics
```

---

### Event Identity

Every published event MUST have its own event identity.

Conceptually:

```text
Event
├── EventId
├── EventType
├── Scope
├── Publisher identity
├── Context
├── Payload
└── Metadata
```

`EventId` MUST remain distinct from:

```text
MessageId
OperationId
CorrelationId
IdempotencyKey
```

Conceptually:

```text
EventId
→ identifies one event occurrence

MessageId
→ identifies one logical message

OperationId
→ identifies one logical operation

CorrelationId
→ correlates related activity

IdempotencyKey
→ identifies repeated submissions of the same logical action
```

The event system MUST NOT overload an existing identity merely to avoid
introducing event identity.

---

### Event Type

`EventType` identifies the semantic category of an event.

For example:

```text
quran.index.updated
kg.graph.rebuilt
engine.health.changed
```

The exact naming convention remains a contract/implementation decision.

The distinction is:

```text
EventType
→ what kind of event is this?

EventId
→ which occurrence of that event is this?
```

Two events may therefore share an `EventType` while having different
`EventId` values.

---

### Event Payload

The event payload belongs to the event definition.

For example:

```text
DocumentIndexed
{
    document_id,
    artifact_reference
}
```

Core treats the event payload as opaque with respect to domain semantics.

Core MUST NOT inspect engine-specific fields merely to determine what an event
means.

The engine that defines the event owns the semantic interpretation of its
payload.

---

### Published Event Immutability

Once an event has been published, its logical content MUST be immutable.

Conceptually:

```text
Event
   ↓
published
   ↓
logical content remains unchanged
```

A subscriber MUST NOT be able to mutate the event observed by another
subscriber.

The implementation may use:

```text
owned values
cloning
shared immutable data
reference-counted immutable structures
```

or another appropriate mechanism.

The observable event semantics must remain immutable.

---

### Event Payload Ownership

The implementation SHOULD avoid unnecessarily copying large immutable event
payloads for every subscriber.

Conceptually:

```text
Event
   ↓
shared immutable payload
   ├── Subscriber A
   ├── Subscriber B
   └── Subscriber C
```

The exact ownership and memory strategy remains an implementation decision.

Large managed content SHOULD normally be referenced through appropriate Core
mechanisms rather than embedded directly into an event.

---

### Large Event Payloads

Events should carry notification information and references rather than act as
containers for arbitrarily large content.

For example:

```text
Preferred:

Event
→ ArtifactReference
```

rather than:

```text
Event
→ complete 500 MB dataset
```

Large managed content remains owned by the Artifact system.

This does not establish a new event-specific wire-size protocol.

When an event is transported, the applicable Phase 7 message/frame limits
remain authoritative.

---

### Event Scope

Every event MUST have an explicit semantic scope.

Possible scope categories include:

```text
runtime
engine
operation
capability
explicitly defined local scope
```

Not every event must support every scope.

The event contract determines the scope in which the event is meaningful and
deliverable.

For example:

```text
EngineStarted
→ engine scope

OperationCompleted
→ operation scope

CapabilityUpdated
→ capability scope
```

---

### Subscription Scope

Subscriptions MUST be associated with an explicit event scope.

A subscription to one engine or scope MUST NOT automatically observe unrelated
events from other scopes.

For example:

```text
subscribe(
    event_type = DocumentIndexed,
    scope = EngineA
)
```

does not implicitly subscribe to:

```text
EngineB
EngineC
```

unless explicitly requested and authorized.

---

### Event Filtering

The base event mechanism may support simple filtering by generic properties
such as:

```text
event type
scope
publisher/owner
```

The base event mechanism MUST NOT become a domain-specific event query
language.

Filtering such as:

```text
payload.language == "ar"
&&
payload.surah > 10
```

is outside the generic Core event mechanism.

Core may perform generic filtering without understanding the semantic meaning
of engine payload fields.

---

### Event Publication Model

The base event model uses publisher/subscriber semantics.

Conceptually:

```text
Publisher
   ↓
Event
   ↓
Subscription
   ↓
Subscriber Handler
```

Publication is explicit.

Core MUST NOT automatically convert every:

```text
function call
state change
log entry
```

into an event.

The producer explicitly decides when an event should be published.

---

### Publisher Ownership

Every runtime-managed publisher MUST have an explicit owner.

Possible owners include:

```text
engine
runtime subsystem
other explicitly owned Core component
```

A publisher MUST NOT become ownerless.

Conceptually:

```text
Owner
   ↓
EventPublisher
```

When the owner terminates:

```text
owner terminates
   ↓
publisher closes
   ↓
no new event publication
   ↓
subscriptions terminate according to their lifecycle contract
```

---

### Publisher Lifecycle

The base publisher lifecycle is:

```text
CREATED
   ↓
ACTIVE
   ↓
CLOSED
```

`CLOSED` is terminal.

A closed publisher MUST NOT accept new events.

The event system MUST NOT require an unnecessarily complex publisher state
machine.

---

### Subscription Ownership

Every runtime-managed subscription MUST have an explicit owner.

Conceptually:

```text
Owner
   ↓
Subscription
```

If the owner terminates:

```text
owner terminates
   ↓
subscription cancelled/closed
```

unless an explicit independent ownership contract defines another lifetime.

A subscription MUST NOT become accidentally immortal.

---

### Subscription Lifecycle

The base subscription lifecycle is:

```text
CREATED
   ↓
ACTIVE
   ↓
   ├── CLOSED
   └── CANCELLED
```

Terminal subscription states are terminal.

A subscriber handler failure does not automatically destroy the subscription.

A subscription MAY continue receiving later events after an individual
delivery failure unless its contract or runtime policy explicitly states
otherwise.

---

### Delivery Model

The base event system is push-based.

Conceptually:

```text
Publisher
   ↓
Subscriber Handler
```

A subscriber MUST NOT be able to indefinitely block the publisher or unrelated
subscribers.

Event delivery must be isolated sufficiently that a slow or failed subscriber
does not stop the publisher from serving other subscriptions.

The exact implementation may use:

```text
queues
buffers
tasks
executors
other bounded delivery mechanisms
```

---

### Delivery Guarantees

The default delivery semantic is:

```text
best effort / at-most-once
```

The base mechanism does not guarantee:

```text
every event survives process failure
every event reaches every subscriber
subscriber failure automatically causes redelivery
distributed exactly-once delivery
```

A future event contract may define stronger delivery requirements, but Phase 14
does not implement a distributed exactly-once messaging system.

---

### Event Loss

Event loss is permitted only according to the event's explicit delivery
contract.

The system MUST NOT silently impose a stronger or weaker guarantee than the
event contract specifies.

For example:

```text
non-critical notification
→ best-effort delivery may be appropriate
```

A stronger delivery requirement is not automatically provided by the base
event system.

If guaranteed durable delivery is required, that belongs to a different
architecture rather than being silently added to Phase 14.

---

### Event Backpressure and Overload

The event system MUST use bounded resources.

At minimum, resource usage must be bounded for:

```text
subscription queues
publisher buffers
active subscriptions
subscriber handler concurrency
```

When a subscription cannot keep up:

```text
bounded queue full
      ↓
event delivery policy
```

Possible contract-defined outcomes include:

```text
drop event according to delivery semantics
disconnect subscription
reject publication
other explicitly defined behavior
```

The event system MUST NOT respond to overload by allocating unbounded memory.

Event overload MUST NOT automatically crash the engine unless the relevant
event path is explicitly defined as required runtime work.

---

### Subscriber Isolation

Subscribers are independent delivery targets.

For example:

```text
Event
 ├── Subscriber A
 ├── Subscriber B
 └── Subscriber C
```

A failure in Subscriber A MUST NOT automatically prevent delivery to B or C.

Conceptually:

```text
Subscriber A → failed
Subscriber B → success
Subscriber C → success
```

The publisher remains operational unless the publication mechanism itself has
failed.

---

### Subscriber Concurrency

Different subscriptions MAY process events concurrently.

By default, events delivered through one subscription preserve their publication
order and are handled sequentially with respect to that subscription.

Conceptually:

```text
Subscriber A
A1
↓
handler
↓
A2
↓
handler
↓
A3
```

while:

```text
Subscriber A
+
Subscriber B
+
Subscriber C
```

may execute concurrently.

This provides:

```text
per-subscription ordering
+
cross-subscription concurrency
```

A contract may explicitly permit concurrent handling within a subscription,
but the base event model does not require it.

---

### Event Ordering

Events published by one publisher to one ordered subscription MUST preserve
their publication order by default.

For example:

```text
Publisher
→ A
→ B
→ C
```

results in:

```text
Subscriber
→ A
→ B
→ C
```

The event system does not guarantee global ordering across unrelated
publishers.

For example:

```text
Publisher A → A1
Publisher B → B1
Publisher A → A2
```

does not require a globally defined order between A1, B1, and A2.

Ordering is therefore:

```text
per publisher/subscription
```

unless a broader ordering contract is explicitly defined.

---

### Event Cancellation

Event subscriptions use the established Core cancellation mechanisms.

Conceptually:

```text
Owner / Context
      ↓
Subscription
      ↓
Subscriber Handler
```

Cancellation terminates the subscription according to its lifecycle.

Phase 14 MUST NOT create a competing event-specific cancellation framework.

---

### Event Context

Events may carry only the context needed for safe correlation and authorized
handling.

Relevant context may include:

```text
OperationId
CorrelationId
EngineId
CapabilityId
security information where appropriate
tracing information where appropriate
```

The entire runtime context MUST NOT automatically be copied into every event.

Event context should remain minimal and purpose-specific.

---

### Event Security

Internal status does not imply unrestricted trust.

A subscriber MUST only receive events that it is authorized to observe.

Conceptually:

```text
Event Published
      ↓
Subscription Scope
      ↓
Authorization
      ↓
Delivery
```

Security uses the existing Phase 9 security system.

Phase 14 MUST NOT create an independent authorization framework.

---

### Event Payload Confidentiality

Event payloads MUST NOT expose secrets or sensitive information merely
because an event is internal.

Event producers remain responsible for safe event definitions.

Core observability mechanisms should also respect the Phase 11 redaction
requirements.

---

### Events and Observability

An event is distinct from an observability signal.

```text
Event
≠
Log

Event
≠
Metric

Event
≠
Trace
```

An event may itself generate logs, metrics, traces, or diagnostics, but those
are separate representations serving different purposes.

Phase 11 remains responsible for observability.

---

### Events and Provenance

Events are notifications.

Provenance records historical relationships and execution facts.

Therefore:

```text
Event
→ notification

Provenance
→ historical relationship
```

An event may refer to an operation, artifact, or execution, but it does not
replace the Phase 10 provenance record.

For example:

```text
Event:
ModelPublished
```

may coexist with:

```text
Provenance:
Operation X
→ produced Model:v7
```

---

### Events and Retry

Phase 14 MUST NOT create an independent retry framework for subscriber
delivery.

If a subscriber handler fails:

```text
Event
 ↓
Subscriber
 ↓
handler failure
```

the failure may be reported through existing observability/diagnostic
mechanisms.

Explicit retry behavior uses the Phase 13 retry/idempotency mechanisms when
the applicable contract requires it.

The event system itself does not silently add:

```text
retry
backoff
jitter
retry budgets
```

---

### Events and Idempotency

The event system does not provide a separate idempotency framework.

If an event consumer requires idempotent handling, it uses the established
Phase 13 mechanisms.

The distinction remains:

```text
EventId
→ event occurrence identity

IdempotencyKey
→ repeated logical action identity
```

They MUST NOT be treated as interchangeable.

---

### Event Persistence

Internal events are not durable by default.

A publisher or process failure may result in undelivered events being lost.

The base event system does not require:

```text
persistent event log
durable event storage
consumer offsets
checkpoints
recovery history
```

Durable event infrastructure is outside Phase 14.

---

### Event Replay

Replay is not part of the base internal event mechanism.

The base mechanism does not require:

```text
replay cursors
consumer offsets
historical event log
checkpoint recovery
```

A future durable/replayable event architecture can be introduced separately.

---

### Event Acknowledgement

The base event mechanism does not require subscriber acknowledgements such as:

```text
ACK
NACK
delivery confirmation
consumer offset
```

Introducing acknowledgements would imply a larger reliability/redelivery
system that is intentionally outside Phase 14.

---

### Event and Internal Scope

The word `internal` is significant.

Phase 14 primarily supports:

```text
within an engine
within a runtime
between related internal components
```

An internal event is not automatically a public ecosystem-wide event API.

Events that need to cross a formal engine/platform communication boundary
must use an explicitly defined communication mechanism.

Phase 14 does not automatically route internal events through the Control
Plane.

---

### In-Process vs Distributed Events

Phase 14 provides a transport-independent event abstraction.

The base implementation is local/in-process and does not require distributed
event infrastructure or durable transport.

Conceptually:

```text
Internal Event API
      ↓
local event implementation
```

A future implementation may bridge events to another communication system,
but such bridging requires an explicit architectural decision.

Phase 14 is not a distributed message broker.

---

### Event and Control Plane Boundary

The internal event system and Control Plane are separate.

```text
Internal Event
→ local/internal notificationAbsolutely bro. 😎 Since we already verified that **nothing meaningful is left in Phase 14**, let's freeze it with the final specification.

````markdown
## Phase 14: Internal Events

### Goal

Provide a small, reusable, lifecycle-aware mechanism for publishing and
consuming internal engine/runtime events.

Phase 14 provides event identity, event publication, subscription, scope,
delivery, cancellation, ownership, ordering, and lifecycle support.

Internal events are optional infrastructure.

Phase 14 MUST NOT require the ecosystem to adopt an event-driven architecture.

---

### Planned implementation

Add:

```text
event identity
event type
event scope
event payload
event publication
event subscription
delivery semantics
subscriber isolation
ordering
bounded event buffering
publisher lifecycle
subscription lifecycle
cancellation
ownership
security integration
context/correlation propagation
runtime shutdown integration
````

The event mechanism builds on the existing:

```text
Phase 5
→ operation/context mechanisms

Phase 8
→ runtime lifecycle and shutdown

Phase 9
→ security and authorization

Phase 10
→ artifact and provenance mechanisms

Phase 11
→ observability and diagnostics

Phase 12
→ bounded task/concurrency mechanisms

Phase 13
→ retry/idempotency mechanisms where explicitly required
```

Phase 14 MUST NOT replace or create competing versions of these mechanisms.

---

### Internal Event Definition

An internal event is a discrete, one-way notification representing that
something happened or became true within an engine or runtime boundary.

Conceptually:

```text
Event
= notification of an occurrence
```

An event is not:

```text
Request
Response
Stream Item
Transport Frame
Log
Metric
Provenance Record
```

The distinction is:

```text
Request / Response
→ command or direct interaction

Stream
→ ordered results belonging to an operation

Event
→ notification that something happened
```

An event does not request the subscriber to perform the original action.

For example:

```text
Request:
IndexDocument

Event:
DocumentIndexed
```

The request asks for work.

The event announces that work or a related occurrence happened.

---

### Event vs Streaming

An event and a stream are separate abstractions.

Streaming:

```text
Operation
   ↓
ordered logical items
   ↓
completion
```

Event:

```text
occurrence
   ↓
published notification
```

Therefore:

```text
Stream Item
≠
Event
```

A stream is associated with an operation and has stream lifecycle semantics.

An internal event is an independent notification occurrence within its
declared scope.

---

### Event vs Transport Message

An internal event may be represented as a logical message when it crosses an
appropriate communication boundary, but the event abstraction is independent
from transport framing.

Conceptually:

```text
Internal Event
     ↓
event message representation
     ↓
Phase 7 framing
     ↓
Transport Frames
```

A purely local event may remain entirely in-process:

```text
Internal Event
     ↓
local publisher/subscriber
```

Phase 14 MUST NOT require every event to use transport.

Phase 7 remains responsible for transport framing, fragmentation, and
reassembly.

---

### Event Ownership and Semantics

Core provides event infrastructure.

Engines and runtime components define:

```text
event meaning
event type
event payload schema
event emission conditions
domain-specific semantics
```

For example:

```text
Arabic Engine
→ ArabicAnalysisCompleted

Knowledge Engine
→ KnowledgeGraphRebuilt
```

Core MUST NOT interpret engine-specific event semantics.

The architectural boundary is:

```text
Core
→ mechanism

Engine
→ semantics
```

---

### Event Identity

Every published event MUST have its own event identity.

Conceptually:

```text
Event
├── EventId
├── EventType
├── Scope
├── Publisher identity
├── Context
├── Payload
└── Metadata
```

`EventId` MUST remain distinct from:

```text
MessageId
OperationId
CorrelationId
IdempotencyKey
```

Conceptually:

```text
EventId
→ identifies one event occurrence

MessageId
→ identifies one logical message

OperationId
→ identifies one logical operation

CorrelationId
→ correlates related activity

IdempotencyKey
→ identifies repeated submissions of the same logical action
```

The event system MUST NOT overload an existing identity merely to avoid
introducing event identity.

---

### Event Type

`EventType` identifies the semantic category of an event.

For example:

```text
quran.index.updated
kg.graph.rebuilt
engine.health.changed
```

The exact naming convention remains a contract/implementation decision.

The distinction is:

```text
EventType
→ what kind of event is this?

EventId
→ which occurrence of that event is this?
```

Two events may therefore share an `EventType` while having different
`EventId` values.

---

### Event Payload

The event payload belongs to the event definition.

For example:

```text
DocumentIndexed
{
    document_id,
    artifact_reference
}
```

Core treats the event payload as opaque with respect to domain semantics.

Core MUST NOT inspect engine-specific fields merely to determine what an event
means.

The engine that defines the event owns the semantic interpretation of its
payload.

---

### Published Event Immutability

Once an event has been published, its logical content MUST be immutable.

Conceptually:

```text
Event
   ↓
published
   ↓
logical content remains unchanged
```

A subscriber MUST NOT be able to mutate the event observed by another
subscriber.

The implementation may use:

```text
owned values
cloning
shared immutable data
reference-counted immutable structures
```

or another appropriate mechanism.

The observable event semantics must remain immutable.

---

### Event Payload Ownership

The implementation SHOULD avoid unnecessarily copying large immutable event
payloads for every subscriber.

Conceptually:

```text
Event
   ↓
shared immutable payload
   ├── Subscriber A
   ├── Subscriber B
   └── Subscriber C
```

The exact ownership and memory strategy remains an implementation decision.

Large managed content SHOULD normally be referenced through appropriate Core
mechanisms rather than embedded directly into an event.

---

### Large Event Payloads

Events should carry notification information and references rather than act as
containers for arbitrarily large content.

For example:

```text
Preferred:

Event
→ ArtifactReference
```

rather than:

```text
Event
→ complete 500 MB dataset
```

Large managed content remains owned by the Artifact system.

This does not establish a new event-specific wire-size protocol.

When an event is transported, the applicable Phase 7 message/frame limits
remain authoritative.

---

### Event Scope

Every event MUST have an explicit semantic scope.

Possible scope categories include:

```text
runtime
engine
operation
capability
explicitly defined local scope
```

Not every event must support every scope.

The event contract determines the scope in which the event is meaningful and
deliverable.

For example:

```text
EngineStarted
→ engine scope

OperationCompleted
→ operation scope

CapabilityUpdated
→ capability scope
```

---

### Subscription Scope

Subscriptions MUST be associated with an explicit event scope.

A subscription to one engine or scope MUST NOT automatically observe unrelated
events from other scopes.

For example:

```text
subscribe(
    event_type = DocumentIndexed,
    scope = EngineA
)
```

does not implicitly subscribe to:

```text
EngineB
EngineC
```

unless explicitly requested and authorized.

---

### Event Filtering

The base event mechanism may support simple filtering by generic properties
such as:

```text
event type
scope
publisher/owner
```

The base event mechanism MUST NOT become a domain-specific event query
language.

Filtering such as:

```text
payload.language == "ar"
&&
payload.surah > 10
```

is outside the generic Core event mechanism.

Core may perform generic filtering without understanding the semantic meaning
of engine payload fields.

---

### Event Publication Model

The base event model uses publisher/subscriber semantics.

Conceptually:

```text
Publisher
   ↓
Event
   ↓
Subscription
   ↓
Subscriber Handler
```

Publication is explicit.

Core MUST NOT automatically convert every:

```text
function call
state change
log entry
```

into an event.

The producer explicitly decides when an event should be published.

---

### Publisher Ownership

Every runtime-managed publisher MUST have an explicit owner.

Possible owners include:

```text
engine
runtime subsystem
other explicitly owned Core component
```

A publisher MUST NOT become ownerless.

Conceptually:

```text
Owner
   ↓
EventPublisher
```

When the owner terminates:

```text
owner terminates
   ↓
publisher closes
   ↓
no new event publication
   ↓
subscriptions terminate according to their lifecycle contract
```

---

### Publisher Lifecycle

The base publisher lifecycle is:

```text
CREATED
   ↓
ACTIVE
   ↓
CLOSED
```

`CLOSED` is terminal.

A closed publisher MUST NOT accept new events.

The event system MUST NOT require an unnecessarily complex publisher state
machine.

---

### Subscription Ownership

Every runtime-managed subscription MUST have an explicit owner.

Conceptually:

```text
Owner
   ↓
Subscription
```

If the owner terminates:

```text
owner terminates
   ↓
subscription cancelled/closed
```

unless an explicit independent ownership contract defines another lifetime.

A subscription MUST NOT become accidentally immortal.

---

### Subscription Lifecycle

The base subscription lifecycle is:

```text
CREATED
   ↓
ACTIVE
   ↓
   ├── CLOSED
   └── CANCELLED
```

Terminal subscription states are terminal.

A subscriber handler failure does not automatically destroy the subscription.

A subscription MAY continue receiving later events after an individual
delivery failure unless its contract or runtime policy explicitly states
otherwise.

---

### Delivery Model

The base event system is push-based.

Conceptually:

```text
Publisher
   ↓
Subscriber Handler
```

A subscriber MUST NOT be able to indefinitely block the publisher or unrelated
subscribers.

Event delivery must be isolated sufficiently that a slow or failed subscriber
does not stop the publisher from serving other subscriptions.

The exact implementation may use:

```text
queues
buffers
tasks
executors
other bounded delivery mechanisms
```

---

### Delivery Guarantees

The default delivery semantic is:

```text
best effort / at-most-once
```

The base mechanism does not guarantee:

```text
every event survives process failure
every event reaches every subscriber
subscriber failure automatically causes redelivery
distributed exactly-once delivery
```

A future event contract may define stronger delivery requirements, but Phase 14
does not implement a distributed exactly-once messaging system.

---

### Event Loss

Event loss is permitted only according to the event's explicit delivery
contract.

The system MUST NOT silently impose a stronger or weaker guarantee than the
event contract specifies.

For example:

```text
non-critical notification
→ best-effort delivery may be appropriate
```

A stronger delivery requirement is not automatically provided by the base
event system.

If guaranteed durable delivery is required, that belongs to a different
architecture rather than being silently added to Phase 14.

---

### Event Backpressure and Overload

The event system MUST use bounded resources.

At minimum, resource usage must be bounded for:

```text
subscription queues
publisher buffers
active subscriptions
subscriber handler concurrency
```

When a subscription cannot keep up:

```text
bounded queue full
      ↓
event delivery policy
```

Possible contract-defined outcomes include:

```text
drop event according to delivery semantics
disconnect subscription
reject publication
other explicitly defined behavior
```

The event system MUST NOT respond to overload by allocating unbounded memory.

Event overload MUST NOT automatically crash the engine unless the relevant
event path is explicitly defined as required runtime work.

---

### Subscriber Isolation

Subscribers are independent delivery targets.

For example:

```text
Event
 ├── Subscriber A
 ├── Subscriber B
 └── Subscriber C
```

A failure in Subscriber A MUST NOT automatically prevent delivery to B or C.

Conceptually:

```text
Subscriber A → failed
Subscriber B → success
Subscriber C → success
```

The publisher remains operational unless the publication mechanism itself has
failed.

---

### Subscriber Concurrency

Different subscriptions MAY process events concurrently.

By default, events delivered through one subscription preserve their publication
order and are handled sequentially with respect to that subscription.

Conceptually:

```text
Subscriber A
A1
↓
handler
↓
A2
↓
handler
↓
A3
```

while:

```text
Subscriber A
+
Subscriber B
+
Subscriber C
```

may execute concurrently.

This provides:

```text
per-subscription ordering
+
cross-subscription concurrency
```

A contract may explicitly permit concurrent handling within a subscription,
but the base event model does not require it.

---

### Event Ordering

Events published by one publisher to one ordered subscription MUST preserve
their publication order by default.

For example:

```text
Publisher
→ A
→ B
→ C
```

results in:

```text
Subscriber
→ A
→ B
→ C
```

The event system does not guarantee global ordering across unrelated
publishers.

For example:

```text
Publisher A → A1
Publisher B → B1
Publisher A → A2
```

does not require a globally defined order between A1, B1, and A2.

Ordering is therefore:

```text
per publisher/subscription
```

unless a broader ordering contract is explicitly defined.

---

### Event Cancellation

Event subscriptions use the established Core cancellation mechanisms.

Conceptually:

```text
Owner / Context
      ↓
Subscription
      ↓
Subscriber Handler
```

Cancellation terminates the subscription according to its lifecycle.

Phase 14 MUST NOT create a competing event-specific cancellation framework.

---

### Event Context

Events may carry only the context needed for safe correlation and authorized
handling.

Relevant context may include:

```text
OperationId
CorrelationId
EngineId
CapabilityId
security information where appropriate
tracing information where appropriate
```

The entire runtime context MUST NOT automatically be copied into every event.

Event context should remain minimal and purpose-specific.

---

### Event Security

Internal status does not imply unrestricted trust.

A subscriber MUST only receive events that it is authorized to observe.

Conceptually:

```text
Event Published
      ↓
Subscription Scope
      ↓
Authorization
      ↓
Delivery
```

Security uses the existing Phase 9 security system.

Phase 14 MUST NOT create an independent authorization framework.

---

### Event Payload Confidentiality

Event payloads MUST NOT expose secrets or sensitive information merely
because an event is internal.

Event producers remain responsible for safe event definitions.

Core observability mechanisms should also respect the Phase 11 redaction
requirements.

---

### Events and Observability

An event is distinct from an observability signal.

```text
Event
≠
Log

Event
≠
Metric

Event
≠
Trace
```

An event may itself generate logs, metrics, traces, or diagnostics, but those
are separate representations serving different purposes.

Phase 11 remains responsible for observability.

---

### Events and Provenance

Events are notifications.

Provenance records historical relationships and execution facts.

Therefore:

```text
Event
→ notification

Provenance
→ historical relationship
```

An event may refer to an operation, artifact, or execution, but it does not
replace the Phase 10 provenance record.

For example:

```text
Event:
ModelPublished
```

may coexist with:

```text
Provenance:
Operation X
→ produced Model:v7
```

---

### Events and Retry

Phase 14 MUST NOT create an independent retry framework for subscriber
delivery.

If a subscriber handler fails:

```text
Event
 ↓
Subscriber
 ↓
handler failure
```

the failure may be reported through existing observability/diagnostic
mechanisms.

Explicit retry behavior uses the Phase 13 retry/idempotency mechanisms when
the applicable contract requires it.

The event system itself does not silently add:

```text
retry
backoff
jitter
retry budgets
```

---

### Events and Idempotency

The event system does not provide a separate idempotency framework.

If an event consumer requires idempotent handling, it uses the established
Phase 13 mechanisms.

The distinction remains:

```text
EventId
→ event occurrence identity

IdempotencyKey
→ repeated logical action identity
```

They MUST NOT be treated as interchangeable.

---

### Event Persistence

Internal events are not durable by default.

A publisher or process failure may result in undelivered events being lost.

The base event system does not require:

```text
persistent event log
durable event storage
consumer offsets
checkpoints
recovery history
```

Durable event infrastructure is outside Phase 14.

---

### Event Replay

Replay is not part of the base internal event mechanism.

The base mechanism does not require:

```text
replay cursors
consumer offsets
historical event log
checkpoint recovery
```

A future durable/replayable event architecture can be introduced separately.

---

### Event Acknowledgement

The base event mechanism does not require subscriber acknowledgements such as:

```text
ACK
NACK
delivery confirmation
consumer offset
```

Introducing acknowledgements would imply a larger reliability/redelivery
system that is intentionally outside Phase 14.

---

### Event and Internal Scope

The word `internal` is significant.

Phase 14 primarily supports:

```text
within an engine
within a runtime
between related internal components
```

An internal event is not automatically a public ecosystem-wide event API.

Events that need to cross a formal engine/platform communication boundary
must use an explicitly defined communication mechanism.

Phase 14 does not automatically route internal events through the Control
Plane.

---

### In-Process vs Distributed Events

Phase 14 provides a transport-independent event abstraction.

The base implementation is local/in-process and does not require distributed
event infrastructure or durable transport.

Conceptually:

```text
Internal Event API
      ↓
local event implementation
```

A future implementation may bridge events to another communication system,
but such bridging requires an explicit architectural decision.

Phase 14 is not a distributed message broker.

---

### Event and Control Plane Boundary

The internal event system and Control Plane are separate.

```text
Internal Event
→ local/internal notification

Control Plane
→ formal communication and routing
```

Therefore:

```text
Internal Event
≠
Control Plane Message
```

The Control Plane MUST NOT automatically become the transport for every internal
event.

---

### Event Publication and Domain Logic

Core MUST NOT invent domain events.

The producer explicitly determines:

```text
when an event occurs
what event type represents it
what payload it contains
```

Core only provides the infrastructure required to publish and deliver it.

This prevents the generic event system from silently becoming a domain workflow
engine.

---

### Event and Large Managed Content

Events should reference managed content rather than embedding arbitrarily
large content.

For example:

```text
Event
├── EventType
├── EventId
└── ArtifactReference
```

rather than embedding the complete artifact.

The Phase 10 Artifact system remains responsible for:

```text
artifact identity
artifact versions
content access
integrity
lifecycle
```

---

### Publisher Shutdown

When an engine/runtime begins shutdown:

```text
Engine
   ↓
DRAINING
   ↓
stop accepting new work
   ↓
publisher closes according to lifecycle policy
   ↓
subscriptions close/cancel according to contract
   ↓
owned handlers/tasks terminate
   ↓
STOPPED
```

The event mechanism integrates with Phase 8 lifecycle and does not introduce
an independent shutdown state machine.

Shutdown delivery behavior must be deterministic.

Whether already queued events are drained or abandoned is governed by the
applicable delivery/lifecycle contract rather than being implicitly
unbounded.

---

### Event Resource Limits

The event system must prevent accidental unbounded resource growth.

Potential limits include:

```text
maximum active publishers
maximum active subscriptions
maximum subscription queue depth
maximum concurrent subscriber handlers
maximum event payload/resource size
```

Exact numeric limits remain implementation/configuration choices.

---

### Event Size Boundary

Phase 14 does not establish a separate wire-level message size protocol.

For local events, implementation limits may be imposed for safe resource use.

For transported events:

```text
Phase 7 message/frame constraints
```

remain authoritative.

The event system MUST NOT reinterpret a transport frame as a separate event.

---

### Explicit Non-Goals

Phase 14 must not become:

* a mandatory event-driven architecture;
* a distributed message broker;
* Kafka-like durable event infrastructure;
* a persistent event log;
* a pub/sub platform;
* a distributed event-replay system;
* a distributed exactly-once delivery system;
* a guaranteed-durable messaging system;
* a separate retry system;
* a separate idempotency system;
* a workflow engine;
* a domain event-processing engine;
* a Control Plane routing mechanism;
* a transport framing system;
* a replacement for the Phase 10 provenance system;
* a replacement for the Phase 11 observability system;
* a replacement for the Phase 12 streaming system.

---

### Files and Folders

**Events**

* `src/events/mod.rs`
* `src/events/event.rs`
* `src/events/publisher.rs`
* `src/events/subscriber.rs`

**Event lifecycle / delivery**

* `src/events/lifecycle.rs`
* `src/events/delivery.rs`
* `src/events/scope.rs`

**Context integration**

* `src/operation/`
* `src/runtime/`
* `src/security/context.rs`

**Related systems**

* `src/provenance/`
* `src/artifact/`
* `src/observability/`
* `src/retry/`
* `src/idempotency/`
* `src/transport/`

**Tests**

* `tests/events.rs`
* `tests/runtime.rs`
* `tests/conformance.rs`

Exact filenames may be adjusted if the repository already provides
equivalent modules. The architectural boundaries defined in this phase must
remain.

---

### Boundary

Core provides:

```text
event identity mechanisms
event publication
subscription
scope handling
delivery infrastructure
bounded buffering/resource controls
publisher lifecycle
subscription lifecycle
cancellation integration
ownership enforcement
security integration
context propagation
runtime shutdown integration
```

Engines provide:

```text
event meaning
event type definitions
payload schema
event emission conditions
domain semantics
```

The event system remains optional infrastructure.

Internal events are not automatically public API, distributed messages,
durable records, workflow triggers, or historical provenance records.

---

### Done when

A test engine or Core fixture can:

1. define an internal event type;
2. create a uniquely identified event occurrence;
3. publish an event explicitly;
4. associate the event with an explicit scope;
5. associate the event with an explicit publisher/owner;
6. subscribe to an event by supported type/scope;
7. apply simple generic event filtering;
8. deliver events using the default best-effort/at-most-once semantics;
9. preserve per-publisher/per-subscription event ordering;
10. process independent subscriptions concurrently;
11. isolate subscriber failures from unrelated subscribers;
12. prevent one slow subscriber from indefinitely blocking the publisher or
    unrelated subscriptions;
13. enforce bounded subscription/publisher resource usage;
14. apply the event's explicit overload/delivery policy when a subscriber cannot
    keep up;
15. maintain immutable logical event content after publication;
16. maintain distinct EventId, MessageId, OperationId, CorrelationId, and
    IdempotencyKey semantics;
17. propagate only the relevant context needed for correlation and authorized
    handling;
18. enforce Phase 9 security before delivering scoped internal events;
19. cancel subscriptions through established Core cancellation mechanisms;
20. terminate owned publishers/subscriptions when their owners terminate;
21. integrate event lifecycle with Phase 8 engine shutdown;
22. distinguish event publication failure from subscriber handler failure;
23. avoid making subscriber failure automatically destroy the subscription;
24. avoid durable persistence, replay, acknowledgement, or automatic retry in
    the base mechanism;
25. integrate with Phase 13 retry/idempotency when an explicit consumer
    contract requires it;
26. integrate with Phase 10 artifact/provenance mechanisms without duplicating
    their responsibilities;
27. integrate with Phase 11 observability without turning events into logs or
    metrics;
28. distinguish internal events from Phase 12 stream items;
29. distinguish internal events from Phase 7 transport frames/messages;
30. remain transport-independent for local event usage;
31. preserve all previously verified Core behavior.

Unit tests and integration tests must cover:

```text
event identity
event type
event scope
publisher ownership
subscription ownership
publication
subscription
basic filtering
delivery semantics
ordering
cross-subscription concurrency
subscriber isolation
bounded queues/resources
overload behavior
payload immutability
cancellation
owner termination
publisher shutdown
subscription shutdown
security enforcement
context propagation
observability integration
provenance/artifact references
retry boundary
idempotency boundary
transport boundary
engine/runtime lifecycle integration
```

---

### Architectural Invariant

The central invariant of Phase 14 is:

```text
Phase 14
→ announces internal occurrences.

It does not
→ define domain meaning,
→ provide durable distributed messaging,
→ execute workflows,
→ replace streams,
→ replace provenance,
→ replace observability,
→ replace retry/idempotency.
```

The fundamental abstraction remains:

```text
Engine / Runtime
      │
      │ publish
      ↓
    Event
      │
      ↓
 Subscription
      │
      ↓
 Subscriber Handler
```

with:

```text
best-effort / at-most-once by default
per-publisher/subscription ordering
cross-subscription concurrency
bounded resources
no durability by default
no replay by default
no acknowledgement protocol
no automatic retry
```

Phase 14 is intentionally a small, lifecycle-aware internal notification
mechanism rather than a general-purpose messaging platform.

```

This is consistent with the current scope's explicit positioning of Phase 14 as **optional infrastructure rather than a required event-driven architecture**, while keeping event definitions engine-owned. :contentReference[oaicite:0]{index=0}

I would freeze this version and move on to **Phase 15: Control Plane**, which is the next phase where the architectural discussion becomes substantially more important.
```

Control Plane
→ formal communication and routing

```

Therefore:

```text
Internal Event
≠
Control Plane Message
```

The Control Plane MUST NOT automatically become the transport for every internal
event.

---

### Event Publication and Domain Logic

Core MUST NOT invent domain events.

The producer explicitly determines:

```text
when an event occurs
what event type represents it
what payload it contains
```

Core only provides the infrastructure required to publish and deliver it.

This prevents the generic event system from silently becoming a domain workflow
engine.

---

### Event and Large Managed Content

Events should reference managed content rather than embedding arbitrarily
large content.

For example:

```text
Event
├── EventType
├── EventId
└── ArtifactReference
```

rather than embedding the complete artifact.

The Phase 10 Artifact system remains responsible for:

```text
artifact identity
artifact versions
content access
integrity
lifecycle
```

---

### Publisher Shutdown

When an engine/runtime begins shutdown:

```text
Engine
   ↓
DRAINING
   ↓
stop accepting new work
   ↓
publisher closes according to lifecycle policy
   ↓
subscriptions close/cancel according to contract
   ↓
owned handlers/tasks terminate
   ↓
STOPPED
```

The event mechanism integrates with Phase 8 lifecycle and does not introduce
an independent shutdown state machine.

Shutdown delivery behavior must be deterministic.

Whether already queued events are drained or abandoned is governed by the
applicable delivery/lifecycle contract rather than being implicitly
unbounded.

---

### Event Resource Limits

The event system must prevent accidental unbounded resource growth.

Potential limits include:

```text
maximum active publishers
maximum active subscriptions
maximum subscription queue depth
maximum concurrent subscriber handlers
maximum event payload/resource size
```

Exact numeric limits remain implementation/configuration choices.

---

### Event Size Boundary

Phase 14 does not establish a separate wire-level message size protocol.

For local events, implementation limits may be imposed for safe resource use.

For transported events:

```text
Phase 7 message/frame constraints
```

remain authoritative.

The event system MUST NOT reinterpret a transport frame as a separate event.

---

### Explicit Non-Goals

Phase 14 must not become:

* a mandatory event-driven architecture;
* a distributed message broker;
* Kafka-like durable event infrastructure;
* a persistent event log;
* a pub/sub platform;
* a distributed event-replay system;
* a distributed exactly-once delivery system;
* a guaranteed-durable messaging system;
* a separate retry system;
* a separate idempotency system;
* a workflow engine;
* a domain event-processing engine;
* a Control Plane routing mechanism;
* a transport framing system;
* a replacement for the Phase 10 provenance system;
* a replacement for the Phase 11 observability system;
* a replacement for the Phase 12 streaming system.

---

### Files and Folders

**Events**

* `src/events/mod.rs`
* `src/events/event.rs`
* `src/events/publisher.rs`
* `src/events/subscriber.rs`

**Event lifecycle / delivery**

* `src/events/lifecycle.rs`
* `src/events/delivery.rs`
* `src/events/scope.rs`

**Context integration**

* `src/operation/`
* `src/runtime/`
* `src/security/context.rs`

**Related systems**

* `src/provenance/`
* `src/artifact/`
* `src/observability/`
* `src/retry/`
* `src/idempotency/`
* `src/transport/`

**Tests**

* `tests/events.rs`
* `tests/runtime.rs`
* `tests/conformance.rs`

Exact filenames may be adjusted if the repository already provides
equivalent modules. The architectural boundaries defined in this phase must
remain.

---

### Boundary

Core provides:

```text
event identity mechanisms
event publication
subscription
scope handling
delivery infrastructure
bounded buffering/resource controls
publisher lifecycle
subscription lifecycle
cancellation integration
ownership enforcement
security integration
context propagation
runtime shutdown integration
```

Engines provide:

```text
event meaning
event type definitions
payload schema
event emission conditions
domain semantics
```

The event system remains optional infrastructure.

Internal events are not automatically public API, distributed messages,
durable records, workflow triggers, or historical provenance records.

---

### Done when

A test engine or Core fixture can:

1. define an internal event type;
2. create a uniquely identified event occurrence;
3. publish an event explicitly;
4. associate the event with an explicit scope;
5. associate the event with an explicit publisher/owner;
6. subscribe to an event by supported type/scope;
7. apply simple generic event filtering;
8. deliver events using the default best-effort/at-most-once semantics;
9. preserve per-publisher/per-subscription event ordering;
10. process independent subscriptions concurrently;
11. isolate subscriber failures from unrelated subscribers;
12. prevent one slow subscriber from indefinitely blocking the publisher or
    unrelated subscriptions;
13. enforce bounded subscription/publisher resource usage;
14. apply the event's explicit overload/delivery policy when a subscriber cannot
    keep up;
15. maintain immutable logical event content after publication;
16. maintain distinct EventId, MessageId, OperationId, CorrelationId, and
    IdempotencyKey semantics;
17. propagate only the relevant context needed for correlation and authorized
    handling;
18. enforce Phase 9 security before delivering scoped internal events;
19. cancel subscriptions through established Core cancellation mechanisms;
20. terminate owned publishers/subscriptions when their owners terminate;
21. integrate event lifecycle with Phase 8 engine shutdown;
22. distinguish event publication failure from subscriber handler failure;
23. avoid making subscriber failure automatically destroy the subscription;
24. avoid durable persistence, replay, acknowledgement, or automatic retry in
    the base mechanism;
25. integrate with Phase 13 retry/idempotency when an explicit consumer
    contract requires it;
26. integrate with Phase 10 artifact/provenance mechanisms without duplicating
    their responsibilities;
27. integrate with Phase 11 observability without turning events into logs or
    metrics;
28. distinguish internal events from Phase 12 stream items;
29. distinguish internal events from Phase 7 transport frames/messages;
30. remain transport-independent for local event usage;
31. preserve all previously verified Core behavior.

Unit tests and integration tests must cover:

```text
event identity
event type
event scope
publisher ownership
subscription ownership
publication
subscription
basic filtering
delivery semantics
ordering
cross-subscription concurrency
subscriber isolation
bounded queues/resources
overload behavior
payload immutability
cancellation
owner termination
publisher shutdown
subscription shutdown
security enforcement
context propagation
observability integration
provenance/artifact references
retry boundary
idempotency boundary
transport boundary
engine/runtime lifecycle integration
```

---

### Architectural Invariant

The central invariant of Phase 14 is:

```text
Phase 14
→ announces internal occurrences.

It does not
→ define domain meaning,
→ provide durable distributed messaging,
→ execute workflows,
→ replace streams,
→ replace provenance,
→ replace observability,
→ replace retry/idempotency.
```

The fundamental abstraction remains:

```text
Engine / Runtime
      │
      │ publish
      ↓
    Event
      │
      ↓
 Subscription
      │
      ↓
 Subscriber Handler
```

with:

```text
best-effort / at-most-once by default
per-publisher/subscription ordering
cross-subscription concurrency
bounded resources
no durability by default
no replay by default
no acknowledgement protocol
no automatic retry
```

Phase 14 is intentionally a small, lifecycle-aware internal notification
mechanism rather than a general-purpose messaging platform.

---

## Phase 15: Control Plane

### Goal

Provide the shared Core control-plane mechanisms responsible for system-level
admission, contract and capability identification, destination eligibility,
routing, engine registration, context propagation, and routing failure
reporting.

The Control Plane determines **WHERE** an operation attempt should be
delivered.

The Engine Runtime remains responsible for determining whether the selected
engine can accept and execute that request.

Capabilities remain responsible for defining and executing domain-specific
behavior.

The fundamental authority model is:

```text
Caller / Planner
→ WHAT logical operation/capability is needed

Control Plane
→ WHERE the attempt should go

Transport
→ HOW the message moves

Engine Runtime
→ CAN this destination accept it?
  + HOW it executes locally

Capability
→ WHAT the operation actually does
````

The Control Plane MUST NOT become a planner, workflow engine, reasoning
engine, inference engine, or domain execution system.

---

### Planned implementation

Add the shared Control Plane mechanisms for:

```text
system-level admission
request validation
contract identification
contract compatibility / resolution
capability identification
engine registration
engine-instance membership
destination eligibility
routing-policy evaluation
destination selection
context propagation
routing decision tracking
routing lifecycle integration
routing failure reporting
runtime integration
```

The Control Plane builds on existing Core systems including:

```text
contracts
capabilities
identity
security
runtime lifecycle
transport
operations/context
health
observability
streaming
retry/idempotency
artifacts
provenance
```

It MUST NOT replace or duplicate those systems.

---

### Control Plane Authority

The Control Plane is responsible for deciding whether a logical communication
request can enter its routing path and where an allowed attempt should be
delivered.

The Control Plane may determine:

```text
admission
contract compatibility
capability identity
destination eligibility
routing
forwarding
```

The Control Plane does NOT determine:

```text
domain meaning
capability implementation
multi-step workflow
reasoning strategy
model inference
business logic
execution semantics
```

The Control Plane ends its responsibility when a valid concrete destination
has been selected and the request has been handed to the communication
mechanism.

It must not remain responsible for execution progress, stream results,
handler behavior, artifact generation, or workflow completion.

---

### Control Plane vs Engine Runtime

There are two distinct admission decisions:

```text
Control Plane admission
→ system-level communication/routing admission

Engine Runtime admission
→ destination-engine execution admission
```

Conceptually:

```text
Logical Request
      ↓
Control Plane
      ↓
destination selected
      ↓
Transport
      ↓
Engine Runtime
      ↓
engine admission
      ↓
capability execution
```

The Control Plane MUST NOT replace Engine Runtime admission.

The Engine Runtime remains the final local authority on whether the selected
engine instance can accept the request.

A routing decision is therefore not a guarantee of execution acceptance.

For example:

```text
Control Plane:
Engine A appears READY
        ↓
Engine A becomes DRAINING
        ↓
request reaches Engine A
        ↓
Engine Runtime rejects new work
```

This is valid behavior.

The Control Plane MUST NOT override the Engine Runtime's local admission
decision.

---

### Security Boundary

Phase 9 remains the authoritative security system.

Phase 15 MUST NOT create a second authentication or authorization framework.

Where appropriate, the Control Plane may perform system-level authorization
checks such as whether a principal may use a particular communication path
or destination.

The Engine Runtime remains responsible for engine/capability-level
authorization.

Conceptually:

```text
Caller
   ↓
Security context
   ↓
Control Plane
   ↓
routing
   ↓
Engine Runtime
   ↓
capability authorization/execution
```

The exact placement of individual authorization checks remains governed by the
existing security model.

The important invariant is:

```text
Control Plane
≠
Security system
```

---

### Contract Identification

The Control Plane may identify the contract targeted by the incoming request.

Conceptually:

```text
Request
   ↓
ContractId / contract descriptor
```

Contract identification answers:

```text
What contract does this request target?
```

This is distinct from executable capability resolution.

The Control Plane uses existing Core contract metadata.

It MUST NOT inspect domain payload contents merely to determine domain
semantics.

---

### Contract Compatibility and Resolution

The Control Plane may resolve a compatible contract definition/version for
routing purposes.

Conceptually:

```text
Request
   ↓
Contract identity
   ↓
contract/version compatibility
   ↓
compatible contract definition
```

Compatibility filtering may consider:

```text
ContractId
version
supported contract versions
destination compatibility
other generic contract metadata
```

The Control Plane MUST NOT reinterpret an incompatible domain payload to make
it compatible.

Actual contract semantics remain owned by the contract system and the
appropriate engine/capability.

---

### Capability Identification

The Control Plane may identify which capability a request targets.

For example:

```text
CapabilityId = Quran.Search
```

This information may be used to determine which engines advertise support for
that capability.

Capability identification is not capability execution.

The Control Plane MUST NOT:

```text
load capability handlers
obtain executable function pointers
invoke handlers
interpret domain logic
```

Instead:

```text
Control Plane
→ identifies/routes capability

Engine Runtime
→ resolves executable capability implementation
```

---

### Logical Engine Identity

`EngineId` represents the logical identity of an engine.

For example:

```text
EngineId = ArabicEngine
```

A logical engine may have one or many runtime instances.

The logical engine identity MUST remain stable independently of scaling.

For example:

```text
ArabicEngine
├── arabic-01
├── arabic-02
└── arabic-03
```

---

### Engine Instance Identity

`EngineInstanceId` represents one concrete runtime participant.

Conceptually:

```text
EngineId
→ logical engine

EngineInstanceId
→ concrete runtime instance
```

For example:

```text
EngineId = ArabicEngine
EngineInstanceId = arabic-02
```

The Control Plane normally resolves a request to a concrete
`EngineInstanceId`.

The separation between logical engine identity and concrete instance identity
MUST remain explicit.

Instances MUST NOT be modeled as unrelated logical engines merely because
multiple instances exist.

---

### Engine Registration

An Engine Runtime registers itself with the Control Plane by presenting a
structured registration description.

Conceptually:

```text
Engine Runtime starts
      ↓
registration descriptor
      ↓
Control Plane
      ↓
registration validation
      ↓
routing membership
```

Registration information must be capable of describing:

```text
EngineId
EngineInstanceId
supported capabilities
supported contracts / versions
transport endpoint
runtime version
lifecycle/readiness information
routing metadata
```

Not every registration needs to contain every optional field.

The exact registration structure and wire representation remain implementation
choices.

---

### Registration Ownership

The Engine Runtime presents its registration information.

The Control Plane owns the routing membership view.

Therefore:

```text
Engine Runtime
→ declares membership

Control Plane
→ maintains routing membership
```

An engine MUST NOT directly modify the Control Plane's routing tables.

---

### Registration Validation

The Control Plane MUST perform generic structural validation of registration
information.

Examples include:

```text
EngineId validity
EngineInstanceId validity
capability descriptor validity
contract/version metadata validity
endpoint validity
registration identity/security validity
```

Registration validation is generic Core behavior.

The Control Plane MUST NOT attempt to determine whether the engine's domain
implementation is semantically correct.

For example:

```text
"Does this Arabic engine actually implement Arabic morphology correctly?"
```

is an engine/domain concern, not a registration-validation concern.

---

### Capability Advertisement

Capability advertisement is declarative.

An engine may advertise multiple capabilities:

```text
ArabicEngine / arabic-01
├── Arabic.Tokenize
├── Arabic.Sarf
├── Arabic.Nahw
└── Arabic.Analyze
```

A capability may be advertised by multiple logical engines:

```text
Quran.Search
├── QuranEngine
├── KnowledgeEngine
└── SearchEngine
```

when their contract and capability metadata make them compatible.

The Control Plane treats capability advertisements as routing metadata.

Capability correctness remains an engine responsibility.

---

### Registration and Lifecycle

Registration membership is NOT a second engine lifecycle system.

Phase 8 remains authoritative over actual Engine Runtime lifecycle.

Conceptually:

```text
Engine Runtime lifecycle
        ↓
Control Plane routing eligibility
```

For example:

```text
Engine Runtime = SERVING
        ↓
instance may be routing-eligible
```

and:

```text
Engine Runtime = DRAINING
        ↓
instance is not eligible for new normal routing
```

The Control Plane MUST NOT create a competing lifecycle state machine whose
state conflicts with the runtime lifecycle.

---

### Destination Resolution

Destination resolution consists of two conceptual stages:

```text
Eligibility
+
Selection
```

First determine which destinations are eligible.

Then select one according to routing policy.

Conceptually:

```text
Request
   ↓
contract/capability requirements
   ↓
eligible destinations
   ↓
routing policy
   ↓
selected EngineInstanceId
```

---

### Destination Eligibility

A destination is eligible only when it satisfies the applicable generic
requirements.

These may include:

```text
capability support
contract compatibility
engine/runtime availability
instance readiness
security constraints
routing constraints
```

For example:

```text
Engine A
capability ✅
contract ✅
ready ✅

Engine B
capability ✅
contract ❌

Engine C
capability ✅
ready ❌
```

produces:

```text
eligible destination = Engine A
```

The routing algorithm MUST NOT select a destination merely because it is
registered.

---

### Capability-Level Eligibility

Engine-level readiness is not sufficient by itself.

For example:

```text
Engine A
→ overall READY

Search capability
→ READY

Grammar capability
→ NOT READY
```

A request for Grammar MUST NOT be routed to A merely because A is generally
READY.

Capability-level readiness/availability may therefore participate in
destination eligibility.

---

### Health and Routing

Phase 11 provides health information.

Phase 15 consumes that information when determining routing eligibility.

The relationship is:

```text
Health
→ observation/input

Control Plane
→ routing decision
```

Health MUST NOT directly perform routing.

For example:

```text
Engine A
→ liveness healthy
→ readiness ready

Engine B
→ liveness healthy
→ readiness false
```

can result in:

```text
A → eligible
B → ineligible
```

Health information is an input to routing policy rather than an independent
routing authority.

---

### Load and Routing

Load information may be used by routing policy.

For example:

```text
Engine A = 95% load
Engine B = 30% load
```

may result in B being selected.

However, load is an input to routing policy, not an independent routing
authority.

High load does not automatically make a destination invalid unless the
configured routing policy defines that behavior.

---

### Routing Policy

The Control Plane is authoritative for destination selection.

Routing policy may consider:

```text
platform configuration
engine metadata
health/readiness
capability eligibility
contract compatibility
load
caller-provided routing constraints
other explicitly supported routing metadata
```

The final routing decision belongs to the Control Plane.

Possible routing strategies may include:

```text
round robin
weighted selection
capacity-aware selection
locality-aware selection
latency-aware selection
explicit preference
```

The exact routing algorithm remains an implementation/configuration choice.

The architecture requires only that routing follow an explicit, deterministic
policy.

---

### Routing Determinism

Given equivalent:

```text
request
routing constraints
routing state
routing policy
```

the Control Plane MUST make a predictable routing decision according to that
policy.

The implementation MUST NOT introduce arbitrary or unexplained destination
selection.

---

### Logical Destination

A logical destination represents a request for a service/capability/logical
engine rather than a specific instance.

For example:

```text
Capability = Arabic.Analyze
```

may resolve to:

```text
ArabicEngine / arabic-02
```

among multiple eligible instances.

Logical destinations permit scaling without changing the logical caller-facing
identity of an engine.

---

### Explicit Destination

A request may explicitly identify a concrete `EngineInstanceId`.

For example:

```text
EngineInstanceId = arabic-02
```

An explicit destination remains subject to:

```text
security
registration
contract compatibility
capability compatibility
routing constraints
runtime admission
```

Specifying an instance MUST NOT bypass safety or authorization boundaries.

---

### Hard Destination vs Preference

Routing constraints distinguish between a hard destination and a preference.

A hard destination means:

```text
"Use this concrete destination."
```

If the destination is unavailable or ineligible:

```text
→ report destination unavailable
```

The Control Plane MUST NOT silently select another destination for a hard
destination unless the request contract explicitly permits fallback.

A preference means:

```text
"Prefer this destination."
```

If the preferred destination is unavailable:

```text
→ another eligible destination may be selected
```

This distinction MUST remain explicit.

---

### Caller Routing Constraints

A caller may supply routing preferences or constraints only when the applicable
contract permits them.

Examples include:

```text
preferred instance
preferred locality
preferred region
other supported routing metadata
```

Caller routing information is not automatically absolute authority.

The Control Plane remains authoritative over the final destination selection.

---

### Routing Fallback

Fallback behavior is determined by routing semantics.

```text
hard destination
→ no automatic fallback

logical destination / preference
→ eligible fallback may be selected
```

The Control Plane MUST NOT assume that all requests permit fallback.

---

### Routing Per Attempt

Each concrete execution attempt receives a destination decision.

For example:

```text
Operation X
   ↓
Attempt 1
   ↓
Engine A

Attempt 1 fails

Phase 13 permits Attempt 2
   ↓
Control Plane resolves destination again
   ↓
Engine B
```

Phase 15 does not decide whether another attempt should exist.

Phase 13 owns retry policy.

Phase 15 resolves the destination for an attempt that has already been
authorized to execute.

---

### Retry Destination Policy

Whether a retry prefers the same instance, another instance, or ordinary
routing remains a routing-policy choice.

Possible policies include:

```text
prefer previous instance
avoid previous instance
ordinary routing
```

No one strategy is universally required.

---

### Existing Attempt Stability

Once a destination has been selected for an attempt, later routing-policy
changes MUST NOT retroactively change that attempt's destination.

Conceptually:

```text
Attempt 1
→ routing policy P1
→ Engine A
```

then:

```text
routing policy changes to P2
```

does not move Attempt 1 to another engine.

A later retry may use the current routing policy when resolving its new
destination.

---

### Registration Changes and Existing Attempts

If an engine unregisters or becomes ineligible after an attempt's destination
has already been selected:

```text
Attempt 1
→ Engine A selected

Engine A
→ unregisters
```

the existing attempt's destination does not retroactively change.

The transport/runtime determines what happens to the active execution.

If Phase 13 creates another attempt:

```text
Attempt 2
→ new destination resolution
```

uses current routing state.

---

### Routing State Consistency

Each routing decision MUST observe a coherent routing state.

A routing decision MUST NOT observe a partially updated membership/policy
configuration.

Conceptually:

```text
Routing State Snapshot N
→ one coherent decision
```

The implementation may achieve this using:

```text
immutable snapshots
locks
versioned state
transactional updates
other concurrency-safe mechanisms
```

The exact mechanism remains an implementation choice.

---

### Routing-State Staleness

The Control Plane's routing state may have unavoidable timing differences from
actual engine runtime state.

The architecture therefore accepts:

```text
Control Plane routing decision
→ based on routing state known at decision time
```

while:

```text
Engine Runtime
→ final local admission authority
```

The Control Plane should maintain reasonably current routing information through
appropriate registration, lifecycle, health, or equivalent mechanisms.

A specific heartbeat/lease protocol is not frozen by this phase.

---

### No Implicit Waiting

If no eligible destination currently exists:

```text
→ destination unavailable / no eligible destination
```

The Control Plane MUST NOT implicitly wait for an engine to appear.

A higher-level policy may decide to:

```text
wait
retry
queue
```

but that behavior is outside the base destination-resolution decision.

---

### No Implicit Queueing

If a destination has no available capacity, the Control Plane MUST NOT
implicitly queue requests indefinitely.

The base mechanism may return an explicit unavailable/capacity outcome.

Queueing behavior, when required, belongs to a separately defined policy.

---

### Routing vs Retry

The Control Plane does not decide whether an operation should be retried.

For example:

```text
No eligible destination
```

means:

```text
routing failure/unavailability
```

not automatically:

```text
retry operation
```

Phase 13 owns retry decisions.

The Control Plane may resolve a different destination for a later attempt after
Phase 13 permits that attempt.

---

### Routing vs Transport

The Control Plane determines:

```text
WHERE
```

Transport determines:

```text
HOW bytes move
```

The relationship is:

```text
Control Plane
      ↓
selected destination
      ↓
Transport
      ↓
Engine Runtime
```

The Control Plane MUST NOT become a transport implementation.

Transport socket details, framing, fragmentation, reassembly, buffering, and
wire-level behavior remain owned by Phase 7 transport mechanisms.

---

### Frame Routing

The Control Plane operates on logical requests/messages.

It MUST NOT route individual transport frames.

Invalid model:

```text
Frame 1 → Engine A
Frame 2 → Engine B
```

Correct model:

```text
Logical Request
      ↓
destination resolution
      ↓
selected EngineInstanceId
      ↓
transport framing/transmission
```

Phase 7 remains responsible for frame handling.

---

### Routing vs Streaming

For an application-level stream:

```text
Operation
   ↓
Control Plane
   ↓
Engine Instance A
   ↓
Stream
```

the Control Plane routes the logical operation/attempt.

Once the stream is established, the stream remains associated with that
execution destination.

The Control Plane MUST NOT dynamically route individual stream items to
unrelated destinations.

For example:

```text
stream item 1 → A
stream item 2 → B
stream item 3 → C
```

is not the default supported behavior.

This preserves Phase 12 stream ordering and execution ownership.

---

### Routing vs Artifacts

The Control Plane may forward an `ArtifactReference` or other generic artifact
metadata when needed for routing/communication.

It MUST NOT become responsible for:

```text
artifact retrieval
artifact storage
artifact integrity verification
artifact lifecycle
artifact versioning
```

Those remain Phase 10 responsibilities.

For example:

```text
Request
→ ArtifactReference A:v7
→ route request
```

does not imply that the Control Plane retrieves A:v7.

---

### Routing vs Internal Events

Phase 14 events may optionally describe routing-related occurrences such as:

```text
EngineRegistered
EngineBecameUnavailable
RouteMembershipChanged
```

However, the internal event system is not the authoritative routing state.

The relationship is:

```text
Event
→ optional notification

Control Plane routing state
→ authoritative routing state
```

The Control Plane MUST NOT require the event system merely to maintain its
basic routing semantics unless a later implementation explicitly chooses to
use it internally.

---

### Context Propagation

The Control Plane MUST preserve the logical operation context across the
Control Plane → Transport → Engine Runtime boundary.

Relevant context may include:

```text
OperationId
CorrelationId
deadline
cancellation
security context
tracing context
provenance context
```

The Control Plane MUST NOT unnecessarily create unrelated replacement
contexts.

The destination engine must receive the request as the same logical operation
and execution context.

---

### Context and Identity

The Control Plane MUST preserve the distinction between:

```text
MessageId
OperationId
CorrelationId
TraceId
EngineId
EngineInstanceId
```

and other Core identities.

Routing metadata must not overwrite logical operation identity.

---

### Routing Result

A routing result should contain enough information for downstream execution and
observability.

Conceptually, it may include:

```text
selected EngineId
selected EngineInstanceId
routing policy/selection information where appropriate
routing state/snapshot identifier where applicable
```

The routing result should provide enough information to answer:

```text
Why was this request routed to this instance?
```

without exposing unnecessary internal state.

---

### Control Plane and Observability

Phase 11 remains responsible for observability.

The Control Plane should expose enough information for observing:

```text
routing decisions
destination resolution latency
routing failures
registration changes
eligible destination counts
membership changes
```

The Control Plane MUST NOT create a second telemetry system.

Routing failures and engine execution failures must remain distinguishable.

---

### Failure Ownership

Control Plane failures may include:

```text
ControlPlaneUnavailable
InvalidRoutingRequest
ContractResolutionFailure
NoEligibleDestination
DestinationUnavailable
RoutingPolicyFailure
RegistrationFailure
```

Transport failures remain transport-owned.

Engine Runtime failures remain engine-runtime-owned.

Capability failures remain capability/engine-owned.

These failures MUST remain distinguishable.

---

### Control Plane Unavailable vs No Eligible Destination

These are not equivalent:

```text
Control Plane unavailable
≠
No eligible destination
```

If the Control Plane itself cannot perform routing, this should be reported as a
Control Plane/system failure.

If the Control Plane is functioning but no registered destination satisfies
the request, that is a destination/routing availability failure.

---

### Transport Failure vs Routing Failure

If:

```text
Control Plane
→ correctly selects Engine A
```

but:

```text
Transport
→ cannot establish delivery
```

the resulting failure is a transport failure, not a destination-resolution
failure.

Phase 13 may later determine whether another attempt should be made.

---

### Engine Runtime Rejection

If:

```text
Control Plane
→ correctly selects Engine A

Transport
→ delivers successfully

Engine Runtime
→ rejects because it is now DRAINING
```

the Control Plane has still completed its responsibility correctly.

The Engine Runtime rejection remains a runtime admission outcome.

The Control Plane MUST NOT override that rejection.

---

### Error Causality

The Control Plane MUST preserve the original failure cause when forwarding
errors.

For example:

```text
Control Plane
→ destination selected

Transport
→ delivery succeeded

Engine Runtime
→ capability execution failed
```

must not be flattened into:

```text
ControlPlaneError
```

merely because the request passed through the Control Plane.

Failure ownership and causal context must remain observable.

---

### Control Plane Concurrency

The Control Plane MUST safely support concurrent:

```text
destination resolution
engine registration
membership updates
health/readiness updates
draining
unregistration
routing-policy changes
```

Routing decisions must observe coherent state.

The implementation may use:

```text
RwLock
immutable snapshots
atomic state
versioned state
other concurrency-safe mechanisms
```

The exact synchronization strategy remains an implementation choice.

---

### Routing Snapshots

A routing snapshot is a logical consistency concept.

It means:

```text
one routing decision
→ one coherent membership + policy state
```

It does not require a literal `RoutingSnapshot` struct.

The implementation may represent this using any appropriate mechanism.

A snapshot/policy change MUST NOT retroactively modify a destination already
selected for an existing attempt.

---

### Deployment Topology

The architecture defines a logical Control Plane, not a mandatory number of
processes.

A deployment may contain:

```text
one Control Plane instance
multiple Control Plane instances
```

without changing the logical responsibility of the Control Plane.

Phase 15 does not define:

```text
consensus
leader election
distributed agreement
global strongly consistent routing
```

as mandatory functionality.

Those concerns remain deployment/platform architecture decisions unless
explicitly introduced later.

---

### Runtime Membership Persistence

Runtime engine membership does not need to be permanently durable by default.

A typical lifecycle may be:

```text
Engine starts
→ registers

Engine stops
→ membership disappears
```

Persistent routing configuration and transient runtime membership are separate
concepts.

Phase 15 does not require permanent storage for runtime registrations.

---

### Planner and Workflow Boundary

The Control Plane MUST NOT become a planner or workflow engine.

If a planner produces:

```text
Step 1 → Arabic Engine
Step 2 → Knowledge Graph
Step 3 → Grading Engine
```

the Control Plane may route each already-defined step:

```text
Step 1 → destination
Step 2 → destination
Step 3 → destination
```

but it does not determine:

```text
Step 1 → Step 2 → Step 3
```

The workflow relationship belongs to the planner/orchestrator.

---

### Domain-Payload Boundary

The Control Plane MUST NOT inspect application payloads to invent routing
decisions.

For example:

```text
payload:
"find verses about patience"
```

must not cause the Control Plane to infer:

```text
→ Quran Search
```

based on domain reasoning.

The request must already provide appropriate generic routing information such
as:

```text
CapabilityId
ContractId
logical destination
routing constraints
```

The payload remains application/domain data.

Therefore:

```text
Request metadata
→ routing information

Payload
→ application semantics
```

---

### Capability Semantics Boundary

The Control Plane knows enough capability metadata to route correctly.

It does not know:

```text
what the capability implementation does
how the algorithm works
how the engine stores its data
how domain reasoning works
```

The boundary is:

```text
metadata
✅

domain semantics
❌
```

---

### Explicit Non-Goals

Phase 15 must not become:

* a planner;
* a workflow engine;
* a reasoning engine;
* a model inference engine;
* a domain execution engine;
* a business-logic router;
* a distributed consensus system;
* a service-mesh replacement;
* a transport implementation;
* a frame-routing system;
* a stream scheduler;
* a retry system;
* an idempotency system;
* an artifact store;
* an artifact resolver;
* an observability platform;
* an event broker;
* a durable runtime-membership database.

Phase 15 provides Core routing and control-plane mechanisms only.

---

### Files and Folders

**Control Plane**

* `src/control/mod.rs`
* `src/control/plane.rs`
* `src/control/admission.rs`
* `src/control/registration.rs`
* `src/control/membership.rs`
* `src/control/resolution.rs`
* `src/control/routing.rs`
* `src/control/policy.rs`

**Routing identity**

* `src/identity/`
* `src/engine/`

**Related contracts**

* `src/contracts/`
* `src/capability/`

**Runtime integration**

* `src/runtime/`
* `src/operation/`
* `src/middleware/`

**Related systems**

* `src/security/`
* `src/health/`
* `src/observability/`
* `src/transport/`
* `src/streaming/`
* `src/retry/`
* `src/idempotency/`
* `src/artifact/`
* `src/provenance/`
* `src/events/`

**Tests**

* `tests/control_plane.rs`
* `tests/routing.rs`
* `tests/registration.rs`
* `tests/integration.rs`
* `tests/conformance.rs`

Exact filenames may be adjusted if the repository already provides equivalent
modules. The architectural boundaries defined in this phase must remain.

---

### Boundary

The Control Plane owns:

```text
system-level admission
contract identification/compatibility
capability identification
engine registration membership
destination eligibility
routing policy
destination selection
routing context propagation
routing-state management
routing failure reporting
```

The Control Plane does NOT own:

```text
engine execution
capability implementation
domain semantics
workflow planning
retry decisions
stream lifecycle
transport framing
artifact management
security framework
health observation
observability infrastructure
```

The final responsibility boundary is:

```text
WHAT?
→ Caller / Planner / Capability Contract

WHERE?
→ Control Plane

HOW?
→ Transport + Engine Runtime

CAN NOW?
→ Engine Runtime

WHAT DOES IT ACTUALLY DO?
→ Capability
```

---

### Done when

A test engine or Core fixture can:

1. accept an incoming logical request through the Control Plane;
2. perform system-level admission;
3. preserve the established security context;
4. identify the requested contract;
5. resolve/filter compatible contract versions;
6. identify the requested capability;
7. distinguish capability identification from executable capability resolution;
8. register a logical engine and concrete engine instance;
9. validate engine registration structurally;
10. maintain routing membership independently from engine lifecycle ownership;
11. distinguish `EngineId` from `EngineInstanceId`;
12. support one logical engine with multiple runtime instances;
13. support multiple capabilities on one engine instance;
14. support a capability being advertised by multiple logical engines;
15. determine destination eligibility before selection;
16. incorporate engine readiness into routing eligibility;
17. incorporate capability readiness where applicable;
18. incorporate contract compatibility into routing eligibility;
19. incorporate security/routing constraints into eligibility;
20. select a concrete `EngineInstanceId` according to an explicit routing policy;
21. support logical destinations;
22. support explicit concrete destinations;
23. distinguish hard destinations from preferences;
24. prevent fallback for hard destinations unless explicitly permitted;
25. permit fallback among eligible destinations for logical/preference-based
    routing;
26. make routing decisions per execution attempt;
27. permit later retries to resolve a different destination without making
    routing responsible for retry decisions;
28. preserve an attempt's selected destination after routing-policy changes;
29. preserve an attempt's selected destination after registration changes;
30. use coherent routing state for each destination decision;
31. support concurrent routing decisions and membership updates safely;
32. prevent implicit indefinite waiting when no destination is available;
33. prevent implicit indefinite request queueing;
34. propagate operation, correlation, security, tracing, deadline, cancellation,
    and provenance context where applicable;
35. preserve the distinction between all Core identity types;
36. route logical requests rather than transport frames;
37. avoid changing application payload semantics during routing;
38. keep established application streams associated with their selected
    execution destination;
39. distinguish routing failures from transport failures;
40. distinguish Control Plane failure from destination unavailability;
41. distinguish routing failure from Engine Runtime rejection;
42. preserve causal failure information;
43. expose enough routing information for Phase 11 observability;
44. optionally integrate with Phase 14 internal events without making events
    the authoritative routing mechanism;
45. integrate with Phase 10 artifact references without becoming the artifact
    system;
46. integrate with Phase 13 retries without owning retry policy;
47. integrate with Phase 8 lifecycle without creating a competing lifecycle
    state machine;
48. maintain routing behavior without requiring durable runtime membership;
49. remain independent of whether the logical Control Plane is deployed as one
    or multiple instances;
50. preserve all previously verified Core behavior.

Unit tests and integration tests must cover:

```text
Control Plane admission
security boundary integration
contract identification
contract compatibility
capability identification
registration validation
logical engine identity
engine-instance identity
multiple instances per logical engine
multiple capabilities per instance
capability on multiple engines
membership management
lifecycle/membership integration
destination eligibility
health/readiness integration
capability readiness
hard destination behavior
preference behavior
fallback behavior
routing policy evaluation
deterministic selection
per-attempt routing
routing-state consistency
routing-policy changes
registration changes
explicit destination handling
logical destination handling
concurrent routing
concurrent membership updates
context propagation
routing/transport separation
routing/frame separation
routing/streaming separation
routing/retry separation
routing/artifact separation
routing/event separation
routing failure classification
transport failure classification
engine rejection handling
error causality
observability integration
planner/workflow boundary
domain-payload boundary
shutdown integration
```

---

### Architectural Invariant

The central Phase 15 invariant is:

```text
Control Plane
→ WHERE

Engine Runtime
→ CAN / HOW

Capability
→ WHAT
```

Therefore:

```text
Control Plane ≠ Engine Runtime
Control Plane ≠ Security
Control Plane ≠ Retry
Control Plane ≠ Transport
Control Plane ≠ Streaming
Control Plane ≠ Workflow
Control Plane ≠ Domain Logic

EngineId ≠ EngineInstanceId

Routing ≠ Retry
Routing ≠ Health
Routing ≠ Transport
Routing ≠ Streaming

Logical Request ≠ Transport Frame

Capability Identification ≠ Capability Execution
```

The Control Plane must remain a **routing and control boundary**, not become the
place where Nizaam starts thinking, planning, executing domain logic, or
managing application workflows.

---

## Phase 16: Engine SDK

### Goal

Provide a stable, curated, engine-facing development interface over Core so
engine authors can build engines without depending directly on Core's
implementation internals.

The Engine SDK is an API-ergonomics and compatibility boundary.

It simplifies correct Core usage, hides unstable implementation details,
provides safe defaults, exposes supported extension points, and preserves the
semantics established by earlier Core phases.

The Engine SDK MUST NOT create a second runtime, redefine Core semantics, or
replace any existing Core subsystem.

The fundamental architecture is:

```text
Engine Developer
       ↓
   Engine SDK
       ↓
      Core
````

Core remains the actual infrastructure and execution authority.

---

### SDK Audience and Trust Boundary

The Engine SDK is intended primarily for:

```text
Engine authors
Engine integration developers
Engine test fixtures
```

It is NOT the universal SDK for the Nizaam ecosystem.

The following are separate:

```text
Internal Core API
→ used by Nizaam/Core implementation

Engine SDK API
→ supported interface for engine implementations

External API
→ public API used by application/client consumers
```

Therefore:

```text
External API
≠
Engine SDK
≠
Internal Core API
```

The Go API does not depend on the Rust Engine SDK.

The future Database does not have to use the Engine SDK and may use Core
directly where its infrastructure role requires it.

---

### Core Team Direct Access

The Nizaam Core team/application code is an internal trusted consumer of Core.

Nizaam-owned components may directly import and use Core internals when
implementing or adapting engines, infrastructure, or future Core-level
components, subject to the repository's internal architecture and ownership
rules.

This direct Core access is separate from the Engine SDK contract.

Conceptually:

```text
Nizaam-owned implementation
        ↓
      Core
```

and:

```text
Third-party / external engine implementation
        ↓
   Engine SDK
        ↓
      Core
```

The ability of Nizaam-owned code to directly use or modify Core MUST NOT cause
those internals to become part of the stable Engine SDK API.

---

### SDK Restriction Model

The Engine SDK MUST expose only supported engine-facing functionality.

SDK consumers MUST NOT receive a supported API for replacing or mutating
Core's internal mechanisms such as:

```text
runtime state machine
routing tables
Control Plane internals
transport framing
connection management
task registry
cancellation engine
stream internals
retry engine
idempotency storage
health aggregation
event delivery machinery
artifact storage internals
provenance storage internals
observability provider internals
```

The restriction is architectural and API-level.

The SDK must not expose public extension points whose purpose is to replace
these internal Core mechanisms.

The SDK may expose controlled configuration or extension hooks where Core
explicitly supports them.

---

### Direct Core Access Boundary

The normal Engine SDK API MUST be sufficient for ordinary engine development.

Direct access to Core internals is not part of the supported Engine SDK
contract.

If an infrastructure-grade Nizaam component genuinely requires lower-level
Core functionality, it may use Core directly as an internal Nizaam component
rather than weakening the SDK abstraction for every engine.

The architecture therefore distinguishes:

```text
supported SDK extension
≠
internal Core customization
```

---

### SDK as Facade

The Engine SDK is a facade over Core.

It provides:

```text
stable public abstractions
developer ergonomics
safe defaults
controlled extension points
Core invariant enforcement
```

It does NOT provide:

```text
new runtime semantics
new lifecycle semantics
new cancellation semantics
new streaming semantics
new retry semantics
new event semantics
new health semantics
new artifact semantics
new provenance semantics
```

The rule is:

> The SDK hides complexity, not semantics.

---

### Stable SDK Concepts

The SDK should expose a curated set of stable engine-facing concepts such as:

```text
Engine
Capability
Request
Response
Context
Stream
Configuration
Health
Error
ArtifactReference
Event integration
Observability integration
```

These concepts must map to the corresponding Core semantics.

The SDK MUST NOT automatically expose every public Core struct.

---

### Hidden Core Mechanisms

The following remain implementation details unless explicitly promoted into
the stable SDK contract:

```text
runtime state machine internals
routing tables
Control Plane membership state
transport frames
frame fragmentation
transport connection internals
buffer management
task registry
retry state machine
idempotency storage
health aggregation implementation
event queues
artifact storage implementation
provenance storage implementation
observability provider internals
```

Internal types may change without requiring an Engine SDK breaking change.

---

### API Surface Curability

The SDK MUST remain intentionally smaller than Core.

Core may expose many low-level mechanisms internally.

The SDK should expose only the functionality required for supported engine
development.

The SDK MUST NOT become an automatically generated wrapper around every Core
type.

Conceptually:

```text
Core
→ broad infrastructure API

Engine SDK
→ small curated engine API
```

---

### Engine Builder

The SDK MUST provide a structured mechanism for defining an engine.

Conceptually:

```rust
Engine::builder(...)
```

or an equivalent builder/factory API.

The engine definition may include:

```text
engine identity
engine metadata
capabilities
configuration
lifecycle hooks
supported extension points
```

The exact type and method names remain an implementation decision.

The SDK MUST NOT allow engine construction to bypass the Core Runtime
lifecycle.

---

### Capability Registration

The SDK MUST provide an ergonomic capability-registration mechanism.

Conceptually:

```rust
engine.capability(...)
```

or:

```rust
register_capability(...)
```

The SDK may support typed registration such as:

```rust
register_capability::<Request, Response>(handler)
```

where practical.

Typed SDK APIs SHOULD provide:

```text
compile-time request/response checking
IDE support
reduced manual conversion
reduced runtime validation
```

while Core continues to support the universal contract model.

---

### Capability Semantics

The SDK exposes the stable capability contract.

It does not expose Core's internal dispatch tables or registry implementation.

The relationship is:

```text
Engine author
→ implements capability

SDK
→ adapts capability to Core

Core
→ resolves/dispatches/execut es capability
```

The SDK MUST NOT move domain semantics into Core merely for developer
convenience.

---

### Request and Response

The SDK MUST provide an ergonomic request/response API.

Engine developers should work primarily with:

```text
Request
Response
Context
```

rather than:

```text
transport frames
connection objects
routing metadata internals
serialization buffers
internal envelopes
```

Where Core already provides universal request/response types, the SDK SHOULD
prefer ergonomic views/adapters over unnecessary duplicate protocol types.

---

### Universal Request / Response Boundary

Core may use:

```text
UniversalRequest
UniversalResponse
```

internally.

The SDK should not require engine authors to manually construct or manipulate
all universal envelope/protocol details.

The SDK may expose stable request/response wrappers or views.

Duplicate types MUST only be introduced when they provide a clear stable API
benefit and have an explicit mapping to Core semantics.

---

### Contract Exposure

The SDK MUST expose enough contract information for an engine author to define
a valid capability.

It SHOULD avoid exposing low-level contract-construction machinery such as:

```text
raw descriptor internals
internal envelope representation
registry implementation
dispatch metadata internals
```

unless that information is intentionally part of the stable engine-facing
contract.

The SDK should expose the minimum contract surface necessary for supported
engine development.

---

### Serialization Boundary

The SDK MUST NOT become an independent serialization framework.

Engine authors should be able to work with typed request/response values where
the contract supports typed APIs.

Core/contract infrastructure remains responsible for the actual protocol and
serialization representation.

Conceptually:

```text
Engine typed value
      ↓
Engine SDK
      ↓
Core contract/serialization
      ↓
universal representation
```

---

### Context API

The SDK MUST expose a controlled engine-facing view of operation context.

The API may expose:

```text
operation identity
correlation identity
deadline
cancellation state
security context where permitted
tracing context where permitted
other explicitly supported context
```

The SDK MUST NOT expose mutable internal context state.

Foundational values such as:

```text
OperationId
CorrelationId
security principal
deadline
```

must not be arbitrarily mutable by capability code.

---

### Context Semantics

The SDK context is primarily read-oriented.

Where derived context is supported, derivation must follow Core semantics.

The SDK MUST NOT allow engine code to silently replace foundational operation
identity or authorization state.

---

### Lifecycle API

The SDK MUST expose convenient engine lifecycle hooks without implementing a
second lifecycle state machine.

Possible hooks include:

```text
initialize
start
ready
shutdown
```

These hooks map to the existing Core Runtime lifecycle.

The authoritative lifecycle remains:

```text
Core Runtime
```

not:

```text
SDK Runtime
```

---

### Runtime Ownership

The Engine SDK MUST NOT create its own runtime.

Conceptually:

```text
Engine
   ↓
SDK
   ↓
Core Runtime
```

not:

```text
Engine
   ↓
SDK Runtime
   +
Core Runtime
```

There must not be competing systems for:

```text
lifecycle
task ownership
cancellation
streaming
shutdown
```

---

### Managed Tasks

The SDK MUST expose a supported mechanism for engine-owned background work
through the Core-managed task system.

Conceptually:

```text
engine.spawn_task(...)
```

or:

```text
ctx.spawn(...)
```

The exact API remains an implementation choice.

The supported mechanism must integrate with:

```text
ownership
cancellation
shutdown
resource limits
```

The normal SDK API SHOULD NOT encourage detached unmanaged tasks.

---

### Async Execution

The SDK MUST support the asynchronous execution model used by Core.

Capability handlers may need asynchronous execution because engines may perform:

```text
database work
network requests
artifact reads
model inference
other engine calls
```

The exact trait signature remains an implementation decision.

The SDK MUST NOT require engine developers to manually manage Core runtime
internals merely to implement asynchronous capabilities.

---

### Blocking / CPU-Heavy Work

The SDK SHOULD provide a supported mechanism for engines to execute blocking or
CPU-intensive work without unnecessarily blocking the main Core execution path.

This is particularly relevant to engines performing:

```text
Arabic analysis
ML inference
embedding generation
large graph processing
other CPU-heavy computation
```

The actual executor/thread-pool mechanism remains a Core/Runtime concern.

---

### Cancellation

The SDK MUST provide convenient access to Core cancellation semantics.

Conceptually:

```text
execute(context, request)
```

with controlled access to:

```text
ctx.is_cancelled()
```

or an equivalent mechanism.

The SDK MUST NOT create a separate cancellation hierarchy.

Cancellation remains owned by Core.

---

### Deadlines

The SDK MUST expose the applicable operation deadline to engine code.

Conceptually:

```text
ctx.deadline()
```

The Core Runtime remains responsible for deadline semantics and enforcement.

The SDK MUST NOT require engines to construct an independent deadline
management system.

---

### Streaming API

The SDK MUST expose logical application streaming rather than transport-frame
management.

Conceptually:

```text
Stream<Item>
```

with operations appropriate to the existing Core stream semantics.

Possible operations include:

```text
send(...)
finish(...)
cancel(...)
```

The exact API remains implementation-defined.

The SDK MUST preserve:

```text
ordering
backpressure
cancellation
deadline
terminal states
ownership
```

---

### Streaming vs Transport

An SDK stream operation represents a logical application item.

For example:

```text
stream.send(item)
```

means:

```text
logical stream item
```

not:

```text
transport frame
```

Engine developers should not need to understand Phase 7 frame fragmentation
to produce normal stream output.

---

### Stream Terminal Semantics

The SDK MUST preserve explicit terminal semantics.

Conceptually:

```text
COMPLETED
FAILED
CANCELLED
```

remain distinguishable.

The SDK MUST NOT hide these meanings behind ambiguous operations.

Core remains the authority for actual stream state.

---

### Configuration API

The SDK MUST provide convenient access to resolved engine configuration.

The engine should consume resolved configuration rather than manually
reimplementing:

```text
loading
parsing
resolution
secret retrieval
```

where those mechanisms already belong to Core.

The SDK MUST NOT duplicate the configuration pipeline.

---

### Engine-Specific Configuration

The SDK MUST support engine-specific configuration data.

For example:

```rust
struct ArabicConfig {
    model_path: String,
    morphology_db: String,
}
```

Conceptually:

```text
Core / SDK
→ configuration mechanism

Engine
→ configuration meaning
```

Core does not need to understand the domain semantics of engine-specific
configuration fields.

---

### Health API

The SDK MUST provide a convenient mechanism for an engine to contribute health
information.

Possible information includes:

```text
liveness
readiness
degraded state
capability-level readiness
```

The SDK is only the developer-facing interface.

Phase 11 remains authoritative for health semantics and aggregation.

The SDK MUST NOT create a second health system.

---

### Observability API

The SDK SHOULD provide convenient engine-facing access to:

```text
logging
metrics
tracing
diagnostics
```

The implementation forwards into Core's established observability system.

The SDK MUST NOT create a competing telemetry system.

Observability context must remain associated with the operation and engine
execution where applicable.

---

### Artifact API

The SDK SHOULD provide engine-facing abstractions for:

```text
ArtifactReference
artifact reading
artifact writing
artifact publication
artifact access
```

where required.

The SDK MUST NOT expose the storage provider's internal implementation.

The relationship remains:

```text
Engine
 ↓
SDK Artifact API
 ↓
Core Artifact System
 ↓
Storage/provider
```

Phase 10 remains authoritative for artifact identity, versioning, integrity,
lifecycle, and storage semantics.

---

### Provenance API

The SDK SHOULD allow engine-specific provenance metadata where required.

Core should automatically preserve relationships such as:

```text
Operation
Attempt
Engine
Capability
Artifact
```

where applicable.

Engine code SHOULD NOT need to manually reconstruct the complete Core
provenance model.

Phase 10 remains authoritative.

---

### Retry and Idempotency API

The SDK MAY provide convenient declarations/configuration for engine
capabilities such as:

```text
retryability
idempotency requirements
retry-related metadata
```

But the SDK MUST NOT implement a separate retry engine or idempotency store.

The relationship is:

```text
SDK
→ declaration / configuration

Core Phase 13
→ execution / enforcement
```

The SDK MUST preserve Phase 13 distinctions between:

```text
retryability
idempotency
retry safety
```

---

### Event API

The SDK MAY provide convenient access to Core internal event publication.

Conceptually:

```text
engine.events().publish(...)
```

The actual event mechanism remains owned by Phase 14.

The SDK MUST NOT provide:

```text
second event bus
second event lifecycle
second delivery queue
second replay mechanism
```

The SDK should hide publisher/subscriber implementation details.

---

### Control Plane Integration

Engine developers SHOULD NOT need to manually manage Control Plane internals.

The SDK/Core runtime should automatically integrate:

```text
engine identity
instance metadata
capability advertisement
contract advertisement
registration
membership
routing
```

through the established Control Plane.

The SDK MUST NOT expose normal engine code to:

```text
routing tables
membership snapshots
destination-selection internals
```

The engine author describes the engine and its capabilities.

Core manages Control Plane interaction.

---

### Transport Integration

The SDK SHOULD NOT require engines to construct the transport stack manually.

Normal engines should not need to manage:

```text
connections
frame headers
frame fragmentation
reassembly
connection pools
transport routing
```

The relationship remains:

```text
Engine
 ↓
SDK
 ↓
Core Runtime
 ↓
Control Plane + Transport
```

Raw transport internals are not part of the normal SDK API.

---

### Error API

The SDK MUST expose a stable engine-facing error model.

It SHOULD preserve meaningful information such as:

```text
category
cause
context
retryability information where applicable
engine-specific error information where permitted
```

The SDK MUST NOT leak unstable Core implementation error types.

Conceptually:

```text
Core internal error
      ↓
SDK stable error representation
      ↓
Engine code
```

---

### Engine-Specific Errors

Engines MAY define rich domain-specific error types.

For example:

```text
ArabicError::InvalidRoot
```

The SDK/Core layer should adapt those errors into the generic Core error model
while preserving useful engine-specific information.

Core MUST NOT become dependent on domain-specific error definitions merely to
support their use.

---

### SDK Error Causality

The SDK MUST preserve useful failure causality.

It must not flatten:

```text
engine failure
dependency failure
transport failure
runtime failure
cancellation
deadline
```

into one generic SDK error that loses the original reason.

---

### Versioning

The Engine SDK is a compatibility boundary.

Breaking SDK changes MUST be deliberate and versioned.

Core internals MAY evolve without breaking engines when the SDK compatibility
contract remains valid.

Conceptually:

```text
Engine
 ↓
SDK version
 ↓
supported Core implementation
```

An engine should not depend on individual internal Core module versions.

---

### SDK and Core Compatibility

The SDK SHOULD define an explicit compatibility relationship with Core.

The exact mechanism may be:

```text
supported version range
feature compatibility
workspace coupling
other explicit compatibility policy
```

but the relationship must be deliberate.

The engine developer SHOULD NOT need to understand which internal Core
implementation version provides:

```text
runtime.rs
routing.rs
stream.rs
```

as long as the supported SDK/Core compatibility contract remains valid.

---

### Stable and Experimental APIs

The SDK MAY distinguish:

```text
stable SDK API
experimental SDK API
internal Core API
```

Experimental functionality MUST be clearly separated from stable compatibility
commitments.

The exact namespace or feature mechanism remains an implementation choice.

---

### Type Re-Exports

The SDK MAY re-export Core types when they are intentionally part of the stable
engine-facing contract.

Examples may include:

```text
EngineId
CapabilityId
OperationId
ArtifactReference
```

A Core type MUST NOT be re-exported merely because it is convenient.

Re-exporting creates a compatibility commitment and should therefore be
intentional.

---

### Extension Points

The SDK MUST expose only intentional, supported extension points.

Appropriate examples include:

```text
capability handler
lifecycle hooks
health provider
configuration integration
event definitions
observability integration
```

The SDK MUST NOT provide normal extension points for replacing:

```text
runtime registry
Control Plane routing
transport framing
cancellation engine
stream implementation
retry engine
idempotency storage
health aggregation
artifact storage
provenance storage
```

Extension points should occur at semantic boundaries, not internal mechanism
boundaries.

---

### Raw Core Escape Hatch

Raw Core access is not part of the normal Engine SDK API.

Advanced Nizaam-owned infrastructure components may directly depend on Core when
their role genuinely requires lower-level access.

However, this direct Core usage belongs to the internal Nizaam implementation
boundary and MUST NOT be treated as part of the Engine SDK compatibility
contract.

The SDK therefore does not need to expose:

```text
get_core_mut()
replace_runtime()
replace_router()
replace_transport()
```

or equivalent unrestricted hooks.

---

### SDK Restrictions and Nizaam Ownership

The architecture intentionally uses two levels of control:

```text
Nizaam Core Team
→ trusted internal Core access
→ may extend/change Core as required by Nizaam-owned implementations

Engine Developer
→ supported SDK access
→ restricted to stable and intentional extension points
```

This is not contradictory.

The SDK restriction exists to protect:

```text
Core invariants
API stability
runtime consistency
security boundaries
upgrade compatibility
engine isolation
```

while direct Core access exists for the team that owns and evolves the
infrastructure itself.

---

### SDK Should Make Correct Usage Easy

The SDK SHOULD provide safe defaults for:

```text
cancellation
deadlines
task ownership
runtime registration
health wiring
observability context
shutdown
stream lifecycle
```

An ordinary engine should not need to manually wire these systems.

---

### SDK Should Make Unsafe Usage Difficult

The public API should guide developers toward valid Core usage.

Examples:

```text
managed task creation
→ preferred over detached task creation

EngineBuilder
→ preferred over manual runtime registration

Context
→ preferred over manual cancellation state manipulation

SDK Stream
→ preferred over direct buffer/channel management
```

The SDK should encode Core invariants where practical.

For example, operations that are invalid after stream termination should be
difficult or impossible through the normal SDK API.

---

### SDK Safety

The normal Engine SDK API SHOULD NOT require engine developers to use Rust
`unsafe` merely to interact with Core.

If an engine itself requires `unsafe`, that remains an engine implementation
choice.

The SDK MUST NOT make `unsafe` necessary because Core's stable interface is
poorly encapsulated.

---

### SDK and Testing

The SDK MUST make engine testing practical without requiring a full production
deployment.

A test engine should be constructible around the same stable SDK abstractions.

Conceptually:

```text
Test Engine
   ↓
Capability
   ↓
Request
   ↓
Response
```

Core may provide test infrastructure such as:

```text
InMemoryTransport
InMemoryControlPlane
TestRuntime
```

but those remain Core testing mechanisms, not second SDK runtimes.

---

### Test Doubles

The SDK SHOULD expose meaningful abstraction boundaries that can be replaced or
mocked in tests where appropriate.

Possible examples include:

```text
artifact access
dependency access
clock/time source
other explicit engine-facing providers
```

The SDK MUST NOT become a general-purpose dependency-injection framework.

---

### Generated Code

Phase 16 is not itself a code-generation system.

Typed contracts MAY be generated by another system when required.

The SDK must support typed contracts whether their types are:

```text
handwritten
generated
```

The actual code-generation architecture remains outside Phase 16 unless later
required by the contract system.

---

### Engine Packaging

Engine projects SHOULD depend primarily on the Engine SDK rather than directly
depending on every Core internal module.

Conceptually:

```text
Engine package
     ↓
Engine SDK
     ↓
Core
```

This reduces accidental coupling to Core implementation details.

The exact Cargo/workspace structure remains an implementation decision.

---

### Feature Flags

The SDK MAY use feature flags to expose optional functionality such as:

```text
runtime
streaming
artifacts
events
observability
```

but the exact feature layout is not part of the architectural contract.

Feature flags must not create alternative runtime semantics.

---

### Documentation

The SDK MUST provide documentation aimed at engine developers, not merely
generated Rust API documentation.

At minimum, documentation should explain:

```text
engine creation
capability implementation
lifecycle
request/response handling
context
cancellation
deadlines
streaming
background tasks
configuration
health
observability
artifacts
events
errors
testing
SDK/Core compatibility
```

Documentation must emphasize semantic rules and correct usage patterns.

---

### Semantic Transparency

The SDK hides implementation complexity but MUST remain semantically
transparent.

An engine developer should understand:

```text
what Core guarantees
what cancellation means
what deadlines mean
what stream termination means
what errors mean
what retryability means
what idempotency means
what health states mean
```

without needing to know:

```text
how Core implements those mechanisms internally
```

---

### Explicit Non-Goals

Phase 16 must not become:

* a universal Nizaam SDK;
* an external public API SDK;
* a second Core runtime;
* a second lifecycle system;
* a second cancellation system;
* a second streaming system;
* a second retry system;
* a second event system;
* a second health system;
* a second configuration system;
* a second artifact system;
* a second provenance system;
* a second transport system;
* a second Control Plane;
* a domain abstraction framework;
* a dependency-injection framework;
* a generated-wrapper-for-all-Core-types system;
* a mechanism for arbitrary replacement of Core internals.

---

### Files and Folders

**Engine SDK**

* `src/sdk/mod.rs`
* `src/sdk/engine.rs`
* `src/sdk/capability.rs`
* `src/sdk/request.rs`
* `src/sdk/response.rs`
* `src/sdk/context.rs`
* `src/sdk/stream.rs`
* `src/sdk/error.rs`

**SDK integrations**

* `src/sdk/configuration.rs`
* `src/sdk/health.rs`
* `src/sdk/observability.rs`
* `src/sdk/artifact.rs`
* `src/sdk/events.rs`
* `src/sdk/lifecycle.rs`
* `src/sdk/tasks.rs`

**Related Core systems**

* `src/runtime/`
* `src/contracts/`
* `src/capability/`
* `src/operation/`
* `src/streaming/`
* `src/control/`
* `src/transport/`
* `src/retry/`
* `src/idempotency/`
* `src/security/`
* `src/health/`
* `src/config/`
* `src/artifact/`
* `src/provenance/`
* `src/observability/`
* `src/events/`

**Tests**

* `tests/sdk.rs`
* `tests/sdk_engine.rs`
* `tests/sdk_capability.rs`
* `tests/sdk_streaming.rs`
* `tests/sdk_lifecycle.rs`
* `tests/sdk_integration.rs`
* `tests/conformance.rs`

Exact filenames may be adjusted if the existing repository already provides
equivalent modules. The architectural boundaries defined in this phase must
remain.

---

### Boundary

Core owns:

```text
runtime
transport
contracts
capabilities
security
streaming
retry
idempotency
events
Control Plane
health
configuration
artifacts
provenance
observability
```

The Engine SDK owns only:

```text
stable engine-facing API
developer ergonomics
supported engine extension points
adaptation to Core abstractions
SDK-level safety and compatibility boundary
```

Nizaam-owned infrastructure components may bypass the SDK and consume Core
directly when their internal role requires it.

Engine authors using the supported SDK surface cannot replace Core's internal
runtime mechanisms through SDK extension points.

---

### Done when

An engine developer can:

1. create an engine through the SDK;
2. declare engine identity and metadata;
3. register capabilities without manipulating Core registries directly;
4. use typed request/response abstractions where supported;
5. receive a controlled operation context;
6. observe cancellation;
7. observe deadlines;
8. implement asynchronous capabilities;
9. execute managed background tasks;
10. use logical streaming while preserving Core stream semantics;
11. access resolved configuration;
12. provide engine-specific configuration;
13. expose health information;
14. emit logs, metrics, traces, and diagnostics through Core observability;
15. access artifacts through stable SDK abstractions;
16. participate in provenance through supported APIs;
17. declare retry/idempotency metadata without implementing another retry
    subsystem;
18. publish internal events without implementing another event bus;
19. participate in Control Plane registration without manually managing routing
    internals;
20. start and shut down through Core lifecycle integration;
21. return stable SDK errors without leaking private Core error types;
22. define engine-specific errors while preserving generic Core error causality;
23. use stable engine-facing types without depending on arbitrary Core internals;
24. use supported extension points without replacing Core infrastructure;
25. test engines through SDK abstractions without requiring a complete
    production deployment;
26. remain compatible with the supported SDK/Core compatibility policy;
27. build a normal engine without requiring `unsafe` merely to interact with
    Core;
28. preserve all previously verified Core behavior.

SDK tests and integration tests must verify:

```text
engine creation
engine metadata
capability registration
typed capability support
request/response adaptation
context access
identity preservation
cancellation
deadlines
managed tasks
streaming
stream terminal semantics
configuration
health integration
observability integration
artifact integration
provenance integration
retry/idempotency integration
event integration
Control Plane integration
lifecycle integration
shutdown
error adaptation
error causality
SDK/Core compatibility
extension-point restrictions
absence of competing runtime systems
test-engine support
```

---

### Architectural Invariant

The fundamental Phase 16 model is:

```text
Core
→ infrastructure authority

Engine SDK
→ stable restricted engine-facing interface

Engine
→ domain implementation
```

And for trust boundaries:

```text
Nizaam-owned code
→ may directly use Core

Supported Engine code
→ uses Engine SDK
```

The Engine SDK is intentionally restricted.

Those restrictions protect the stability and integrity of Core; they do not
prevent the Nizaam team from directly evolving Core because the Nizaam team
owns the infrastructure.

The final principle is:

```text
SDK hides HOW Core works
SDK exposes WHAT the engine can do
Core remains the authority
```

The SDK should make correct Core usage easy, make unsupported architectural
behavior difficult, and never turn the engine-facing layer into a second
implementation of Core itself.

---

## Phase 17: Testing and Conformance

### Goal

Perform final hardening and verification of Nizaam Core by proving that the
implementation satisfies the functional contracts, architectural invariants,
security boundaries, resource constraints, lifecycle guarantees, compatibility
rules, and cross-phase integration requirements established by Phases 1–16.

Testing does not begin in Phase 17.

Unit, component, integration, and feature testing MUST continue throughout
development.

Phase 17 is the final verification and release-readiness phase.

Phase 17 MUST NOT introduce another Core runtime subsystem or redefine the
architecture established by earlier phases.

The central question is:

```text
"Does Nizaam Core work while continuing to obey its architecture?"
````

Functional correctness and architectural correctness are both required.

---

### Planned implementation

Add the final verification infrastructure for:

```text
unit/component verification
integration testing
end-to-end testing
architectural conformance
architecture-boundary checks
dependency-direction checks
SDK API-surface verification
security regression
identity verification
lifecycle verification
ownership verification
resource-limit verification
transport conformance
streaming conformance
retry/idempotency conformance
Control Plane conformance
event conformance
artifact/provenance integration verification
context-propagation verification
fault injection
stress testing
concurrency/race validation
shutdown/restart validation
compatibility validation
migration validation where explicitly supported
performance regression validation
memory/resource leak validation
documentation/API consistency
release gates
```

Phase 17 collects and verifies requirements established by previous phases
rather than inventing a new independent architecture.

---

### Continuous Testing

Testing remains continuous throughout the entire project.

Earlier phases SHOULD maintain:

```text
unit tests
component tests
integration tests
feature-specific tests
regression tests
```

Phase 17 adds the final system-wide verification and release gates.

A feature MUST NOT be postponed from testing merely because Phase 17 has not
started.

---

### Functional Correctness vs Architectural Correctness

A functional test verifies behavior such as:

```text
Request
   ↓
Response
```

with an expected result.

An architectural test verifies that the same behavior occurs while preserving
architectural boundaries such as:

```text
Request
   ↓
Security
   ↓
Control Plane
   ↓
Transport
   ↓
Engine Runtime
   ↓
Capability
```

and confirms that:

```text
Security cannot be bypassed
Control Plane does not execute capabilities
Transport does not interpret domain semantics
Engine does not bypass Runtime
SDK does not expose forbidden Core mechanisms
```

Both forms of verification are mandatory where applicable.

---

### Definition of Conformance

Conformance is verification that an implementation obeys the stable contracts
and architectural boundaries defined by Nizaam Core.

Conformance covers, where applicable:

```text
identity semantics
lifecycle rules
security boundaries
transport guarantees
streaming guarantees
retry/idempotency guarantees
routing boundaries
SDK restrictions
ownership rules
resource bounds
context propagation
failure semantics
compatibility guarantees
```

A conformance test MUST verify externally observable behavior or an explicitly
defined architectural contract rather than unnecessarily coupling the test to
a private implementation structure.

---

### Conformance Requirements

Every mandatory architectural guarantee MUST have:

```text
implementation path
verification path
executable test
```

The corresponding test MUST be capable of failing when the guarantee is
violated.

Documentation alone is not sufficient proof of conformance.

---

### Mandatory, Optional, and Not Applicable Conformance

Conformance requirements are classified conceptually as:

```text
REQUIRED
OPTIONAL / FEATURE-SPECIFIC
NOT APPLICABLE
```

Core architectural guarantees are mandatory.

Feature-specific conformance becomes mandatory when the relevant feature is
claimed or enabled.

For example:

```text
Transport
→ REQUIRED

Streaming
→ REQUIRED when streaming is supported

Events
→ REQUIRED when the event system is supported

Artifact subsystem
→ NOT APPLICABLE to an implementation that does not use the feature
```

A component MUST NOT fail generic conformance merely because it does not
implement an explicitly optional feature.

However, once a feature is claimed as supported, its applicable conformance
suite MUST pass.

---

### Phase-to-Conformance Traceability

Every important architectural requirement SHOULD be traceable through:

```text
Phase requirement
      ↓
implementation location
      ↓
verification/conformance test
```

For example:

```text
Phase 7 requirement:
large logical messages survive frame fragmentation
        ↓
transport implementation
        ↓
large-message conformance test
```

and:

```text
Phase 15 requirement:
Control Plane must not execute capabilities
        ↓
Control Plane implementation
        ↓
Control Plane boundary test
```

This prevents important architectural requirements from existing only in
`scope.md`.

---

### Conformance Matrix

The project SHOULD maintain a conformance matrix describing which verification
layers cover important requirements.

Conceptually:

```text
Requirement                  Unit   Integration   Conformance   E2E
----------------------------------------------------------------------
Identity separation           ✓         ✓             ✓
Lifecycle                     ✓         ✓             ✓
Security                      ✓         ✓             ✓
Transport framing              ✓         ✓             ✓
Streaming                      ✓         ✓             ✓
Retry/idempotency              ✓         ✓             ✓
Control Plane                  ✓         ✓             ✓
SDK restrictions               ✓         ✓             ✓
Shutdown                      ✓         ✓             ✓
```

The exact representation is an implementation/process decision.

The purpose is to answer:

```text
"What exactly has been verified?"
```

rather than relying only on a green CI result.

---

### Conformance Test Independence

Conformance tests SHOULD primarily interact with stable/public behavior.

A conformance suite MUST NOT depend unnecessarily on:

```text
private struct layout
specific synchronization primitive
specific internal registry
specific internal function name
specific implementation data structure
```

For example, changing:

```text
HashMap
```

to:

```text
immutable routing snapshot
```

must not invalidate a routing conformance test merely because the internal
representation changed.

The conformance suite should survive internal implementation rewrites as long
as architectural guarantees remain unchanged.

---

### White-Box vs Black-Box Tests

The test system MUST distinguish between:

```text
White-box tests
→ implementation correctness

Black-box conformance tests
→ public/stable architectural behavior
```

White-box tests may inspect internal state where necessary.

Black-box conformance tests MUST exercise supported public boundaries.

A conformance test MUST NOT prove a guarantee by directly performing an
internal action that a real consumer could never perform.

---

### Architecture Boundary Tests

Architecture boundaries MUST be verified explicitly.

Examples include:

```text
Core
≠
Domain Logic

Transport
≠
Application Streaming

Retry
≠
Routing

Control Plane
≠
Planner

SDK
≠
Core Internal Runtime
```

Tests MUST verify both:

```text
required behavior
```

and:

```text
forbidden behavior
```

---

### Forbidden-Behavior Verification

Negative architectural rules are first-class conformance requirements.

Examples:

```text
Control Plane executes capability
→ FORBIDDEN

SDK replaces Core runtime
→ FORBIDDEN

Transport interprets domain payload
→ FORBIDDEN

Retry bypasses cancellation
→ FORBIDDEN

Stream publishes after terminal state
→ FORBIDDEN

Engine becomes an implicit Core domain dependency
→ FORBIDDEN
```

The project SHOULD include explicit tests or architecture checks capable of
detecting such violations.

---

### Dependency Direction

The project MUST verify dependency direction.

The intended relationship includes:

```text
Engine
   ↓
Engine SDK
   ↓
Core
```

and:

```text
Core
→ must not depend on individual domain engine implementations
```

The SDK MUST NOT depend on individual engine implementations.

The Go API MUST NOT depend on the Rust Engine SDK as part of the defined
architecture.

The exact dependency-checking tooling is an implementation choice.

Potential mechanisms include:

```text
Cargo dependency checks
module visibility
crate boundaries
forbidden imports
compile-fail tests
architecture-analysis tooling
```

---

### Domain Leakage Verification

Core MUST remain domain-agnostic.

Phase 17 SHOULD verify that Core does not accidentally acquire direct
dependencies or semantics for:

```text
Quran domain
Hadith domain
Arabic grammar
Fiqh concepts
specific ML models
specific engine schemas
other engine-owned business semantics
```

Domain correctness remains an engine/domain concern.

The distinction remains:

```text
Core Conformance
→ infrastructure correctness

Engine Conformance
→ engine/Core integration correctness

Domain Tests
→ semantic/domain correctness
```

---

### SDK Conformance

The Engine SDK MUST be tested from the perspective of a normal engine developer.

The conformance suite MUST verify that an engine can be implemented through the
supported SDK surface without requiring direct access to Core internals.

It MUST also verify that the normal SDK surface does not provide supported APIs
for replacing:

```text
Core Runtime
Control Plane
Transport
Cancellation system
Stream internals
Task registry
Retry engine
Idempotency storage
Health aggregation
Event delivery infrastructure
Artifact storage internals
```

Nizaam-owned internal code may still use Core directly.

Phase 17 MUST verify both:

```text
supported Engine SDK usage
```

and:

```text
Nizaam-owned direct Core usage
```

so that SDK restrictions do not accidentally prevent Nizaam itself from
evolving Core.

---

### SDK API Surface

The stable Engine SDK API SHOULD be checked for:

```text
public type presence
public method compatibility
visibility boundaries
unexpected public types
forbidden internal exposure
deprecation consistency
```

For example, accidental exposure of an internal runtime type as `pub` should
be detectable.

The exact API-surface tooling is an implementation choice.

---

### Security Conformance

Security conformance is mandatory for release.

It MUST cover the interactions between:

```text
identity
authentication
authorization
Control Plane
Engine Runtime
Engine SDK
streaming
retry
events
```

The suite MUST include negative cases such as:

```text
unauthorized request
unauthorized capability
unauthorized destination
invalid credentials
expired credentials
cross-engine isolation
```

and combinations such as:

```text
unauthorized + retry
unauthorized + streaming
unauthorized + event
unauthorized + explicit destination
```

Security regression testing MUST remain a protected release gate.

---

### Identity Conformance

The following identities MUST remain semantically distinct:

```text
MessageId
OperationId
AttemptId
CorrelationId
EventId
EngineId
EngineInstanceId
IdempotencyKey
ArtifactId
```

Phase 17 MUST include cross-system identity verification.

Examples:

```text
retry
→ OperationId remains the same
→ AttemptId changes

new event occurrence
→ EventId changes

new engine instance
→ EngineInstanceId changes
→ EngineId may remain the same
```

No subsystem may silently overload one identity type for another.

---

### Context Conformance

Relevant context MUST survive the major Core execution boundaries without
unintended replacement or loss.

Where applicable, verify propagation of:

```text
OperationId
CorrelationId
deadline
cancellation
security context
tracing context
provenance context
```

through:

```text
Control Plane
   ↓
Transport
   ↓
Engine Runtime
   ↓
Capability
```

Context behavior must preserve logical operation identity.

---

### Lifecycle Conformance

Phase 17 MUST test all relevant lifecycle systems:

```text
Engine
Attempt
Stream
Task
Publisher
Subscription
```

Tests MUST cover:

```text
legal transitions
illegal transitions
terminal states
cancellation
shutdown
owner termination
cleanup
```

Negative tests MUST verify invalid post-terminal operations.

Example:

```text
COMPLETED
   ↓
publish again
```

must result in an explicit rejection/error rather than silent acceptance.

---

### Cross-Lifecycle Conformance

The most important lifecycle tests are cross-system interactions.

Examples:

```text
engine shuts down
→ stream cancels

engine shuts down
→ task cancels

owner terminates
→ subscription terminates

attempt cancelled
→ retry is not created

stream completes
→ producer cannot publish again
```

Cross-lifecycle behavior MUST be tested in addition to isolated state-machine
tests.

---

### Ownership Conformance

Runtime-managed resources MUST have explicit ownership where their phase
requires ownership.

Verify ownership for:

```text
streams
tasks
publishers
subscriptions
```

and verify:

```text
owner terminates
→ owned resources terminate appropriately
```

Unrelated owners MUST NOT be able to accidentally control another owner's
resources.

---

### Resource-Limit Conformance

Phase 17 MUST deliberately exercise configured resource limits.

Tests may create controlled overload involving:

```text
streams
tasks
subscription queues
retry budgets
large logical messages
slow consumers
routing membership
```

The system MUST demonstrate:

```text
bounded behavior
explicit rejection/failure where applicable
backpressure where applicable
no unbounded memory growth
no silent data corruption
```

Tests do not require absurd production-scale numbers.

The purpose is to verify that declared resource bounds are actually enforced.

---

### Cleanup and Leak Verification

Owned resources MUST have a complete termination and cleanup path.

Tests SHOULD repeatedly perform:

```text
create
→ use
→ terminate
→ cleanup
```

for resources such as:

```text
streams
tasks
subscriptions
connections
routing membership
idempotency records
```

where the applicable resource is persistent or long-lived.

The project SHOULD verify:

```text
no unexpected active tasks
no leaked streams
no orphan subscriptions
no stale routing membership
no unreleased critical resources
```

before release candidates.

---

### Transport Conformance

Transport conformance MUST validate Phase 7 message-framing guarantees.

Tests MUST cover:

```text
small logical messages
large logical messages
multiple transport frames
frame ordering
reassembly
oversized frame handling
payload integrity
```

The conformance suite MUST preserve the distinction:

```text
4-byte frame header
≠
4-byte total message limit
```

and:

```text
MAX_FRAME_LENGTH
→ payload limit for one frame
```

rather than the complete logical message.

A logical message larger than one frame MUST survive fragmentation and
reassembly without payload loss.

---

### Transport vs Streaming Conformance

Phase 17 MUST verify that transport fragmentation and application-level
streaming remain distinct.

For example:

```text
one logical message
→ multiple transport frames
→ one logical message
```

and:

```text
one logical stream
→ many logical items
→ each logical item may itself span multiple transport frames
```

The application consumer MUST observe logical messages/items, not raw transport
frame boundaries.

---

### Streaming Conformance

When streaming is supported, tests MUST cover:

```text
logical item ordering
partial results
final results
empty streams
completion
cancellation
failure
deadline
backpressure
consumer disappearance
no silent item dropping
no publication after terminal cancellation/completion
```

The test suite MUST verify that a logical stream item remains one logical item
even when its transport representation spans multiple frames.

---

### Retry and Idempotency Conformance

When retry/idempotency is supported, tests MUST cover:

```text
retryable transient failure
non-retryable failure
attempt limits
backoff
jitter
deadline exhaustion
cancellation
unknown outcome
idempotent duplicate
in-flight duplicate
completed duplicate
idempotency conflict
partial-stream retry protection
artifact retry safety
```

A particularly important test is:

```text
Attempt 1
→ side effect succeeds
→ response is lost

Retry request
→ same idempotency key
```

which MUST NOT produce an unintended duplicate side effect.

---

### Retry and Routing Conformance

Cross-phase verification MUST include:

```text
Attempt 1
→ Control Plane selects Engine A

Engine A fails

Phase 13 permits another attempt

Attempt 2
→ Control Plane resolves destination again
→ Engine B
```

The test SHOULD verify:

```text
same OperationId
different AttemptId
new destination where policy permits
same applicable security context
same operation deadline
same applicable configuration snapshot
```

Routing MUST NOT silently become the retry mechanism.

---

### Partial-Stream Retry Conformance

If an attempt has already emitted externally observable stream items:

```text
partial output
→ attempt failure
```

the same stream MUST NOT be blindly restarted by the retry system.

Unless explicit stream retry/resume/deduplication semantics are supported,
automatic retry after partial externally observable output MUST be prohibited.

---

### Control Plane Conformance

Control Plane conformance MUST cover:

```text
registration
logical EngineId
EngineInstanceId
capability eligibility
contract compatibility
health/readiness
hard destinations
preferences
fallback
routing selection
routing-state consistency
per-attempt resolution
```

The suite MUST verify:

```text
Control Plane does not execute capabilities
Control Plane does not override Engine Runtime admission
```

---

### Control Plane / Security Conformance

Routing MUST NOT bypass security.

Test combinations such as:

```text
security → routing
security → explicit destination
security → retry
security → streaming
```

to verify that all routed execution remains under the applicable security
context.

---

### Control Plane / Runtime Conformance

A stale routing decision is allowed to encounter a changed Engine Runtime
state.

For example:

```text
Control Plane
→ selects Engine A

Engine A
→ enters DRAINING

request arrives
→ Engine Runtime rejects new work
```

This is a valid state interaction.

The conformance suite MUST verify that the Control Plane cannot override the
Engine Runtime's final local admission decision.

---

### Event Conformance

When internal events are supported, test:

```text
event identity
event type
scope
publication
subscription
ordering
subscriber concurrency
subscriber isolation
bounded delivery
overload semantics
cancellation
owner termination
shutdown
security
context
```

The suite MUST also verify that the event system does not accidentally become:

```text
durable by default
replayable by default
an acknowledgement protocol
a retry system
a distributed message broker
```

---

### Artifact and Provenance Conformance

Where artifacts/provenance are supported, test their interaction with:

```text
requests
attempts
retries
events
streams
Control Plane
SDK
```

The suite MUST preserve the distinction between:

```text
artifact identity
artifact version
artifact reference
provenance record
operation
attempt
```

Artifact retry behavior MUST not accidentally create unintended duplicate
logical artifacts.

---

### Observability Conformance

Important execution events must remain observable with enough identity to
preserve causal understanding.

For example:

```text
Operation X
Attempt 2
EngineInstance A
Capability Y
```

should remain correlatable.

The suite MUST preserve failure causality.

For example:

```text
routing succeeded
transport failed
```

must not be incorrectly reported as:

```text
Control Plane failed
```

Phase 11 remains authoritative for observability implementation.

---

### Failure/Recovery Conformance

Phase 17 MUST intentionally introduce failures in areas such as:

```text
transport
engine runtime
capability
dependency
stream producer
task
retry
Control Plane
health
configuration
artifact access
event subscriber
```

Tests MUST verify, according to the declared contract:

```text
failure scope
propagation
cleanup
observability
recovery behavior
retry behavior
```

The test suite MUST distinguish:

```text
known failure
cancellation
deadline
unknown outcome
```

where those concepts are defined.

---

### Recovery Guarantees

Phase 17 MUST verify only recovery guarantees that the architecture actually
promises.

For example:

```text
in-memory event
→ process crash
→ event may be lost
```

is valid when event durability is not promised.

Where persistent state explicitly promises recovery:

```text
persistent state
→ restart
→ promised state remains recoverable
```

must be verified.

The project MUST NOT create accidental recovery requirements merely because a
test author expected stronger behavior.

---

### Fault Injection

Fault injection SHOULD provide deterministic scenarios such as:

```text
transport connection loss
engine rejection
capability failure
stream failure
task cancellation
unknown retry outcome
Control Plane unavailable
stale health state
invalid configuration
artifact storage failure
event subscriber failure
```

The exact tooling remains an implementation choice.

---

### Limited Chaos Testing

The project SHOULD include limited systemic fault/stress scenarios such as:

```text
engine termination
connection loss
delayed responses
resource pressure
concurrent shutdown
```

Chaos testing MUST remain a verification technique.

Phase 17 MUST NOT become a separate distributed chaos-engineering platform.

---

### Concurrency and Race Verification

Concurrency-sensitive areas MUST be stressed, including combinations such as:

```text
register + route
route + unregister
stream + shutdown
retry + cancellation
event publish + unsubscribe
task completion + shutdown
```

Tests SHOULD detect:

```text
double termination
lost item
duplicate item
use-after-close
deadlock
race/state-ordering failure
orphan task
orphan stream
stale routing state
```

Where appropriate, the project SHOULD use concurrency tooling capable of
detecting relevant race/deadlock/state-ordering failures.

The exact tool is not architecturally fixed.

---

### Deterministic Concurrency Testing

Where possible, critical concurrency tests SHOULD control ordering explicitly
using mechanisms such as:

```text
barriers
controlled scheduling
synchronization points
```

For example:

```text
Attempt 1 starts
      ↓
cancel
      ↓
retry decision
```

must deterministically verify:

```text
no Attempt 2
```

rather than relying on an accidental thread schedule.

---

### Shutdown Conformance

Shutdown MUST be tested while the system has active:

```text
requests
streams
tasks
retry delays
event handlers
transport activity
registration changes
```

The expected lifecycle must remain consistent with previous phase guarantees.

For example:

```text
Engine DRAINING
→ no new normal work

existing operation
→ continues/cancels according to policy

termination
→ cancellation
→ cleanup
```

---

### Restart Conformance

The system SHOULD be tested through:

```text
start
→ serve
→ stop
→ start again
```

where restart semantics are supported.

The suite SHOULD verify:

```text
registration reconstruction
routing membership reconstruction
resource cleanup
absence of invalid stale streams/tasks
```

Runtime membership that is intentionally ephemeral does not need to survive a
crash as persistent state.

---

### Long-Running Tests

Phase 17 SHOULD include controlled long-running scenarios to detect:

```text
memory growth
resource leaks
task accumulation
subscription accumulation
routing membership leaks
stream cleanup failures
```

The purpose is to detect repeated lifecycle leakage rather than perform
arbitrary large-scale benchmarking.

Patterns such as:

```text
create
→ terminate
→ leak
→ repeat
```

must be detectable.

---

### Performance Regression

Performance tests are regression guards rather than fixed universal targets.

Potential measurements include:

```text
request latency
routing latency
transport throughput
frame processing
stream throughput
task scheduling
event delivery
SDK overhead
```

Phase 17 SHOULD compare against an appropriate project baseline.

The architecture MUST NOT freeze arbitrary hardware-dependent limits such as:

```text
routing < 1 ms
```

without a separate explicit requirement.

---

### Compatibility Conformance

Because the Engine SDK is the engine-facing compatibility boundary, Phase 17
MUST validate supported SDK/Core combinations.

Conceptually:

```text
Engine built against SDK X
        ↓
Core version Y
        ↓
supported?
```

Supported combinations MUST remain functional.

Unsupported combinations MUST fail clearly rather than producing undefined
behavior.

---

### SDK/Core Compatibility Matrix

The project SHOULD maintain a compatibility model such as:

```text
           Core A   Core B   Core C
SDK 1         ✓       ✓       -
SDK 2         -       ✓       ✓
SDK 3         -       -       ✓
```

The exact matrix and tooling are implementation choices.

---

### Migration Conformance

Where compatibility with persisted or versioned state is explicitly promised,
migration tests MUST verify supported transitions.

Examples may include:

```text
old artifact metadata
→ new Core

old idempotency state
→ new Core

supported configuration state
→ new Core
```

The project MUST NOT promise infinite backward compatibility automatically.

Only explicitly supported compatibility paths require migration tests.

---

### Crash Consistency

For persistent systems that promise crash consistency, Phase 17 MUST verify
the declared guarantees around interruption during state updates.

For example:

```text
write state
↓
process crashes
```

must result in the documented recovery state.

Ephemeral systems do not acquire persistence guarantees merely because they are
tested during crashes.

---

### Reference Conformance Engine

Phase 17 MUST provide a minimal reference/conformance engine whose purpose is
testing Core infrastructure rather than implementing real domain functionality.

Conceptually:

```text
ConformanceEngine
├── Echo
├── AlwaysFail
├── Slow
├── LargeResponse
├── Stream
├── StreamThenFail
├── RetryableFailure
├── ArtifactProducer
├── EventProducer
└── BackgroundTask
```

The reference engine should provide controlled deterministic behavior for:

```text
Runtime
Transport
Control Plane
Streaming
Retry
Events
SDK
Artifacts
```

without depending on Quran, Arabic, KG, or other domain logic.

---

### Reference Engine Isolation

The reference engine MUST remain test infrastructure.

Production Core MUST NOT depend on it.

The dependency direction is:

```text
Tests / Conformance
        ↓
Core / SDK
```

not:

```text
Core
   ↓
Tests / Conformance Engine
```

The exact repository location may be:

```text
tests/
test-support/
conformance/
```

or an equivalent structure.

---

### Domain Engine Conformance

Actual Nizaam engines should have their own integration/conformance checks.

For each claimed capability, these may verify:

```text
registration
contract compatibility
lifecycle
security
request/response
streaming when supported
shutdown
SDK compliance
```

They MUST NOT replace domain-specific semantic testing.

The Arabic Engine may therefore have:

```text
Engine Conformance
→ integration correctness

Arabic Domain Tests
→ linguistic/algorithmic correctness
```

These remain separate.

---

### Reference Engine Deliberate Failure

The conformance engine SHOULD include deliberately failing or slow capabilities
so that failure paths can be exercised deterministically.

Examples include:

```text
AlwaysFail
SlowResponse
StreamThenFail
CancelledWork
RetryableFailure
UnknownOutcomeSimulator
```

These are test tools and are not production domain capabilities.

---

### No Silent Failure Conformance

Phase 17 MUST explicitly test for paths where:

```text
error occurs
→ data disappears
→ caller sees success
```

Examples include:

```text
stream item silently dropped
event lost despite a non-lossy delivery contract
task rejected but reported accepted
routing failed but request reported routed
artifact publication failed but operation reported successful
```

Every such path must either:

```text
succeed honestly
or
return/report an explicit failure/outcome
```

according to the established contract.

---

### No Hidden Test-Only Semantics

Test environments may use:

```text
in-memory transport
mock storage
reference engine
test dependencies
```

but they MUST NOT accidentally bypass architectural requirements merely to make
tests easier.

For example:

```text
test runtime
→ automatically bypasses security/routing
```

while:

```text
production runtime
→ enforces security/routing
```

does not provide meaningful conformance.

Black-box conformance tests must exercise the same supported architectural
boundaries as production.

---

### Honest Test Doubles

Mocks that always return success are useful for isolated unit testing but are
not sufficient for architecture conformance.

Conformance requires controlled implementations that exercise:

```text
success
failure
delay
cancellation
large payloads
streaming
retry
artifact handling
event handling
```

The reference engine exists specifically to provide these scenarios.

---

### Documentation Conformance

By Phase 17, stable documentation is part of the public contract for:

```text
Engine SDK semantics
Core guarantees
lifecycle
streaming
retry
routing
security
compatibility
```

Implementation changes that intentionally alter a documented architectural
guarantee MUST be treated as an architectural change rather than an ordinary
internal refactor.

---

### Architecture Regression Protection

Architecture checks established by Phase 17 SHOULD continue running after
Phase 17.

For example:

```text
Core
→ does not depend on domain engines
```

must remain continuously protected after the initial conformance phase.

Phase 17 establishes these checks; future CI/regression workflows keep them
active.

---

### Architecture Changes After Phase 17

After Phase 17, changing a frozen architectural invariant is not an ordinary
bug fix.

For example:

```text
Phase 15 says A
new implementation requires B
```

requires:

```text
architecture change
   ↓
scope update
   ↓
affected implementation changes
   ↓
conformance updates
```

This preserves `scope.md` as an actual architectural contract.

Internal implementation rewrites that preserve the existing guarantees do not
constitute architecture changes.

---

### Test Reproducibility

Serious conformance, fault, stress, and concurrency failures SHOULD retain
enough information to reproduce the failure.

Where applicable, preserve:

```text
test name
Core/SDK version
configuration
scenario
seed
environment information
```

The exact report format remains an implementation/process decision.

---

### Test Layers

The overall verification model is:

```text
                         Phase 17
                            │
        ┌───────────────────┼───────────────────┐
        ↓                   ↓                   ↓
   Functional           Conformance        Architecture
    Testing               Testing             Checks
        │                   │                   │
        └───────────────────┼───────────────────┘
                            ↓
                     Integration / E2E
                            ↓
                Security / Resource Tests
                            ↓
                 Fault / Stress / Race Tests
                            ↓
                Shutdown / Restart Tests
                            ↓
                 Compatibility Validation
                            ↓
                       Release Gates
```

Additional supporting layers include:

```text
Reference Conformance Engine
Architecture Dependency Checks
Compile-fail/Negative API Tests
Performance Benchmarks
Long-running Tests
```

No single testing layer replaces the others.

---

### Release Gates

Phase 17 uses hierarchical release gates.

#### PR Gate

Fast verification SHOULD include:

```text
unit tests
component tests
basic integration tests
architecture checks
basic security checks
```

#### Merge / Release Candidate Gate

Full verification SHOULD include:

```text
full integration
conformance suite
security regression
negative/boundary tests
resource/lifecycle tests
SDK compatibility
```

#### Release Gate

Extended verification SHOULD include:

```text
full test suite
stress tests
fault injection
restart/shutdown validation
performance regression
long-running resource tests
```

The exact CI system and automation are implementation choices.

---

### Release-Blocking Conditions

A release MUST NOT proceed when a release-blocking condition remains unresolved.

At minimum, release-blocking conditions include:

```text
mandatory test failure
mandatory conformance failure
security conformance failure
critical architectural boundary violation
required compatibility failure
critical resource/lifecycle failure
```

Known issues should be explicitly classified, for example:

```text
BLOCKER
CRITICAL
IMPORTANT
NON-BLOCKING
KNOWN LIMITATION
```

The exact severity vocabulary is a project/process decision.

Only issues classified as release-blocking should prevent release.

---

### Release Candidate Clean State

Before declaring a release candidate, the system SHOULD demonstrate:

```text
no unexpected active tasks
no leaked active streams
no orphan subscriptions
no stale runtime membership
no unreleased critical resources
```

where these conditions are observable through the available diagnostics.

---

### Cross-Phase Conformance

Phase 17 MUST explicitly test important boundaries between phases.

Examples include:

```text
Phase 7 + Phase 12
transport fragmentation
↔
logical streaming
```

```text
Phase 12 + Phase 13
stream cancellation
↔
retry suppression
```

```text
Phase 13 + Phase 15
retry attempt
↔
new destination resolution
```

```text
Phase 15 + Phase 16
engine registration
↔
SDK abstraction
```

```text
Phase 9 + Phase 15
security
↔
destination routing
```

Cross-phase behavior is a first-class conformance concern.

---

### Core Conformance vs Engine Domain Correctness

Phase 17 MUST maintain the separation:

```text
Core Conformance
→ generic infrastructure correctness

Engine Conformance
→ Core/Engine integration correctness

Domain Tests
→ domain/algorithm correctness
```

For example, Phase 17 may verify that:

```text
Arabic Engine
→ correctly registers Arabic.Sarf
→ receives the correct request type
→ respects lifecycle/security/streaming rules
```

but it must not claim to prove that:

```text
Arabic.Sarf
→ linguistically produces the correct Arabic morphology
```

That remains an engine/domain test.

---

### Explicit Non-Goals

Phase 17 must not become:

* a new runtime;
* a new orchestration system;
* a new monitoring platform;
* a new production architecture;
* a test-only architecture with semantics different from production;
* a giant chaos-engineering platform;
* an alternative implementation of any Core subsystem;
* a replacement for unit/component testing;
* a mechanism for inventing new architectural guarantees merely for testing.

Phase 17 verifies the architecture built in Phases 1–16.

---

### Files and Folders

**Conformance**

* `tests/conformance.rs`
* `tests/conformance_core.rs`
* `tests/conformance_transport.rs`
* `tests/conformance_streaming.rs`
* `tests/conformance_retry.rs`
* `tests/conformance_events.rs`
* `tests/conformance_control_plane.rs`
* `tests/conformance_sdk.rs`
* `tests/conformance_security.rs`

**Integration**

* `tests/integration.rs`
* `tests/e2e.rs`

**Architecture checks**

* `tests/architecture.rs`
* `tests/compile_fail.rs`

**Test support**

* `tests/support.rs`
* `tests/conformance_engine.rs`

**Benchmarks / stress**

* `benches/`
* `tests/stress.rs`
* `tests/fault_injection.rs`

Exact filenames and directories may be adjusted to fit the existing
repository. The architectural verification requirements defined above must
remain.

---

### Boundary

Phase 17 owns:

```text
verification
conformance
architecture checks
cross-phase integration validation
security regression
resource/lifecycle validation
fault/stress validation
compatibility validation
release gating
```

Phase 17 does NOT own:

```text
runtime architecture
transport architecture
streaming architecture
retry architecture
event architecture
Control Plane architecture
SDK architecture
domain implementation
```

Those were defined by earlier phases.

Phase 17 verifies them.

---

### Done when

Phase 17 is complete when:

1. all mandatory functional tests pass;
2. all mandatory architectural conformance checks pass;
3. all mandatory security checks pass;
4. all required cross-phase integration tests pass;
5. required negative and boundary tests pass;
6. lifecycle and ownership tests pass;
7. resource-limit and cleanup tests pass;
8. required SDK compatibility checks pass;
9. fault, restart, and shutdown validation passes;
10. performance regression checks pass where required;
11. long-running resource checks pass where required;
12. required migration checks pass where compatibility is promised;
13. reference/conformance engine behavior is validated;
14. scope requirements are traceable to implementation and verification;
15. serious conformance/fault results are reproducible where applicable;
16. documentation matches the stable public contracts;
17. no unresolved release-blocking issue remains;
18. architecture checks are established for continued regression protection.

---

### Final Architectural Principle

Phase 17 does not attempt to prove that every possible behavior has been tested.

It proves that:

```text
every mandatory architectural guarantee
        ↓
has an implementation path
        ↓
has an explicit verification path
        ↓
has executable evidence
        ↓
passes under normal, failure, and boundary conditions
```

The final verification model is:

```text
Phases 1–16
→ define and build the system

Phase 17
→ challenge the system
→ verify the system
→ harden the system
→ establish release confidence
```

The central rule is:

> **A green test suite is not proof of architectural correctness unless the architecture itself is covered by explicit conformance checks.**

The final stopping condition is:

```text
BUILD
  ↓
INTEGRATE
  ↓
BREAK
  ↓
VERIFY
  ↓
CONFORM
  ↓
RELEASE
```

Phase 17 is the final hardening phase, not the beginning of another
architecture.

---

## Architectural Decisions

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
* Phase 7 introduces a shared provider-neutral transport boundary through the `Transport`, `Connection`, `ByteSink`, `ByteSource`, and `MessageStream` abstractions.
* Transport framing uses a fixed 20-byte binary header with big-endian multi-byte fields. Transport metadata remains separate from the logical universal message payload.
* Transport message identity is distinct from the Core `MessageId`; transport-level message IDs belong to the framing layer and logical message identity remains part of the universal contract.
* Individual transport frames are bounded by `MAX_FRAME_LENGTH`; logical messages may exceed one frame and are fragmented and reassembled without changing the logical payload.
* Fragmentation requires contiguous zero-based fragment ordering beginning at index zero, with explicit final-fragment signaling. Duplicate, missing, or unexpected fragment sequences are protocol errors, and Phase 7 does not provide retransmission or retry at the framing layer.
* Complete reassembled logical messages are bounded separately from individual frames to prevent unbounded reassembly.
* Transport treats frame payloads as opaque bytes and does not interpret engine-specific payload semantics, contract meaning, operation context, or higher-level message metadata.
* `UniversalClient<T: Transport>` is the shared client mechanism for sending universal requests through an abstract transport; higher-level typed capability clients are expected to build on this mechanism rather than introduce separate transport stacks.
* `EngineServer` provides the server-side communication boundary, including handler registration, capability-based request dispatch, and serving/draining/stopped request-admission behavior.
* Phase 7 integrates with the existing Capability System rather than introducing a second capability registry or dispatch mechanism. The server routes universal requests into the established capability handler boundary.
* The Phase 7 in-memory transport is an implementation used to verify the abstract transport contract. It does not establish an architectural commitment to a concrete network transport, async runtime, serialization provider, or persistence system.
* The Engine Runtime owns the engine lifecycle state machine and controls lifecycle transition validity.
* The canonical engine lifecycle is `Created → Starting → Configuring → Dependencies → Capabilities → Registering → Ready → Serving → Draining → Stopped`.
* `Created` is the runtime's initial construction state and is not part of the normal startup progression after runtime creation.
* `FAILED` is a lifecycle failure condition rather than a normal lifecycle state. Startup failures must not allow an engine to reach `READY` or `SERVING` while unresolved.
* A terminal startup failure must ultimately reach `STOPPED` through the defined shutdown path. Phase 8 does not introduce automatic startup retry loops.
* `READY` and `SERVING` are distinct states. `READY` means required initialization is complete and the engine is eligible to serve, while `SERVING` means normal request admission is active.
* Only an engine in `SERVING` may accept new normal requests. Startup states, `READY`, `DRAINING`, and `STOPPED` reject new normal work.
* Lifecycle admission occurs before request validation, capability resolution, or capability handler invocation.
* Requests already admitted before `DRAINING` remain runtime-owned work and may continue according to their existing execution context, cancellation, and deadline rules.
* `DRAINING` cannot transition back to `SERVING`, and `STOPPED` is terminal.
* The Engine Runtime provides the shared request execution coordination mechanism but does not own engine-specific workflows, business rules, domain state, payload semantics, or domain synchronization.
* The Phase 8 request path preserves the established Core boundaries: transport reconstructs messages, runtime performs admission and execution coordination, contracts provide structural validation, context provides execution state, capability dispatch resolves and invokes handlers, and engines retain ownership of handler semantics.
* The runtime must not resolve or dispatch a capability for a request that has already failed lifecycle admission.
* Universal request structural validation occurs before capability handler invocation.
* Existing cancellation and deadline mechanisms from Phase 5 remain the authoritative execution-boundary mechanisms. Phase 8 does not introduce a competing cancellation or deadline system.
* Cancellation and deadline precedence is preserved before capability resolution and handler execution.
* The Engine Runtime permits independent admitted requests to execute concurrently and must not introduce global serialization of engine capability handlers.
* Request execution contexts remain isolated between unrelated requests. Operation, cancellation, deadline, security, and provenance context must not be shared mutably between unrelated requests.
* Core does not mandate a specific executor, async runtime, worker pool, scheduler, or thread model for request execution.
* Phase 8 does not establish a mandatory global concurrency limit. Resource-aware bounded concurrency and advanced scheduling remain deferred to Phase 12.
* Engine shutdown follows `SERVING → DRAINING → STOPPED`. Shutdown first stops new request admission, then signals runtime-owned background work, allows already-admitted work to finish, joins runtime-owned work, and finally reaches `STOPPED`.
* `DRAINING` is a graceful shutdown state and does not mean immediate cancellation of already-admitted work.
* Runtime-owned background tasks participate in runtime shutdown through cancellation signaling and joining. Advanced scheduling, resource accounting, and execution policy remain deferred to Phase 12.
* Required and optional dependency behavior remains distinct. Required dependency initialization must succeed before `READY`; optional dependency absence may not block readiness when the engine can still provide its required behavior.
* Phase 8 does not introduce automatic dependency retry loops. Dependency cycles must fail deterministically rather than causing indefinite startup waiting.
* Loss of a dependency after the engine is already serving does not automatically force the runtime to `STOPPED` solely because of that dependency loss.
* Capability registration and capability availability remain distinct. A capability may be registered during startup but is normally callable only when its engine is `SERVING`.
* Required capability initialization failures prevent the engine from reaching `READY`, while optional capability failures may permit readiness when required engine behavior remains valid.
* Engine registration and capability registration remain separate mechanisms. Engine registration establishes runtime participation, while capability registration establishes the capabilities provided by the engine.
* Phase 8 does not introduce an independent global routing or destination system. Detailed destination resolution and inter-engine routing remain deferred to Phase 15 Control Plane.
* The Engine Runtime may invoke engine-owned capability handlers but must not implement, interpret, or manipulate their domain workflows or domain state.
* Runtime-owned shutdown uses the same Core cancellation mechanism established for operation and task contexts.
* `InvalidTransition` is a Nizaam-owned technical error type provided by the Core Error System. Lifecycle transition validation does not define a competing runtime-specific error system.
* `InvalidTransition` remains independent of lifecycle implementation details and is represented through the Nizaam Error System without coupling the Error System back to the Runtime module.
* Phase 8 does not implement retry, idempotency, advanced streaming, security middleware/authorization policy, artifact persistence, detailed observability/health, Internal Events, Control Plane routing, Engine SDK, domain workflows, engine-specific storage, or a mandatory global concurrency policy.
* Phase 8 integration tests verify composition of the already implemented Core mechanisms rather than requiring a production engine implementation. This keeps runtime behavior testable without introducing engine-specific semantics into Core.
* Phase 8 completion does not require Core to model every possible future engine edge case. Additional engine-specific and cross-system edge cases can be covered progressively as actual engines and later Core phases introduce concrete requirements.
* Mandatory middleware is enforced by the Core runtime request path rather than by application convention.
* An empty/unconfigured middleware pipeline is fail-closed for externally handled runtime requests because the Phase 9 security boundary is mandatory.
* `EngineServer` integrates the `ExecutionPipeline` directly rather than creating a second request-processing system.
* Security middleware uses provider-neutral `Authenticator`, `Authorizer`, and credential-extraction abstractions. No JWT, OAuth, API key, mTLS, OIDC, external IdP, or concrete security provider is selected by Core.
* `SecurityContext` contains the authenticated principal and optional calling-service identity. This preserves both original caller identity and intermediate service identity when both are relevant.
* `EngineContext.security` remains optional before authentication. Core does not invent an anonymous or fake principal merely to satisfy the context shape.
* Child `EngineContext` values preserve the trusted `SecurityContext`, keeping security context propagation aligned with the established Phase 5 context model.
* Generic Core authorization consumes the requested `CapabilityId` identified from the universal request descriptor and does not require resolving the executable handler before authorization.
* `CapabilityId` used by security/runtime integration comes from the established identity module and is not redefined by the Capability System.
* The universal request constructor remains explicit through `UniversalRequest::new(MessageEnvelope::new(...))`; no implicit conversion from `MessageEnvelope` is introduced.
* Middleware may inspect or enrich shared runtime state, but request-specific security state remains scoped to the current `EngineContext` and must not be stored as mutable global current-request state.
* Request middleware runs in registration order. Response middleware runs in reverse registration order, giving outer middleware the expected response-finalization symmetry.
* Middleware rejection and middleware failure both stop downstream capability execution. The distinction is preserved in the pipeline error model.
* Middleware cannot be treated as a substitute for capability/domain authorization. Engine-owned authorization may still apply after generic Core authorization and before domain handler execution.
* The server captures the admitted capability identity and rejects a middleware path that attempts to change that capability before handler dispatch. This prevents authorization of one capability followed by execution of another.
* A request that continues through middleware without establishing trusted security context is rejected before capability dispatch. This is a deliberate fail-closed rule for the externally admitted runtime path.
* Cancellation and deadline checks remain owned by the established Phase 5 context mechanisms. Phase 9 integrates with them rather than creating a second security-specific timeout or cancellation mechanism.
* Basic tracing and metrics integration remain middleware concerns in Phase 9; complete telemetry, health, diagnostics, and configuration remain Phase 11 responsibilities.
* Pipeline failures are logged through the existing Core `LoggingSystem` as structured errors while credential material is excluded from logs.
* Authentication request debugging is redacted. Credential bytes are represented as `[REDACTED]` in `Debug` output so debug formatting cannot disclose authentication material.
* Security processing remains independent from transport implementation, serialization provider, storage, domain authorization, and Control Plane routing.
* Phase 9 does not add retry, idempotency, artifact persistence, advanced streaming/concurrency policy, Internal Events, Control Plane routing, Engine SDK behavior, or domain workflows.
* Core manages artifact identity, versioning, references, lifecycle mechanics, integrity mechanisms, publication, resolution, retrieval abstractions, access mechanisms, and provenance linkage without owning artifact domain semantics.
* `Artifact`, `ArtifactVersion`, and `ArtifactReference` are distinct concepts and must not be collapsed into one type.
* `ArtifactId` provides stable logical artifact identity across versions. Version identity and content digest remain distinct from artifact identity.
* An `ArtifactVersion` represents one exact immutable logical content state and may reference a physical content representation through `ContentReference`.
* Artifact versions do not imply ordering or Semantic Versioning semantics unless explicitly authorized later.
* A published artifact version is immutable with respect to its content, content identity, version identity, and integrity information. Content changes require creation of a new version.
* One logical artifact version may have multiple physical representations. Physical representation differences do not automatically create new logical artifact versions.
* `ContentReference` remains provider-neutral. Core does not become an object-storage, database, filesystem, S3, or other storage-provider implementation.
* Artifact retrieval must remain compatible with large content and must not require complete artifact materialization in Core memory.
* Resolution and retrieval are separate mechanisms. Resolution identifies an exact artifact version; retrieval obtains content; integrity verification determines whether retrieved content matches the recorded integrity information.
* Artifact versions must contain verifiable integrity information before successful publication.
* Publication is distinct from creation and must behave as an externally atomic `VALIDATED → PUBLISHED` transition.
* Unpublished versions must remain outside the normal published-artifact resolution path.
* Mutable aliases are resolution metadata and do not modify artifact versions. Exact versions remain independently addressable and immutable.
* Exact artifact versions are canonical for executable provenance and reproducibility. Mutable aliases may be retained as submission/history metadata but must not replace the resolved exact version.
* A running operation must not silently switch artifact versions because a mutable alias changes.
* `SUPERSEDED`, `ARCHIVED`, and `REVOKED` have distinct meanings. Supersession does not erase historical validity, while revocation prevents normal trusted use according to applicable policy.
* Historical provenance is append-oriented and must not be silently rewritten when artifact lifecycle state changes later.
* Provenance records reference artifacts and exact versions rather than duplicating artifact content.
* Provenance remains extensible for operation attempts introduced by Phase 13 without implementing retry semantics in Phase 10.
* Artifact access and authorization reuse the Phase 9 security boundary. Phase 10 does not introduce an independent artifact authorization framework.
* Artifact identity and content remain independent from individual Phase 7 transport frames. Large artifacts may span multiple bounded transport frames without becoming multiple artifact versions.
* Physical deduplication and content-addressable storage are optional provider concerns. Equal content digests do not make logical artifact versions identical.
* Artifact failure conditions remain semantically distinct, including not found, invalid reference, integrity failure, validation failure, resolution failure, publication failure, retrieval failure, access denial, and revoked artifact conditions.
* Phase 11 keeps **Configuration, Health, and Observability as independent Core systems**; none becomes a hidden lifecycle controller, domain-logic system, or correctness dependency.
* **Configuration is immutable by default** for an active runtime instance. Runtime mutation is supported only through explicitly controlled updates.
* Runtime configuration changes use the pipeline **Proposed Update → Parse → Validate → Resolve → Apply Atomically**. Failed updates leave the last valid configuration active and cannot partially modify runtime state.
* **Configuration snapshots are the runtime configuration boundary**; a new valid update produces a new snapshot rather than rebuilding configuration per request. Already-running operations do not silently switch configuration semantics.
* Core provides generic configuration mechanisms while **engines own domain-specific configuration semantics and semantic validation**.
* Secret handling remains a **reference/resolution boundary**, not a Core secret-management platform; sensitive values must not be emitted through logs, metrics, traces, diagnostics, or errors.
* Health reports operational condition but **does not own lifecycle**. Readiness remains aligned with the Phase 8 serving boundary, while lifecycle remains runtime-owned.
* Health aggregation preserves distinct **Healthy, Degraded, Unhealthy, and Unknown** conditions and evaluates component health deterministically. Required dependency failure takes precedence over unknown observations.
* Observability remains composed of **logging, metrics, tracing, diagnostics, correlation, and telemetry**, without replacing the existing Logging or Error systems.
* Provider-specific telemetry, monitoring backends, secret providers, deployment systems, self-healing, and dynamic configuration control-plane behavior remain outside Phase 11.

## Corrections / Changes

* The Cargo package is `core` and its library target is `nizaam_core`; the existing directory, library root, and private module scaffold remain in place.
* Runtime execution ordering was corrected so configured runtime stages execute before request middleware, while the middleware boundary still remains mandatory before capability dispatch.
* Cancellation and deadline state is re-checked after request middleware so middleware cannot continue a request that has become cancelled or expired into downstream capability execution.
* The communication integration path was updated to use an actual test `SecurityMiddleware` stack rather than a pass-through middleware that would violate the fail-closed trusted-context contract.
* The security integration test imports and uses the public `Authorizer` contract correctly.
* Authentication credential material is redacted from `AuthenticationRequest` debug formatting.
* `EngineServer` records `RequestPipelineError` through the existing Core Logging System before returning the failure response where the communication contract remains usable.
* The server imports `MiddlewareChainError` from its actual defining module rather than relying on a private re-export.

## Open Questions

None currently. Concrete trait signatures, provider choices, serialization, async runtime, transport implementation, and eventual crate splitting are deliberately deferred by the plan rather than unresolved architecture. Phase 7's in-memory transport is a verification implementation of the abstract boundary and does not resolve or authorize a concrete production network transport, serialization provider, or async runtime.

## Current State

Phase 11, **Observability, Health, and Configuration**, is implemented and merged. The Core foundation now extends through Phase 11, providing generic observability mechanisms, operational health reporting, and a controlled configuration pipeline while preserving the previously established Runtime, Security, Artifact, Provenance, Error, Logging, and Context boundaries.

The implementation includes configuration loading/parsing/validation/resolution, immutable snapshots and atomic updates; health liveness/readiness/dependency/capability aggregation; and structured metrics, tracing, diagnostics, and correlation integration. These systems remain independent rather than being merged into lifecycle, security, logging, or domain behavior.

The latest full library test run contains **800 tests**, with the reported failure resolved by correcting stopped-lifecycle health aggregation; the earlier run showed 799 passing and one failing specifically because `Stopped` was incorrectly aggregated as `Healthy`.

## Next Step

Proceed to **Phase 12: Streaming, Concurrency, and Background Tasks**, building on the Phase 11 health and configuration foundations while preserving the existing separation between transport fragmentation and application-level streaming. Phase 12 owns logical streaming, stream lifecycle, ordering, buffering, backpressure, stream cancellation, resource behavior, bounded concurrency, and advanced background-task scheduling; it must not replace the mechanisms already established in Phases 5, 8, 9, 10, or 11.  
