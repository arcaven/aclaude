# Rust Coding Rules

## Safety

```rust
// Every binary and library crate
#![forbid(unsafe_code)]
```

- No `unwrap()` in production code — use `?` or `expect()` with actionable message
- `unwrap()` is acceptable in tests

## Type Design

- **Newtypes for IDs:** `NodeId(String)`, `FindingId(String)` — prevents mixing ID types
- **Validated constructors at trust boundaries:** `new()` validates (CLI input, file parsing)
- **`new_unchecked()` for tests and trusted internal sources**
- **`#[non_exhaustive]` on enums that will grow** — forces callers to handle future variants
- **Private fields with getters** on types where invariants must hold

## Error Handling

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KosError {
    // Semantic variants per domain
}

pub type Result<T> = std::result::Result<T, KosError>;
```

- Use `thiserror` for error enums — structured variants, not string bags
- Define `pub type Result<T>` per crate for ergonomics
- `Display` impl is for user-facing output in a CLI — be clear and actionable

## Module Structure

```
src/
  main.rs         # CLI entry point (clap)
  lib.rs          # Public API re-exports
  error.rs        # KosError enum
  model/          # Schema types (Node, Edge, Confidence, etc.)
  orient/         # orient subcommand
  validate/       # validate subcommand
  graph/          # graph rendering subcommand
```

## Dependencies

- Workspace-level dependency declarations in root `Cargo.toml`
- Edition 2024, MSRV 1.85+
- Key crates:
  - `serde` / `serde_yaml` (YAML serialization — the graph substrate)
  - `serde_json` (JSONL output)
  - `clap` (CLI parsing with derive)
  - `thiserror` (error types)
  - `petgraph` (graph traversal for drift/ripple)
- Use `cargo clippy -- -D warnings` — warnings are errors

## Testing

- Unit: `#[cfg(test)] mod tests {}` in same file
- Integration: `tests/` directory, named by feature
- Test names as documentation: `node_confidence_rejects_invalid()`, not `test_1()`
- Test boundaries: empty, missing fields, invalid confidence values, broken edge targets
- Snapshot tests with `insta` for graph rendering output

## Architecture

kos is a single-binary CLI. No workspace (single crate) until complexity warrants splitting.

- `lib.rs` is the public API — `main.rs` calls into it
- Model types mirror the YAML schema exactly (serde does the mapping)
- Each subcommand is a module with a public `run()` function
