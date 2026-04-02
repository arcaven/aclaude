# Git Commit Standards

## Format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types
feat, fix, docs, style, refactor, perf, test, build, ci, chore

### kos-specific actions (from KOS process cycle)
harvest, promote, graveyard, probe, finding, schema, charter, question

### Rules
- Imperative, present tense: "add feature" not "added feature"
- No capitalized first letter in description
- No period at end
- Body optional (blank line after subject)
- Footer: Refs, Closes, BREAKING CHANGE

### No AI Attribution
Do not add "Generated with Claude Code", "Co-Authored-By: Claude", or any
AI attribution to commits. The human is the author.
