# AGENTS.md

Guidance for AI coding agents working in this repository. This is a Rust **2024** (MSRV **1.85.0**) workspace focused on high-performance AST slicing, compiler-aware context delivery, deterministic simulation, and adversarial verification oracles for large-scale codebases (>1.2M LOC). The rules below distill *Rust for Rustaceans* (Gjengset), *Programming Rust* (Blandy, Orendorff, Tindall), *The Rustonomicon*, *Zero To Production In Rust* (Berman), Tokio runtime and async design patterns, and the ground-truth verification methodologies from FoundationDB, TigerBeetle, and `pgrust`. Follow them unless a task explicitly says otherwise.

## Project

- **Rust 2024 / MSRV 1.85.0**: `Cargo.toml` declares edition `2024` and `rust-version = "1.85.0"`. `rust-toolchain.toml` pins the active toolchain (`clippy`, `rustfmt`, `miri`, `rust-analyzer`, `rust-src`). Local toolchain overrides are managed exclusively through `rust-toolchain.toml`.
- **Layout (Cargo Workspace)**:
  - `crates/cargo-canon` (or root binary crate): executable CLI entrypoint and cargo subcommand router (`cargo canon <COMMAND>` / `cargo groundtruth <COMMAND>`). Dispatches to subcommands: `slice`, `simulate`, `diff-fuzz`, `verify`, `invariants`, `state-machine`, `daemon`.
  - `crates/canon-core`: workspace foundation:
    - CLI argument parsing via `clap` (derive API with structured subcommands).
    - Configuration discovery and deserialization (`.groundtruth.toml` / `.canon.toml`) including `TypeScriptConfig`, `GoConfig`, and `CrossBoundaryConfig`.
    - Structured logging initialization (`tracing`, `tracing-subscriber`).
    - Standardized terminal diagnostics (`colored`, `anstream`).
    - Core error primitives (`thiserror`).
  - `crates/canon-slice`: AST parsing and compiler-aware context slicing engine:
    - Fast Rust AST parser using `syn` and `quote` (with fallback to `tree-sitter-rust` for resilient parsing during syntax errors).
    - Fast TypeScript / TSX AST parser using `oxc_parser` (`oxc_allocator`, `oxc_ast`, `oxc_span`).
    - Fast Go AST parser and interface resolver using `tree-sitter-go`.
    - Polyglot type skeleton stripper: empties Rust function/method bodies (`syn::visit_mut::VisitMut`), strips TypeScript function bodies, React hook internals (`useEffect`, `useState`), and JSX elements (`extract_ts_skeleton`), and strips Go function and method bodies (`extract_go_skeleton`) to `/* implementation omitted */`, preserving visibility, exported types, interfaces, contracts, and signatures.
    - Implicit interface & method-set resolver: computes value receiver $\mathcal{M}(T)$ and pointer receiver $\mathcal{M}(*T)$ method sets to resolve structural interface satisfaction ($T \text{ satisfies } I \iff \mathcal{M}(I) \subseteq \mathcal{M}(T)$, $*T \text{ satisfies } I \iff \mathcal{M}(I) \subseteq \mathcal{M}(*T)$).
    - Semantic call-graph, contract, and cross-boundary indexer: ingests local SCIP (`scip-rust`) SQLite databases, interfaces via JSON-RPC with `rust-analyzer`, extracts Go HTTP routes (Gin, Chi, Echo, `net/http`), and correlates backend Rust/Go routes/models with frontend TypeScript fetch calls and consumer interfaces (`CrossBoundaryImpact`).
    - Markdown and JSON formatters for agent prompt injection.
  - `crates/canon-harness`: verification oracles and ground-truth harnesses:
    - Deterministic Simulation Testing (DST) runner integrating with `madsim` for seeded async execution, simulated time, and fault injection.
    - Tier 1 Go concurrency & leak runner (`simulate --lang go`): executes `go test -race` (TSan) paired with `uber-go/goleak` across randomized `GOMAXPROCS` sweeps (`[1, 2, 4, 8]`) with automated AI repair prompt generation.
    - Differential fuzzing runner wrapping `bolero::check!()` comparing legacy against candidate implementations, plus cross-boundary Serde $\leftrightarrow$ TypeScript $\leftrightarrow$ Go serialization contract fuzzing and standalone reproducer generation (`#[test]`, `.ts`, `_test.go`).
    - Bounded model checking runner invoking `cargo kani` with JSON output parsing and automated counterexample extraction.
    - Metamorphic property testing harnesses powered by `proptest` (Rust), `fast-check` (TypeScript), and `pgregory.net/rapid` (Go).
  - `crates/canon-state`: Sans-I/O pure state machine framework:
    - Enforces pure domain logic (`step(&mut State, Event) -> (State, Vec<Action>)` in Rust, `Step(State, Event) (State, []Action)` in Go).
    - Strictly `#![no_std]` compatible; pure synchronous execution, caller-provided deterministic time, isolated from external side effects.
    - Exhaustive transition matrix fuzzer capable of >1,000,000 state transitions/sec for Rust state machines, pure TypeScript reducers (`--ts-reducer`), and pure Go state machines (`--lang go` / `--go-state-machine`).
  - `crates/canon-daemon`: agent sidecar and background indexing daemon:
    - High-throughput IPC / Unix domain socket server (`/tmp/groundtruth.sock` or `/tmp/canon.sock`) for sub-millisecond agent tool responses.
    - Incremental file watcher via `notify` monitoring `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, and `.go` to update the local symbol index in < 20ms on file mutations.
    - Non-destructive tool hook handlers (`hook post-read`).
- **NON-DESTRUCTIVE ADVISORY CONTEXT INJECTION (CRITICAL INVARIANT)**:
  - **Preserve target source files strictly byte-identical on disk at all times during tool hook execution** (e.g., `hook post-read` across `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.go`). Keep line numbers and byte offsets perfectly stable so downstream patch and diff editing tools (Claude Code, Antigravity) operate with complete accuracy.
  - **Deliver all injected context** (skeletons, caller hierarchies, trait contracts, cross-boundary bindings, invariants) **exclusively out-of-band** as advisory stdout, JSON-RPC responses, or formatted markdown blocks.
- **CLOSED-LOOP GROUND-TRUTH VERIFICATION (CRITICAL INVARIANT)**:
  - In codebases exceeding 1M LOC, maintain continuous ground-truth feedback to eliminate context dilution, attention decay, and semantic drift.
  - **Pair every implementation and refactor of critical logic** (parsers, state machines, concurrency coordinators, network protocols) **with an automated, mathematically rigorous ground-truth oracle**:
    - Bounded model proof (`#[kani::proof]`) formally proving panic-freedom, absence of overflows, and complete invariant adherence across all symbolic inputs.
    - Differential property harness (`bolero`) verifying exact byte-for-byte behavioral and checksum equivalence against legacy code across $10^5+$ randomized inputs, and cross-boundary Serde $\leftrightarrow$ TypeScript $\leftrightarrow$ Go contract equivalence.
    - Deterministic simulation test (`madsim` for Rust, TSan + `uber-go/goleak` for Go) passing $\ge 5,000$ randomized seeds under simulated network and scheduling faults.
    - Metamorphic invariant suite (`proptest` / `fast-check` / `pgregory.net/rapid`) proving universal algebraic properties (round-trip, idempotence, permutation invariance, state bounds).
