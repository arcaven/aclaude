# Third-Party Notices

## Claude Code CLI

aclaude invokes the Claude Code CLI (`claude`) as a subprocess. Claude Code
is proprietary software owned by Anthropic PBC. It is **not** bundled with
or redistributed by aclaude.

- Copyright: © Anthropic PBC. All rights reserved.
- License: Subject to Anthropic's [Commercial Terms of Service](https://www.anthropic.com/commercial-terms)
- Users must install Claude Code separately and authenticate with their own
  Anthropic API key or supported cloud provider credentials.

aclaude does not fork, modify, or redistribute any Claude Code source code.

## Anthropic Agent SDK

aclaude uses the `@anthropic-ai/claude-agent-sdk` npm package as the
sanctioned integration path for spawning and managing Claude Code sessions.

- License: Subject to Anthropic's [Commercial Terms of Service](https://www.anthropic.com/commercial-terms)
- The Agent SDK permits use in products and services made available to
  customers and end users.

## Anthropic SDK

aclaude uses the `@anthropic-ai/sdk` npm package for direct API access
(token usage tracking, model listing).

- License: Subject to Anthropic's terms of service.

## Authentication

aclaude delegates authentication entirely to Claude Code and the Agent SDK.
Users must authenticate with their own credentials:

- **Anthropic API key** (via Claude Console) — the standard path
- **AWS Bedrock** or **Google Cloud Vertex AI** — supported cloud providers
- **Claude Code OAuth** — only when running Claude Code directly (not
  through aclaude as a product/service)

OAuth tokens from Claude Free/Pro/Max consumer accounts are intended
exclusively for Claude Code and Claude.ai. aclaude does not use, store,
or proxy consumer OAuth tokens.

## Usage Policy

All use of Claude through aclaude is subject to Anthropic's
[Usage Policy](https://www.anthropic.com/usage-policy).
