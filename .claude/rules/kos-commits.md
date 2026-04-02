# kos Commit Conventions

## kos-specific actions (from KOS process cycle)

In addition to conventional commit types (feat, fix, docs, etc.),
use these kos action types when working with the knowledge graph:

- `harvest`: update nodes after a probe cycle completes
- `promote`: move a node to a higher confidence tier
- `graveyard`: move a node to graveyard (ruled out)
- `probe`: begin or continue an exploration
- `finding`: write a finding from a probe
- `schema`: update the node schema
- `charter`: update the charter document

### Format
`[action]: [node-ids affected] — [one line description]`

### Examples
```
harvest: question-director-design — probe complete, see finding-042
promote: elem-byoa-platform — validated across 3 repos
graveyard: grv-monolith-approach — ruled out, see graveyard entry
probe(marvel): workspace model — exploring organizational hierarchy
finding(kos): distributed graph — three-tier model confirmed
```

### No AI Attribution
Do not add "Generated with Claude Code", "Co-Authored-By: Claude", or any
AI attribution to commits. The human is the author.