- **`make` & `cargo` Workflow**:
  - Execute builds, tests, and quality checks exclusively through defined `make <target>` recipes and standard `cargo` commands. Maintain all tool dependencies (`cargo-audit`, `cargo-deny`, `cargo-machete`, `cargo-kani`, `cargo-bolero`, `cargo-miri`) strictly pinned in workspace configuration.

## Commands

```sh
make help              # list available targets
make all               # build all workspace binaries and crates
make check             # fast workspace compile check (cargo check --workspace --all-targets)
make build             # optimized release binaries -> target/release/*
make build-debug       # unoptimized debug binaries -> target/debug/*
make test              # unit & integration tests (cargo test --workspace --all-targets)
make sim               # deterministic simulation passes (madsim runner: 5000 seeds)
make sim-replay SEED=  # deterministically replay a single simulation seed (e.g. SEED=0xDEADBEEF)
make diff-fuzz         # run bolero differential fuzzing harnesses (ITERATIONS=50000)
make verify            # run bounded model checking via cargo kani
make miri              # run cargo miri test on unsafe/atomic blocks to detect Undefined Behavior
make bench             # run criterion benchmarks -> target/criterion/
make bench-compare     # compare baseline against candidate benchmark results
make qa                # strict QA sequence: fmt-check -> clippy -> deny -> machete -> loc
make fmt               # cargo fmt --all (rewrites in-place)
make fmt-check         # cargo fmt --all -- --check (fails on unformatted files)
make clippy            # cargo clippy --workspace --all-targets --all-features -- -D warnings
make deny              # cargo deny check (licenses, bans, duplicate crates, advisories)
make machete           # cargo machete (detect unused dependencies in Cargo.toml)
make audit             # cargo audit (RustSec security vulnerability scan; requires network)
make loc               # scripts/loc.sh: fail on any Rust file over 1500 lines of code
make clean             # cargo clean

# CLI & Engine Tool Invocations (`cargo canon` / `cargo groundtruth`)
cargo canon slice <FILE>                                           # extract type skeleton (Rust / TS / TSX / Go)
cargo canon slice <FILE> --cross-boundary                          # slice with Rust <-> TS <-> Go route & interface correlation
cargo canon simulate --seeds 5000                                  # deterministic simulation passes (madsim)
cargo canon simulate --lang go --seeds 100                         # Tier 1 Go concurrency & leak simulation (TSan + goleak)
cargo canon diff-fuzz --target-trait <TRAIT>                       # differential fuzzing between implementations
cargo canon diff-fuzz --cross-boundary --rust-model M --ts-type T  # cross-boundary Serde vs TypeScript contract fuzzing
cargo canon diff-fuzz --cross-boundary --go-model M                # cross-boundary Serde vs Go struct contract fuzzing
cargo canon invariants --scaffold <SYMBOL> --lang rust             # scaffold proptest metamorphic suite (Rust)
cargo canon invariants --scaffold <SYMBOL> --lang typescript       # scaffold fast-check metamorphic suite (TypeScript)
cargo canon invariants --scaffold <SYMBOL> --lang go               # scaffold rapid metamorphic suite (Go)
cargo canon state-machine --fuzz-transitions                       # fuzz pure Sans-I/O Rust state machine
cargo canon state-machine --ts-reducer <PATH> --fuzz-transitions   # fuzz pure TypeScript reducer transitions
cargo canon state-machine --lang go --transitions 500000           # fuzz pure Sans-I/O Go state machine transitions
cargo canon daemon --start                                         # start background indexing daemon
cargo canon daemon --hook-post-read --file <PATH>                  # advisory post-read tool hook (.rs / .ts / .tsx / .go)
```

