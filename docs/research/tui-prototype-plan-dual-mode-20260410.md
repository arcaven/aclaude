# Plan: Prototype TUI — Headless Claude Code with Portrait Overlay

## Context

forestage currently spawns Claude Code with inherited stdio (user gets vanilla
Claude Code TUI) or in NDJSON streaming mode (programmatic, no TUI). This
probe builds a prototype ratatui TUI that wraps Claude Code as a headless
subprocess, rendering its own UI around the NDJSON event stream. The TUI
includes a persona portrait overlay in the upper-right corner with dynamic
size switching via `/persona portrait size [small|medium|large|original]`.

This is a PROBE — working demo quality, not production. The goal is to
verify that the architecture works: bidirectional NDJSON streaming, ratatui
rendering, portrait overlay via ratatui-image, and approximate Claude Code
UX.

### Dual-mode architecture (structural, not scope)

tmux is always the substrate. Both human-operated and marvel-managed agent
sessions run in tmux panes. The architecture accounts for two distinct
consumers of the same Claude Code subprocess:

**Human operator TUI** (this probe builds this): Rich, persona-themed
ratatui interface. Conversation viewport, portrait overlay, input area,
styled status bar. The user interacts directly with Claude Code through
forestage's rendering layer. This is the primary experience for a human
choosing to use forestage instead of vanilla Claude Code.

**Marvel diagnostic view** (this probe enables, does not build): When
marvel manages agent sessions autonomously, a human may attach to a
tmux pane to inspect what an agent is doing. This is not a conversation
interface — it's an inspection port. The operator sees: event log
(tool calls, responses, thinking), session metrics (tokens, cost,
context window), agent state (thinking, tool use, waiting), and manual
controls (pause, resume, inject, signal). Think `kubectl logs` or
`htop` — diagnostic, not interactive. The agent doesn't need a TUI to
operate; the diagnostic view is for the human supervising it.

The bridge pattern makes both possible without either knowing about the
other. This probe builds consumer #1 (TUI) while ensuring consumer #2
(diagnostic) can attach to the same bridge later.

### tmux integration continuity

The existing `session.rs` already writes tmux statusline data via
tmux-cmc during streaming sessions. The bridge must continue this
pattern — `SessionMetrics` updates flow to tmux statusline regardless
of which view (TUI, diagnostic, or headless) is consuming the event
stream. The statusline is a tmux concern, not a TUI concern. forestage
sessions always have a tmux pane; the statusline is always available.

## Design Principles (coloring choices, not adding scope)

The probe makes structural choices that lean toward the dual-mode future
described in F16, F18, and the marvel resource model, without implementing
those modes:

1. **Session bridge is shared infrastructure, not TUI-specific.** The
   subprocess lifecycle, NDJSON parsing, and event channel live at the
   top level (`src/bridge.rs`, `src/protocol_ext.rs`), not inside
   `src/tui/`. Both human TUI and future marvel diagnostic view consume
   the same bridge. The bridge API has no ratatui types.

2. **tmux is always present.** Both human TUI and marvel-managed agents
   run in tmux panes. The bridge integrates with tmux-cmc for statusline
   updates (continuing the pattern already in `session.rs` and
   `statusline.rs`). Statusline writes happen at bridge level — any
   consumer gets tmux status for free.

3. **Status is always emittable.** `SessionMetrics` (tokens, cost,
   context %, tool counts, rate limit status) is defined at bridge
   level and updated by the bridge from event data. Any consumer can
   read it — TUI status bar, tmux statusline, marvel sidecar file, or
   a future diagnostic panel. The TUI reads metrics; it doesn't own them.

4. **Config is watchable.** PortraitSize and other session-scoped config
   live in AppState but are sourced from the config system. The probe
   reads config once at startup. The struct is designed so a future
   watcher (inotify on a marvel-provided sidecar config file) can mutate
   it at runtime without restructuring.

5. **Permission events are surfaced, not swallowed.** When Claude Code
   emits permission-related events in the NDJSON stream, they become
   typed events in the protocol layer. The TUI renders them as prompts.
   A marvel diagnostic view could intercept them and respond
   programmatically (auto-approve, escalate to supervisor, deny by
   policy).

6. **The bridge emits, the view consumes.** The session bridge is a
   producer (mpsc channel of events). The TUI is one consumer. A marvel
   diagnostic view would be another consumer of the same channel.
   The bridge doesn't know what's rendering its events.

## New dependencies (all MIT, no conflicts)

```toml
ratatui = "0.30"
crossterm = "0.29"
ratatui-image = { version = "10.0", default-features = false, features = ["crossterm"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
tokio = { version = "1", features = ["sync", "rt", "macros", "io-util", "process"] }
```

## New files

```
src/
  bridge.rs            — Session: async subprocess lifecycle, bidirectional NDJSON pipes (SHARED)
  protocol_ext.rs      — BridgeEvent enum, parse_bridge_event(), SessionMetrics (SHARED)
  tui/
    mod.rs             — entry point: run_tui(), terminal setup/teardown, event loop
    app.rs             — AppState struct, PortraitSize enum, apply_event(), render fns
    portrait_widget.rs — PortraitWidget wrapping ratatui-image StatefulProtocol
    input.rs           — handle_key(), slash command parser, InputAction enum
    scroll.rs          — ScrollState with auto-scroll
    layout.rs          — compute_layout() → portrait Rect + conversation + input + status
```

