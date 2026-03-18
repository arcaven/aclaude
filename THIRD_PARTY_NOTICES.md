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

**For distributed/shared use (the default):**

- **Anthropic API key** (`ANTHROPIC_API_KEY`) — the required auth method
  when aclaude is used as a distributed tool or product. Obtain from
  [console.anthropic.com](https://console.anthropic.com/).
- **AWS Bedrock** or **Google Cloud Vertex AI** — supported cloud providers
  with their own credential management.

**For personal use on your own machine:**

- **Claude Code OAuth** (Pro/Max subscription) — when you are the sole user
  running aclaude on your own machine with your own subscription, OAuth
  auth inherited from Claude Code works as it would for any Claude Code
  session. This is personal use of Claude Code, not distribution.

**What is not permitted:**

- Using OAuth tokens from Claude Free/Pro/Max consumer accounts in aclaude
  when distributing it as a product or service to others
- Advertising "bring your Max subscription" as a feature of aclaude
- Proxying, extracting, or relaying consumer OAuth tokens through aclaude

This distinction follows Anthropic's written policy: the Agent SDK's
Commercial Terms of Service require API key authentication when building
products and services. OAuth/consumer plan auth is reserved for direct
Claude Code and Claude.ai use.

## Usage Policy

All use of Claude through aclaude is subject to Anthropic's
[Usage Policy](https://www.anthropic.com/usage-policy).
