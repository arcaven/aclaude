# aclaude

BOYC (Bring Your Own Claude) agent orchestration platform. Phase 1: standalone single-agent CLI.

## Build / Run / Test

Requires: Node.js 20+, `just`, `tmux` (optional, for sessions).

```sh
just build          # compile TypeScript
just dev            # run CLI in dev mode (tsx)
just dev config     # show resolved config
just dev persona list
just dev persona show <name>
just test           # run vitest
just lint           # eslint
just start          # launch tmux session
```

## Architecture

```
cli/          TypeScript CLI (commander + Claude Agent SDK + Anthropic SDK)
tmux/         tmux launcher + layout configs
personas/     theme YAMLs (character data, language-agnostic)
config/       reference configs (defaults.toml, example.toml)
docs/         architecture docs + research notes
```

### Dual SDK Design

- **`@anthropic-ai/claude-agent-sdk`** — session runner. Spawns Claude Code as subprocess, inherits its auth (OAuth, API key, Bedrock, Vertex). Handles the agent loop, tool execution, streaming.
- **`@anthropic-ai/sdk`** — raw API access. Per-turn token usage from `SDKAssistantMessage.message.usage`, model listing, direct API calls when needed.
- **Hooks** (`cli/src/hooks.ts`) — in-process JS callbacks wired into agent SDK's `options.hooks`. Tracks tool usage, session lifecycle, audit trail. Replaces pennyfarthing's shell-based hooks.

See `docs/research/cc-vs-sdk-20260316.md` for the full comparison.

## Conventions

- **Config format:** TOML. Merge order: defaults → global (~/.config/aclaude/) → local (.aclaude/) → env (ACLAUDE_*) → CLI flags.
- **Auth:** Delegates to Claude Code via Agent SDK. Users authenticate with their own Anthropic API key, AWS Bedrock, or Google Cloud Vertex AI credentials. aclaude does not store, manage, or proxy authentication. See THIRD_PARTY_NOTICES.md for details.
- **No file deletion:** Never delete user files. Overwrite only with explicit intent.
- **Parallel-safe:** Each session gets a UUID. No shared mutable state between sessions.
- **Dependencies:** Keep minimal. No frameworks beyond commander for CLI parsing.

## Config

TOML files in `config/`. See `config/example.toml` for all options.

Environment variables use `ACLAUDE_` prefix with double-underscore for nesting:
- `ACLAUDE_SESSION__MODEL=claude-opus-4-6`
- `ACLAUDE_PERSONA__THEME=dune`

## Personas

Theme YAMLs live in `personas/themes/`. Each theme has a roster of characters keyed by role (dev, sm, tea, reviewer, etc.). The `persona.role` config key selects which character to use.

## Values

- **Portability:** Runs anywhere Node.js runs. No platform-specific deps.
- **Composability:** CLI subcommands, tmux integration, OTEL — all optional layers.
- **User sovereignty:** All config is local. No phone-home. Telemetry is opt-in and self-hosted.
- **Easy install:** `npm install` and go. No build toolchain beyond TypeScript.

## How to Work Here (kos Process)

### Re-introduction
Read charter.md before any substantive work. It contains:
- Current bedrock (what's committed)
- Current frontier (what's under exploration)
- Current graveyard (what's been ruled out)

### Session Protocol
1. Read charter.md (orient)
2. Identify the highest-value open question — or capture new ideas in _kos/ideas/
3. Write an Exploration Brief in _kos/probes/
4. Do the probe work
5. Write a finding in _kos/findings/
6. Harvest: update affected nodes, move files if confidence changed
7. Update charter.md if bedrock changed

Cross-repo questions belong in the orchestrator's _kos/, not here.

### Ideas (pre-hypothesis brainstorming)
Ideas live in _kos/ideas/ as markdown files. Generative, possibly contradictory,
no commitment. When an idea crystallizes, extract into a frontier question + brief.

### Node Files
Nodes live in _kos/nodes/[confidence]/[id].yaml
Schema follows kos schema v0.3.
One node per file. Filename = node id.

### Confidence Changes
Moving a file between confidence directories IS the promotion.
Always accompany with a commit message explaining the evidence.

### Harvest Verification
Before starting the next cycle, verify:
- [ ] Finding written and committed
- [ ] Charter updated if bedrock changed
- [ ] Frontier questions updated (closed, opened, or revised)
- [ ] Exploration briefs marked complete or carried forward