**Why this layout:** `bridge.rs` and `protocol_ext.rs` are at `src/` level
because they are TUI-agnostic. A future `src/dashboard/` (marvel diagnostic
view) or a headless `src/agent.rs` (marvel workload, no rendering) would
consume the same bridge and events. The `tui/` module is one view layer.

## Modified files

| File | Change |
|------|--------|
| `Cargo.toml` | Add 5 deps |
| `src/lib.rs` | Add `pub mod bridge;`, `pub mod protocol_ext;`, `pub mod tui;` |
| `src/main.rs` | Add `Commands::Tui` subcommand |

## Step 1: Cargo.toml

Add ratatui, crossterm, ratatui-image, image, tokio to `[dependencies]`.

## Step 2: `src/protocol_ext.rs` — Extended event parsing (SHARED)

New `BridgeEvent` enum wrapping the existing `ClaudeEvent` plus:
- `TextDelta { text }` — from `stream_event` with `content_block_delta`
- `ToolResult { tool_use_id, content }` — from `user` type tool_result events
- `RateLimit { status, resets_at }` — from `rate_limit_event`
- `PermissionRequest { tool, path, description }` — from permission prompt events
- `Core(ClaudeEvent)` — delegates to existing parser

`parse_bridge_event(line) -> Option<BridgeEvent>` tries core parser first,
then handles stream_event/rate_limit_event/user-tool_result/permission.

Also defines `SessionMetrics` — a plain struct aggregating the session's
observable state. Any consumer (TUI, tmux statusline, sidecar file, marvel
control plane) can read it:

```rust
pub struct SessionMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub context_pct: f64,
    pub num_turns: u64,
    pub tool_use_count: u64,
    pub active_tool: Option<String>,
    pub rate_limit_status: Option<String>,
    pub model: String,
    pub session_id: Option<String>,
}
```

`SessionMetrics` is updated by `bridge.rs` from event data. TUI reads it
for the status bar. Future marvel sidecar writes it to a JSON file for the
control plane to poll.

## Step 3: `src/bridge.rs` — Async subprocess session (SHARED)

```rust
pub struct Session {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    event_rx: mpsc::Receiver<BridgeEvent>,
    metrics: Arc<Mutex<SessionMetrics>>,
}
```

`spawn()`: builds command with `claude -p --input-format stream-json
--output-format stream-json --verbose --include-partial-messages
--model X --append-system-prompt Y`, pipes stdin/stdout. Spawns a tokio
task that reads stdout lines via `AsyncBufReadExt`, parses each with
`parse_bridge_event`, updates `SessionMetrics`, sends to `mpsc::channel(256)`.

When `SessionMetrics` updates and a tmux-cmc client is available, the
bridge writes statusline data (continuing the pattern from `session.rs`
line 152-166 and `statusline.rs`). This happens at bridge level so both
TUI and headless/diagnostic modes get tmux statusline for free.

`send_user_message(text)`: writes `{"type":"user","message":{"role":"user","content":"..."}}\n` to stdin.

`metrics()`: returns `Arc<Mutex<SessionMetrics>>` — any consumer can read
the latest metrics without going through the event channel. The TUI reads
this for the status bar. A future marvel sidecar could clone the Arc and
periodically serialize it to a JSON file.

`shutdown()`: kills child process gracefully.

**No ratatui types in this module.** The bridge is a subprocess manager
and event producer. It doesn't know what renders its events. This is the
component that both human TUI and marvel diagnostic mode share.

## Step 4: `src/tui/scroll.rs` — Scroll state

ScrollState with offset, auto_scroll flag. Auto-scroll follows new content.
Arrow keys / PageUp disable auto-scroll. End key re-enables.

## Step 5: `src/tui/app.rs` — State and rendering

**AppState:**
- `messages: Vec<Message>` — conversation history
- `streaming_text: String` — current partial response
- `tool_calls: Vec<ToolCallEntry>` — tool use tracking
- `input_buffer: String` — user input
- `portrait_size: PortraitSize` — Small/Medium/Large/Original
- `portrait_paths: PortraitPaths` — from existing resolve_portrait()
- `metrics: Arc<Mutex<SessionMetrics>>` — shared ref from bridge
- `is_waiting: bool` — between send and first response
- `scroll: ScrollState`
- `pending_permission: Option<PermissionRequest>` — if Claude Code asks

**apply_event(BridgeEvent):** mutates state per event type. Metrics are
updated by the bridge (shared Arc), so the TUI just reads them on each
frame rather than maintaining a separate copy.

**PermissionRequest handling:** when a permission event arrives, the TUI
sets `pending_permission` and renders a prompt. User approves/denies via
the input handler. In the future, marvel could intercept this event at
the bridge level and respond programmatically instead of routing it to
the TUI.

