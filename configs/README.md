# Configuration Templates & Starter Kits

This directory contains drop-in configuration templates and starter kits for integrating `cargo-canon` into your Rust, TypeScript, and Go projects and AI coding agent toolchains.

---

## Available Templates

| Template | Target Path in Your Project | Purpose |
| :--- | :--- | :--- |
| [`.canon.toml`](.canon.toml) | `.canon.toml` (repo root) | Full configuration options for slicing, simulation, fuzzing, Kani, daemon, TypeScript, Go (Tier 1 TSan/goleak & Tier 2 state machine), and cross-boundary contracts. |
| [`claude-config.json`](claude-config.json) | `.claude/config.json` | Configures Claude Code to automatically invoke the post-read advisory context hook (`.rs`, `.ts`, `.tsx`, `.go`). |
| [`ground-truth.yml`](ground-truth.yml) | `.github/workflows/ground-truth.yml` | GitHub Actions CI workflow running deterministic simulation, differential fuzzing, and model checking across Rust and Go. |
| [`pre-commit.sh`](pre-commit.sh) | `.git/hooks/pre-commit` or `.githooks/pre-commit` | Fast local git pre-commit hook verifying state machine reachability and running 500 simulation seeds. |

---

## Quick-Start Setup in Your Project

### 1. Enable Project Configuration

```sh
# Copy reference config into your repository root
cp configs/.canon.toml /path/to/your-project/.canon.toml
```

### 2. Configure Claude Code Post-Read Hook

```sh
# Create .claude directory and install hook config
mkdir -p /path/to/your-project/.claude
cp configs/claude-config.json /path/to/your-project/.claude/config.json
```

### 3. Setup CI Ground-Truth Workflow

```sh
# Install GitHub Actions workflow
mkdir -p /path/to/your-project/.github/workflows
cp configs/ground-truth.yml /path/to/your-project/.github/workflows/
```

### 4. Setup Git Pre-Commit Hook

```sh
# Copy pre-commit hook and ensure it is executable
cp configs/pre-commit.sh /path/to/your-project/.git/hooks/pre-commit
chmod +x /path/to/your-project/.git/hooks/pre-commit
```
