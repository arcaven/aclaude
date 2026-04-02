# aclaude — opinionated Claude Code distribution

# Default: list recipes
default:
    @just --list

# ─── Build & Run ───────────────────────────────────────

# Build the binary
build:
    cargo build

# Build release binary
build-release:
    cargo build --release

# Run aclaude with arguments
run *args:
    cargo run -- {{args}}

# Run in dev mode (same as run, for parity with old workflow)
dev *args:
    cargo run -- {{args}}

# ─── Test ──────────────────────────────────────────────

# Run all tests
test:
    cargo test

# Run doc tests
test-doc:
    cargo test --doc 2>/dev/null || echo "no library target — skipping doc tests"

# Run a specific test by name
test-one name:
    cargo test -- {{name}}

# ─── Quality Checks ───────────────────────────────────

# Pre-commit check (matches CI)
check: check-fmt check-clippy check-deny

# Full check including extras
check-all: check check-toml

# Check formatting (nightly rustfmt)
check-fmt:
    cargo +nightly fmt --all -- --check

# Run clippy with warnings as errors
check-clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Check licenses and advisories
check-deny:
    cargo deny check advisories licenses bans

# Check TOML formatting
check-toml:
    taplo fmt --check

# Alias
lint: check

# ─── Formatting ───────────────────────────────────────

# Format Rust code (nightly)
fmt:
    cargo +nightly fmt --all

# Format TOML files
fmt-toml:
    taplo fmt

# Format everything
fmt-all: fmt fmt-toml

# ─── CI Mirror ────────────────────────────────────────

# Run all CI jobs locally (fail-fast order)
ci: check-fmt check-clippy build check-deny test test-doc

# ─── Development ──────────────────────────────────────

# Watch mode: check + test on change
watch:
    cargo watch -x 'check' -x 'test --lib'

# Generate docs
doc:
    cargo doc --no-deps

# Generate and open docs
doc-open:
    cargo doc --no-deps --open

# Clean build artifacts
clean:
    cargo clean

# ─── tmux ─────────────────────────────────────────────

# Start tmux session
start:
    tmux/start-session.sh

# ─── Persona ──────────────────────────────────────────

# List available personas
persona-list:
    cargo run -- persona list

# Show a specific persona
persona-show name:
    cargo run -- persona show {{name}}

# Show resolved config
config:
    cargo run -- config

# ─── Setup ────────────────────────────────────────────

# First-time environment setup
setup:
    rustup component add clippy
    rustup toolchain install nightly --component rustfmt
    cargo install cargo-watch cargo-deny cargo-insta
    @echo "Optional: brew install taplo (TOML formatter)"
    @echo "Optional: cargo install cargo-nextest (parallel tests)"

# Install git hooks
install-hooks:
    lefthook install

# ─── Maintenance ──────────────────────────────────────

# Security audit
audit:
    cargo audit
