# Rust Claude Code Reimplementations — Adversarial Assessment

Date: 2026-04-10
Context: Evaluated four Rust projects forked at github.com/arcaven as
potential sources of inspiration and patterns for forestage's Rust codebase.
All four forks are unmodified mirrors of their upstreams (0 commits ahead).

## Projects Assessed

| Project | Upstream | Stars | Lines | Files | License |
|---------|----------|------:|------:|------:|---------|
| **claurst** | Kuberwastaken/claurst | 8,878 | 112k | 200 | GPL-3.0 |
| **pi_agent_rust** | Dicklesworthstone/pi_agent_rust | 707 | 595k | 429 | MIT + Anthropic/OpenAI rider |
| **srg-claude-code-rust** | srothgan/claude-code-rust | 73 | 62k | 122 | Apache-2.0 |
| **claw-code-rust** | claw-cli/claw-code-rust | 231 | 25k | 90 | MIT |

---

## Comparative Scorecard

| Dimension | claurst | pi_agent_rust | srg-claude-code-rust | claw-code-rust |
|-----------|:-------:|:-------------:|:--------------------:|:--------------:|
| **Code Quality** | A- | B+ | B+ | B+ |
| **AI Slop** | B- | A (near zero) | C+ | C+ |
| **Design Patterns** | B | B+ | B | B |
| **Performance** | B | A | C+ | B- |
| **Feature Completeness** | A | A | B+ | C- |
| **Idiomatic Rust** | A- | A- | B+ | A- |
| **Testing** | Good | Exceptional | Decent | Decent |
| **Overall** | **A-** | **A-** | **B+** | **B** |

---

## Per-Project Analysis

### 1. claurst (Kuberwastaken/claurst) — A-

**Architecture:** 12-crate workspace (core, api, query, tui, commands, mcp,
bridge, tools, plugins). Mirrors the TypeScript Claude Code architecture.

**Strengths:**
- Most feature-complete: 40+ slash commands, 50+ provider backends, full MCP
  (JSON-RPC 2.0, stdio + HTTP), managed agents/team swarm, auto-compact with
  circuit breaker, session persistence (JSONL + SQLite), ratatui TUI with Vim
  mode, mobile/web bridge with JWT.
- Strongest type system: ProviderId/ModelId newtypes, CommandResult (13
  variants), QueryOutcome (5 variants), TokenWarningState enum.
- Best workspace decomposition of the four.

**Weaknesses:**
- 128 `#[allow(dead_code)]` markers scattered throughout.
- Over-commented slash commands ("1. Parses, 2. Loads, 3. Builds").
- QueryConfig is 127 lines of optional fields — wants a builder.
- AutoCompactState uses bools instead of enum (missed state machine).
- String proliferation: functions accept `impl Into<String>` then `.to_string()`.

**Patterns worth studying:**
- 12-crate workspace layout and boundary design
- ProviderRegistry factory pattern
- StreamParser strategy trait
- Hierarchical AGENTS.md loading with frontmatter and @include expansion
- Auto-compact circuit breaker

---

### 2. pi_agent_rust (Dicklesworthstone/pi_agent_rust) — A-

**Architecture:** Single crate (no workspace), 595k lines. Full agentic loop
with WASM/QuickJS extension system, 9 LLM providers, bubbletea Elm
Architecture TUI.

**Strengths:**
- Near-zero AI slop — hand-written code throughout, no TODO graveyard.
- `#![forbid(unsafe_code)]` across entire codebase.
- Performance engineering: jemalloc allocator, thin LTO, sub-100ms startup.
- Extraordinary test suite: 266k lines of tests including provider contract
  conformance with VCR cassettes, security scenarios, fuzz testing, property
  tests.
- WASM/QuickJS extension system for sandboxed tool access.
- Extension lifecycle hooks (pre/post tool_call, pre/post tool_result).

**Weaknesses:**
- 595k lines in a single crate — should be a workspace.
- Not Claude Code-specific (port of "Pi Agent" from TypeScript).
- Large file sizes (extensions.rs at 50k lines, extensions_js.rs at 25k).

**Patterns worth studying:**
- Elm Architecture TUI (bubbletea/bubbles/glamour)
- Extension system with WASM host + lifecycle hooks
- VCR cassette-based provider conformance testing
- Security: path canonicalization, symlink protection, fs escape prevention
- Agent loop with QueueMode and max_tool_iterations state machine

