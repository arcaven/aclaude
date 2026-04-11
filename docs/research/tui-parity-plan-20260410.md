# Plan: forestage TUI Feature Parity with Claude Code

## Context

The forestage TUI prototype runs Claude Code as a headless subprocess via NDJSON
streaming and renders a custom ratatui interface. It handles basic text streaming,
tool name tracking, input history, tab completion, portrait overlay, and a status
bar. This plan closes the feature gap with vanilla Claude Code while retaining
forestage's portrait feature and dual-mode bridge architecture.

## Reference Architecture Analysis

Patterns drawn from adversarial review of four Rust reimplementations
(`forestage/docs/research/rust-claude-reimplementations-20260410.md`) and
deep source audit of `/tmp/rust-claude-review/`:

**srg-claude-code-rust** (Apache-2.0, B+) — code-borrowable:
- `AppStatus` 6-state enum gates input and drives rendering
- Two-layer render cache: width-independent content + width-dependent borders
- `CacheSplitPolicy` (soft 1.5KB / hard 4KB) splits at paragraph boundaries
- LRU eviction via atomic `CACHE_ACCESS_TICK` counter
- Panic-safe markdown: `catch_unwind()` + plain text fallback
- `InlinePermission` with oneshot response channel back to bridge
- `LayoutInvalidation` enum for targeted cache invalidation

**pi_agent_rust** (MIT+rider, A-) — study patterns only:
- `UiStreamDeltaBatcher`: 45ms/2KB hybrid threshold, coalesces consecutive
  same-kind deltas into single flush. Tool events bypass batcher (immediate).
- Two-level cache: per-message cache + conversation prefix cache. During
  streaming, only tail (current response) re-renders; prefix is frozen.
- `streaming_needs_markdown_renderer()` fast-path: byte scan for markup
  characters, skip renderer for plain text.
- `RenderBuffers` struct: reusable `String` allocations per frame region.
- VCR cassettes: record NDJSON streams, auto-redact API keys, binary-safe.

**claurst** (GPL-3.0, A-) — ideas only, no code:
- `PermissionDialogKind` per tool type: Bash gets "allow prefix*" option,
  FileWrite gets "project-level" scope, FileRead gets 3 options.
- `DisplayMessage` enum: real messages + synthetic `SystemAnnotation`s
  ("Context compacted here") injected without modifying history.
- `RenderContext` struct: carries width, highlight flags, tool name mappings
  to keep rendering functions pure.
- Virtual list: only renders visible items for long conversations.
- Notification banners with auto-fade for transient status messages.

**Licensing discipline**: forestage is MIT. Never copy GPL code (claurst).
Study pi_agent_rust but don't copy (rider complexity). srg-claude-code-rust
(Apache-2.0) and claw-code-rust (MIT) are safe to reference with attribution.

---

## Phase 1: Protocol Foundation — Full Event Vocabulary and Turn State

**What it adds**: Tool calls show streaming input JSON with elapsed-time spinner.
Completed tools show one-line preview. `system/init` populates session metadata.
Token counts use actual context window from init. Text batching eliminates
per-token re-renders.

### State machine (validated by srg-claude-code-rust AppStatus)

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppStatus {
    Connecting,      // waiting for system/init
    Ready,           // idle, accepting input
    Thinking,        // assistant turn started, no content yet
    Streaming,       // text_delta arriving
    ToolRunning,     // tool_use in progress
    Error,           // subprocess died, rate limit
}
```

Input gated by status — only `Ready` accepts typed input. Status drives
spinner, placeholder text, and status bar indicator.

### Turn-based conversation model

Replace flat `Vec<Message>` with structured turns. Includes synthetic
annotation support (pattern from claurst DisplayMessage):

```rust
pub enum ConversationItem {
    UserMessage { text: String },
    AssistantTurn {
        blocks: Vec<TurnBlock>,
        is_active: bool,
    },
    SystemNotice { text: String },  // compact markers, auth notices
}

