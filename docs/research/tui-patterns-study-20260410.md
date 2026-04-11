# TUI Patterns Study — Lessons from Rust Claude Code Reimplementations

Date: 2026-04-10
Context: Studied four Rust projects for patterns applicable to building
forestage's TUI around Claude Code's NDJSON streaming protocol. Ideas and
architectural patterns only — no code copied from GPL or rider-licensed
projects.

## Pattern 1: State Machine for TUI Mode

**Source:** srg-claude-code-rust (Apache-2.0), claurst (GPL — idea only)

An enum drives all visual feedback and input gating:

```
Connecting → Ready → Thinking → Running → Ready
                  → CommandPending → Ready
                  → Error → Ready
```

- **Connecting:** subprocess spawning, TUI shown but input disabled
- **Ready:** idle, waiting for user input
- **Thinking:** model is reasoning (extended thinking block arriving)
- **Running:** tool execution in progress, streaming text arriving
- **CommandPending:** slash command in flight (spinner)
- **Error:** error banner, offer retry/reset

Input area is enabled only in Ready state. Spinner animates only in
CommandPending. Status bar updates reflect current state. Each NDJSON
event triggers a state transition.

**Why this matters for forestage:** the subprocess produces events
asynchronously. Without a state machine, the TUI would need ad-hoc
checks everywhere for "are we mid-response?" "is a tool running?"
"did the subprocess crash?" The enum centralizes this.

## Pattern 2: Component-Based Rendering

**Source:** srg-claude-code-rust (Apache-2.0)

TUI is decomposed into independent components, each rendering to a
ratatui `Rect`:

```
ui/
  chat_view.rs      — composes sub-components, dispatches by ActiveView
  chat.rs           — scrollable conversation message list
  input.rs          — multi-line input textbox
  footer.rs         — status bar (model, tokens, cost, context %)
  help.rs           — help overlay (captures all input when open)
  markdown.rs       — markdown rendering with syntax highlighting
  message.rs        — single message render with block cache
  tool_call/        — tool use visualization (name, input, output, errors)
```

Layout is computed once per frame, areas passed to sub-components.
No component knows about other components' state.

**For forestage:** add persona-specific components — portrait panel,
persona name/role in status bar, theme-colored borders.

## Pattern 3: Streaming Text Batching

**Source:** pi_agent_rust (MIT+rider — idea only)

Don't render each NDJSON chunk immediately. Buffer for 45ms or 2KB,
then flush all pending text at once. This prevents:
- Per-token re-renders (expensive with markdown parsing)
- Flickering during fast streaming
- TUI falling behind the stream

The batcher runs on a separate timer. When either threshold is hit,
it drains the buffer and sends a single "flush" message to the TUI
update loop.

**For forestage:** Claude Code's `--include-partial-messages` can produce
many small chunks. Batching is load-bearing for smooth rendering.

## Pattern 4: Incremental Markdown Parsing with Cache Invalidation

**Source:** srg-claude-code-rust (Apache-2.0)

Text arrives in chunks. The markdown renderer maintains a running parse
state. Each new chunk is appended to the existing parse, not re-parsed
from scratch. When a chunk arrives:

1. Append to accumulated text
2. Feed new bytes to incremental parser
3. Mark render cache dirty
4. On next frame, re-render only dirty blocks

Long messages are segmented at paragraph boundaries (double newline)
with a soft limit (~1.5KB) and hard limit (~4KB). Code fences are
tracked to avoid splitting inside code blocks.

**For forestage:** Claude Code responses can be very long (full file
contents, large diffs). Without cache segmentation, rendering a
single 50KB message would freeze the TUI.

## Pattern 5: Event Dispatch Architecture

**Source:** srg-claude-code-rust (Apache-2.0), pi_agent_rust (idea only)

NDJSON line → parsed event → dispatch by type → mutate app state →
re-render on next frame.

Event types map directly to NDJSON:

```
system/init      → initialize session state (tools, model, MCP servers)
assistant/text   → append to conversation viewport
assistant/think  → append to collapsible thinking block
assistant/tool   → create tool call panel (name, input, pending status)
user/tool_result → update tool call panel (output, success/error)
rate_limit_event → update status bar (rate limit info)
result/success   → finalize turn (cost, duration, token summary)
```

Each handler is a pure function: takes app state + event, returns
mutated state. No handler triggers re-rendering directly — that's
the frame loop's job.

**For forestage:** this is the core integration point. protocol.rs
already parses NDJSON events. Expand the ClaudeEvent enum to cover
all event types, then write per-event handlers that mutate TUI state.

## Pattern 6: Slash Command Registry

**Source:** claurst (GPL — idea only)

Slash commands are trait objects in a HashMap:

- Name + aliases for dispatch
- Description for /help
- Async execute() with mutable context
- Returns a typed result enum (message, config change, conversation
  reset, exit, etc.)

The TUI maps results to state changes: "message" displays text,
"config change" updates settings, "conversation reset" clears
viewport.