- **Run `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test` immediately after editing any Rust source file (`.rs`).** Restrict execution of the QA suite strictly to Rust source file modifications, reserving documentation, Markdown files (`.md`), and non-Rust assets for direct updates. In the event of a compiler or linter diagnostic, report the complete output immediately, and proceed only once both clippy and the test suite pass with zero errors and zero warnings.
- `make qa` execution sequence:
  1. `fmt-check`: verify syntax layout so subsequent compiler line reports remain perfectly accurate.
  2. `clippy`: enforce strict static analysis with `-D warnings`, surfacing optimal idioms, clean borrows, and minimal allocations.
  3. `deny`: verify crate security advisories, confirm license compliance, and enforce unified dependency trees.
  4. `machete`: ensure `Cargo.toml` contains exclusively active dependencies to keep build times minimal.
  5. `loc`: enforce modularity by requiring every Rust source file to remain within 1,500 lines of code (`scripts/loc.sh`).
- **Dependency updates**: Run `cargo check` immediately after editing `Cargo.toml` to synchronize `Cargo.lock`. Commit `Cargo.toml` and `Cargo.lock` paired together in the same atomic commit.
- **Microbenchmarks**: Place benchmarks in `benches/` using `criterion`. Fully isolate benchmark setup using `b.iter_batched` and evaluate outputs through `criterion::black_box` to guarantee accurate compiler measurement. Capture baseline numbers with `make bench` and validate improvements via `make bench-compare`.

## Architecture Governance & Crate Boundaries

The codebase enforces strict layer boundaries and structural rules:

- **Dependency Boundaries**:
  - `crates/canon-state`: Confine strictly to pure Sans-I/O domain logic. Maintain `#![no_std]` compatibility, operating exclusively through pure synchronous state transformations with caller-injected deterministic time.
  - `crates/canon-slice`: Restrict dependencies strictly to AST parsing (`syn`, `quote`, `tree-sitter-rust`, `oxc_parser`, `oxc_allocator`, `oxc_ast`, `oxc_span`) and symbol query layers.
  - `crates/canon-core`: Maintain total independence as the shared foundation, containing exclusively common types, configuration models, and error primitives.
  - `crates/canon-harness`: Encapsulate verification oracles (`madsim`, `bolero`, `kani`, `proptest`) exclusively within test harnesses and standalone verification targets.
  - `crates/cargo-canon` (CLI router) and `crates/canon-daemon`: Assemble and coordinate the lower-tier library crates.
- **Function and File Caps**:
  - Domain logic, parsers, and AST slicing algorithms: Keep functions within 80 lines, parameters to 4 or fewer (bundle larger parameter sets into typed request structs or builders), and return values to structured structs or enums.
  - CLI and daemon command routers: Keep functions within 100 lines and parameters to 4 or fewer.
  - All source files: Keep total length within 1,500 lines of code (`scripts/loc.sh`, excluding comments and blank lines).

## Using Imported Crates: `cargo doc` First

Always inspect APIs directly through `cargo doc` or local crate documentation to confirm method signatures and exact trait implementations. Treat the local `Cargo.lock` as the authoritative source of truth:

```sh
cargo doc --package syn --no-deps --open          # inspect syn AST types and visitor traits
cargo doc --package oxc_parser --no-deps --open   # inspect oxc TypeScript/JS parser AST and options
cargo doc --package tokio --no-deps --open        # view tokio runtime and synchronization primitives
cargo doc --package madsim --no-deps --open       # view deterministic simulation runtime APIs
cargo doc --package bolero --no-deps --open       # view differential fuzzing harness macros
cargo doc --package kani --no-deps --open         # view bounded model checking proof attributes
cargo doc --package tracing --no-deps --open      # view structured diagnostic instrument APIs
```

- **Inspect imported crate documentation prior to first use in any file**: Confirm required trait bounds (`Send`, `Sync`, `Clone`, `Deref`), lifetime constraints, and error variants before calling unfamiliar APIs.
- **Prioritize standard library primitives and existing workspace crates**: Implement functionality with existing dependencies and standard library features whenever possible. Keep the dependency graph lean and minimal.
- **Strictly adhere to documented concurrency invariants**: When crate documentation outlines thread-safety or async constraints (such as cancellation sensitivity or lock duration), follow those constraints rigorously and annotate the call site with explicit architectural rationale.

---

## 1. Concurrency & Async Architecture (Gjengset, Blandy et al., Tokio)

**Concurrency is structural; parallelism is hardware.** In Rust, the type system enforces data race freedom through `Send` and `Sync`, while deadlocks, starvation, cancellation races, and non-deterministic event orderings require rigorous architectural discipline.