pub enum TurnBlock {
    Text { content: String, is_streaming: bool },
    ToolCall(ToolCallItem),
    Thinking { content: String, is_streaming: bool, is_expanded: bool },
}

pub struct ToolCallItem {
    pub id: String,
    pub name: String,
    pub input_json: String,
    pub result_preview: String,
    pub status: ToolStatus,
    pub started_at: Instant,
    pub is_expanded: bool,
    pub cached_render: Option<Vec<Line<'static>>>,  // phase 2 cache
}
```

### Render context (pattern from claurst RenderContext)

```rust
pub struct RenderCtx {
    pub width: u16,
    pub show_thinking: bool,
    pub tools_collapsed: bool,
}
```

Passed to all render functions. Keeps rendering pure — no reaching into
global state.

### Stateful NDJSON parser

`parse_bridge_event` is currently stateless. The protocol requires tracking
open tool calls across events (`content_block_stop` doesn't carry tool_use_id).
Introduce `BridgeParser`:

```rust
pub struct BridgeParser {
    pending_tool: Option<(String, String)>,  // (id, name)
    pending_thinking: bool,
}

impl BridgeParser {
    pub fn parse(&mut self, line: &str) -> Option<BridgeEvent> { ... }
}
```

New `BridgeEvent` variants:
- `SessionInit { session_id, permission_mode, available_slash_commands, context_window_size, model, version }`
- `ToolCallStart { id, name }`
- `ToolInputDelta { partial_json }`
- `ToolCallStop`
- `ThinkingStart`, `ThinkingDelta { text }`, `ThinkingStop`
- `MessageStart`, `MessageStop`

### Text batching (pattern from pi_agent_rust UiStreamDeltaBatcher)

Buffer `TextDelta` events for 45ms or 2KB before flushing to conversation
model. Coalesce consecutive text deltas into a single string. Tool events
bypass the batcher — they flush immediately (tool state transitions must
be visible without latency).

```rust
struct TextBatcher {
    buffer: String,
    last_flush: Instant,
}

impl TextBatcher {
    fn push(&mut self, text: &str) -> Option<String> {
        self.buffer.push_str(text);
        if self.buffer.len() >= 2048
            || self.last_flush.elapsed() >= Duration::from_millis(45)
        {
            self.last_flush = Instant::now();
            Some(std::mem::take(&mut self.buffer))
        } else {
            None
        }
    }
    fn flush(&mut self) -> Option<String> { ... }
}
```

Integrated into the event loop's bridge event arm.

### Files modified

- `protocol_ext.rs` — BridgeParser struct, new BridgeEvent variants,
  SessionMetrics gains `context_window_size`, `permission_mode`,
  `available_slash_commands`
- `bridge.rs` — reader task uses BridgeParser, add `--include-hook-events`
  to subprocess args
- `tui/app.rs` — AppStatus enum, ConversationItem model, RenderCtx,
  rewrite apply_event and render_conversation
- `tui/mod.rs` — TextBatcher in event loop, status-gated input

### Events consumed

`content_block_start`, `content_block_delta` (text_delta, input_json_delta,
thinking_delta), `content_block_stop`, `message_start`, `message_delta`,
`message_stop`, `system/init`.

---

## Phase 2: Tool Call Rendering and Diff Display

**What it adds**: Edit tool calls render as unified diffs (green/red +/- lines).
Read shows file content preview. Bash shows command and output. Write shows
path. Completed tools cached for 20fps rendering efficiency.

### Two-layer render cache (pattern from srg-claude-code-rust)

Tool call rendering splits into width-independent content (cached on
`ToolCallItem`) and width-dependent presentation (computed at render time).
Cache is set when status transitions to `Complete` and invalidated only when
`is_expanded` toggles.

```rust
// Layer 1: cached on ToolCallItem.cached_render (width-independent)
fn render_tool_content(item: &ToolCallItem) -> Vec<Line<'static>>

