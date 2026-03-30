# aclaude

An opinionated [Claude Code](https://docs.anthropic.com/en/docs/claude-code) distribution. Wraps the Claude Code CLI with persona theming, configurable defaults, tmux session management and some additional features yet to be described here.

aclaude is an exploration of features useful in Claude Code-like programs when used with systems like [marvel](https://github.com/arcavenae/marvel) [switchboard](https://github.com/arcavenae/switchboard) [spectacle](https://github.com/arcavenae/spectacle) and an also an expression of preferences layered on top of the Claude Code foundation.

## Install

Requires: [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) (`claude`).

```sh
# curl (alpha — updates on every push to main)
curl -fsSL https://raw.githubusercontent.com/arcavenae/aclaude/main/install.sh | bash -s -- --alpha
```

```sh
# Homebrew (alpha)
brew install arcavenae/tap/aclaude-a
```

Stable channel (`aclaude` / `arcavenae/tap/aclaude`) will be available once a tagged release is cut. Until then, use the alpha channel — it tracks main and is signed and notarized.

### Uninstall

```sh
# curl-installed
curl -fsSL https://raw.githubusercontent.com/arcavenae/aclaude/main/install.sh | bash -s -- --uninstall
```

```sh
# Homebrew
brew uninstall aclaude-a    # or aclaude for stable
```

Config at `~/.config/aclaude/` is preserved by uninstall. Delete manually if unwanted.

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
