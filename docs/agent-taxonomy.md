# Agent Taxonomy

How forestage models who an agent is and what it does. This is the
forestage-side projection of the platform-wide **B14** taxonomy
(orchestrator charter, finding-019). Read this before adding flags,
config fields, or content packs that touch character or role.

## Why a taxonomy

Every survey we did of agent frameworks (BMAD, multiclaude, gastown,
pennyfarthing, drbothen, ~20 others) collapsed two or more of the
ideas below into one slot. The resulting model couldn't answer
ordinary questions cleanly:

- "Can the same character work as a reviewer one session and a
  release manager the next?"
- "Can two agents on a team be the same character?"
- "Who do I show portraits for — the character or the job?"
- "Where does my professional background live, separate from who I'm
  *playing*?"

The taxonomy below answers each of these by giving every concept its
own slot. The orchestrator charter §B14 has the canonical definitions;
this doc is the forestage user-facing projection.

## The five primitives

| Primitive | Plain English | The test | Library item? |
|-----------|--------------|----------|---------------|
| **Theme** | The roster (a fictional universe) | "From the world of ___" | Yes — theme YAMLs |
| **Persona** | The costume (a character within a theme) | "She is PLAYING ___" | Yes — character entries inside a theme |
| **Identity** | The lens (who they ARE now, professionally) | "He IS a ___" | Yes (free-form today, identity packs later) |
| **Role(s)** | The job(s) on this team | "Today she is the ___" | Yes (free-form CSV today, role packs later) |
| **Process** | The mission, the game | "The team is here to ___" | Yes — owned by marvel/director, not forestage |

Composition:

```
Agent = Persona + Identity + Role(s) + LLM + Tools
        (costume)  (lens)    (jobs)    (engine) (capabilities)

Team  = Agents + Roles + Process
        (who)    (what)   (why/how)
```

forestage owns four of these (Theme, Persona, Identity, Role).
**Process is a marvel/director concern** — it belongs in team
manifests, not on the forestage CLI.

## What each one is *not*

- **Theme is NOT a team.** A theme is the source material a persona
  is drawn from. "Discworld" is a roster of ~10 characters, not an
  assignment of jobs.
- **Persona is NOT a role.** Granny Weatherwax is a character. She
  can fill any role — reviewer, release manager, troubleshooter.
  The pre-B14 model bound personas to fixed role keys; that
  conflation produced the granny→ponder portrait bug
  (orc finding-033).
- **Role is NOT permanent.** Role is a job assignment for a
  particular team and engagement. The same agent can carry different
  roles on different teams.
- **Identity is NOT a persona.** Identity is the professional lens —
  "homicide detective", "IP attorney", "site reliability engineer".
  Naomi Nagata (persona) became an IP attorney (identity), and on
  this team she's the Product Owner and Business Analyst (roles).

## CLI surface

forestage exposes the four owned primitives as flags on every
command that launches an agent:

| Flag | Primitive | Format | Required? |
|------|-----------|--------|-----------|
| `-t, --theme` | Theme | slug, fuzzy-resolved | optional (default from config) |
| `--persona` | Persona | character slug, fuzzy-resolved | optional |
| `--identity` | Identity | free-form string | optional |
| `-r, --role` | Role(s) | CSV of free-form strings | optional |

### Examples

**Just a character:**
```bash
forestage --theme the-expanse --persona naomi-nagata
```

**Character + identity (the lens):**
```bash
forestage --theme the-expanse --persona naomi-nagata \
  --identity "IP attorney"
```

**Character + single role:**
```bash
forestage --theme discworld --persona granny-weatherwax \
  --role reviewer
```

**Character + multiple roles** (CSV):
```bash
forestage --theme discworld --persona granny-weatherwax \
  --role "reviewer,troubleshooter"
```

**Full taxonomy:**
```bash
forestage --theme the-expanse --persona naomi-nagata \
  --identity "IP attorney" \
  --role "product-owner,business-analyst"
```

**Fuzzy resolution** — type a fragment, get the slug:
```bash
forestage --theme exp --persona naomi
# resolves to --theme the-expanse --persona naomi-nagata
```

If `--theme` is missing or ambiguous, forestage tries to back-propagate
from `--persona`: if "naomi" only appears in one theme, that theme is
selected automatically. See `src/resolve.rs` for the resolver design.

## What the system prompt looks like

forestage builds the system prompt by composing the four layers in
order — persona, identity, role(s) — separated by blank lines. Each
layer is omitted if the corresponding input is empty.

`persona::build_full_prompt` is the single source of truth; the unit
tests in `src/persona.rs` pin the exact strings.

### Persona-only (immersion=low)

Input: `--theme the-expanse --persona naomi-nagata --immersion low`
(immersion comes from config, default `high`).

Output:
```
Bring a touch of Naomi Nagata's personality (from The Expanse by James S.A. Corey (2011-2021)) to your responses. Background: Implementation, Belter engineering, making impossible things work
```

### Persona + identity

Input: `--persona naomi-nagata --identity "IP attorney" --immersion low`

Output:
```
Bring a touch of Naomi Nagata's personality (from The Expanse by James S.A. Corey (2011-2021)) to your responses. Background: Implementation, Belter engineering, making impossible things work

In this context, you have become a IP attorney. Bring that professional perspective to your work.
```