// Layer 2: applied at render time
fn render_tool_with_frame(content: &[Line], width: u16, spinner_frame: u64) -> Vec<Line<'static>>
```

### New file: `src/tui/diff.rs`

```rust
pub fn render_tool_call(item: &ToolCallItem) -> Vec<Line<'static>>
pub fn render_edit_diff(file: &str, old: &str, new: &str) -> Vec<Line<'static>>
pub fn render_tool_result(name: &str, result: &str, expanded: bool) -> Vec<Line<'static>>
```

Uses `similar` crate (`TextDiff::from_lines`). 3 lines of context around
changes. Truncate with `[... N more lines ...]`.

### Tool-specific renderers (validated by all three projects)

All three projects dispatch rendering by tool name:
- `Edit` → header `~ file_path`, body = unified diff of old_string → new_string
- `Read` → header `  file_path`, body = first 10 lines (expandable)
- `Write` → header `+ file_path`, body = first 5 lines
- `Bash` → header `$ command`, body = first 10 lines of output (expandable)
- `Grep`/`Glob` → header with pattern, body = file list preview
- Other → raw input_json truncated to 120 chars

Max visible output lines: 12 (constant from srg-claude-code-rust). Expandable
via Ctrl+O toggling `is_expanded`.

### Files

- `tui/diff.rs` (new)
- `tui/app.rs` (render_conversation dispatches to diff.rs for tool blocks)
- `tui/mod.rs` (pub mod diff)
- `Cargo.toml` (`similar = { version = "2", features = ["text"] }`)

---

## Phase 3: Permission Mode and Approval Dialog

**What it adds**: Shift+Tab cycles permission mode. Status bar shows current
mode. Permission hook events render as inline approval dialogs. Press a/d to
allow/deny.

### Tool-specific permission options (pattern from claurst PermissionDialogKind)

Different tools get different option sets:

```rust
pub enum PermissionPrompt {
    Bash {
        command: String,
        options: Vec<PermissionOption>,  // allow once, session, prefix*, deny
    },
    FileEdit {
        path: String,
        options: Vec<PermissionOption>,  // allow once, session, project, deny
    },
    FileRead {
        path: String,
        options: Vec<PermissionOption>,  // allow once, session, deny
    },
    Generic {
        tool: String,
        description: String,
        options: Vec<PermissionOption>,
    },
}
```

### Oneshot response channel (pattern from srg-claude-code-rust)

Permission responses flow back to the bridge via oneshot channel, not
through the main event loop. The bridge spawns a response handler that
writes the approval/denial to subprocess stdin.

### Permission mode cycling

```rust
pub enum PermissionMode {
    Default, AcceptEdits, Plan, Auto, Bypass,
}
```

Populated from `SessionInit.permission_mode`. Shift+Tab cycles. Status bar
shows `[mode]` with color (green default, yellow acceptEdits/plan, red bypass).

### Layout

`TuiLayout` gains `permission_prompt: Rect` — 5 rows above input when active.
Permission prompt blocks normal input (checked in handle_key before character
input). Portrait unaffected.

### Files

- `tui/app.rs` (PermissionMode, PermissionPrompt, render_permission)
- `tui/input.rs` (CyclePermissionMode, PermissionAllow/Deny, input gating)
- `tui/layout.rs` (permission_prompt rect)
- `tui/mod.rs` (handle permission actions)
- `bridge.rs` (send_permission_response)
- `protocol_ext.rs` (parse hook_event)

---

## Phase 4: Slash Commands and Dynamic Auto-Complete

**What it adds**: All major slash commands. Dynamic command list from
system/init. @ file path completion. Unknown commands forwarded to Claude Code.

### Slash command dispatch

Local commands handled by forestage:
- `/exit` — quit (already done)
- `/login` — auth info (already done)
- `/clear` — clear conversation display
- `/help` — show available commands and keybindings
- `/cost` — show session cost from metrics
- `/persona` — portrait size (already done)

Forwarded to Claude Code as user messages:
- `/compact`, `/model`, `/init`, `/review`, `/bug`, `/stats`, `/doctor`
- Any MCP/skill commands from `available_slash_commands`

```rust
pub enum SlashCmd {
    Exit, Login, Clear, Help, Cost,
    PortraitSize(String),
    ForwardToAgent(String),  // send as user message
    Unknown(String),
}
```

### Dynamic tab completion

`tab_complete` takes `available_slash_commands: &[String]` from AppState
(populated by SessionInit). Merges with static local commands.

### @ file path completion

When input contains `@` followed by partial text, Tab completes file paths
using `std::fs::read_dir` on the current directory. Insert completed path
after `@`.

### Transient notifications (pattern from claurst)

Status messages auto-clear after 5 seconds instead of persisting until the
next event. Track `status_message_at: Option<Instant>` in AppState, clear
when elapsed > 5s.

### Files

- `tui/input.rs` (new SlashCmd variants, @ completion, dynamic commands)
- `tui/app.rs` (available_slash_commands, notification timeout)
- `tui/mod.rs` (handle Clear/Help/Cost/Forward)

---

## Phase 5: Keyboard Shortcuts and Cursor-Positioned Input

**What it adds**: Full line editing (Ctrl+A/E/W/U), cursor positioning within
the input buffer, mouse wheel scroll, Ctrl+G for external editor.

### InputState (replaces bare String)

All three projects track cursor position for mid-line editing:

```rust
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
}