- **Communicate via Explicit Ownership Transfer (Channels & Ownership)**:
  - Default to transferring data ownership across threads and tasks via channels.
  - Reserve mutexes strictly for protecting small, local internal state (an in-memory cache, a token bucket, a state machine).
  - Decision rule: Transferring ownership or pipeline coordination $\to$ channels; protecting shared in-memory structs $\to$ `parking_lot::Mutex` or `tokio::sync::Mutex`.
- **Channel Discipline & Bounded Queues**:
  - **Enforce bounded channels across all production pipelines**: Use `tokio::sync::mpsc::channel(capacity)` with deliberate, measured capacity to guarantee backpressure propagation and protect process memory.
  - **Single-producer/single-consumer single result**: Use `tokio::sync::oneshot` for request-response RPC pairs between actors and callers.
  - **State distribution / configuration updates**: Use `tokio::sync::watch` for broadcasting the latest value to multiple consumers where intermediate dropped states are acceptable.
  - **Fan-out event pub/sub**: Use `tokio::sync::broadcast` for broadcast delivery where every receiver must observe messages within a ring buffer.
  - **Channel ownership**: Assign channel lifecycle ownership strictly to the sender task. Allow sender handles to drop to close the channel cleanly; handle closure on the receiving end by matching `None` on `.recv().await`.
- **Async Task Lifecycles & Graceful Shutdown**:
  - **Define explicit exit conditions and designated owners for every spawned task.** Ensure every `tokio::spawn(async move { ... })` call has an explicit owner and an identifiable completion or cancellation path.
  - Retain `JoinHandle<T>` or register spawned tasks in a `tokio::task::JoinSet`. Await task completion and handle panics and errors explicitly.
  - **Hierarchical shutdown trees**: Use `tokio_util::sync::CancellationToken` for hierarchical cancellation. Pass child tokens down from parent services; on `SIGINT` or `SIGTERM`, cancel the root token to cascade shutdown deterministically down the entire task tree.
- **Cancellation Safety in `tokio::select!`**:
  - In Rust async, dropping a `Future` immediately cancels it. In a `tokio::select!` block, the first branch that completes causes all other branches to drop.
  - **Ensure every future evaluated inside a `tokio::select!` branch is provably cancellation-safe.** Restrict `select!` branches to operations whose drop behavior leaves system state completely consistent.
  - Cancellation-safe operations: `mpsc::Receiver::recv`, `watch::Receiver::changed`, `oneshot::Receiver`.
  - Isolate multi-step stateful operations (`AsyncReadExt::read_exact`, unbuffered framing decoders, multi-stage database transactions) inside dedicated actor tasks, or execute them to completion prior to entering a `select!` block.
  - Apply `biased;` in `tokio::select!` when explicit priority ordering is required; otherwise rely on standard pseudo-random fairness.
- **Mutex Discipline Across `.await` Boundaries**:
  - **Release all synchronous mutex guards (`std::sync::Mutex`, `parking_lot::Mutex`) before reaching an `.await` boundary.** Keep synchronous critical sections small and tightly scoped so guards drop immediately before async suspension points:

    ```rust
    let payload = {
        let mut guard = state.lock();
        guard.prepare_payload()
    }; // guard dropped here
    client.send(payload).await?;
    ```

  - Use `tokio::sync::Mutex` exclusively when mutable state must remain locked across an `.await` call.
- **Hot Reloading & Pointer Swaps via `ArcSwap`**:
  - Manage large, read-heavy data structures (e.g., symbol graphs, parsed ASTs, routing tables) via `arc_swap::ArcSwap<T>` or `arc_swap::ArcSwapOption<T>`.
  - Enable readers to load pointers concurrently in sub-nanoseconds (`state.load()`), while writers update the shared reference atomically (`state.store(Arc::new(new_index))`) with zero downtime.
- **Atomics & Memory Ordering**:
  - Use `std::sync::atomic` (`AtomicBool`, `AtomicU64`, `AtomicPtr`) for lock-free flags and counters.
  - Select the loosest sufficient memory ordering: `Ordering::Relaxed` for monotonic counters, `Ordering::Acquire`/`Ordering::Release` for synchronizing handshakes, and `Ordering::SeqCst` strictly when a globally consistent total order across multiple variables is mandatory.
  - Guard multi-field invariants using a mutex or atomic pointer swap; atomics protect single words.

---

## 2. API Design, Error Handling & Type Safety (Gjengset, Berman)

- **Explicit `Result<T, E>` and `Option<T>` Returns**:
  - Surface all expected runtime failures (I/O failures, syntax errors, timeouts, invalid user input) as strongly typed `Result<T, E>` values. Reserve panics strictly for broken internal programmer invariants.
  - **Propagate errors consistently using the `?` operator.** Restrict `.expect()` strictly to unit tests or to lines where invariants are formally proven in an adjacent comment: `// SAFETY: checked len > 0 above`.