---

### 3. srg-claude-code-rust (srothgan/claude-code-rust) — B+

**Architecture:** Flat src/ with logical module groups (agent, app, ui,
config, connect, events). No workspace.

**Strengths:**
- Good feature coverage: MCP (28 files), streaming, session management with
  policy-based eviction, slash commands, permission hooks, plugin CLI.
- Aggressive linting: `unwrap_used = "deny"`, `panic = "deny"`.
- State machine for AppStatus, CacheSplitPolicy strategy.

**Weaknesses:**
- 984 allocations via `.clone()`/`.to_string()`/`.to_owned()` across src.
- 8 permission helper functions repeating the same classification pattern.
- `render_message_with_tools_collapsed_and_separator_and_layout_generation()`.
- segment_lines cloned during streaming height measurement hot path.
- Multiple `#[allow(dead_code)]` markers.

**Patterns worth studying:**
- CacheSplitPolicy (pluggable cache eviction)
- AppStatus state machine
- Session picker with policy-based retention
- Aggressive clippy deny configuration

---

### 4. claw-code-rust (claw-cli/claw-code-rust) — B

**Architecture:** 10-crate workspace (core, tools, provider, server, tui,
tasks, safety, etc.). Best crate decomposition relative to its size.

**Strengths:**
- Cleanest workspace layout — each crate has a focused responsibility.
- Most idiomatic trait design: Tool trait with reference-based execute,
  proper Send + Sync bounds.
- Newtype IDs via macro with full trait coverage.
- ScriptedProviderBuilder for fluent test DSLs.

**Weaknesses:**
- Half-built: MCP crate exists but is empty, no slash commands, no hooks.
- Context compaction stubbed with `#[ignore]` tests.
- 56-line macro generating 6+ trait impls per ID type.
- Double-mutex: `Mutex<HashMap<SessionId, Arc<Mutex<RuntimeSession>>>>`.
- Core loop works; everything around it is scaffolding.

**Patterns worth studying:**
- 10-crate workspace boundary design (clean even though incomplete)
- Tool trait design (reference-based, async, Send + Sync)
- Newtype ID macro pattern
- ScriptedProviderBuilder test DSL
- ToolOrchestrator batch dispatch

---

## Licensing Analysis

forestage is MIT-licensed. The following analysis considers what borrowing
code or patterns from each project would mean.

### claw-code-rust — MIT

**License:** MIT
**Compatibility with forestage (MIT):** Full compatibility.
**What borrowing means:** Include the MIT copyright notice and permission
notice in any files containing borrowed code. No other obligations.
**Can we:**
- Copy code verbatim? Yes, with attribution.
- Adapt patterns and rewrite? Yes, with attribution for substantial portions.
- Use as a dependency? Yes.
- Relicense derivatives? Yes (MIT is permissive).

**Risk:** None. MIT-to-MIT is the simplest case.

### srg-claude-code-rust — Apache-2.0

**License:** Apache License 2.0
**Compatibility with forestage (MIT):** Compatible. Apache-2.0 code can be
included in an MIT project, but the Apache-2.0 terms apply to the borrowed
portions.
**What borrowing means:**
- Must include the Apache-2.0 NOTICE file (if one exists) and license text.
- Must state changes made to borrowed files.
- Patent grant included — protects against patent claims from contributors.
- Cannot use contributor trademarks.
**Can we:**
- Copy code verbatim? Yes, with Apache-2.0 notice preserved on those files.
- Adapt patterns and rewrite? Yes. Clean-room reimplementation of patterns
  (not copying code) requires no attribution.
- Use as a dependency? Yes.
- Relicense derivatives? The MIT portions stay MIT; the Apache-2.0 borrowed
  portions stay Apache-2.0. Dual-license headers on mixed files.

**Risk:** Low. Apache-2.0 is permissive. Main overhead is notice/attribution
bookkeeping. The patent grant is a benefit, not a burden.

### claurst — GPL-3.0

