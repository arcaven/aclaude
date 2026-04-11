# Agent SDK Pass-Through Analysis

date: 2026-03-18
question: can forestage be additive to Claude Code, or does wrapping via the Agent SDK lose features?
relates-to: F13 (UX Parity and Enhancement vs Vanilla Claude Code)

## Summary

The Agent SDK is a coordination layer over the Claude Code subprocess, not
a reimplementation. forestage inherits nearly all Claude Code capabilities
by default — but only if configured correctly. The critical finding: the
SDK defaults to **isolated mode** (no settings loaded) unless you explicitly
opt in via `settingSources`.

## What passes through (inherited for free)

- All built-in tools (Read, Edit, Bash, Glob, Grep, Write, Agent, etc.)
- Authentication (OAuth, API key, Bedrock, Vertex)
- Model inference and context management
- CLAUDE.md loading — **requires `settingSources: ['project', 'user', 'local']`**
- Project and user settings (.claude/settings.json, settings.local.json)
- MCP servers (stdio/sse/http managed by subprocess)
- Tool schemas and permission enforcement
- Session persistence and resume
- Context compaction
- File checkpointing (opt-in via `enableFileCheckpointing`)

## What forestage adds (the value layer)

- Persona theming (100 theme rosters, immersion levels)
- TOML config with 5-layer merge chain
- tmux session management and statusline
- Context window usage tracking and visualization
- In-process JS hooks (replacing shell-based hooks)
- Self-updating binary distribution

## What the SDK does NOT pass through

| Feature | Impact | Mitigation |
|---------|--------|------------|
| Slash command invocation | Commands enumerable but not invocable via SDK message flow | Parse and route manually, or accept limitation |
| Shell-based hooks (.claude/settings) | User-configured shell hooks don't fire | forestage's JS hooks replace these; document the difference |
| Interactive CLI commands (/clear, /continue) | CLI-specific UX not exposed | Implement equivalents (forestage already has /quit, /usage) |
| Interactive permission dialogs | Allow/Deny/Always Allow UI not exposed | `permissionMode: 'default'` delegates to subprocess; `canUseTool()` callback available for custom logic |
| Real-time context window display | Context size only in final SDKResultMessage | Track per-turn via SDKAssistantMessage.message.usage (already implemented) |
| Error recovery UX | CLI shows user-friendly dialogs; SDK returns error subtypes | Must present authentication_failed, billing_error, rate_limit, etc. |

## SDK query() options surface (50+ fields)

Key options forestage should expose or consider:

| Option | Current state | Action |
|--------|--------------|--------|
| `settingSources` | **Was missing** — fixed to `['project', 'user', 'local']` | Critical fix: without this, CLAUDE.md and settings don't load |
| `systemPrompt` | Set to persona prompt | Could use `{ type: 'preset', preset: 'claude_code', append: personaPrompt }` to layer on top of Claude Code's built-in prompt |
| `permissionMode` | 'default' | Could expose in TOML config |
| `maxBudgetUsd` | Not set | Could expose in TOML config for cost control |
| `enableFileCheckpointing` | Not set | Enables rewind; worth enabling |
| `sandbox` | Not configured | Could expose for security-conscious users |
| `additionalDirectories` | Not set | Could expose in config |
| `agents` | Not set | Could define custom subagents programmatically |
| `mcpServers` | Not set (inherited from Claude Code) | Could add forestage-specific MCP servers |
| `maxThinkingTokens` | Not set | Could expose in config |
| `betas` | Not set | Could enable 1M context (`context-1m-2025-08-07`) |

## Hooks available (12 events)

1. PreToolUse — before tool execution (can intercept/modify)
2. PostToolUse — after successful tool execution
3. PostToolUseFailure — after tool failure
4. Notification — system notifications
5. UserPromptSubmit — prompt submission
6. SessionStart — session init (startup, resume, clear, compact)
7. SessionEnd — session termination (with reason)
8. Stop — stop state change
9. SubagentStart / SubagentStop — subagent lifecycle
10. PreCompact — before context compaction
11. PermissionRequest — permission prompt (can allow/deny/ask)

forestage currently uses PreToolUse, PostToolUse, PostToolUseFailure,
SessionStart, and SessionEnd. The others are available for future use.

## Key finding: additive by default