- **Domain-Specific Errors with `thiserror`**:
  - In library crates (`canon-core`, `canon-slice`, `canon-harness`, `canon-state`, `canon-daemon`), define strongly typed error enums derived via `thiserror::Error`:

    ```rust
    #[derive(thiserror::Error, Debug)]
    pub enum SliceError {
        #[error("failed to parse AST for file {path}: {source}")]
        ParseFailure {
            path: PathBuf,
            #[source]
            source: syn::Error,
        },
        #[error("symbol '{0}' not found in index")]
        SymbolNotFound(String),
        #[error("I/O error reading source file: {0}")]
        Io(#[from] std::io::Error),
    }
    ```

  - Restrict `anyhow::Result` and `miette::Result` exclusively to binary and CLI entrypoint boundaries (`crates/cargo-canon/src/main.rs`) for human-facing error reporting.
- **Make Valid States Representable by Construction (Typestate & Newtypes)**:
  - Enforce domain constraints through the Rust type system.
  - Wrap raw primitives in validated newtypes (`struct VerifiedSeed(u64);`, `struct SymbolPath(String);`).
  - Apply the typestate pattern for multi-step lifecycles (`StateEngine<Uninitialized>` $\to$ `StateEngine<Indexed>` $\to$ `StateEngine<Verified>`), consuming the previous state by value (`self`) to verify transitions at compile time.
- **Borrowing Hygiene & Clean Lifetimes**:
  - Accept borrowed slices (`&str`, `&[T]`, `&Path`) in function arguments to allow callers maximum flexibility.
  - Use owned or reference-counted smart pointers (`Arc<str>`, `Arc<[T]>`, `bytes::Bytes`) for long-lived shared data to keep struct signatures clean and decoupled from nested lifetime parameters.
- **Deterministic Resource Cleanup via RAII (`Drop`)**:
  - Encapsulate all external resources (temporary directories, background threads, IPC sockets, file locks) in RAII guard types that implement `Drop` to guarantee automatic, deterministic cleanup.

---

## 3. Performance, Memory & Zero-Cost Abstractions (Blandy, Gjengset)

- **Empirically Driven Optimization**:
  - Base all performance optimizations strictly on empirical measurements. Follow the disciplined engineering cycle: clean implementation $\to$ criterion benchmark $\to$ flamegraph/heap profile $\to$ targeted optimization $\to$ commit with comparative benchmark validation (`make bench-compare`).
- **Contiguous Layout & Cache Locality**:
  - Store collections in contiguous memory (`Vec<T>`, `Box<[T]>`) to maximize CPU prefetcher efficiency and cache locality.
  - For small collections or token buffers, eliminate heap allocation using `smallvec::SmallVec<[T; N]>`, `tinyvec`, or `compact_str::CompactStr` for strings $\le 24$ bytes.
- **Arena Allocation for AST Processing**:
  - Utilize arena allocators (`bumpalo::Bump`) for transient AST transformations and syntax passes. Clear arenas in bulk to deallocate all transient AST nodes in a single pointer reset ($O(1)$).
- **Zero-Copy I/O & Buffering**:
  - Wrap synchronous file I/O in `std::io::BufReader` / `BufWriter` (default buffer size 8KB) and async I/O in `tokio::io::BufReader` / `BufWriter`.
  - Use `bytes::Bytes` and `bytes::BytesMut` for network and IPC buffer management. Slice `Bytes` handles to increment atomic reference counts while retaining shared backing byte arrays.
  - Use `std::borrow::Cow<'a, str>` to preserve zero-allocation borrowed views on unmodified paths.
- **Cache-Line Alignment for Multi-Threaded Counters**:
  - Align per-worker statistics and atomic event counters to 64-byte cache lines using `crossbeam_utils::CachePadded<AtomicU64>` to guarantee independent cache lines and maximize multi-core throughput.
- **Compile-Time Monomorphization & Inlining**:
  - Mark small, performance-critical cross-crate functions with `#[inline]`. Leave large functions unadorned to optimize instruction cache efficiency and binary size.
  - Use `&dyn Trait` or `Box<dyn Trait>` when dynamic dispatch is acceptable and minimizing compile times or binary sizes takes precedence over monomorphization.

---

## 4. Core Implementation Invariants & Runtime Rules

1. **Asynchronous Execution Hygiene**:
   - Route blocking filesystem calls (`std::fs`), synchronous thread delays, heavy AST parsing, and long CPU computations through Tokio's dedicated blocking pool via `tokio::task::spawn_blocking(move || { ... }).await`.
2. **Deterministic Time Injection**:
   - Inject simulated clocks (`madsim::time::Instant` or `tokio::time::pause()`) throughout tests and state machines to ensure 100% reproducible execution.
3. **Out-of-Band Context Delivery**:
   - Deliver all slicing outputs and advisor blocks strictly out-of-band (stdout, IPC socket, formatted markdown). Keep source files on disk perfectly byte-identical during tool executions to preserve IDE and agent editing offsets.
4. **Cancellation-Safe Async Design**:
   - Ensure all operations in raceable or selectable async blocks uphold cancellation safety. Retain intermediate state in transactional buffers or isolate partial operations inside dedicated actors.
5. **Ownership & Borrowing Precision**:
   - Leverage borrowed slices (`&T`), read-only smart pointers (`Arc<T>`), and structured ownership transfers. Keep `.clone()` strictly for intentional duplications of independent resources.
