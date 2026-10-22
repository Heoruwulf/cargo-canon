# Configuration Templates & Starter Kits

This directory contains drop-in configuration templates and starter kits for integrating `cargo-canon` into your Rust, TypeScript, and Go projects and AI coding agent toolchains.

---

## Available Templates

| Template | Target Path in Your Project | Purpose |
| :--- | :--- | :--- |
| [`canon.example.toml`](canon.example.toml) | `~/.config/canon/canon.toml` or `.canon.toml` | Full configuration template with daemon infrastructure controls and 4 real-world project scenarios (polyglot monorepo with worktrees, pure Go service, systems Rust engine, and TypeScript frontend). |
| [`claude-config.json`](claude-config.json) | `.claude/config.json` | Configures Claude Code to automatically invoke the post-read advisory context hook (`.rs`, `.ts`, `.tsx`, `.go`). |
| [`ground-truth.yml`](ground-truth.yml) | `.github/workflows/ground-truth.yml` | GitHub Actions CI workflow running deterministic simulation, differential fuzzing, and model checking across Rust and Go. |
| [`pre-commit.sh`](pre-commit.sh) | `.git/hooks/pre-commit` or `.githooks/pre-commit` | Fast local git pre-commit hook verifying state machine reachability and running 500 simulation seeds. |

---

## Quick-Start Setup in Your Project

### 1. Enable Project Configuration

```sh
# Copy reference config as system-wide daemon configuration:
mkdir -p ~/.config/canon
cp configs/canon.example.toml ~/.config/canon/canon.toml

# Or copy into your repository root:
cp configs/canon.example.toml /path/to/your-project/.canon.toml
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
