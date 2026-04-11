# forestage Session Management

## Overview

forestage uses tmux as its session substrate and tmux-cmc (control mode client)
to manage sessions programmatically. All tmux interaction is wrapped behind
`forestage session` subcommands — users don't need to know tmux commands.

For details on tmux control mode patterns and capabilities, see:
[tmux-cmc control mode architecture](../../tmux-cmc/docs/control-mode-architecture.md)

## Architecture

forestage uses a **smart default** approach — start simple, upgrade when needed:

**Single session (Pattern 1):** Control mode attaches directly to the user's
session. No extra sessions. No overhead. The control mode client and the
user's terminal client coexist on the same session.

```
socket "ac"
└── forestage-happy-tiger
    └── clients: [control-mode, terminal]
```

**Multiple sessions (Pattern 2):** When a second session is created, forestage
upgrades to a dedicated `_ctrl` session (one per socket). The control
connection persists through session lifecycle — shift change, rolling
restart, creating and destroying sessions.

```
socket "ac"
├── _ctrl (running cat, control mode attached)
├── forestage-happy-tiger (user session)
└── forestage-calm-falcon (second session)
```

The upgrade is transparent — the user doesn't think about control sessions.
`session list` and `session status` hide `_ctrl` by default (`--all` to show).

See [tmux-cmc control mode architecture](../../tmux-cmc/docs/control-mode-architecture.md)
for the full pattern catalog and tmux capabilities.

## CLI Commands

```bash
# Start a session (default: forestage-{petname})
forestage session start                    # attach after creating
forestage session start --no-attach        # create without attaching
forestage session start -t my-project      # named session

# List sessions
forestage session list                     # user sessions only
forestage session list --names             # names only (scriptable)
forestage session list --all               # include control sessions

# Session status
forestage session status                   # table with windows, created, state
forestage session status --all             # include control sessions

# Attach to a session
forestage session attach                   # auto-selects if only one
forestage session attach -t my-project     # by name

# Stop a session
forestage session stop                     # auto-selects if only one
forestage session stop -t my-project       # by name
forestage session stop --all               # kill entire tmux server
```

## Session Naming

Default names are generated as `forestage-{adjective}-{animal}` using a
built-in petname generator (50 adjectives × 50 animals). Examples:
`forestage-happy-tiger`, `forestage-calm-falcon`, `forestage-vivid-jackal`.

Custom names via `-t`: `forestage session start -t my-project`.

Control sessions are prefixed with `_ctrl-` and hidden from default
listings. They're visible with `--all`.

## Socket

All sessions run on a single tmux socket configured in `config.tmux.socket`
(default: `ac`). The socket name is an implementation detail — users interact
through `forestage session` commands, not `tmux -L ac`.

## Planned Changes

- **Shared control session (#15):** One `_ctrl` per socket instead of one
  per user session. Single tmux-cmc connection manages all sessions.
- **Pane management (#17):** `forestage session pane add/list` for multi-pane
  layouts within sessions.
- **Session dashboard:** Replace `cat` in the control session with a live
  status display (idea: session-dashboard.md in aae-orc).
