# aclaude Charter

BYOA agent CLI — console environment with persona theming, wrapping Claude
Code via the Agent SDK. Phase 1: standalone single-agent CLI.

Follows the kos process: Orient → Ideate → Question → Probe → Harvest → Promote.
Authoritative graph: `_kos/nodes/`.
Cross-repo questions belong in the orchestrator's charter.

Last updated: 2026-03-30

---

## Bedrock

*Established. Evidence-based or decided with rationale.*

### B1: Dual SDK Design

`@anthropic-ai/claude-agent-sdk` for session running (spawns Claude Code as
subprocess, inherits auth). `@anthropic-ai/sdk` for raw API access (token
usage, model listing). Hooks are in-process JS callbacks, not shell scripts.

Evidence: implemented and working, validated in distribution probe.

### B2: TOML Configuration

Config merge order: defaults → global (~/.config/aclaude/) → local (.aclaude/)
→ env (ACLAUDE_*) → CLI flags. TOML format for all config files.

Evidence: implemented, follows orchestrator convention.

### B3: Distribution Model (Partial)

Bun compile produces signed standalone binaries. Self-update via gen-version.ts
pattern. Homebrew tap. Dual alpha/stable channels. 63MB binary.

Evidence: distribution probe (aae-orc sprint/rd/aclaude-distribution.md), 38
findings. Works, but see F1 (language question) and F4 (binary size).

---

## Frontier

*Actively open — under exploration, not yet resolved.*

### F1: Two-Audience Problem

aclaude serves human operators (TUI, status bars, persona flair) and
autonomous agents under marvel (fast startup, minimal overhead, programmatic
control). These may not be the same binary, language, or SDK. Optimizing for
both risks least common denominator.

Cross-ref: orchestrator F13 (UX parity), F16 (assumption provenance).

### F2: Language Choice

TypeScript inherited from pennyfarthing, not chosen against examined
requirements. Works (bun compile, themes embed, Agent SDK native) but:
15s first-prompt latency, no TUI without frameworks, bun compile virtual FS
friction, 63MB binary. Alternatives not evaluated: Go, Rust, Python.

Cross-ref: orchestrator F8 (bootstrapping through probe code), F16.

### F3: Theme Bundling vs Loading

100 themes embedded in binary (~1.7MB). Agent subprocess can't see them.
Arguments for runtime loading (pack system, smaller binary, multi-console).
Arguments for bundling (zero-dependency, offline, version coherence).
Possible hybrid: minimal default + external loading.

Cross-ref: orchestrator F12 (persona pack), F15 (persona model).

### F4: Session Bootstrap Layering

Docker-like model: entrypoint (baked) → installed base (packs) → session
injection (per-launch). Open tension: Claude Code config inheritance
(full, isolated, or switchable). For autonomous agents: different bootstrap
(no TUI, task assignment, workspace boundaries).

Cross-ref: orchestrator F18.

### F5: Version Manager Coordination

Brew and self-update are parallel version managers with no coordination.
Should aclaude detect brew install and defer? Or should one channel win?

Cross-ref: orchestrator F11 (distribution model).

---

## Graveyard

*Tried, ruled out, permanently recorded.*

### G1: pennyfarthing

The predecessor. Per-repo install, never became distributable. Every user
except the developer churned out because install/update was never solved.
Concepts (persona theming, tmux integration) carried forward; implementation
discarded.

Evidence: distribution probe finding — pennyfarthing's fatal flaw was
distribution, not concept.