### Persona + single role

Input: `--persona granny-weatherwax --role reviewer`

Role layer (singular form):
```
Your current role on this team is: reviewer.
```

### Persona + multiple roles

Input: `--persona granny-weatherwax --role "reviewer,troubleshooter"`

Role layer (plural form, order preserved, whitespace trimmed):
```
Your current roles on this team are: reviewer, troubleshooter.
```

### All four layers

Input:
```bash
forestage --theme the-expanse --persona naomi-nagata \
  --identity "IP attorney" \
  --role "product-owner,business-analyst"
```

Output structure (immersion=low for brevity):
```
Bring a touch of Naomi Nagata's personality (from The Expanse by James S.A. Corey (2011-2021)) to your responses. Background: Implementation, Belter engineering, making impossible things work

In this context, you have become a IP attorney. Bring that professional perspective to your work.

Your current roles on this team are: product-owner, business-analyst.
```

Exactly two blank-line separators between three non-empty layers —
the `full_prompt_combines_persona_identity_and_roles` test pins this
shape.

### Empty inputs are skipped

Input: `--persona naomi-nagata` (no identity, no role, immersion=none)

Output: empty string. forestage passes nothing to
`--append-system-prompt` and Claude Code uses its default system
prompt. Test: `full_prompt_skips_empty_role_and_identity_layers`.

## Configuration file equivalents

Anything you set on the CLI can also live in `~/.config/forestage/config.toml`
or `.forestage/config.toml` (5-layer merge — see
`src/config.rs::load_config`).

```toml
[persona]
theme     = "the-expanse"
character = "naomi-nagata"          # the persona slug
identity  = "IP attorney"
role      = "product-owner,business-analyst"   # CSV
immersion = "high"                  # high | medium | low | none
```

`character` is the config key; the CLI flag is `--persona`. (The
config field name predates the B14 vocabulary cleanup; the contract
on disk is stable.)

## Portrait resolution depends on persona only

When forestage shows a portrait for an agent, it derives the lookup
stem from the **Character** struct alone — never from the role. The
order it tries:

1. `short_name` (e.g. "Granny" for Granny Weatherwax)
2. Full `character` name (slugified)
3. First name only

Each candidate stem is tried against each size directory
(`small/medium/large/original`) as exact match first, then prefix
match (so CDN-hashed filenames like `granny-35211.png` resolve via
the `granny` stem).

Pre-B14, a `manifest.json` role-key index could override this and
return the wrong character's portrait whenever `--persona` and
`--role` named different characters — this was the granny→ponder bug
class (orc finding-033). The override is gone. The signature
`resolve_portrait(theme_slug, agent: &Character)` does not accept a
role; the
`resolve_portrait_signature_does_not_take_a_role` test makes the
contract type-checked.

See `src/portrait.rs::resolve_portrait` and the regression test
`resolve_portrait_returns_each_characters_own_file`.

## Marvel manifests project the same shape

When marvel launches a forestage agent as part of a team, it passes
the four owned primitives via flags (B8) and adds two more layers of
its own (workspace, team, permission policy). A team manifest
fragment:

```yaml
team: review-squad
roles:
  - name: reviewer
    persona:
      theme: discworld
      character: granny-weatherwax
    identity: principal engineer
    replicas: 2

  - name: release-manager
    persona:
      theme: breaking-bad
      character: jesse-pinkman
    identity: site reliability engineer
    replicas: 1
```

Process — the team's mission and method — lives at the team level in
marvel manifests, not on individual agents. forestage doesn't
currently model Process directly; it only consumes what marvel hands
it via `--workspace`, `--team`, `--script`, etc.

## Anti-patterns

- **Don't use `--role` to select a character.** Role is what the
  agent does on this team. `--persona` selects the character.
- **Don't bind roles to personas in theme YAMLs.** Theme YAMLs are
  rosters keyed by character slug. Any character can fill any role.
- **Don't add a role parameter to portrait resolution.** Portrait
  derives from `Character` only; this is enforced by tests.
- **Don't put theme assignments inside team manifests.** Theme is a
  property of the persona (which roster they came from), not of the
  team or role.
- **Don't conflate identity and role on the CLI.** Identity is who
  the agent IS; role is what the agent DOES today. They compose.

## References

These all live in the orchestrator (`aae-orc/`), not in this repo:

- `charter.md` §B14 — canonical definitions of the five primitives
- `_kos/findings/finding-019-agent-taxonomy.md` — the survey of
  ~20 systems and the discovery of the five-primitive split
- `_kos/findings/finding-020-agentic-primitives-full-map.md` — the
  nine layers of the full agentic engineering stack
- `_kos/findings/finding-033-taxonomy-migration-semantic-drift.md` —
  the granny→ponder regression that motivated removing the
  manifest role-key override
- `vision.md` — the platform-level pitch (the movie-pitch test)

forestage code paths:

- `src/persona.rs` — `Character`, `build_full_prompt`,
  `resolve_character`
- `src/portrait.rs` — `resolve_portrait`,
  `resolve_portrait_in_dir`
- `src/resolve.rs` — `--theme`/`--persona` fuzzy resolution
- `tests/persona_cli.rs` — end-to-end CLI smoke tests