**Render functions:**
- `render_conversation(frame, state, area)` — scrollable Paragraph with
  styled user/assistant messages, streaming cursor, tool call entries
- `render_input(frame, state, area)` — input box with placeholder text
- `render_status(frame, state, area)` — model | tokens | cost | status

## Step 6: `src/tui/portrait_widget.rs` — Image overlay

```rust
pub struct PortraitWidget {
    picker: Picker,
    image_state: Option<Box<dyn StatefulProtocol>>,
    current_path: Option<PathBuf>,
}
```

`new()`: calls `Picker::from_query_stdio()` after raw mode enabled, before
Terminal::new(). Returns None if terminal doesn't support images.

`set_size(size, portrait_paths)`: resolves best path for size, loads image
with `image::open()`, creates protocol state via `picker.new_resize_protocol()`.
Only reloads if path changed.

`render(frame, area)`: renders `StatefulImage::new(None).resize(Resize::Fit(None))`
at the given Rect.

## Step 7: `src/tui/layout.rs` — Layout computation

```
┌─────────────────────────────┬──────────┐
│ CONVERSATION VIEWPORT       │ PORTRAIT │
│ (scrollable)                │ (upper   │
│                             │  right)  │
├─────────────────────────────┴──────────┤
│ > INPUT AREA                            │
├─────────────────────────────────────────┤
│ STATUS BAR                              │
└─────────────────────────────────────────┘
```

Portrait column widths: Small=20, Medium=32, Large=48, Original=min(64, w/3).
Portrait hidden when terminal < 60 cols wide. Input = 3 rows. Status = 1 row.

## Step 8: `src/tui/input.rs` — Key handling and slash commands

`handle_key(event, state) -> InputAction` where InputAction is:
- `SendMessage(text)` — send to subprocess
- `SlashCommand(cmd)` — process locally
- `Quit` — Ctrl+C
- `ScrollUp/ScrollDown/ScrollEnd`
- `None`

Slash commands parsed from input: `/persona portrait size [small|medium|large|original]`.
Invalid commands show brief status message.

## Step 9: `src/tui/mod.rs` — Event loop

```rust
pub async fn run_tui(config, claude_args) -> Result<()>
```

1. Resolve portrait via existing `portrait::resolve_portrait()`
2. `enable_raw_mode()`, then `Picker::from_query_stdio()`, then `Terminal::new()`
3. Init PortraitWidget, set default size Medium
4. Spawn `bridge::Session::spawn()` — shared bridge, not TUI-specific
5. Clone `session.metrics()` Arc for the status bar
6. Spawn crossterm event reader on `spawn_blocking` thread → mpsc channel
7. 20fps tick timer via `tokio::time::interval(50ms)`
8. `tokio::select!` loop over: subprocess events, terminal events, tick
9. On tick: `terminal.draw()` with layout + all render functions
10. Cleanup: shutdown session, disable raw mode, leave alternate screen

The event loop is the only TUI-specific orchestration. The bridge runs
independently. For a marvel diagnostic view, steps 2-3 and 6 would be
different — no portrait, no user input area, but instead: scrollable
event log (tool calls with args/results, text responses, thinking
indicators), metrics panel (tokens, cost, context %, agent state), and
manual controls (pause, inject prompt, send signal). Steps 4-5 and
8-10 would be identical — same bridge, same metrics Arc, same event
channel, same cleanup.

## Step 10: `src/main.rs` — Entry point

Add `Tui` to `Commands` enum:
```rust
/// Launch interactive TUI (prototype)
Tui,
```

Match arm builds a single-threaded tokio runtime and calls `tui::run_tui()`.

## Step 11: `src/lib.rs`

Add `pub mod tui;`.

## Implementation order

1. Cargo.toml (deps, verify compile)
2. protocol_ext.rs (no TUI deps, just parsing)
3. session_bridge.rs (tokio + protocol_ext)
4. scroll.rs (pure data)
5. app.rs (state types + render functions)
6. portrait_widget.rs (ratatui-image integration)
7. layout.rs (layout math)
8. input.rs (key handling + slash commands)
9. mod.rs (event loop assembly)
10. lib.rs + main.rs (wiring)

## Intentionally deferred

- Marvel diagnostic view (the second bridge consumer — enabled by architecture, not built here)
- Multi-line input (single line only)
- Markdown rendering (plain text)
- Tool input/output display (name only, not JSON args)
- Session resume across TUI restarts
- Error recovery / subprocess reconnect
- Config persistence for portrait size
- Mouse interaction
- Paste support
- Thinking block display (collapsed)
- Marvel sidecar metrics export (SessionMetrics → JSON file for control plane polling)

## Verification

```sh
# Build
cd forestage && cargo build

# Run tests
cargo test

# Launch TUI prototype
cargo run -- tui

# In the TUI:
# - Type a message, press Enter → see streaming response
# - /persona portrait size large → portrait resizes
# - /persona portrait size small → portrait resizes
# - Ctrl+C → clean exit
# - Arrow keys → scroll conversation

# Verify portrait renders in WezTerm
# Verify graceful degradation without portrait cache
# Verify subprocess cleanup on exit (no orphan claude processes)
```
