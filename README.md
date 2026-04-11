# forestage

An opinionated [Claude Code](https://docs.anthropic.com/en/docs/claude-code) distribution. Wraps the Claude Code CLI with persona theming, configurable defaults, tmux session management and some additional features yet to be described here.

forestage is an exploration of features useful in Claude Code-like programs when used with systems like [marvel](https://github.com/arcavenae/marvel) [switchboard](https://github.com/arcavenae/switchboard) [spectacle](https://github.com/arcavenae/spectacle) and an also an expression of preferences layered on top of the Claude Code foundation.

**Rewritten in Rust** (2026-04-01) — eliminating all Node.js/npm dependencies following the [axios supply chain incident](docs/security/axios-supply-chain-2026-03-31.md). Single static binary, no runtime dependencies beyond the `claude` CLI itself.

## Install

Requires: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`).

```sh
# curl (alpha — updates on every push to main)
curl -fsSL https://raw.githubusercontent.com/arcavenae/forestage/main/install.sh | bash -s -- --alpha
```

```sh
# Homebrew (alpha)
brew install arcavenae/tap/forestage-a
```

Stable channel (`forestage` / `arcavenae/tap/forestage`) will be available once a tagged release is cut. Until then, use the alpha channel — it tracks main and is signed and notarized.

### Uninstall

```sh
# curl-installed
curl -fsSL https://raw.githubusercontent.com/arcavenae/forestage/main/install.sh | bash -s -- --uninstall
```

```sh
# Homebrew
brew uninstall forestage-a    # or forestage for stable
```

Config at `~/.config/forestage/` is preserved by uninstall. Delete manually if unwanted.

## Usage

```sh
forestage                          # start interactive session with default persona
forestage -t dune -r dev           # start as Dune's dev character
forestage -m claude-opus-4-6       # override model
forestage -p "explain this code"   # one-shot prompt (non-interactive)
forestage persona list             # list available themes
forestage persona show dune        # show theme details
forestage persona show dune --portrait  # show with inline portrait (Kitty/Ghostty)
forestage persona portraits        # show portrait cache status
forestage config                   # show resolved configuration
forestage update                   # check for updates
forestage version                  # show version, commit, channel, build time
```

### Passing arguments to Claude Code

Arguments after `--` are forwarded directly to the `claude` CLI. This lets you use any Claude Code flag without forestage needing to know about it:

```sh
forestage -- --allowedTools Bash,Read --max-turns 5
forestage -p "fix the tests" -- --allowedTools Bash,Read,Edit
forestage -- --no-session-persistence --max-budget-usd 0.50
forestage -t dune -- --resume SESSION_ID
```

### Agent mode

The `--streaming` flag uses the NDJSON subprocess protocol for programmatic use. This is the integration point for [marvel](https://github.com/arcavenae/marvel) and other orchestrators:

```sh
forestage --streaming                          # structured JSON session
forestage --streaming -t the-expanse -r dev    # with persona
forestage --streaming -- --max-turns 10        # with claude flags
```

Streaming mode provides structured access to token usage, session cost, and tool invocations. It prints a usage summary on exit.

## What It Does

- **Persona theming** — 118 theme rosters (Dune, West Wing, Hitchhiker's Guide, ...) with per-role characters, styles, and optional portrait images (Kitty/Ghostty inline display)
- **Configuration** — TOML config with 5-layer merge: defaults -> global (`~/.config/forestage/`) -> local (`.forestage/`) -> env (`FORESTAGE_*`) -> CLI flags
- **Claude Code passthrough** — any `claude` CLI flag works via `--` separator
- **Three session modes** — interactive TUI (default), one-shot prompt (`-p`), streaming agent (`--streaming`)
- **tmux integration** — session management, statusline with context window usage, git info
- **Self-updating** — `forestage update` checks for the latest release via GitHub
- **Dual-track distribution** — `forestage` (stable, tagged releases) and `forestage-a` (alpha, every push to main) can coexist

## How It Works

forestage spawns the Claude Code CLI (`claude`) as a subprocess. It does not use the Node.js Agent SDK or any npm packages. forestage does not fork, modify, or redistribute Claude Code. Your credentials, your session — forestage adds configuration and personality on top.

For interactive use, forestage passes through to Claude Code's TUI with the persona system prompt injected via `--append-system-prompt`. For programmatic use (`--streaming`), forestage uses the `claude` CLI's NDJSON streaming protocol to capture structured events (token usage, tool invocations, session metadata).

## Auth

Uses your existing Claude Code credentials (Max/Pro subscription, API key, Bedrock, or Vertex AI). Set `ANTHROPIC_API_KEY` for API key auth, or let forestage inherit from Claude Code.

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for details on auth boundaries.

## Configuration

```toml
# ~/.config/forestage/config.toml
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

Environment variables: `FORESTAGE_SESSION__MODEL=claude-opus-4-6`

## Building from Source

Requires Rust 1.85+.

```sh
just build          # cargo build
just test           # cargo test
just ci             # full CI check (fmt, clippy, deny, test)
```

## Credits

Portrait images by [slabgorb](https://github.com/slabgorb). Persona themes jointly developed by [slabgorb](https://github.com/slabgorb) and [arcaven](https://github.com/arcaven).

## License

MIT. See [LICENSE](LICENSE).

Claude Code is subject to Anthropic's [Commercial Terms of Service](https://www.anthropic.com/commercial-terms). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
