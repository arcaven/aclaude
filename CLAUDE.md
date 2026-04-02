# aclaude

Opinionated Claude Code distribution with persona theming. Wraps the `claude`
CLI as a subprocess using the NDJSON streaming protocol.

Rewritten from TypeScript to Rust (2026-04-01) following the axios npm supply
chain incident. See `docs/security/axios-supply-chain-2026-03-31.md`.

## Build / Run / Test

Requires: Rust 1.85+ (Edition 2024), `just`, `tmux` (optional, for sessions).

```sh
just build          # cargo build
just dev            # cargo run (with args)
just test           # cargo test
just lint           # fmt + clippy + deny (pre-commit mirror)
just ci             # full CI mirror
just fmt            # cargo +nightly fmt --all
just start          # launch tmux session
```

## Architecture

```
src/
  main.rs             CLI entry point (clap)
  lib.rs              Public API re-exports
  error.rs            AclaudeError enum
  config.rs           TOML config: 5-layer merge
  persona.rs          Theme loading (embedded), system prompt building
  session.rs          Claude CLI subprocess management
  hooks.rs            Tool usage tracking, audit trail
  statusline.rs       Tmux status bar rendering
  updater.rs          Self-update via GitHub releases

personas/themes/     118 YAML theme files (embedded at compile time via build.rs)
config/              TOML config defaults and examples
tmux/                Session launcher
docs/                Architecture docs, research, security notes
```

### Claude Code Integration

aclaude spawns `claude` as a child process. No SDK dependency — the NDJSON
subprocess protocol is the stable contract. For interactive sessions:

```
claude --model <model> --append-system-prompt "<persona prompt>"
```

For programmatic use (one-shot):

```
claude -p "prompt" --model <model> --output-format json --append-system-prompt "<persona>"
```

### Theme Embedding

`build.rs` reads all `personas/themes/*.yaml` at compile time and generates
a Rust source file with embedded theme content. Themes are available at
runtime without filesystem access — the binary is self-contained.

@.claude/rules/_index.md

## Conventions

- **Language:** Rust. Entire codebase.
- **Config format:** TOML. Merge order: defaults -> global (~/.config/aclaude/) -> local (.aclaude/) -> env (ACLAUDE_*) -> CLI flags.
- **Auth:** Delegates to Claude Code. Users authenticate with their own credentials. aclaude does not store, manage, or proxy authentication.
- **No file deletion:** Never delete user files. Overwrite only with explicit intent.
- **Parallel-safe:** Each session gets a UUID. No shared mutable state between sessions.

## Values

- **Portability:** Single static binary. No runtime dependencies.
- **Composability:** CLI subcommands, tmux integration, OTEL — all optional layers.
- **User sovereignty:** All config is local. No phone-home. Telemetry is opt-in and self-hosted.
- **Supply chain safety:** No npm/node ecosystem. Rust dependencies audited via cargo-deny.

## How to Work Here (kos Process)

### Re-introduction
Read charter.md before any substantive work. It contains:
- Current bedrock (what's committed)
- Current frontier (what's under exploration)
- Current graveyard (what's been ruled out)

### Session Protocol
1. Read charter.md (orient)
2. Identify the highest-value open question — or capture new ideas in _kos/ideas/
3. Write an Exploration Brief in _kos/probes/
4. Do the probe work
5. Write a finding in _kos/findings/
6. Harvest: update affected nodes, move files if confidence changed
7. Update charter.md if bedrock changed

Cross-repo questions belong in the orchestrator's _kos/, not here.

### Ideas (pre-hypothesis brainstorming)
Ideas live in _kos/ideas/ as markdown files. Generative, possibly contradictory,
no commitment. When an idea crystallizes, extract into a frontier question + brief.

### Node Files
Nodes live in _kos/nodes/[confidence]/[id].yaml
Schema follows kos schema v0.3.
One node per file. Filename = node id.

### Confidence Changes
Moving a file between confidence directories IS the promotion.
Always accompany with a commit message explaining the evidence.

### Harvest Verification
Before starting the next cycle, verify:
- [ ] Finding written and committed
- [ ] Charter updated if bedrock changed
- [ ] Frontier questions updated (closed, opened, or revised)
- [ ] Exploration briefs marked complete or carried forward
