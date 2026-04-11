# forestage Architecture

## Overview

forestage is a BOYC (Bring Your Own Claude) agent orchestration platform. Phase 1 is a standalone single-agent CLI that wraps the Claude SDK with persona theming, a config system, and tmux integration.

## Directory Structure

- **cli/** — TypeScript CLI application (commander + Claude SDK)
- **tmux/** — tmux session launcher and layout configs
- **personas/** — Theme YAML files defining character rosters
- **config/** — Reference configuration files (TOML)
- **docs/** — Architecture and design documentation

## Config Resolution

TOML-based, layered merge (last wins):

1. Built-in defaults (`config/defaults.toml`)
2. Global user config (`$XDG_CONFIG_HOME/forestage/config.toml`)
3. Local project config (`.forestage/config.toml`)
4. Environment variables (`FORESTAGE_*` prefix, double-underscore nesting)
5. CLI flags

## Persona System

Each theme YAML contains a roster of characters keyed by agent role (dev, sm, tea, reviewer, etc.). The config selects a theme + role, and the CLI builds a system prompt from the character's attributes.

Immersion levels control how much persona bleeds into responses:
- **high** — Full character with catchphrases, quirks, user title
- **medium** — Character name and style, occasional catchphrase
- **low** — Light personality flavor, focus on expertise
- **none** — Plain assistant, no persona

## Session Model

Each interactive session gets a UUID. The session runner maintains a message history and streams responses from the Claude API. Context usage is tracked and displayed in the statusline.

## Telemetry

Optional, opt-in OTEL integration. Disabled by default. When enabled, traces session lifecycle events (start, message, end) to a self-hosted collector.

## Future Phases

- Phase 2: Multi-agent coordination, workflow engine, agent sidecars
- Phase 3: K8s-inspired resource model, director-level orchestration
