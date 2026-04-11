# forestage Charter

BYOA agent CLI — console environment with persona theming. Spawns `claude`
CLI as a subprocess using the NDJSON streaming protocol. Rust binary, no
SDK dependency.

Follows the kos process: Orient → Ideate → Question → Probe → Harvest → Promote.
Authoritative graph: `_kos/nodes/`.
Cross-repo questions belong in the orchestrator's charter.

Last updated: 2026-04-11 (harvest: TUI prototype, session maturity, terminal images)

---

## Bedrock

*Established. Evidence-based or decided with rationale.*

### B1: Rust Implementation

forestage is written in Rust (Edition 2024, MSRV 1.85+). Single static binary,
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

Config merge order: defaults → global (~/.config/forestage/) → local (.forestage/)
→ env (FORESTAGE_*) → CLI flags. TOML format for all config files.

Evidence: implemented, follows orchestrator convention.

### B3: Compile-Time Theme Embedding

118 theme YAMLs embedded in binary via build.rs code generation at compile
time. Themes available at runtime without filesystem access — binary is
self-contained. Portraits remain external (global cache).

Evidence: build.rs reads personas/themes/*.yaml and generates Rust source.

### B4: Headless Claude Code with Custom TUI (Bridge Architecture)

Claude Code runs as a headless subprocess via NDJSON streaming. forestage
renders a custom ratatui TUI around the event stream. The bridge pattern
(bridge.rs, protocol_ext.rs at src/ level) is TUI-agnostic — the TUI is
one consumer module under src/tui/, enabling dual-mode (human TUI + future
marvel diagnostic) without restructuring.

**Bridge layer** (no ratatui dependencies):
- bridge.rs: subprocess lifecycle, event channel (mpsc), SessionMetrics
  (Arc<Mutex<>>), stdin writer, statusline throttling
- protocol_ext.rs: BridgeParser (stateful, 17 event types), BridgeEvent
  enum (16 variants), SessionMetrics aggregation (tokens, cost, context,
  tool counts, model, permissions, slash commands)

**TUI layer** (src/tui/, 8 modules):
- AppStatus state machine (Connecting, Ready, Thinking, Streaming,
  ToolRunning, Error) drives input gating and visual feedback
- 45ms/2KB text batching eliminates per-token render overhead
- Unified diff rendering for Edit operations, tool-specific renderers
  (Read/Write/Bash/Grep/Glob)
- Permission mode toggle (Shift+Tab) with inline approval dialog
- Slash commands with dynamic tab completion and @ file path completion
- Cursor-positioned input with Ctrl+A/E/W/U editing, mouse wheel scroll
- Thinking block display (Alt+T toggle)
- Markdown rendering (code blocks, headers, lists, bold/italic/code)
- Transcript mode (Ctrl+O cycling normal/transcript/focus)
- Portrait overlay with hotkeys (Ctrl+P position, Alt+P on/off, Alt+S
  size cycle)
- Input field expansion, bracketed paste support

**`--mode forestage|claude` flag** selects runtime:
- `forestage` (default): headless subprocess + custom ratatui TUI
- `claude`: inherited stdio passthrough to native Claude Code TUI

Configurable via CLI flag, config file, or FORESTAGE_SESSION__MODE env var.

Evidence: 8 phases, 21 commits, ~7000 lines, 94 tests. PR #24 merged to
develop. finding-010-tui-prototype (partial — architecture validated,
performance not yet profiled).
Cross-ref: orchestrator F14.

### B5: Session Management (Smart Control Session)

tmux session management with smart control session architecture:
- Pattern 1 (single session): control mode attaches directly to user
  session. No dedicated _ctrl. Zero overhead.
- Pattern 2 (multiple sessions): shared _ctrl session created
  automatically, one per socket. Transparent upgrade from Pattern 1.
- Cleanup: _ctrl removed when last user session stops.

CLI subcommands: `session start` (petname generation, auto-attach),
`session attach` (auto-select single), `session stop` (--all),
`session list` (--names, --all), `session status` (formatted table).

Evidence: finding-006-control-session-probe (session-010). Probed and
verified that tmux control mode attaches directly to existing sessions —
no dedicated control session required for single-session use. The research
claim that control mode and terminal clients "get weird" on the same
session was tested and disproven (F006-d). The -d flag is the real
constraint, not control mode itself (F006-f).

### B6: Terminal Image Support (Three-Tier Detection)

Three-tier detection replaces hardcoded Kitty/Ghostty allowlist:
1. Known-good: terminal-specific env vars (KITTY_WINDOW_ID,
   WEZTERM_EXECUTABLE, WEZTERM_PANE, GHOSTTY_RESOURCES_DIR) — these
   survive inside tmux where TERM_PROGRAM is overwritten.
2. Known-bad: TERM=dumb, TERM=linux, TERM_PROGRAM=apple_terminal.
3. Unknown: attempt with best available tool, graceful failure (cosmetic).

Display tool fallback chain: kitten icat → wezterm imgcat → skip.
tmux graphics passthrough auto-enabled on forestage's dedicated socket
(allow-passthrough on). Config override: `[portrait] display =
auto|always|never` with FORESTAGE_PORTRAIT__DISPLAY env var.

Evidence: finding-009-terminal-image-support (session-011). Distribution-
verified — alpha release, Homebrew tap, installed binary tested (WezTerm,
macOS). No tmux-cmc changes required (existing set_option API sufficient).

---

## Frontier

*Actively open — under exploration, not yet resolved.*

### F1: Two-Audience Problem [partially resolved]

forestage serves human operators (TUI, status bars, persona flair) and
autonomous agents under marvel (fast startup, minimal overhead, programmatic
control). The `--mode forestage|claude` flag provides runtime selection, and
the bridge architecture (B4) validates the split: same subprocess protocol,
different consumers.

Remaining questions: should marvel teams use `--mode claude` (no TUI
overhead) or a third mode (headless NDJSON consumer with no rendering)?
How does the bridge layer serve a marvel sidecar that monitors agent
sessions? Is the SessionMetrics Arc<Mutex<>> the right interface for
external consumers, or does marvel need a different contract?

Cross-ref: orchestrator F13 (UX parity), F16 (assumption provenance).

### F2: Theme Bundling vs Runtime Loading

Themes are compiled into the binary (build.rs). Arguments for runtime loading
(pack system, smaller binary, multi-console reuse). Arguments for bundling
(zero-dependency, offline, version coherence). Rust's compile-time embedding
is more reliable than bun compile's virtual FS — makes bundling more
attractive than it was in TypeScript. Possible hybrid: bundle defaults, load
additional from ~/.local/share/forestage/ or via marvel pack resolution.

Cross-ref: orchestrator F12 (persona pack), F15 (persona model).

### F3: Session Bootstrap Layering

Docker-like model: entrypoint (baked) → installed base (packs) → session
injection (per-launch). forestage spawns claude with --append-system-prompt
for persona injection. Open tension: Claude Code config inheritance (full,
isolated, or switchable). For autonomous agents: different bootstrap (no TUI,
task assignment, workspace boundaries).

Cross-ref: orchestrator F18.

### F4: Version Manager Coordination

Brew and self-update are parallel version managers with no coordination.
build.rs generate_version() injects version info at compile time. Should
forestage detect brew install and defer? Or should one channel win? Dual-track
model needs re-validation for Rust artifacts.

Cross-ref: orchestrator F11 (distribution model).

### F5: Distribution Model (Rust)

The TypeScript distribution probe (38 findings) is mostly stale. Portable
findings survive: brew tap pipeline pattern, dual alpha/stable channels,
self-update UX, code signing workflow. Need to re-validate: binary size,
cross-compilation, CI pipeline (currently failing on cargo-deny config).

Cross-ref: orchestrator F11.

### F6: TUI Performance and Edge Cases

Finding-010 validated the architecture but flagged unresolved performance
and completeness questions:
- Performance/timing issues observed under load (F010-e). Potential causes:
  20fps tick rate, no render cache, markdown renderer called per-frame.
- Permission response protocol (send_permission_response) not validated
  against Claude Code's hook system — format is unconfirmed.
- Long conversation behavior (1000+ turns) untested.
- Portrait cell_size calculation may not match ratatui-image's internal
  scaling.

Cross-ref: orchestrator F14 (remaining open questions).

---

## Graveyard

*Tried, ruled out, permanently recorded.*

### G1: pennyfarthing

The predecessor. Per-repo install, never became distributable. Every user
except the developer churned out because install/update was never solved.
Concepts (persona theming, tmux integration) carried forward; implementation
discarded.

### G2: TypeScript + bun compile + Agent SDK

forestage's original implementation stack, inherited from pennyfarthing.
Worked: bun compile produced signed binaries, themes embedded, Agent SDK
provided subprocess management. Problems: 15s first-prompt latency, 63MB
binary, bun compile virtual FS friction (5+ broken features), npm supply
chain exposure (axios incident). Replaced by Rust rewrite (2026-04-01).

Evidence: finding-001-rust-rewrite, docs/security/axios-supply-chain-2026-03-31.md.
