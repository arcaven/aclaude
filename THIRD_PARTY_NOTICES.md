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
aclaude does not handle, store, or proxy any authentication tokens.

**Single-user (your own account, your own machine):**

- **Claude Code OAuth** (Pro/Max subscription) — a single user running
  aclaude with their own subscription is using Claude Code normally.
  Max is designed for professional work including coding and engineering.
  aclaude inherits Claude Code's auth as any wrapper would.
- **Anthropic API key** (`ANTHROPIC_API_KEY`) — also works. Obtain from
  [console.anthropic.com](https://console.anthropic.com/).
- **AWS Bedrock** or **Google Cloud Vertex AI** — supported cloud providers.

**Multi-user distribution (others authenticate through your tool):**

- **API key auth required** — if you distribute aclaude as a product or
  service where multiple users authenticate through it, API key
  authentication is required per Anthropic's Commercial Terms of Service.
- Do not route other people's Claude.ai credentials through your tool.

The boundary is single-user vs. multi-user routing, not personal vs.
professional. One person using their Max subscription through aclaude for
work is fine. Building a multi-user product that routes subscribers'
OAuth tokens through it is not.

## Usage Policy

All use of Claude through aclaude is subject to Anthropic's
[Usage Policy](https://www.anthropic.com/usage-policy).
