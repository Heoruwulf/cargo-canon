#!/usr/bin/env bash
# ==============================================================================
# canon-post-read.sh - Advisory Agent Context Injection Hook
#
# Intended for AI coding agent toolchains (Claude Code, Antigravity, Cursor).
# Invoked after a file is read to output advisory context (type skeletons,
# contracts, and call hierarchy) strictly out-of-band to stdout without
# mutating the source file on disk.
# ==============================================================================
set -euo pipefail

TARGET_FILE="${1:-${FILE_PATH:-}}"

if [ -z "$TARGET_FILE" ]; then
    echo "Usage: $0 <file-path>" >&2
    exit 1
fi

if [ ! -f "$TARGET_FILE" ]; then
    exit 0
fi

# Process Rust and TypeScript source files
case "$TARGET_FILE" in
    *.rs|*.ts|*.tsx|*.js|*.jsx)
        ;;
    *)
        exit 0
        ;;
esac

# Dispatch to cargo-canon binary or local target build
if command -v cargo-canon >/dev/null 2>&1; then
    cargo-canon daemon --hook-post-read --file "$TARGET_FILE"
elif [ -x "./target/release/cargo-canon" ]; then
    ./target/release/cargo-canon daemon --hook-post-read --file "$TARGET_FILE"
elif [ -x "./target/debug/cargo-canon" ]; then
    ./target/debug/cargo-canon daemon --hook-post-read --file "$TARGET_FILE"
elif command -v cargo >/dev/null 2>&1; then
    cargo run --quiet -p cargo-canon -- daemon --hook-post-read --file "$TARGET_FILE"
fi
