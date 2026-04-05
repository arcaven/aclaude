# aclaude Session Management

## Overview

aclaude uses tmux as its session substrate and tmux-cmc (control mode client)
to manage sessions programmatically. All tmux interaction is wrapped behind
`aclaude session` subcommands — users don't need to know tmux commands.

For details on tmux control mode patterns and capabilities, see:
[tmux-cmc control mode architecture](../../tmux-cmc/docs/control-mode-architecture.md)

## Architecture

aclaude uses a **smart default** approach — start simple, upgrade when needed:

**Single session (Pattern 1):** Control mode attaches directly to the user's
session. No extra sessions. No overhead. The control mode client and the
user's terminal client coexist on the same session.

```
socket "ac"
└── aclaude-happy-tiger
    └── clients: [control-mode, terminal]
```

**Multiple sessions (Pattern 2):** When a second session is created, aclaude
upgrades to a dedicated `_ctrl` session (one per socket). The control
connection persists through session lifecycle — shift change, rolling
restart, creating and destroying sessions.

```
socket "ac"
├── _ctrl (running cat, control mode attached)
├── aclaude-happy-tiger (user session)
└── aclaude-calm-falcon (second session)
```

The upgrade is transparent — the user doesn't think about control sessions.
`session list` and `session status` hide `_ctrl` by default (`--all` to show).

See [tmux-cmc control mode architecture](../../tmux-cmc/docs/control-mode-architecture.md)
for the full pattern catalog and tmux capabilities.

## CLI Commands

```bash
# Start a session (default: aclaude-{petname})
aclaude session start                    # attach after creating
aclaude session start --no-attach        # create without attaching
aclaude session start -t my-project      # named session

# List sessions
aclaude session list                     # user sessions only
aclaude session list --names             # names only (scriptable)
aclaude session list --all               # include control sessions

# Session status
aclaude session status                   # table with windows, created, state
aclaude session status --all             # include control sessions

# Attach to a session
aclaude session attach                   # auto-selects if only one
aclaude session attach -t my-project     # by name

# Stop a session
aclaude session stop                     # auto-selects if only one
aclaude session stop -t my-project       # by name
aclaude session stop --all               # kill entire tmux server
```

## Session Naming

Default names are generated as `aclaude-{adjective}-{animal}` using a
built-in petname generator (50 adjectives × 50 animals). Examples:
`aclaude-happy-tiger`, `aclaude-calm-falcon`, `aclaude-vivid-jackal`.

Custom names via `-t`: `aclaude session start -t my-project`.

Control sessions are prefixed with `_ctrl-` and hidden from default
listings. They're visible with `--all`.

## Socket

All sessions run on a single tmux socket configured in `config.tmux.socket`
(default: `ac`). The socket name is an implementation detail — users interact
through `aclaude session` commands, not `tmux -L ac`.

## Planned Changes

- **Shared control session (#15):** One `_ctrl` per socket instead of one
  per user session. Single tmux-cmc connection manages all sessions.
- **Pane management (#17):** `aclaude session pane add/list` for multi-pane
  layouts within sessions.
- **Session dashboard:** Replace `cat` in the control session with a live
  status display (idea: session-dashboard.md in aae-orc).
