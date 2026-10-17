#!/usr/bin/env bash
# ==============================================================================
# Git Pre-Commit Hook Template for cargo-canon
#
# Copy this script to `.githooks/pre-commit` or `.git/hooks/pre-commit` in your
# repository and ensure it has executable permissions (`chmod +x`).
# ==============================================================================
set -euo pipefail

echo "==> Running cargo-canon pre-commit verification..."

# 1. State machine reachability and invariant verification
if cargo canon state-machine --matrix >/dev/null 2>&1; then
    echo "  [OK] State machine transition matrix & reachability verified"
else
    echo "  [FAIL] State machine reachability check failed!" >&2
    exit 1
fi

# 2. Fast simulation pass
if cargo canon simulate --seeds 500 >/dev/null 2>&1; then
    echo "  [OK] Deterministic simulation passed (500 seeds)"
else
    echo "  [FAIL] Simulation detected invariant violations!" >&2
    exit 1
fi

echo "==> All pre-commit ground-truth checks passed."