6. **Explicit Task Lifecycle Governance**:
   - Retain every spawned task's `JoinHandle` or register it in a `tokio::task::JoinSet`. Await task completion and propagate or log errors explicitly.
7. **Collection Capacity Management**:
   - Proactively call `.shrink_to_fit()` or reallocate collections after processing large temporary bursts to keep working set memory bounded.
8. **Formal Safety Documentation & Verification for `unsafe`**:
   - Enforce `#![forbid(unsafe_code)]` across all library crates. Where `unsafe` is mathematically proven essential, isolate it behind a safe wrapper, annotate it with a comprehensive `// SAFETY:` invariant comment, and verify pointer validity with `cargo miri test`.

---

## 5. Production CLI & Daemon Services (Berman)

- **Structured Logging via `tracing`**:
  - Route all operational diagnostics and telemetry exclusively through the `tracing` ecosystem (`tracing::info!`, `tracing::warn!`, `tracing::error!`, `tracing::debug!`, `tracing::trace!`).
  - **Emit all log events via `tracing` macros with structured key-value attributes in library crates**:

    ```rust
    tracing::info!(
        target: "slice",
        file = %path.display(),
        symbols_found = symbols.len(),
        duration_ms = elapsed.as_millis(),
        "AST slice completed successfully"
    );
    ```

  - Spans: Instrument key async workflows with `#[tracing::instrument(skip(self))]` to automatically correlate nested operations and capture timing.
  - Initializer: In `main`, initialize `tracing-subscriber` supporting standard environment filtering (`RUST_LOG=info,canon_slice=debug`) with JSON formatting for machine consumers or ANSI colored output for human terminals.
- **CLI Design with `clap` (Derive API)**:
  - Define all CLI arguments and subcommands using `clap` derive macros with clear doc comments that serve as `--help` documentation.
  - Utilize `miette` or `anstream` to format terminal error diagnostics with clear source code snippets, colored underlines, and specific error codes.
- **Daemon Architecture & IPC**:
  - Deliver background indexing and sub-millisecond agent tool hook execution via `crates/canon-daemon`.
  - Conduct IPC over Unix domain sockets (`/tmp/canon.sock`) or named pipes using length-prefixed JSON or bincode framing.
  - Configure `notify::RecommendedWatcher` with debouncing (e.g., 50ms) to consolidate rapid multi-file IDE save events before triggering AST re-indexing.
- **Graceful Shutdown**:
  - Monitor termination signals via `tokio::signal::ctrl_c()` and Unix `tokio::signal::unix::signal(SignalKind::terminate())`.
  - On signal reception, close listening sockets, flush `tracing` subscribers, notify the `CancellationToken` tree, await in-flight tasks with a timeout, and exit cleanly with code 0.

---

## 6. Verification Oracles & Ground-Truth Testing (The 6 Pillars)

In codebases >1M LOC, coding agents cannot maintain global architecture in-context. This repository enforces closed-loop verification where generated or refactored code must satisfy automated ground-truth oracles.

```text
                           ┌────────────────────────────────────────┐
                           │        AI Coding Agent (Claude/AGY)    │
                           └───────┬────────────────────────▲───────┘
                                   │                        │
                 Tool / Hook Calls │                        │ Slices / Counterexamples
                                   ▼                        │
      ┌─────────────────────────────────────────────────────┴───────────────┐
      │                    `cargo-canon` CLI Router                         │
      └───────┬────────────┬─────────────┬─────────────┬────────────┬───────┘
              │            │             │             │            │
              ▼            ▼             ▼             ▼            ▼
        ┌──────────┐ ┌──────────┐  ┌──────────┐  ┌──────────┐ ┌──────────┐
        │  Slice   │ │   DST    │  │Diff-Fuzz │  │  Verify  │ │State-Mach│
        │ (AST/SCIP│ │ (madsim) │  │ (bolero) │  │  (kani)  │ │ (Sans-IO)│
        └────┬─────┘ └────┬─────┘  └────┬─────┘  └────┬─────┘ └────┬─────┘
             │            │             │             │            │
             ▼            ▼             ▼             ▼            ▼
      [Fast Skeleton] [Seed Loop]   [Dual Run]   [SMT Model]  [State Space]
      [Call Graph   ] [Replay Log]  [Shrinker]   [Counterex]  [Transition ]
```

### Pillar 1: Deterministic Simulation Testing (DST) with `madsim` & Go Concurrency Sweep

- **Purpose**: Concurrency, async networking, timers, and storage fault testing, guaranteeing 100% deterministic reproducibility across runs.
- **Harness**: Uses `madsim` as Tokio's deterministic drop-in replacement in test targets.
- **Tier 1 Go Concurrency & Leak Sweep (`simulate --lang go`)**: Zero-intrusion concurrency and goroutine leak oracle running `go test -race` (TSan) paired with `uber-go/goleak` inspection across randomized `GOMAXPROCS` sweeps (`[1, 2, 4, 8]`) with automated AI repair prompt generation.
- **Execution**: Run $N$ pseudo-random simulation seeds (`cargo canon simulate --seeds 5000`).
- **Reproducibility**: Reproduce any failure deterministically from its single `u64` seed (`cargo canon simulate --seed 0xDEADBEEF`).
- **AI Feedback Loop (`--ai-prompt-on-fail`)**: Automatically format the seed, event timeline, task schedules, and panic trace into an actionable repair prompt for the agent.

