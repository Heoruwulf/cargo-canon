<!-- markdownlint-disable MD013 -->

# Architectural Overview: Eliminating Context Bloat in Claude & Antigravity (AGY)

> **Ground-Truth Token Optimization & Advisory Slicing Engine for Polyglot Codebases (>1.2M LOC)**  
> **Engine:** [`cargo-canon`](../README.md)  
> **Target Toolchains:** Claude Code (`.claude/config.json`), Antigravity CLI (`agy`), and autonomous multi-agent swarms.

---

## 1. Executive Summary: The Context Bloat Problem

In large software systems exceeding 1,000,000 lines of code, AI coding assistants (Claude 3.5 Sonnet, Antigravity/Gemini) face severe operational degradation caused by **context bloat**:

1. **Context Window Exhaustion & Attention Decay ("Lost in the Middle"):**  
   Dumping raw source files—replete with deeply nested method bodies, redundant JSX trees, Tailwind class strings, SQL strings, and local hook internals—rapidly consumes the model's 200,000-token context window. Research demonstrates that as context windows fill beyond 30–40% capacity, LLM reasoning sharpness degrades exponentially, leading to missed trait bounds, forgotten invariant contracts, and hallucinated API signatures.
2. **Compounding Multi-Turn Token Costs & High Latency:**  
   Every subsequent turn in an agent conversation re-transmits the cumulative transcript. Reading 10 raw files (averaging 1,500 lines / 12,000 tokens each) burns 120,000 tokens on turn one. Across a 10-turn refactoring task, this compounds to **>1,500,000 input tokens**, causing sluggish round-trip latency (>30s/turn) and high inference costs.
3. **The "Open-Loop" Hallucination Spiral:**  
   When an agent lacks verified type skeletons, interface contracts, and cross-boundary route bindings, it guesses. When those guesses trigger compiler errors or test failures, the agent attempts multi-turn blind repairs, writing superficial unit tests and burning hundreds of thousands of tokens chasing phantom race conditions.
4. **Tool Hook File Desynchronization:**  
   Naive pre-read or post-read hooks that inject comments or docstrings directly into files on disk alter byte offsets and line numbers. When the agent later attempts a precise patch or diff edit, the edit fails due to offset drift, prompting the agent to re-read files repeatedly.

### How `cargo-canon` Solves Context Bloat

[`cargo-canon`](../README.md) acts as a high-performance **compiler-aware context delivery and verification oracle**:

- **70%–95% Token Reduction:** Slices raw files down to compilable interface skeletons, trait contracts, and route bindings in sub-milliseconds using [`syn`](../crates/canon-slice/src/skeleton.rs) (Rust), [`oxc_parser`](../crates/canon-slice/src/ts_skeleton.rs) (TypeScript/TSX), and [`tree-sitter-go`](../crates/canon-slice/src/go_skeleton.rs) (Go).
- **Strictly Non-Destructive Advisory Delivery:** Delivers context **strictly out-of-band** (via stdout or Unix Domain Socket IPC) through [`execute_post_read_hook`](../crates/canon-daemon/src/hook.rs#L12-L41). On-disk files remain 100% byte-identical, guaranteeing zero patch collisions.
- **Closed-Loop Ground-Truth Verification:** Replaces token-expensive hallucinated debugging loops with deterministic oracles: Discrete-Event Simulation ([`simulate`](../crates/canon-harness/src/simulate/engine.rs)), Differential Fuzzing ([`diff-fuzz`](../crates/canon-harness/src/diff_fuzz/mod.rs)), Formal Verification ([`verify`](../crates/canon-harness/src/verify/mod.rs)), and Sans-I/O State Machine Fuzzing ([`state-machine`](../crates/canon-state/src/lib.rs)).

---

## 2. Illustrated Before & After Examples: The Mechanics of AST Slicing

Below are three concrete examples illustrating how `cargo-canon` strips away implementation bloat while preserving 100% of the type, contract, and semantic information an agent needs.

---

### Example A: Polyglot React / TSX Dashboard (`Dashboard.tsx`)

#### 1. What Claude / AGY Reads Without `cargo-canon` (Raw Source: 420 Lines / ~3,850 Tokens)

Without slicing, the agent reads hundreds of lines of DOM hierarchy, SVG paths, tailwind classes, and local hook state:

```tsx
// frontend/src/components/Dashboard.tsx (420 lines, 14.8 KB)
import React, { useState, useEffect, useCallback, useMemo } from "react";
import {
  UserProfile,
  MetricReport,
  fetchUserMetrics,
  submitAuditLog,
} from "../api/client";
import { NotificationBadge } from "./NotificationBadge";
import { formatCurrency, calculateThroughput } from "../utils/math";

export interface DashboardProps {
  userId: string;
  refreshIntervalMs?: number;
  enableRealtimeAlerts?: boolean;
  onStatusChange?: (status: "online" | "degraded" | "offline") => void;
}

export function Dashboard({
  userId,
  refreshIntervalMs = 5000,
  enableRealtimeAlerts = true,
  onStatusChange,
}: DashboardProps): JSX.Element {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [metrics, setMetrics] = useState<MetricReport[]>([]);
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);
  const [filterQuery, setFilterQuery] = useState<string>("");

  useEffect(() => {
    let mounted = true;
    const loadInitialData = async () => {
      setIsRefreshing(true);
      try {
        const p = await fetchUserMetrics(userId);
        if (mounted) {
          setProfile(p.profile);
          setMetrics(p.metrics);
          onStatusChange?.("online");
        }
      } catch (err) {
        if (mounted) onStatusChange?.("degraded");
      } finally {
        if (mounted) setIsRefreshing(false);
      }
    };
    loadInitialData();
    const timer = setInterval(loadInitialData, refreshIntervalMs);
    return () => {
      mounted = false;
      clearInterval(timer);
    };
  }, [userId, refreshIntervalMs, onStatusChange]);

  const handleFilterChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setFilterQuery(e.target.value);
    },
    [],
  );

  // ... [280 LINES OF DOM NODES, SVG ICONS, AND TAILWIND STYLING OMITTED] ...

  return (
    <div className="flex min-h-screen flex-col bg-slate-950 p-6 text-slate-100 antialiased">
      <header className="flex items-center justify-between border-b border-slate-800 pb-4">
        <h1 className="text-2xl font-bold tracking-tight">
          System Telemetry Dashboard
        </h1>
        <NotificationBadge active={enableRealtimeAlerts} />
      </header>
      <main className="grid grid-cols-1 gap-6 md:grid-cols-3">
        {/* 150 lines of nested table cards, buttons, modals, and charts */}
      </main>
    </div>
  );
}
```

#### 2. What Claude / AGY Receives With `cargo-canon` ([`canon-slice::ts_skeleton`](../crates/canon-slice/src/ts_skeleton.rs))

`cargo-canon` uses [`oxc_parser`](../crates/canon-slice/src/ts_skeleton.rs#L19) to strip internal React hooks (`useState`, `useEffect`), local closures, and JSX trees down to the clean component signature and exported types:

````markdown
# Canon Context Slice: `frontend/src/components/Dashboard.tsx`

> **Token Reduction:** 94.8% context saved vs raw source (14.8 KB raw -> 0.77 KB delivered).

## Type Skeleton & Interface Contracts

```typescript
export interface DashboardProps {
  userId: string;
  refreshIntervalMs?: number;
  enableRealtimeAlerts?: boolean;
  onStatusChange?: (status: "online" | "degraded" | "offline") => void;
}

export function Dashboard(props: DashboardProps): JSX.Element;
```

## Cross-Boundary Impact

- **Rust Backend Model:** `crates/server/src/routes/user.rs:42 (UserProfile)`
- **TypeScript Consumer:** `frontend/src/types/user.ts:18 (UserProfile)`
- **API Call Sites:**
  - `fetchUserMetrics` in `frontend/src/api/client.ts:88`
````

> **Direct Impact:** 3,850 tokens reduced to **195 tokens** (**94.9% token reduction**). The agent immediately knows the props, callbacks, and backend model bindings without reading 400 lines of Tailwind and DOM templates.

---

### Example B: Rust Network Protocol & State Engine (`server.rs`)

#### 1. What Claude / AGY Reads Without `cargo-canon` (Raw Source: 850 Lines / ~8,200 Tokens)

The raw file contains Tokio socket listeners, framing loops, atomic tracking, error match statements, and low-level buffer operations:

```rust
// crates/canon-daemon/src/server.rs (850 lines, 31.2 KB)
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;

pub struct ServerTelemetry {
    pub hooks_served: AtomicU64,
    pub raw_bytes_served: AtomicU64,
    pub delivered_bytes_served: AtomicU64,
    pub bytes_saved: AtomicU64,
}

impl ServerTelemetry {
    pub fn record_reduction(&self, raw_bytes: u64, delivered_bytes: u64) -> f64 {
        self.hooks_served.fetch_add(1, Ordering::Relaxed);
        self.raw_bytes_served.fetch_add(raw_bytes, Ordering::Relaxed);
        self.delivered_bytes_served.fetch_add(delivered_bytes, Ordering::Relaxed);
        let saved = raw_bytes.saturating_sub(delivered_bytes);
        self.bytes_saved.fetch_add(saved, Ordering::Relaxed);
        if raw_bytes > 0 {
            (saved as f64 / raw_bytes as f64) * 100.0
        } else {
            0.0
        }
    }

    pub fn snapshot(&self) -> (u64, u64, u64, u64, f64, u64) {
        let hooks = self.hooks_served.load(Ordering::Relaxed);
        let raw = self.raw_bytes_served.load(Ordering::Relaxed);
        let delivered = self.delivered_bytes_served.load(Ordering::Relaxed);
        let saved = self.bytes_saved.load(Ordering::Relaxed);
        let pct = if raw > 0 { (saved as f64 / raw as f64) * 100.0 } else { 0.0 };
        (hooks, raw, delivered, saved, pct, saved / 4)
    }
}

pub async fn run_server_with_options(
    options: ServerOptions,
    token: CancellationToken,
) -> Result<(), DaemonError> {
    // ... [750 LINES OF TOKIO FRAMING, ERROR MATCHING, TIMEOUTS, SOCKET DROPS] ...
}
```

#### 2. What Claude / AGY Receives With `cargo-canon` ([`canon-slice::skeleton`](../crates/canon-slice/src/skeleton.rs))

`cargo-canon` parses the AST via `syn` and runs [`SkeletonStripper`](../crates/canon-slice/src/skeleton.rs#L10-L29), clearing all function and method statement blocks while preserving structs, field visibility, signatures, doc comments, and trait contracts:

````markdown
# Canon Context Slice: `crates/canon-daemon/src/server.rs`

> **Token Reduction:** 76.8% context saved vs raw source (31.2 KB raw -> 7.2 KB delivered).

## Type Skeleton & Interface Contracts

```rust
pub struct ServerTelemetry {
    pub hooks_served: AtomicU64,
    pub raw_bytes_served: AtomicU64,
    pub delivered_bytes_served: AtomicU64,
    pub bytes_saved: AtomicU64,
}

impl ServerTelemetry {
    pub fn record_reduction(&self, raw_bytes: u64, delivered_bytes: u64) -> f64;
    pub fn snapshot(&self) -> (u64, u64, u64, u64, f64, u64);
}

pub struct ServerOptions {
    pub socket_path: PathBuf,
    pub workspace_root: PathBuf,
    pub watcher_active: bool,
}

pub async fn run_server_with_options(
    options: ServerOptions,
    token: CancellationToken,
) -> Result<(), DaemonError>;
```

## Trait Contracts

- `impl Drop for SocketGuard` (in `crates/canon-daemon/src/server.rs`)
  - `fn drop(&mut self)`

## Upstream Callers

- `crates/cargo-canon/src/commands.rs:handle_daemon_start` calls `run_server_with_options`
````

> **Direct Impact:** 8,200 tokens reduced to **810 tokens** (**90.1% token reduction**). The agent sees exact method inputs, return types, and callers without polluting its context window with complex async runtime boilerplate.

---

### Example C: Go Microservice & Implicit Interface Satisfaction (`user_service.go`)

#### 1. What Claude / AGY Reads Without `cargo-canon` (Raw Source: 620 Lines / ~5,900 Tokens)

In Go, interface satisfaction is structural and implicit (no `implements` keyword). Without `cargo-canon`, an agent must read dozens of methods across multiple files just to determine if `*UserService` implements `UserRepository`:

```go
// backend/pkg/service/user_service.go (620 lines, 21.4 KB)
package service

import (
    "context"
    "database/sql"
    "fmt"
    "github.com/gin-gonic/gin"
)

type UserRepository interface {
    GetUser(ctx context.Context, id string) (*User, error)
    UpdateUser(ctx context.Context, u *User) error
}

type User struct {
    ID    string `json:"id"`
    Email string `json:"email"`
    Role  string `json:"role"`
}

type UserService struct {
    db *sql.DB
}

func (s *UserService) GetUser(ctx context.Context, id string) (*User, error) {
    // 80 lines of db transaction, row scanning, err handling, redis cache checks
}

func (s *UserService) UpdateUser(ctx context.Context, u *User) error {
    // 120 lines of input validation, bcrypt password hashing, sql exec, metric increments
}

func RegisterRoutes(r *gin.Engine, s *UserService) {
    r.GET("/api/v1/users/:id", s.HandleGetUser)
    r.POST("/api/v1/users", s.HandleUpdateUser)
}
// ... 350 lines of HTTP handler wrappers, serialization, and middleware ...
```

#### 2. What Claude / AGY Receives With `cargo-canon` ([`canon-slice::go_skeleton`](../crates/canon-slice/src/go_skeleton.rs))

`cargo-canon` extracts the AST using `tree-sitter-go`, strips function bodies, computes pointer receiver method sets $\mathcal{M}(*T)$, resolves structural satisfaction, and indexes HTTP route bindings:

````markdown
# Canon Context Slice: `backend/pkg/service/user_service.go`

> **Token Reduction:** 86.4% context saved vs raw source (21.4 KB raw -> 2.9 KB delivered).

## Type Skeleton & Interface Contracts

```go
package service

type UserRepository interface {
    GetUser(ctx context.Context, id string) (*User, error)
    UpdateUser(ctx context.Context, u *User) error
}

type User struct {
    ID    string `json:"id"`
    Email string `json:"email"`
    Role  string `json:"role"`
}

type UserService struct {}

func (s *UserService) GetUser(ctx context.Context, id string) (*User, error);
func (s *UserService) UpdateUser(ctx context.Context, u *User) error;
func RegisterRoutes(r *gin.Engine, s *UserService);
```

## Structural Interface Satisfaction

- `*UserService` **satisfies** `UserRepository`
  - `GetUser(ctx context.Context, id string) (*User, error)`
  - `UpdateUser(ctx context.Context, u *User) error`

## Discovered API Route Bindings

- `GET /api/v1/users/:id` -> `UserService.HandleGetUser`
- `POST /api/v1/users` -> `UserService.HandleUpdateUser`
````

> **Direct Impact:** 5,900 tokens reduced to **520 tokens** (**91.2% token reduction**). The implicit interface satisfaction contract is computed statically, saving the agent from exploring multiple files.

---

## 3. Multi-Turn Session Budget: The Compounding Impact

In a typical feature refactor (e.g. adding a tenant isolation field across backend models, service layers, API routes, and frontend components), an agent reads and inspects 8 to 12 files.

### Turn-by-Turn Context Accumulation: Raw vs. `cargo-canon`

| Turn      | Agent Action / Inspected File                        | Raw File Tokens | Raw Transcript Accumulation | `cargo-canon` Sliced Tokens | `cargo-canon` Accumulation |   Token Savings   |
| :-------- | :--------------------------------------------------- | :-------------: | :-------------------------: | :-------------------------: | :------------------------: | :---------------: |
| **T1**    | System Prompt & Task Spec                            |      3,500      |            3,500            |            3,500            |           3,500            |       0.0%        |
| **T2**    | Inspect Backend Model (`crates/core/src/model.rs`)   |      6,800      |           10,300            |             580             |           4,080            |     **60.4%**     |
| **T3**    | Inspect Database Repo (`crates/db/src/repo.rs`)      |     12,400      |           22,700            |             920             |           5,000            |     **78.0%**     |
| **T4**    | Inspect Async Service Coordinator (`src/service.rs`) |     15,200      |           37,900            |            1,240            |           6,240            |     **83.5%**     |
| **T5**    | Inspect Route Handlers (`src/routes.rs`)             |      9,800      |           47,700            |             810             |           7,050            |     **85.2%**     |
| **T6**    | Inspect Frontend API Client (`frontend/src/api.ts`)  |      8,400      |           56,100            |             640             |           7,690            |     **86.3%**     |
| **T7**    | Inspect Component State (`frontend/src/Store.ts`)    |     11,200      |           67,300            |             880             |           8,570            |     **87.3%**     |
| **T8**    | Inspect UI View (`frontend/src/View.tsx`)            |     14,500      |           81,800            |             750             |           9,320            |     **88.6%**     |
| **T9**    | Edit Backend & Verify Routes                         |     18,000      |           99,800            |            1,800            |           11,120           |     **88.9%**     |
| **T10**   | Edit Frontend & Final Pass                           |     22,000      |           121,800           |            2,100            |           13,220           |     **89.1%**     |
| **Total** | **Cumulative Session Token Consumption**             |        —        |     **548,900 tokens**      |              —              |     **75,790 tokens**      | **86.2% Savings** |

### Context Window Headroom Comparison

```text
Raw File Approach (121,800 active context tokens on Turn 10):
┌─────────────────────────────────────────────────────────────────────────────┐
│ System (3%) │ Raw Source Files & DOM Trees (82%)            │ LLM Plan (15%)│
└─────────────────────────────────────────────────────────────────────────────┘
  ▲ 121.8k / 200k tokens consumed. High risk of attention decay and semantic drift.

`cargo-canon` Approach (13,220 active context tokens on Turn 10):
┌─────────────────────────────────────────────────────────────────────────────┐
│ System (26%)│ Skeletons (9%) │ LLM Reasoning & Verification Headroom (65%)  │
└─────────────────────────────────────────────────────────────────────────────┘
  ▲ 13.2k / 200k tokens consumed. >90% headroom remains available for complex logic.
```

---

## 4. Visual Architecture & Workflow Sequences

### Sequence 1: Non-Destructive Advisory Context Injection via Daemon

The diagram below shows how Claude Code and Antigravity (AGY) query the [`canon-daemon`](../crates/canon-daemon) sidecar over a Unix Domain Socket (`/tmp/canon-<hash>.sock`). Target files on disk remain strictly unmodified:

```mermaid
sequenceDiagram
    autonumber
    actor Agent as Coding Agent (Claude / AGY)
    participant Hook as Advisory Tool Hook (`post-read`)
    participant Daemon as Canon IPC Daemon (`/tmp/canon-<hash>.sock`)
    participant Slicer as AST Slicing Engine (`syn` / `oxc` / `tree-sitter`)
    participant Disk as File System on Disk

    Agent->>Hook: Read file `src/protocol.rs`
    Hook->>Disk: Read original file bytes (B_raw)
    Hook->>Daemon: Send `DaemonRequest::PostReadHook { file: "src/protocol.rs" }`
    Daemon->>Slicer: Parse AST & strip implementation bodies
    Slicer-->>Daemon: Return `SliceBundle` (Skeleton + Contracts + Callers)
    Daemon->>Daemon: Record telemetry: AtomicU64 (raw_bytes, delivered_bytes)
    Daemon-->>Hook: Return `DaemonResponse::AdvisoryContext(markdown_block)`
    Hook->>Disk: Assert disk bytes == B_raw (Defensive Byte-Identity Check)
    Hook-->>Agent: Out-of-band advisory context markdown injected
    Note over Agent: Agent edits target file using exact on-disk byte offsets (Zero patch drift!)
```

### Sequence 2: Closing the Verification Loop (Halting Hallucination Spirals)

Context bloat is not only caused by reading files; it is heavily driven by **hallucinated debugging cycles**. When an LLM encounters a failure, it typically writes superficial unit tests, guesses at async timing bugs, and burns tens of thousands of tokens over 5–10 fruitless turns:

```mermaid
flowchart TD
    subgraph Without_Canon["Without cargo-canon: Open-Loop Token Bleed"]
        A1["Agent writes candidate code"] --> A2["Agent generates shallow unit tests"]
        A2 --> A3["Tests pass on happy-path (False Confidence)"]
        A3 --> A4["Production/CI fails on race condition or Serde mismatch"]
        A4 --> A5["Agent reads 5 new files to debug (50k tokens)"]
        A5 --> A6["Agent hallucinates fix & retries (Multi-Turn Spiral)"]
        A6 --> A1
    end

    subgraph With_Canon["With cargo-canon: Closed-Loop Ground Truth"]
        B1["Agent writes candidate code"] --> B2["Run Canon Verification Oracle"]
        B2 --> B3{"Select Oracle"}
        B3 -->|"DST (simulate)"| B4["madsim 5,000 seeds: Detects async deadlock"]
        B3 -->|"Diff-Fuzz"| B5["bolero: Emits minimal failing #[test] reproducer"]
        B3 -->|"Model Check (verify)"| B6["Kani: Synthesizes concrete SMT struct initializers"]
        B4 --> B7["Single-turn deterministic fix with exact seed/repro"]
        B5 --> B7
        B6 --> B7
        B7 --> B8["Mathematical proof of correctness (Task Complete)"]
    end
```

---

## 5. Ground-Truth Telemetry & Real-Time Monitoring

`cargo-canon` tracks bloat reduction using zero-cost, lock-free atomic counters ([`ServerTelemetry`](../crates/canon-daemon/src/server.rs#L20-L59)) across the daemon lifetime:

### Mathematical Model

For each served request $i$:
$$B_{\text{saved}}^{(i)} = \max\left(0, B_{\text{raw}}^{(i)} - B_{\text{delivered}}^{(i)}\right)$$
$$\text{Reduction Ratio } R = \frac{\sum B_{\text{saved}}}{\sum B_{\text{raw}}} \times 100\%$$
$$\text{Estimated Tokens Saved } T_{\text{saved}} = \left\lfloor \frac{\sum B_{\text{saved}}}{4} \right\rfloor$$

_(Calculated conservatively at $\approx 4\text{ bytes/token}$, clamped to prevent negative bloat anomalies on micro-files)._

### Real-Time Telemetry CLI Output

Operators or agents can query daemon metrics at any time:

```bash
cargo canon daemon --status
```

```text
Canon Daemon Status: Active
PID:            48210
Uptime:         24m 12s
Socket Path:    /tmp/canon-9f8a3c2b1e4d.sock
Workspace Root: /home/user/project
Indexed Files:  186 files
Total Symbols:  4,230 declarations
Call Edges:     1,412 edges
Contracts:      528 trait implementations
Routes:         54 API endpoints
File Watcher:   Active (.rs, .ts, .tsx, .js, .jsx, .go)
Hooks Served:   42 requests (38 submitted, 4 omitted)
Drop Telemetry: 2 call graphs pruned, 2 AST skeletons dropped
Read Operations:      42 requests (6 macro, 36 micro, 85.7% micro)
Supplemental Context: 198.2 KB delivered (~49,550 tokens, est. 4B/token)
Raw Source Inspected: 1.84 MB (advisory ratio: 10.8%)
```

For machine-readable JSON integration in CI or agent sidecars:

```bash
cargo canon daemon --status --json
```

```json
{
  "status": "active",
  "pid": 48210,
  "uptime_secs": 1452,
  "socket_path": "/tmp/canon-9f8a3c2b1e4d.sock",
  "workspace_root": "/home/user/project",
  "indexed_files": 186,
  "total_symbols": 4230,
  "total_routes": 54,
  "hooks_served": 42,
  "macro_reads": 6,
  "micro_reads": 36,
  "supplemental_context_bytes": 208115,
  "supplemental_context_tokens": 52028,
  "raw_bytes_served": 1929380,
  "delivered_bytes_served": 208115,
  "bytes_saved": 1721265,
  "token_reduction_pct": 89.21,
  "est_tokens_saved": 430316
}
```

### Empirical Benchmark: Live AGY Editing Session (`polyglot-service`)

Telemetry captured during an active 1.5-hour engineering session on `/home/user/workspace/polyglot-service` using the Antigravity CLI (`agy`) with `cargo canon daemon` serving advisory tool hooks:

```text
Canon Daemon Status: Active
PID:            49152
Uptime:         1h 34m 59s
Socket Path:    /tmp/canon-e2b1c4d8f0a3.sock
Workspace Root: /home/user/workspace/polyglot-service
Indexed Files:  119 files
Total Symbols:  872 declarations
Call Edges:     0 edges
Contracts:      4 trait implementations
Routes:         39 API endpoints
File Watcher:   Active (.rs, .ts, .tsx, .js, .jsx, .go)
Hooks Served:   58 requests (52 submitted, 6 omitted)
Drop Telemetry: 4 call graphs pruned, 2 AST skeletons dropped
Read Operations:      58 requests (8 macro, 50 micro, 86.2% micro)
Supplemental Context: 94.7 KB delivered (~23,675 tokens, est. 4B/token)
Raw Source Inspected: 733.1 KB (advisory ratio: 12.9%)
```

#### Empirical Analysis

1. **Context Window Survival:**  
   Without `cargo-canon`, 58 file read operations would have ingested **733.1 KB (~183,275 raw tokens)** into AGY's context window. In a 200,000-token model, file reads alone would have filled **91.6% of the entire context window**, forcing aggressive session compaction, memory dilution, and loss of earlier system instructions.
2. **High-Signal Delivered Context:**  
   AGY ingested only **94.7 KB (~23,675 tokens)** across the entire 1.5-hour turn sequence, preserving **>176,000 tokens of clean headroom** for complex reasoning, architectural planning, and zero-drift patch synthesis.
3. **Per-Hook ROI:**
   - **Average Raw File Read:** 12.64 KB (~3,160 tokens)
   - **Average Sliced Context:** 1.63 KB (~408 tokens)
   - **Savings per Tool Call:** **~2,752 tokens saved on every file read**.

---

## 6. Configuration & Agent Integration Guide

### 1. Claude Code (`.claude/config.json`)

Configure an automatic post-read hook so Claude receives advisory context on every file read:

```json
{
  "$schema": "https://json.schemastore.org/partial-claude-code-config.json",
  "hooks": {
    "post_read": "cargo canon daemon --hook-post-read --file $FILE_PATH"
  }
}
```

### 2. Antigravity / AGY Integration (`AGENTS.md`)

When Antigravity agents explore large codebases, instruct them to slice targets rather than reading raw files:

```markdown
# Antigravity Context Slicing Directives

When exploring unfamiliar files or modules exceeding 200 lines:

1. Run `cargo canon slice <FILE_PATH>` to inspect the type skeleton.
2. Run `cargo canon slice <FILE_PATH> --cross-boundary` to inspect route & contract impact.
3. Keep target files strictly byte-identical on disk.
```

### 3. Workspace Configuration (`.canon.toml`)

```toml
[general]
codebase_scale = "massive"
default_format = "markdown"

[slice]
max_call_depth = 2
strip_comments = false
include_private_types = true

[daemon]
enable_file_watcher = true
watch_paths = ["crates", "frontend/src", "backend"]

[typescript]
enabled = true
tsconfig_path = "frontend/tsconfig.json"
extensions = ["ts", "tsx", "js", "jsx"]
strip_jsx = true
strip_hook_bodies = true

[go]
enabled = true
module_root = "backend"
extensions = ["go"]
strip_function_bodies = true
index_implicit_interfaces = true

[cross_boundary]
rust_models = "crates/protocol/src"
ts_types = "frontend/src/types"
go_models = "backend/pkg/models"
```

---

## 7. Key Source References

- [`crates/canon-slice/src/skeleton.rs`](../crates/canon-slice/src/skeleton.rs): Rust AST visitor stripping implementation bodies via `syn::visit_mut::VisitMut`.
- [`crates/canon-slice/src/ts_skeleton.rs`](../crates/canon-slice/src/ts_skeleton.rs): Ultra-fast TypeScript / TSX skeleton stripper powered by `oxc_parser`.
- [`crates/canon-slice/src/go_skeleton.rs`](../crates/canon-slice/src/go_skeleton.rs): Go AST parser and implicit interface resolver via `tree-sitter-go`.
- [`crates/canon-slice/src/indexer.rs`](../crates/canon-slice/src/indexer.rs): Cross-boundary route and symbol indexer correlating Rust/Go routes with TypeScript fetch calls.
- [`crates/canon-daemon/src/hook.rs`](../crates/canon-daemon/src/hook.rs): Out-of-band non-destructive advisory post-read tool hook handler.
- [`crates/canon-daemon/src/server.rs`](../crates/canon-daemon/src/server.rs): IPC Unix domain socket daemon with lock-free atomic bloat telemetry.