**License:** GNU General Public License v3.0
**Compatibility with forestage (MIT):** INCOMPATIBLE for code borrowing.
**What borrowing means:** Any code derived from GPL-3.0 source requires the
entire resulting work to be distributed under GPL-3.0. This would force
forestage to relicense from MIT to GPL-3.0.
**Can we:**
- Copy code verbatim? NO — would require forestage to become GPL-3.0.
- Adapt patterns and rewrite? Patterns and ideas are not copyrightable.
  Clean-room reimplementation of an *idea* observed in GPL code is legal,
  but adapting *code* (even rewritten) creates a derivative work risk.
- Use as a dependency? Only if forestage becomes GPL-3.0.
- Study for inspiration? Yes — reading code to understand an approach,
  then implementing independently, is not a GPL violation. But the
  implementation must be genuinely independent, not a "rewrite."

**Risk:** HIGH. GPL-3.0 is copyleft. Any code borrowing, even partial,
contaminates the MIT license. Study the architecture and patterns only.
Implement independently.

**Practical guidance:** claurst's value to forestage is in its *design
decisions* (12-crate workspace layout, which features to implement, how to
organize slash commands), not its code. These are ideas, not expression.
Never copy-paste from claurst. Never have the source open while writing.

### pi_agent_rust — MIT with Anthropic/OpenAI Rider

**License:** MIT License with a custom rider that restricts usage by
"Restricted Parties" defined as OpenAI, Anthropic, their affiliates, and
anyone acting on their behalf.
**Compatibility with forestage (MIT):** Depends on forestage's relationship
to Anthropic.

**The rider states:**
- No rights granted to OpenAI, Anthropic, or their affiliates.
- No one may distribute the software to Restricted Parties.
- Derivative works must include the rider unmodified.
- Breach terminates the license immediately.
- "Affiliate" means >50% ownership or power to direct management.

**What this means for forestage:**
- forestage wraps Claude Code (Anthropic's tool) via the Agent SDK. forestage
  is NOT an Anthropic product — it's an independent project by Arcaven.
- Arcaven is not an Anthropic affiliate (no ownership, no direction).
- Using the Claude API or wrapping Claude Code does not make you an
  Anthropic affiliate under the rider's definition.
- **However:** the rider must be preserved in derivative works. If forestage
  borrows code from pi_agent_rust, forestage's distribution must include the
  rider. This could create confusion for users who assume MIT means MIT.

**Can we:**
- Copy code verbatim? Legally yes (we are not a Restricted Party), but
  must include the rider in borrowed files. Creates licensing complexity.
- Adapt patterns and rewrite? Clean-room reimplementation of patterns
  requires no attribution. Safest path.
- Use as a dependency? Yes, with rider propagation.
- Study for inspiration? Yes, freely.

**Risk:** MEDIUM. Not a legal blocker but an operational complexity.
Including a "no Anthropic" rider in an forestage distribution that wraps
Anthropic's Claude Code would look bizarre and potentially confuse users.
The rider's enforceability is also untested.

**Practical guidance:** Study pi_agent_rust freely for patterns (extension
system, Elm TUI, VCR testing, jemalloc). Implement independently. Don't
copy code — the rider propagation requirement creates unnecessary confusion
in an MIT project that wraps an Anthropic product.

---

## Summary: What to Borrow and How

| Source | License | Borrow Code? | Study Patterns? | Best patterns to study |
|--------|---------|:---:|:---:|---|
| **claw-code-rust** | MIT | Yes | Yes | Workspace layout, Tool trait, newtype macro, test DSL |
| **srg-claude-code-rust** | Apache-2.0 | Yes (with notice) | Yes | Cache policy, AppStatus state machine, clippy deny config |
| **claurst** | GPL-3.0 | **No** | Ideas only | Workspace scale, provider registry, auto-compact, MCP design |
| **pi_agent_rust** | MIT+rider | Avoid | Yes | Elm TUI, extension hooks, VCR testing, jemalloc, forbid(unsafe) |

### The safe path for forestage:

1. **Copy freely from** claw-code-rust (MIT). The Tool trait design and
   workspace layout are directly applicable.
2. **Copy with attribution from** srg-claude-code-rust (Apache-2.0). The
   cache policy and linting config are useful. Keep Apache-2.0 notice on
   borrowed files.
3. **Study but don't copy from** claurst (GPL-3.0) and pi_agent_rust
   (MIT+rider). Both have excellent patterns worth learning from. Implement
   independently after studying.
