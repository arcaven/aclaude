# forestage Charter

BYOA agent CLI — console environment with persona theming. Spawns `claude`
CLI as a subprocess using the NDJSON streaming protocol. Rust binary, no
SDK dependency.

Follows the kos process: Orient → Ideate → Question → Probe → Harvest → Promote.
Authoritative graph: `_kos/nodes/`.
Cross-repo questions belong in the orchestrator's charter.

Last updated: 2026-04-19 (charter harvest: sessions 020/022/026 + marvel-cold-start fix — agent taxonomy shipped, marvel integration flags, #37 Part A, theme count 118→100)

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

100 theme YAMLs embedded in binary via build.rs code generation at compile
time. Themes available at runtime without filesystem access — binary is
self-contained. Portraits remain external (global cache).

Theme YAMLs are keyed by **character slug** (e.g. `naomi-nagata`,
`paul-atreides`), not by role. Rekeyed in session-022 to support the
agent taxonomy (B7): any persona can fill any role. See B7 for the CLI
surface and the B14 taxonomy in the orc charter.

Evidence: build.rs reads personas/themes/*.yaml and generates Rust source.
Rekey: session-022, commit 2851c83.

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

**Cold-start inject race fix (session-030, PR #55):** the terminal
capability picker's stdio query (DA1, CPR) steals the first post-spawn
keystrokes when forestage is launched under marvel's `send-keys`-based
inject path. When `FORESTAGE_MARVEL=1` (set by marvel's forestage
adapter), the picker skips the stdio query and falls back to env-only
detection. See commit 4df3740.

### B7: Agent Taxonomy — Persona / Identity / Role CLI Flags

CLI flags matching the orc's B14 five-primitive taxonomy:
- `--theme <slug>` — the roster (which fictional universe)
- `--persona <character-slug>` — the costume (which character)
- `--identity <free-form>` — the lens (e.g. "homicide detective")
- `--role <csv>` — the job(s), comma-separated (e.g. "reviewer,troubleshooter")

`persona::Character` struct (src/persona.rs:47) models an individual
character resolved from the theme roster. `resolve_character()` looks up
a persona slug against the embedded theme YAMLs;
`build_full_prompt()` composes the persona + identity + role(s) into
the system-prompt segment passed to claude via `--append-system-prompt`.

Multiple roles are now first-class — a single agent can hold
("reviewer", "troubleshooter") simultaneously. Theme is a roster
(container), not a team (session-022).

Evidence: session-022, commits 2851c83 (theme YAML rekey), f6cb783
(CLI flags). Resolves F7 (persona model). Cross-ref: orc B14,
finding-019-agent-taxonomy, finding-020-agentic-primitives.

### B8: Marvel Integration Flags and `--dangerously-skip-permissions`

Six native CLI flags set by marvel's forestage adapter when launching
agents as part of a team:

- `--name` — agent session name (e.g. "squad-worker-g1-0")
- `--workspace` — marvel workspace
- `--team` — marvel team
- `--socket` — marvel daemon socket path (for heartbeat + comms)
- `--permission-mode` — Claude Code permission mode, passed through to
  the claude subprocess
- `--script` — Lua script path (future: native lua supervisor support)

Plus `--dangerously-skip-permissions` (aliased `--yolo`) from session-
026 (#37 Part A) — maps to Claude Code's flag of the same name, intended
for autonomous agents where no interactive approver exists. Never enable
for interactive sessions you don't fully trust.

`config::MarvelConfig` (src/config.rs:135) captures the marvel-injected
identity. Permission mode is threaded through every claude spawn path.
Identity (persona/identity/role) passes to forestage as native flags;
system prompt passes to claude via `--append-system-prompt`.

Evidence: session-020 (fd3b85d marvel flags, end-to-end tested with
marvel daemon launching 2 forestage agents — Dune theme, reviewer+dev
roles — in tmux panes running Claude Code), session-026 (4c4e28a yolo,
ca52de6 adapter passthrough). F15 role-naming bug (aae-orc-p6b) closed.
Cross-ref: marvel B8 (runtime adapter framework).

---

## Frontier

*Actively open — under exploration, not yet resolved.*

### F1: Two-Audience Problem and UX Parity [partially resolved]

forestage serves human operators (TUI, status bars, persona flair) and
autonomous agents under marvel (fast startup, minimal overhead, programmatic
control). The `--mode forestage|claude` flag provides runtime selection, and
the bridge architecture (B4) validates the split: same subprocess protocol,
different consumers.

**UX parity is existential.** If wrapping Claude Code means a worse Claude
Code, forestage fails regardless of persona theming or config management.
The wrapper must be transparent — additive only, never subtractive. Concrete
risks: Claude Code ships updates frequently — does the subprocess protocol
surface new features immediately, or is there lag? Does subprocess invocation
add latency to tool execution or streaming that degrades interactive use? Do
Claude Code features (slash commands, hooks, MCP servers, permission modes)
pass through cleanly, or does forestage need to reimplement them?

Partial answer (2026-03-18): the subprocess protocol is additive when
configured correctly. All tools, settings, CLAUDE.md, MCP servers, and
skills pass through. TUI-specific slash commands (e.g. /stats) produce no
output — this is a rendering gap, not a capability gap.

Remaining questions: should marvel teams use `--mode claude` (no TUI
overhead) or a third mode (headless NDJSON consumer with no rendering)?
How does the bridge layer serve a marvel sidecar that monitors agent
sessions? Is the SessionMetrics Arc<Mutex<>> the right interface for
external consumers, or does marvel need a different contract? Can
`--input-format stream-json` handle all interactive input patterns
(multi-line, file references, image paste)?

Cross-ref: orchestrator F13 (UX parity), F16 (assumption provenance).

### F2: Theme Bundling vs Runtime Loading

Themes are compiled into the binary (build.rs). Arguments for runtime loading
(pack system, smaller binary, multi-console reuse). Arguments for bundling
(zero-dependency, offline, version coherence). Rust's compile-time embedding
is more reliable than bun compile's virtual FS — makes bundling more
attractive than it was in TypeScript. Possible hybrid: bundle defaults, load
additional from ~/.local/share/forestage/ or via marvel pack resolution.

**Pack extraction angle:** themes are content, not code. Other BYOA consoles
(zclaude, dclaude) could use the same themes — the 118 embedded YAMLs are a
forestage-only silo right now. A pack-based model would load themes at runtime
from `~/.local/share/forestage/packs/themes/` or wherever marvel resolves
them. Questions specific to extraction: does this pack include portraits or
just theme YAML? Is there a "built-in default" set that ships with the binary,
with packs as additive? How does this interact with marvel's 4-scope
resolution (repo → shared → user → system)?

Depends on: F7 (Persona Model) — pack format should encode the correct
theme/persona/role model, not inherit pennyfarthing's role-first binding.

Cross-ref: orchestrator F12 (persona pack), F15 (persona model).

### F3: Session Bootstrap Layering

The session needs to be bootstrapped with the right context — persona,
operating instructions, project rules. Three layers analogous to Docker:

1. **Entrypoint** (baked into binary) — "you are forestage." Core identity,
   persona system, operating instructions. Always present. Not overridable
   without rebuilding.
2. **Installed base** (on disk, updateable) — packs, themes, rules, commands
   installed at `~/.local/share/forestage/` or via marvel. Additive,
   versionable, shared across sessions.
3. **Session injection** (per-launch) — which persona, which project, which
   rules to load, environment overrides, feature flags. Ephemeral,
   session-scoped.

**The Claude Code config conflict:** forestage inherits Claude Code's config
system (`~/.claude/`, `.claude/rules/`, CLAUDE.md, settings.json) via
`settingSources`. Users may want three different behaviors:
- Full inheritance (additive — forestage layers on top of existing setup)
- Isolated (forestage provides its own complete context, ignoring user's
  Claude Code setup)
- Switchable (different behavior for personal use vs. marvel-managed teams)

This is a preference, not a fixed behavior. `settingSources: []` (empty) may
be the isolated mode, but this is unverified. A `--no-inherit` flag is an
option. Per-workspace configuration in marvel would also let teams run with
full isolation while personal sessions inherit.

**For autonomous agents (marvel teams):** The bootstrap model is different.
An agent team member doesn't need TUI instructions or persona flair
introduction — it needs task assignment, tool permissions, workspace
boundaries, communication protocol. How marvel injects session context
(environment variables, generated CLAUDE.md, pack resolution) is unresolved.

Questions: How does forestage's config layer interact with Claude Code's
`settingSources`? Should the entrypoint be a system prompt segment (currently
`preset: 'claude_code', append: personaPrompt`) or something more structured?

Depends on: F2 (Theme Bundling), F7 (Persona Model).
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

**Why this matters beyond forestage itself:** solving distribution unblocks
the full platform. marvel needs forestage as a real workload to manage
(not just source-cloned). director needs forestage instances it can connect.
spectacle/packs need an installed tool to test pack injection against. And RD
itself needs forestage usable in daily work immediately — working from source
is a developer-only path. The pennyfarthing failure mode was: per-repo
install, never distributable, every user except the developer churned.

Three axes to re-validate in Rust: self-updating binary (following Claude
Code's `~/.local/share/` + symlink rotation model), brew formula (user
sovereignty), and dual-track stable/alpha channels (following ThreeDoors'
proven model). Version manager coordination (F4) is the unresolved tension
between these axes.

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

**Patterns adopted from adversarial review of four Rust Claude Code
reimplementations (session-011), already implemented or targets for
future work:**
- State machine: AppStatus enum drives visual feedback and input gating
  (implemented). Observed in srg-claude-code-rust (Apache-2.0).
- Streaming text batching: 45ms/2KB buffer prevents per-token render
  cycles (implemented). Observed in pi_agent_rust's UiStreamDeltaBatcher.
- Render cache with split policy: segment long messages at paragraph
  boundaries to prevent cache bloat. Not yet implemented — F010-e may be
  caused by its absence. Observed in srg-claude-code-rust.
- Incremental markdown parsing: append chunks to renderer as they arrive,
  invalidate render cache. Don't wait for complete response.
- VCR cassette testing: record Claude Code's NDJSON output, replay
  deterministically. Observed in pi_agent_rust. Not yet implemented.

**What the TUI provides that vanilla Claude Code doesn't:**
- Persona theming (character name, styled prompts, portrait overlay)
- Custom status bar with token tracking, cost, permission mode
- Configurable layouts (portrait position, theme colors, tmux integration)
- Marvel-ready: same subprocess protocol works for autonomous agents
- Extension hooks (pre/post tool display, custom overlays)

**Open questions not yet resolved:**
- Does `--include-partial-messages` provide enough granularity for smooth
  streaming text, or do we need a custom tokenizer?
- Can permission prompts be intercepted and re-rendered in forestage's
  TUI, or must they use Claude Code's own prompts?
- Should the TUI be a separate crate (forestage-tui) or inline? The
  reviewed projects split: claw-code-rust separates, claurst separates,
  srg-claude-code-rust inlines.

See: `docs/research/rust-claude-reimplementations-20260410.md` for the
full adversarial review with licensing analysis.
Cross-ref: orchestrator F14 (remaining open questions).

### F7: Persona Model — RESOLVED → B7 (see also orc B14)

Resolved by session-022. The five-primitive taxonomy (persona, theme,
identity, role, process) is now canonical at the orc level (B14). On
the forestage side, B7 captures the CLI surface (`--persona`,
`--identity`, `--role`, multi-role support) and B3 captures the theme-
YAML rekey from role-key to character-slug-key. Theme is a roster, not
a team. Any persona can fill any role. finding-019 + finding-020.

Remaining downstream work lives in bd, not as frontier: pack format for
theme extraction is still F2 (themes as a sideshow pack), and marvel-
side manifest schema for persona+identity per role is tracked on the
marvel side.

Cross-ref: orc B14, F15 RESOLVED → B14.

### F8: Session Maturity — Open Issues [partially resolved]

tmux-cmc core protocol works on macOS + Linux. Session management CLI shipped
(B5). Session-019 added pane-to-existing-session behavior: `session start`
now creates a new pane via `split_pane` when the target session exists, with
`--persona` and `--role` per-pane overrides. Default session name is
configurable (`tmux.default_name`, defaults to `"forestage"`). `--new` flag
forces fresh session with petname. This partially addresses forestage#42
(session affinity) and #17 (pane management).

Remaining open items:

- **Session=team CLI terminology** — current commands use "session" vocabulary;
  the mental model is session=team, pane=agent. CLI term refactoring deferred
  until the behavior is validated.
- **Pane listing** (forestage#17 partial) — `session pane add` works via
  `session start`, but `session pane list` is not yet implemented.
- **One-shot prompt outputs raw JSON instead of text** (forestage#13) — the
  programmatic flow needs its own output rendering path.
- **No integration tests against live tmux on Linux** — all platform bugs
  found by users, not by tests. B13 (platform-specific testing) applies here.

**Shipped since session-019:**
- `NewSessionOptions.start_command` + shell-skip on add-window eliminated
  the send-keys command echo (forestage#22 closed, session-026, commits
  aa12331 + 6c49d6d in forestage, f9ec606 in tmux-cmc).
- forestage#19 regression tests added (1e350a7) after the fix.
- Cold-start inject race (marvel-driven spawn) fixed by skipping the
  picker's stdio query under `FORESTAGE_MARVEL=1` (PR #55, 4df3740) —
  see B6.

The control session architecture question (#15) is resolved (B5: smart
Pattern 1/2 upgrade). The terminal image question is resolved (B6).

Cross-ref: orchestrator F20.

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
