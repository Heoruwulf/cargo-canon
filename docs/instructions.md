# Setting Up `cargo-canon` with `your-project` to Improve Claude

Integrating `cargo-canon` with **`your-project`** (a large-scale Rust and TypeScript codebase) transforms Claude from an open-loop code generator into a **closed-loop engineer** by addressing key challenges in massive codebases (>1.2M LOC):

1. **Context Window Protection & Attention Decay:** Slices multi-thousand-line Rust modules, JSX blocks, and React hook internals down to compiler-accurate type skeletons and call graphs (saving 70–95% context tokens).
2. **Non-Destructive Advisory Context:** Feeds skeletons and call hierarchies to Claude **strictly out-of-band** (stdout / IPC) so files on disk remain 100% byte-identical, preventing diff/patch offset mismatches in Claude Code.
3. **Cross-Boundary Rust $\leftrightarrow$ TypeScript Indexing:** Automatically correlates Rust Serde structs and HTTP route handlers with TypeScript types and `fetch`/`axios` call sites.

---

## Step 1: Install `cargo-canon` Globally

From this repository, compile and install the CLI binary into your `$PATH`:

```bash
cargo install --path crates/cargo-canon
# Verify installation
cargo canon --version
```

---

## Step 2: Configure `your-project/.canon.toml`

Place a `.canon.toml` configuration file in the root of `your-project`. Adjust the directory paths to match your project layout (e.g., your Rust crates and TypeScript frontend/packages):

```toml
# your-project/.canon.toml

[general]
codebase_scale = "massive"     # Configures memory pool allocations for >1M LOC
default_format = "markdown"

[slice]
max_call_depth = 2
strip_comments = false
include_private_types = true

[daemon]
enable_file_watcher = true
# Watch directories for live AST and symbol re-indexing (<20ms updates)
watch_paths = [
  "crates",
  "src",
  "packages",
  "frontend/src",
  "web/src"
]

[typescript]
enabled = true
tsconfig_path = "frontend/tsconfig.json"    # Point to root or primary tsconfig
extensions = ["ts", "tsx", "js", "jsx"]
strip_jsx = true                            # Strips JSX elements down to clean interfaces
strip_hook_bodies = true                    # Empties useEffect/useState bodies

[cross_boundary]
# Directories containing backend Serde models and frontend contract interfaces
rust_models = "crates/protocol/src"         # Or crates/shared-types/src
ts_types = "frontend/src/types"             # Or packages/api-types/src
strict_casing_check = true                  # Flags camelCase vs snake_case mismatches
strict_omitempty_check = true               # Flags Option<T> serialization drift
```

---

## Step 3: Wire Claude Code Post-Read Hook

Configure Claude Code to automatically receive advisory context whenever it reads a Rust or TypeScript file.

Create or update `.claude/config.json` in `your-project`:

```json
{
  "$schema": "https://json.schemastore.org/partial-claude-code-config.json",
  "hooks": {
    "post_read": "cargo canon daemon --hook-post-read --file $FILE_PATH"
  }
}
```

> [!NOTE]
> **How this works:** Whenever Claude executes a file read tool on any `.rs`, `.ts`, or `.tsx` file, the daemon returns the file's stripped type skeleton, trait contracts, upstream callers, downstream dependencies, and cross-boundary bindings directly into Claude's context without altering the physical file on disk.

---

## Step 4: Run the Indexing Daemon Sidecar

Before starting your Claude session in `your-project`, start the daemon:

```bash
cd /path/to/your-project

# Start the background indexing daemon
cargo canon daemon --start

# Verify liveness and cache telemetry
cargo canon daemon --ping
cargo canon daemon --status
```

The daemon monitors file mutations incrementally using `notify` and communicates over a repository-scoped Unix domain socket (`/tmp/canon-<repo_hash>.sock`) for sub-millisecond query responses.

---

## Step 5: Add Instructions to `CLAUDE.md` in `your-project`

Add instructions to your repo's `CLAUDE.md` or `AGENTS.md` so Claude knows how to leverage the verification oracles:

```markdown
# Agent Instructions: Canon Ground-Truth Tooling

This repository uses `cargo-canon` for compiler-aware context delivery and ground-truth verification:

1. **Exploring Large Files / Interfaces**:
   - Rather than dumping multi-thousand-line source files into context, slice them:
     `cargo canon slice <FILE_PATH>`
   - To inspect backend-to-frontend route and type impact:
     `cargo canon slice <FILE_PATH> --cross-boundary`

2. **Cross-Boundary Verification**:
   - Before completing refactors modifying shared Serde models or TS contracts:
     `cargo canon diff-fuzz --cross-boundary --rust-model <MODEL_NAME> --ts-type <TS_TYPE_NAME>`

3. **Pure State Machines & Async Protocols**:
   - Verify pure Sans-I/O domain reducers or state machines:
     `cargo canon state-machine --fuzz-transitions`
     `cargo canon state-machine --ts-reducer <PATH_TO_REDUCER>`
   - Run deterministic simulation passes:
     `cargo canon simulate --seeds 5000`
```

---

## Step 6: Verify the Setup

1. **Test slicing a file directly:**

   ```bash
   cargo canon slice crates/core/src/lib.rs
   cargo canon slice frontend/src/App.tsx
   ```

2. **Test cross-boundary discovery:**

   ```bash
   cargo canon slice crates/server/src/routes/user.rs --cross-boundary
   ```

3. **Check daemon responsiveness:**

   ```bash
   cargo canon daemon --hook-post-read --file frontend/src/types/api.ts
   ```
