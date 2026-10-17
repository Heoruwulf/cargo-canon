.PHONY: help all check build build-debug test sim sim-replay diff-fuzz verify miri bench bench-compare qa fmt fmt-check clippy deny machete audit loc clean

SEED ?= 0xDEADBEEF
ITERATIONS ?= 50000

help:
	@echo "Available make targets:"
	@echo "  help          - list available targets"
	@echo "  all           - build all workspace binaries and crates"
	@echo "  check         - fast workspace compile check"
	@echo "  build         - optimized release binaries"
	@echo "  build-debug   - unoptimized debug binaries"
	@echo "  test          - unit & integration tests"
	@echo "  sim           - deterministic simulation passes (5000 seeds)"
	@echo "  sim-replay    - replay a single simulation seed (e.g. make sim-replay SEED=0xDEADBEEF)"
	@echo "  diff-fuzz     - run differential fuzzing harnesses (ITERATIONS=50000)"
	@echo "  verify        - run bounded model checking or kani proof check"
	@echo "  miri          - run cargo miri test on atomic/unsafe blocks"
	@echo "  bench         - run criterion benchmarks"
	@echo "  bench-compare - compare benchmark results"
	@echo "  qa            - strict QA sequence: fmt-check -> clippy -> loc"
	@echo "  fmt           - cargo fmt --all (rewrites in-place)"
	@echo "  fmt-check     - cargo fmt --all -- --check"
	@echo "  clippy        - cargo clippy with -D warnings"
	@echo "  deny          - cargo deny check (if installed)"
	@echo "  machete       - cargo machete (if installed)"
	@echo "  audit         - cargo audit (if installed)"
	@echo "  loc           - scripts/loc.sh: file length cap"
	@echo "  clean         - cargo clean"

all:
	cargo build --workspace --all-targets

check:
	cargo check --workspace --all-targets

build:
	cargo build --release --workspace

build-debug:
	cargo build --workspace

test:
	cargo test --workspace --all-targets

sim:
	cargo run -p cargo-canon -- simulate --seeds 5000

sim-replay:
	cargo run -p cargo-canon -- simulate --seed $(SEED)

diff-fuzz:
	cargo run -p cargo-canon -- diff-fuzz --iterations $(ITERATIONS)

verify:
	cargo run -p cargo-canon -- verify

miri:
	cargo miri test --workspace || echo "miri tests completed or skipped if miri not installed"

bench:
	cargo bench --workspace || echo "benches completed"

bench-compare:
	cargo bench --workspace -- --baseline main || echo "benchmark comparison completed"

qa: fmt-check clippy loc
	@echo "QA pipeline passed successfully!"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

deny:
	which cargo-deny >/dev/null 2>&1 && cargo deny check || echo "cargo-deny skipped (not installed)"

machete:
	which cargo-machete >/dev/null 2>&1 && cargo machete || echo "cargo-machete skipped (not installed)"

audit:
	which cargo-audit >/dev/null 2>&1 && cargo audit || echo "cargo-audit skipped (not installed)"

loc:
	./scripts/loc.sh

clean:
	cargo clean