### Pillar 2: Differential Fuzzing with `bolero` / `cargo-fuzz` & Cross-Boundary Oracles

- **Purpose**: Validating that refactored or AI-generated modules exactly preserve legacy behavior across arbitrary inputs, and verifying cross-boundary Serde $\leftrightarrow$ TypeScript $\leftrightarrow$ Go contract equivalence.
- **Shadow Trait Pattern (Rust <-> Rust)**:
  - Define a common interface trait.
  - Maintain the battle-tested code as `LegacyImpl` and candidate code as `CandidateImpl`.
  - Wrap both in a `bolero::check!()` harness generating arbitrary operation sequences:

    ```rust
    bolero::check!()
        .with_arbitrary::<Vec<Operation>>()
        .for_each(|ops| {
            let mut legacy = LegacyImpl::new();
            let mut candidate = CandidateImpl::new();
            for op in ops {
                assert_eq!(legacy.apply(op), candidate.apply(op));
                assert_eq!(legacy.checksum(), candidate.checksum());
            }
        });
    ```

- **Cross-Boundary Contract Fuzzing (Rust <-> TypeScript <-> Go)**:
  - Fuzz Serde serialized Rust models against TypeScript/Zod interface expectations and Go models via `cargo canon diff-fuzz --cross-boundary --rust-model <RustModel> --ts-type <TsInterface> --go-model <GoModel>`.
  - Catch subtle cross-language edge cases:
    - JavaScript `Number.MAX_SAFE_INTEGER` ($2^{53}-1$) truncation on large 64-bit integers.
    - Field casing discrepancies (`snake_case` in Rust vs `camelCase` in TypeScript and Go).
    - Uninitialized nil slice vs empty slice serialization divergence (Go `null` vs TypeScript/Serde `[]`).
    - Optionality divergence (`Option::None` serializing to explicit `null` vs TypeScript expecting omitted `undefined`).
    - Enum tagging divergences (externally tagged `{"Active":{}}` vs internally tagged `{"type":"Active"}`).
- **Shrinking & Reproduction**: When a discrepancy occurs, allow `bolero` or the cross-boundary runner to minimize the input to the smallest reproducing payload. Export standalone Rust `#[test]`, companion TypeScript, and Go (`_test.go`) test cases via `--export-repro <PATH>`.

### Pillar 3: "Proof-Carrying Patches" via Model Checking (`kani`)

- **Purpose**: Formally proving total panic-freedom, mathematical safety bounds, and strict invariant adherence across all bounded inputs.
- **Policy**: Pair critical algorithms (ring buffers, state machines, parsers, cryptographic routines) with a `#[kani::proof]` verification harness.
- **Counterexample Synthesis**: When a proof fails, parse Kani's SMT output into formatted Rust struct initializers detailing the exact failing input state (`cargo canon verify`).

### Pillar 4: Metamorphic Invariant Testing (`proptest` / `fast-check` / `rapid`)

- **Purpose**: Synthesizing high-level algebraic properties that hold universally across the input space.
- **Polyglot Property Suites**:
  - Rust: Powered by `proptest` for algebraic properties across models and coordinators (`cargo canon invariants --scaffold <Symbol> --lang rust`).
  - TypeScript: Powered by `fast-check` for frontend models and reducers (`cargo canon invariants --scaffold <Symbol> --lang typescript`).
  - Go: Powered by `pgregory.net/rapid` for algebraic properties across Go models and state machines (`cargo canon invariants --scaffold <Symbol> --lang go`).
- **Standard Invariants**:
  - *Round-trip*: $f^{-1}(f(x)) \equiv x$ (serialization, AST parsing/unparsing, compression).
  - *Idempotence*: $f(f(x)) \equiv f(x)$ (formatting, normalization, reconciliation).
  - *Permutation Invariance*: $f(\text{shuffle}(x)) \equiv f(x)$ (set evaluation, query planning).
  - *Monotonicity / Bounds*: State bounds and monotonic counters strictly increase or decrease.
- **Minimized inputs**: Capture failing cases automatically shrunk by `proptest`, `fast-check`, or `rapid` to minimal reproducible test vectors.

### Pillar 5: Architectural Decoupling: "Sans-I/O" Pure State Machines

- **Purpose**: Structuring domain logic as a pure, deterministic state machine that operates exclusively on synchronous inputs and outputs:

  ```rust
  pub fn step(state: &mut State, event: Event) -> (State, Vec<Action>);
  ```

- **Benefits**:
  - Total context clarity: Agents only require the `Event`, `Action`, and `State` types, eliminating hundreds of lines of plumbing.
  - Ultra-high throughput: Pure functions execute $>1,000,000$ transitions/sec in a single thread, enabling exhaustive transition fuzzing (`cargo canon state-machine --fuzz-transitions`).
  - TypeScript Reducer Fuzzing: Apply high-throughput transition fuzzing directly to pure TypeScript state reducers (`(state, action) => state`) via `cargo canon state-machine --ts-reducer <PATH> --fuzz-transitions`.
  - Tier 2 Go State Machine Fuzzing: Pure Sans-I/O discrete-event state machine fuzzing (`Step(State, Event) (State, []Action)`) at >1,000,000 transitions/sec via `cargo canon state-machine --lang go --transitions <N>` or `--go-state-machine <PATH>`.
  - Exhaustive state matrix verification: Inspect reachable states and detect disconnected transitions via `--matrix`.

