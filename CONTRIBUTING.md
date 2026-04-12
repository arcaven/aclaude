# Contributing to forestage

Thank you for your interest in contributing! This guide will help you get started.

## Development Model

This project uses a **solo maintainer + AI agent team** development model. The human maintainer (arcaven) directs a team of AI agents that handle most implementation work. All PRs are reviewed by the maintainer before merge.

You don't need to use AI agents to contribute -- just follow this guide and submit a PR like any other open-source project.

## Prerequisites

- **Rust 1.85+** -- [install](https://rustup.rs/)
- **just** -- [install](https://github.com/casey/just)
- **nightly rustfmt** -- `rustup component add rustfmt --toolchain nightly`

## Getting Started

```bash
git clone https://github.com/ArcavenAE/forestage.git
cd forestage
just build    # Build
just test     # Run tests
just lint     # Run linter (clippy)
just fmt      # Format code (nightly rustfmt)
```

## How to Contribute

### Reporting Bugs

Open a [Bug Report](https://github.com/ArcavenAE/forestage/issues/new?template=bug-report.yml) and include:

- Version or commit hash
- Operating system
- Steps to reproduce
- Expected vs actual behavior

### Suggesting Features

Open a [Feature Request](https://github.com/ArcavenAE/forestage/issues/new?template=feature-request.yml).

This project is part of the [Arcaven Agentic Engineering](https://github.com/ArcavenAE) platform. Features should align with the project's design values: user sovereignty, composability over frameworks, and gradual elaboration.

### Submitting Code

1. Fork the repo and create a feature branch from `develop`:
   ```bash
   git checkout -b feat/your-feature
   ```
2. Write tests for your changes
3. Run the full quality gate:
   ```bash
   just fmt
   just lint
   just test
   ```
4. Create a PR using the PR template

### Commit Message Format

[Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

Examples:
- `feat: add session timeout configuration`
- `fix: prevent crash on empty input`
- `docs: update installation guide`

Rules:
- Imperative, present tense ("add feature" not "added feature")
- No capitalized first letter in description
- No period at end

## Code Standards

- **Safety:** `#![forbid(unsafe_code)]` in all crates
- **No `unwrap()` in production code** -- use `?` or `expect()` with an actionable message
- **Error types:** `thiserror` for structured error enums
- **Formatting:** `cargo +nightly fmt --all`
- **Linting:** `cargo clippy -- -D warnings` must pass

See [CLAUDE.md](CLAUDE.md) for the complete coding standards.

## What NOT to Contribute

- Heavy dependencies where the standard library suffices
- Telemetry, analytics, or phone-home features
- Features that create vendor lock-in or external service dependencies
- Code that stores or manages credentials (auth is always delegated)
- `unsafe` code

## License

This project is [MIT licensed](LICENSE). By contributing, you agree that your contributions will be licensed under the same license.