**For forestage:** slash commands control the subprocess:
- `/pause` — send SIGSTOP to subprocess
- `/resume` — send SIGCONT
- `/restart` — kill and respawn subprocess
- `/inject <json>` — send raw NDJSON to subprocess stdin
- `/logs` — toggle subprocess stderr display
- `/persona` — change persona mid-session (restart with new prompt)
- `/model` — change model (restart subprocess with new --model)
- `/cost` — show accumulated cost from result events

## Pattern 7: VCR Cassette Testing

**Source:** pi_agent_rust (MIT+rider — idea only)

Record real subprocess NDJSON output to JSON files. Replay during
tests. Format:

```json
{
  "test_name": "tool_use_with_bash",
  "interactions": [
    { "stdin": {"type":"user","message":"list files"},
      "stdout_lines": [
        {"type":"system","subtype":"init",...},
        {"type":"assistant","message":{"content":[{"type":"tool_use",...}]}},
        {"type":"user","subtype":"tool_result",...},
        {"type":"assistant","message":{"content":[{"type":"text",...}]}},
        {"type":"result","subtype":"success",...}
      ]
    }
  ]
}
```

Three modes: Record (spawn real subprocess, save output), Playback
(load cassette, feed lines on schedule), Auto (use cassette if
exists, else record).

Sensitive data (API keys, session IDs) redacted before saving.

**For forestage:** this enables testing the TUI without spawning Claude
Code. Record a real session once, replay forever. Catches regressions
in event parsing and rendering.

## Pattern 8: Elm Architecture (Alternative to Immediate Mode)

**Source:** pi_agent_rust (MIT+rider — idea only)

Instead of ratatui's immediate-mode rendering (re-render everything
each frame), the Elm Architecture uses:

- **Model:** immutable state struct
- **Update:** `fn update(model, message) -> (model, command)`
- **View:** `fn view(model) -> rendered_ui`

All state transitions are explicit messages. The update function is
pure (given same model + message, always produces same output). This
enables time-travel debugging (replay message sequences) and makes
testing trivial (send messages, assert model state).

pi_agent_rust uses bubbletea (Rust port of Go's Charm library) which
provides textarea, viewport, spinner widgets on top of this pattern.

**Trade-off vs ratatui:**
- Elm: better testability, cleaner state management, higher learning curve
- ratatui: larger ecosystem, more examples, more contributors, familiar
  to Rust developers who've used tui-rs

**For forestage:** ratatui is the safer choice (ecosystem, contributors,
examples). But the Elm Architecture's state management ideas can inform
how we structure ratatui state — even in immediate mode, we can use a
message-passing pattern internally.

## Pattern 9: Performance Configuration

**Source:** pi_agent_rust (MIT+rider — idea only)

Release profile optimizations:
- `opt-level = 3` — full optimization
- `lto = true` — link-time optimization (smaller binary, slower compile)
- `codegen-units = 1` — maximum optimization per function
- `panic = "abort"` — smaller binary (no unwind tables)
- `strip = true` — remove debug symbols

Runtime: jemalloc allocator for 10-20% improvement on allocation-heavy
paths (streaming NDJSON parsing, markdown rendering).

Separate `[profile.perf]` with `lto = "thin"` and `debug = 1` for
benchmarking with symbols.

**For forestage:** adopt the release profile. Consider jemalloc for
streaming-heavy workloads. The perf profile is valuable for
identifying TUI rendering bottlenecks.

## Pattern 10: Strict Clippy Configuration

**Source:** srg-claude-code-rust (Apache-2.0)

```toml
[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
pedantic = { level = "warn", priority = -1 }
correctness = { level = "deny", priority = -1 }
suspicious = { level = "warn", priority = -1 }
perf = { level = "warn", priority = -1 }
```

Denying `unwrap_used` and `panic` forces proper error handling
throughout. This is especially important for a TUI app where a panic
kills the terminal state (raw mode, cursor hidden, etc.).

**For forestage:** adopt this configuration. A panic in the TUI leaves
the terminal in raw mode with no cursor — worse than a crash in a
non-TUI app.

## Synthesis: Recommended Architecture for forestage TUI

```
forestage-tui crate (new)
├── app.rs           — App struct + AppStatus enum (state machine)
├── events/
│   ├── mod.rs       — event dispatch by ClaudeEvent type
│   ├── streaming.rs — text batching + incremental markdown
│   └── tools.rs     — tool call panel state management
├── ui/
│   ├── mod.rs       — frame render dispatch by ActiveView
│   ├── chat.rs      — conversation viewport (scrollable)
│   ├── input.rs     — multi-line input area
│   ├── footer.rs    — status bar (persona, model, tokens, cost)
│   ├── portrait.rs  — persona portrait panel
│   ├── tool_call.rs — tool execution visualization
│   ├── help.rs      — help overlay
│   └── markdown.rs  — incremental markdown renderer
├── commands/
│   ├── mod.rs       — slash command registry + dispatch
│   └── subprocess.rs — /pause, /resume, /restart, /inject
└── testing/
    ├── cassette.rs  — VCR record/playback for NDJSON streams
    └── fixtures/    — recorded cassettes
```

Dependencies (all MIT):
- ratatui — TUI framework
- crossterm — terminal backend
- syntect — syntax highlighting for code blocks
- pulldown-cmark — markdown parsing (incremental)