### Pillar 6: Compiler-Aware Context Slicing (`slice`)

- **Purpose**: Delivering compact, compiler-accurate context to agents, achieving $>85\%$ token reduction while retaining full type fidelity across Rust, TypeScript/TSX, and Go codebases.
- **Modes & Flags**:
  - `--mode skeleton` (Default): Uses `syn` for Rust, `oxc_parser` for TypeScript/TSX, and `tree-sitter-go` for Go to strip function bodies, React hook internals, and JSX blocks, emitting exclusively visibility, structs, interfaces, enums, type aliases, and function signatures in $< 10$ms.
  - `--mode hierarchy`: Queries local SCIP/LSP symbol graphs to extract upstream callers and downstream callees up to `--depth <N>`.
  - `--mode contracts`: Extracts all `impl Trait for Target` blocks (Rust) and resolves structural interface satisfaction for value $\mathcal{M}(T)$ and pointer $\mathcal{M}(*T)$ receiver method sets (Go).
  - `--mode bundle`: Combines skeleton, contracts, and call hierarchy into a single markdown block for LLM prompts.
  - `--cross-boundary`: Analyzes cross-boundary impact between Rust/Go backend route bindings/data models (Gin, Chi, Echo, `net/http`) and TypeScript frontend client call sites and consumer interfaces.

---

## 7. Testing Guidelines

- **Native Rust Harness & Verification Tooling**:
  - Locate unit tests next to the code in unit modules (`#[cfg(test)] mod tests { ... }`) and integration suites in `tests/`.
  - Use `proptest` for property testing, `kani` for bounded model proofs, `bolero` for fuzzing, and `madsim` for async simulation.
  - Rely exclusively on standard Rust assertions (`assert!`, `assert_eq!`, `assert_matches!`) and native verification primitives.
- **Event-Driven Synchronization**:
  - **Synchronize tests deterministically via event coordination mechanisms**: Rely on channels, `tokio::sync::Notify`, `tokio::sync::Barrier`, or task `JoinHandle` completion to coordinate async progression. For time-dependent logic, advance virtual time instantly using `tokio::time::pause()` or execute inside the `madsim` simulation environment.
- **Test Isolation & Hermetic Execution**:
  - Keep every test hermetic, reproducible, and self-contained, ensuring consistent passes regardless of execution order, environment variables, working directory, or network availability.
  - Manage filesystem operations within `tempfile::TempDir`, allowing automatic cleanup on `Drop`.
- **Falsifiable Bug Fixes**:
  - Accompany every bug fix with a reproducing test (a failing `#[test]`, a shrunk `proptest` case, or a deterministic `madsim` seed) demonstrating the bug before the fix, and passing cleanly after.

---

## Standing Rules

1. **Non-Destructive Context Delivery**: Preserve source files on disk strictly unmodified during tool hook execution (across `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.go`). Deliver all generated skeletons, contracts, call graphs, and cross-boundary impacts out-of-band to ensure agent line and byte offsets remain perfectly stable.
2. **Quality Gate Verification**: Run `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test` immediately after every Rust source edit, ensuring compiler and linter passes are fully green before proceeding.
3. **Rigorous Ground-Truth Oracles**: Validate all critical domain algorithms, parsers, and state transitions using bounded model proofs (`#[kani::proof]`), differential property harnesses (`bolero`), or deterministic simulation passes (`madsim`).
4. **Sans-I/O Domain Purity**: Confine `crates/canon-state` strictly to pure synchronous state transitions (`step(&mut State, Event) -> (State, Vec<Action>)`), accepting deterministic time from caller inputs.
5. **Concurrency & Cancellation Hygiene**: Enforce bounded channels across all production pipelines, release synchronous mutex guards before `.await` points, and verify cancellation safety for all futures evaluated inside `tokio::select!`.
6. **Explicit Typed Error Handling**: Return `Result<T, E>` with strongly typed domain errors derived via `thiserror`, reserving `anyhow` and `miette` exclusively for user-facing CLI binary boundaries.
7. **Relative Markdown Links**: Format links within markdown documentation using repository-relative paths (`[crates/canon-core/src/lib.rs](crates/canon-core/src/lib.rs)`).

---

## Commits & PRs

- Structure commits using Conventional Commits (`feat`, `fix`, `refactor`, `perf`, `docs`, `chore`) with an imperative subject line:
  - `feat(slice): implement syn visitor for stripping function bodies`
  - `fix(harness): prevent cancellation race in madsim event loop`
  - `perf(state): pad per-worker atomic counters to avoid false sharing`
- Document the exact invariant protected and name the verification oracle used (e.g., "Verified with 10,000 madsim seeds" or "Proved panic-freedom with Kani unwinding depth 4").
- Explicitly state the technical rationale, compile-time impact, and security footprint for every dependency addition or modification in `Cargo.toml`.
