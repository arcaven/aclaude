# aclaude Charter

BYOA agent CLI — console environment with persona theming. Spawns `claude`
CLI as a subprocess using the NDJSON streaming protocol. Rust binary, no
SDK dependency.

Follows the kos process: Orient → Ideate → Question → Probe → Harvest → Promote.
Authoritative graph: `_kos/nodes/`.
Cross-repo questions belong in the orchestrator's charter.

Last updated: 2026-04-01 (harvest: Rust rewrite)

---

## Bedrock

*Established. Evidence-based or decided with rationale.*

### B1: Rust Implementation

aclaude is written in Rust (Edition 2024, MSRV 1.85+). Single static binary,
no runtime dependencies. Spawns `claude` CLI as a subprocess using the NDJSON
streaming protocol — no SDK dependency, the subprocess protocol is the stable
contract.

Chosen over TypeScript (inherited from pennyfarthing) following the axios npm
supply chain incident (2026-03-31). Drivers: supply chain safety (cargo-deny,
no npm ecosystem), portability (static binary), performance (native startup,
smaller binary), ecosystem alignment (marvel/switchboard are Go, kos is Rust).

Evidence: PR #3 (feat/rust-rewrite), finding-001-rust-rewrite.
See also: docs/security/axios-supply-chain-2026-03-31.md.

### B2: TOML Configuration

Config merge order: defaults → global (~/.config/aclaude/) → local (.aclaude/)
→ env (ACLAUDE_*) → CLI flags. TOML format for all config files.

Evidence: implemented, follows orchestrator convention.

### B3: Compile-Time Theme Embedding

118 theme YAMLs embedded in binary via build.rs code generation at compile
time. Themes available at runtime without filesystem access — binary is
self-contained. Portraits remain external (global cache).

Evidence: build.rs reads personas/themes/*.yaml and generates Rust source.

---

## Frontier

*Actively open — under exploration, not yet resolved.*

### F1: Two-Audience Problem

aclaude serves human operators (TUI, status bars, persona flair) and
autonomous agents under marvel (fast startup, minimal overhead, programmatic
control). The Rust rewrite resolved the language question but not the audience
question. Is this one binary with runtime modes (--headless, --agent) or two
binaries? The subprocess protocol (NDJSON) is the same for both.

Cross-ref: orchestrator F13 (UX parity), F16 (assumption provenance).

### F2: Theme Bundling vs Runtime Loading

Themes are compiled into the binary (build.rs). Arguments for runtime loading
(pack system, smaller binary, multi-console reuse). Arguments for bundling
(zero-dependency, offline, version coherence). Rust's compile-time embedding
is more reliable than bun compile's virtual FS — makes bundling more
attractive than it was in TypeScript. Possible hybrid: bundle defaults, load
additional from ~/.local/share/aclaude/ or via marvel pack resolution.

Cross-ref: orchestrator F12 (persona pack), F15 (persona model).

### F3: Session Bootstrap Layering

Docker-like model: entrypoint (baked) → installed base (packs) → session
injection (per-launch). aclaude spawns claude with --append-system-prompt
for persona injection. Open tension: Claude Code config inheritance (full,
isolated, or switchable). For autonomous agents: different bootstrap (no TUI,
task assignment, workspace boundaries).

Cross-ref: orchestrator F18.

### F4: Version Manager Coordination

Brew and self-update are parallel version managers with no coordination.
build.rs generate_version() injects version info at compile time. Should
aclaude detect brew install and defer? Or should one channel win? Dual-track
model needs re-validation for Rust artifacts.

Cross-ref: orchestrator F11 (distribution model).

### F5: Distribution Model (Rust)

The TypeScript distribution probe (38 findings) is mostly stale. Portable
findings survive: brew tap pipeline pattern, dual alpha/stable channels,
self-update UX, code signing workflow. Need to re-validate: binary size,
cross-compilation, CI pipeline (currently failing on cargo-deny config).

Cross-ref: orchestrator F11.

---

## Graveyard

*Tried, ruled out, permanently recorded.*

### G1: pennyfarthing

The predecessor. Per-repo install, never became distributable. Every user
except the developer churned out because install/update was never solved.
Concepts (persona theming, tmux integration) carried forward; implementation
discarded.

### G2: TypeScript + bun compile + Agent SDK

aclaude's original implementation stack, inherited from pennyfarthing.
Worked: bun compile produced signed binaries, themes embedded, Agent SDK
provided subprocess management. Problems: 15s first-prompt latency, 63MB
binary, bun compile virtual FS friction (5+ broken features), npm supply
chain exposure (axios incident). Replaced by Rust rewrite (2026-04-01).

Evidence: finding-001-rust-rewrite, docs/security/axios-supply-chain-2026-03-31.md.
