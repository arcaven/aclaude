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
src/tui/
  mod.rs              — entry point: run_tui(), terminal setup/teardown, event loop
  app.rs              — AppState struct, PortraitSize enum, apply_event(), render fns
  session_bridge.rs   — TuiSession: async subprocess spawn, bidirectional NDJSON
  protocol_ext.rs     — TuiEvent enum extending ClaudeEvent with TextDelta, ToolResult, RateLimit
  portrait_widget.rs  — PortraitWidget wrapping ratatui-image StatefulProtocol
  input.rs            — handle_key(), slash command parser, InputAction enum
  scroll.rs           — ScrollState with auto-scroll
  layout.rs           — compute_layout() → portrait Rect + conversation + input + status
```

## Modified files

| File | Change |
|------|--------|
| `Cargo.toml` | Add 5 deps |
| `src/lib.rs` | Add `pub mod tui;` |
| `src/main.rs` | Add `Commands::Tui` subcommand |

## Step 1: Cargo.toml

Add ratatui, crossterm, ratatui-image, image, tokio to `[dependencies]`.

## Step 2: `src/tui/protocol_ext.rs` — Extended event parsing

New `TuiEvent` enum wrapping the existing `ClaudeEvent` plus:
- `TextDelta { text }` — from `stream_event` with `content_block_delta`
- `ToolResult { tool_use_id, content }` — from `user` type tool_result events
- `RateLimit { status, resets_at }` — from `rate_limit_event`
- `Core(ClaudeEvent)` — delegates to existing parser

`parse_tui_event(line) -> Option<TuiEvent>` tries core parser first, then
handles stream_event/rate_limit_event/user-tool_result.

## Step 3: `src/tui/session_bridge.rs` — Async subprocess

```rust
pub struct TuiSession {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    event_rx: mpsc::Receiver<TuiEvent>,
}
```

`spawn()`: builds command with `claude -p --input-format stream-json
--output-format stream-json --verbose --include-partial-messages
--model X --append-system-prompt Y`, pipes stdin/stdout. Spawns a tokio
task that reads stdout lines via `AsyncBufReadExt`, parses each with
`parse_tui_event`, sends to `mpsc::channel(256)`.

`send_user_message(text)`: writes `{"type":"user","message":{"role":"user","content":"..."}}\n` to stdin.

`shutdown()`: kills child process gracefully.

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
- `session_usage: SessionUsage` — token/cost tracking
- `is_waiting: bool` — between send and first response
- `status_text: String` — rate limit notices
- `scroll: ScrollState`
- `model_name: String` — from config

**apply_event(TuiEvent):** mutates state per event type.

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
4. Spawn `TuiSession::spawn()`
5. Spawn crossterm event reader on `spawn_blocking` thread → mpsc channel
6. 20fps tick timer via `tokio::time::interval(50ms)`
7. `tokio::select!` loop over: subprocess events, terminal events, tick
8. On tick: `terminal.draw()` with layout + all render functions
9. Cleanup: shutdown session, disable raw mode, leave alternate screen

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

- Multi-line input (single line only)
- Markdown rendering (plain text)
- Tool input/output display (name only, not JSON args)
- Session resume across TUI restarts
- Error recovery / subprocess reconnect
- Tmux statusline integration from TUI mode
- Config persistence for portrait size
- Mouse interaction
- Paste support
- Thinking block display (collapsed)

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
