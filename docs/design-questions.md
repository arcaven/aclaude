# forestage Design Questions

Open questions about what forestage should be. These are not issues or TODOs —
they're uncertainties that probing is expected to resolve. Cross-referenced
with aae-orc charter frontier items.

Last updated: 2026-04-01 (harvest: Rust rewrite)

---

## What is forestage for?

Two use cases, possibly two products:

**Human operator** — a person at a terminal using Claude Code with added
value: persona theming, status bars, context tracking, portrait images,
tmux integration, configuration management. The UX should be richer than
vanilla Claude Code. TUI matters. Startup time matters. Persona flair matters.

**Autonomous agent workload** — a process managed by marvel, running in a
tmux pane, executing tasks without human interaction. Doesn't need persona
flair. Doesn't need TUI. Needs fast startup, minimal overhead, programmatic
control, predictable behavior. May not need themes at all — or needs exactly
one, injected at launch.

The Rust rewrite gives both audiences a fast, portable binary. The question
is now about runtime behavior: should forestage have a --headless or --agent
flag? Is this one binary with modes, or two binaries? The subprocess protocol
(NDJSON) is the same for both audiences.

Charter: F13 (UX parity), F14 (TUI), F16 (assumption provenance).

## ~~Should forestage be TypeScript?~~ RESOLVED

**Answered: No. Rust.** Rewritten 2026-04-01 following the axios npm supply
chain incident. See finding-001-rust-rewrite, docs/security/axios-supply-chain-2026-03-31.md.

Drivers: supply chain safety (no npm), portability (static binary), performance
(native startup, smaller binary), ecosystem alignment (Go/Rust platform).
Trade-off accepted: higher development cost for UI/string work.

Charter: B1 (Rust Implementation). Previously F2 (Language Choice).

## Should themes/personas be bundled or loaded?

Currently: 118 theme YAMLs embedded in binary via build.rs compile-time code
generation. Portraits are already external (global cache). The agent subprocess
(Claude Code) cannot introspect embedded themes.

Rust's compile-time embedding (build.rs) is more reliable than bun compile's
virtual FS was — no path resolution friction, no broken features. This makes
bundling more attractive than it was in TypeScript.

Arguments for loading at runtime:
- Autonomous agents need one theme or none. Bundling 118 is waste.
- Pack system (marvel) should manage themes as content, not code.
- Decoupling enables other consoles (zclaude, dclaude) to use same themes.
- Smaller binary if themes are external.

Arguments for bundling:
- Zero-dependency: works without any additional install steps.
- No network fetch at startup. Offline-friendly.
- Version coherence: binary + themes are always in sync.

Possible hybrid: bundle a minimal default set, load additional themes
from `~/.local/share/forestage/themes/` or via marvel pack resolution.

Charter: F12 (persona themes as content pack), F15 (persona model).

## How should forestage sessions be bootstrapped?

Docker-like layering model:

1. **Entrypoint** (baked in) — "you are forestage." System prompt injected
   via `--append-system-prompt`. Always present.
2. **Installed base** (on disk) — packs, themes, commands. Updateable
   independently of the binary.
3. **Session injection** (per-launch) — persona, model, permissions,
   feature flags. Ephemeral.

Open tension: forestage inherits Claude Code's config (`~/.claude/`,
`.claude/rules/`, CLAUDE.md). Users may want full inheritance, isolation,
or switching between vanilla Claude Code and forestage. This needs to be a
preference, not hardcoded.

For autonomous agents (marvel teams): bootstrap is different. No TUI
instructions. Task assignment, tool permissions, workspace boundaries.
Same binary, different CLI flags.

Charter: F18 (session bootstrap and context layering).

## Version identity and distribution coordination

build.rs generate_version() injects VERSION, CHANNEL, COMMIT, BUILD_TIME
at compile time via cargo rustc-env directives. Self-update (updater.rs)
validated in TypeScript era, needs re-validation for Rust artifacts.

Open: brew and self-update are parallel version managers. No coordination.
Should forestage detect brew install and defer to `brew upgrade`? Or should
one channel win? Dual-track (alpha via self-update, stable via brew) is
the design but untested for Rust distribution.

Charter: F11 (distribution model).