impl InputState {
    pub fn insert(&mut self, c: char) { ... }
    pub fn delete_back(&mut self) { ... }
    pub fn delete_word_back(&mut self) { ... }  // Ctrl+W
    pub fn clear(&mut self) { ... }              // Ctrl+U
    pub fn home(&mut self) { ... }               // Ctrl+A
    pub fn end(&mut self) { ... }                // Ctrl+E
}
```

### Mouse capture

Enable `crossterm::event::EnableMouseCapture` during terminal setup.
`Event::Mouse(ScrollUp/Down)` → scroll conversation.

### External editor (Ctrl+G)

Suspend TUI → write buffer to tempfile → launch `$EDITOR` → read result →
resume TUI. Uses `tempfile` (already a dev-dep).

### New keyboard shortcuts

| Key | Action |
|-----|--------|
| Ctrl+A | Cursor to start of line |
| Ctrl+E | Cursor to end of line |
| Ctrl+W | Delete word backward |
| Ctrl+U | Clear entire line |
| Ctrl+G | Open external editor |
| Alt+T | Toggle thinking display |
| Mouse wheel | Scroll conversation |

### Files

- `tui/input.rs` (InputState struct, cursor ops, new shortcuts)
- `tui/app.rs` (use InputState, show_thinking flag)
- `tui/mod.rs` (mouse capture setup, editor suspend/resume)

---

## Phase 6: Thinking Block Display

**What it adds**: Thinking content as collapsible panels. Collapsed by
default, Alt+T toggles. Thinking effort in status bar.

### Rendering

Collapsed:
```
▸ Thinking (2.1k chars)
```

Expanded:
```
┌─ Thinking ──────────────────────┐
│ Let me reason through this...    │
│ First, I need to consider...     │
└──────────────────────────────────┘
```

### Protocol

Consumed from Phase 1's ThinkingStart/Delta/Stop events. `SessionMetrics`
gains `thinking_chars: u64` for status bar display.

### Files

- `tui/app.rs` (thinking block rendering, show_thinking toggle)
- `protocol_ext.rs` (thinking_chars metric)

---

## Phase 7: Markdown Rendering

**What it adds**: Assistant text with styled markdown — bold, italic, inline
code, fenced code blocks, headers, bullet lists.

### Panic-safe rendering (pattern from srg-claude-code-rust)

```rust
pub fn render_markdown_safe(text: &str) -> Vec<Line<'static>> {
    std::panic::catch_unwind(|| render_markdown(text))
        .unwrap_or_else(|_| plain_text_fallback(text))
}
```

### Fast-path check (pattern from pi_agent_rust)

```rust
fn needs_markdown_renderer(text: &str) -> bool {
    // Quick byte scan for `, *, [, #, -, > — skip renderer for plain text
}
```

Skip the markdown state machine for messages that are pure prose with no
markup characters. Returns false for ~40% of assistant responses.

### New file: `src/tui/markdown.rs`

Minimal line-by-line state machine. No external crate. States: `Normal`,
`InCodeBlock { lang }`. Detects: fenced code blocks, headers, bullets,
inline bold/code.

### Streaming text cache (pattern from pi_agent_rust prefix cache)

Cache rendered output per committed text block. Only re-render the
currently streaming block each frame. This is the conversation prefix
cache: all finalized messages cached, only tail re-rendered.

### Files

- `tui/markdown.rs` (new)
- `tui/mod.rs` (pub mod markdown)
- `tui/app.rs` (use render_markdown_safe in render_conversation)

---

## Phase 8: Transcript Mode and Diagnostics Placeholder

**What it adds**: Ctrl+O cycles normal → transcript → focus. LSP diagnostics
placeholder after file edits.

### Transcript mode

Leave alternate screen, print full conversation as plain text to terminal's
native scrollback buffer. Re-enter alternate screen on next Ctrl+O.

### Focus mode

`TuiLayout` gains `focus_mode: bool`. Hides status bar and input borders.
Portrait retained.

### Diagnostics placeholder

`ToolCallItem` gains `diagnostics: Vec<DiagnosticEntry>` (empty for now).
After Edit calls, render `[diagnostics: none]` as collapsed section.

### Files

- `tui/mod.rs` (transcript cycling)
- `tui/layout.rs` (focus_mode)
- `tui/app.rs` (TranscriptMode enum, diagnostics struct)

---

## Testing Strategy

### VCR cassette testing (pattern from pi_agent_rust)

Record real Claude Code NDJSON output to files in `tests/cassettes/`.
Feed recorded lines through `BridgeParser` in tests. Enables deterministic
testing of event parsing and state mutation without a live subprocess.

Cassette scenarios:
- Basic conversation (text streaming)
- Tool call with Edit diff
- Permission prompt (hook_event)
- Thinking blocks
- Rate limit event
- system/init with full metadata

Auto-redact: session IDs, any values matching API key patterns.

### Snapshot testing

Use existing `insta` for render output snapshots. Feed known AppState into
render functions, capture ratatui Buffer, snapshot.

---

## Dependency Changes

| Crate | Phase | License | Notes |
|-------|-------|---------|-------|
| `similar = "2"` | 2 | MIT | Already transitive via insta |
| `tempfile = "3"` | 5 | MIT/Apache-2.0 | Already dev-dep |

No new crate introductions.

---

## Portrait Invariant

Every phase adds fields to `TuiLayout` — none remove or zero out `portrait`.
New layout variants (permission_prompt, focus_mode) are additive. Portrait
overlay renders last in the draw call, on top of conversation content.

---

## Verification

After each phase:

```sh
cd forestage && cargo build && cargo test && cargo clippy -- -D warnings && cargo +nightly fmt --all -- --check
```

Manual testing per phase:
1. Type message → verify tool spinner with elapsed time, status transitions
2. Ask Claude to edit a file → verify unified diff with +/- coloring
3. Shift+Tab → verify permission mode cycles, status bar updates
4. /help, /clear, /cost, Tab on /per → verify commands and completion
5. Ctrl+A, Ctrl+E, Ctrl+W, mouse scroll → verify cursor and editing
6. Alt+T → verify thinking blocks toggle
7. Verify markdown styling in assistant responses (bold, code blocks)
8. Ctrl+O → verify transcript mode cycling
