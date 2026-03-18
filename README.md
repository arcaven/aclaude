# aclaude

An opinionated [Claude Code](https://docs.anthropic.com/en/docs/claude-code) distribution. Wraps the Claude Code CLI with persona theming, configurable defaults, and tmux session management.

aclaude is the A in BYOA — an experiment in determining what features are useful to expose in Claude Code-like programs, and an expression of preferences layered on top of the Claude Code foundation.

## Install

Requires: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`).

```sh
# curl (stable)
curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | sh

# curl (alpha — updates on every push to main)
curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | sh -s -- --alpha

# Homebrew
brew install arcaven/tap/aclaude
```

## Usage

```sh
aclaude                          # start session with default persona
aclaude -t dune -r dev           # start as Dune's dev character
aclaude -m claude-opus-4-6       # override model
aclaude persona list             # list 100 available themes
aclaude persona show dune        # show theme details
aclaude config                   # show resolved configuration
aclaude update                   # check for and install updates
```

## What It Does

- **Persona theming** — 100 theme rosters (Dune, West Wing, Hitchhiker's Guide, ...) with per-role characters, styles, and optional portrait images
- **Configuration** — TOML config with 5-layer merge: defaults → global (`~/.config/aclaude/`) → local (`.aclaude/`) → env (`ACLAUDE_*`) → CLI flags
- **tmux integration** — session management, statusline with context window usage, git info
- **Self-updating** — `aclaude update` fetches the latest release, rotates versions in `~/.local/share/aclaude/versions/`
- **Dual-track distribution** — `aclaude` (stable, tagged releases) and `aclaude-a` (alpha, every push to main) can coexist

## How It Works

aclaude invokes Claude Code as a subprocess via the [Agent SDK](https://www.npmjs.com/package/@anthropic-ai/claude-agent-sdk). It does not fork, modify, or redistribute Claude Code. Your credentials, your session — aclaude adds configuration and personality on top.

## Auth

Uses your existing Claude Code credentials (Max/Pro subscription, API key, Bedrock, or Vertex AI). Set `ANTHROPIC_API_KEY` for API key auth, or let aclaude inherit from Claude Code.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for details on auth boundaries.

## Configuration

```toml
# ~/.config/aclaude/config.toml
[session]
model = "claude-sonnet-4-6"

[persona]
theme = "dune"
role = "dev"
immersion = "high"   # high|medium|low|none

[statusline]
enabled = true
git_info = true
context_bar = true
```

Environment variables: `ACLAUDE_SESSION__MODEL=claude-opus-4-6`

## Credits

Portrait images by [slabgorb](https://github.com/slabgorb). Persona themes jointly developed by [slabgorb](https://github.com/slabgorb) and [arcaven](https://github.com/arcaven).

## License

MIT. See [LICENSE](LICENSE).

Claude Code and the Agent SDK are subject to Anthropic's [Commercial Terms of Service](https://www.anthropic.com/commercial-terms). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