With `settingSources` set correctly, forestage is **additive by default**.
Claude Code's full configuration loads (CLAUDE.md, settings, MCP servers),
and forestage's persona/config/hooks layer on top. Users get everything
vanilla Claude Code provides, plus forestage's enhancements.

The SDK is designed for this pattern — it's a wrapper API, not a fork.
The subprocess is real Claude Code running with real tools. forestage's
value is in the configuration, theming, and operational layer around it.

## systemPrompt architecture — RESOLVED

**Problem:** forestage was replacing Claude Code's entire system prompt
with the persona prompt. This lost all built-in tool instructions,
safety guidelines, and capabilities. Users got a worse Claude Code.

**Fix:** use the SDK's preset system prompt with append:
```typescript
systemPrompt: {
  type: 'preset',
  preset: 'claude_code',
  append: personaSystemPrompt
}
```

This layers the persona on top of Claude Code's own prompt. The user
gets everything vanilla Claude Code provides, plus the persona theming.
With immersion "none", the preset is used with no append — identical
to vanilla Claude Code.

**Finding:** this was silently degrading forestage from day one. The
persona prompt included "You are a software engineering assistant"
as a poor substitute for Claude Code's full system prompt. The preset
approach is the correct architecture for any wrapper.

## Findings from end-to-end testing (2026-03-18)

### TUI-specific commands don't produce output via SDK

Claude Code's `/stats` renders a visual panel in its terminal UI. Through
the SDK, the output comes as a text message stream — TUI-specific commands
produce no visible output. `/stats` returns empty. Same likely applies to
other visual commands. The SDK surfaces text and tool events, not terminal
renders.

Affected commands (suspected): `/stats`, possibly `/model` display,
any command that renders a panel or dialog rather than text output.

Commands that work via SDK: `/compact`, `/clear`, and any command that
produces text output or triggers tool use rather than visual rendering.

### Inline shell input vs Claude Code's pinned TUI

Claude Code pins a text input box ~5 lines from the bottom of the terminal,
with bordered panels, a statusline below, and output scrolling above the
input. This is the proprietary CLI's terminal rendering — the SDK provides
a message stream, not a terminal UI.

forestage uses Node.js `readline` — traditional shell-style inline input
where prompts and responses are mixed sequentially. This works but feels
less polished than Claude Code's TUI. It's the most visible UX difference
between forestage and vanilla Claude Code.

Building a custom TUI (e.g. with ink, blessed, or raw ANSI) is feasible
and could differentiate forestage — our TUI could include context bars,
persona info, portrait display, and other things Claude Code doesn't show.
This is a future probe, not a distribution-blocking gap.

### No markdown rendering

Claude Code renders markdown inline — headers, code blocks with syntax
highlighting, lists, bold/italic, tables. The SDK streams raw text deltas
from the Anthropic API. Claude Code's TUI handles rendering client-side.
forestage writes raw markdown to stdout via `process.stdout.write()`.

This is part of the TUI gap (F14). Options for rendering:
- **marked-terminal** — Node.js markdown-to-ANSI renderer
- **cli-markdown** — lightweight alternative
- **glow** — shell out to the Go binary if installed, fall back to raw
- **Custom ANSI renderer** — if building a TUI anyway, render inline

Markdown rendering is visible and frequent — every response contains
markdown. This is likely the second most noticeable UX gap after the
input box (F14), and arguably more impactful since it affects every
response, not just input.

### Session works end-to-end

With the fixes applied (settingSources, preset system prompt, executable
path, embedded themes/defaults), a full session works:
- CLAUDE.md, SOUL.md, rules all load
- Memory and skills are available
- Persona theming active (Trillian/Hitchhiker's Guide)
- Claude Code slash commands pass through
- Token usage tracking works
- Context window percentage updates in statusline
- Error handling catches SDK failures without crashing

## References

- Agent SDK types: `node_modules/@anthropic-ai/claude-agent-sdk/entrypoints/sdk/`
  - `coreTypes.d.ts` — message types, hooks, permission modes
  - `runtimeTypes.d.ts` — Options, Query interface
  - `controlTypes.d.ts` — internal control protocol
- Prior research: `docs/research/cc-vs-sdk-20260316.md`
- Charter: F13 (UX Parity and Enhancement)
