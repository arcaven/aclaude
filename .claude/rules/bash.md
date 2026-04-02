# Bash Rules

- Use `set -euo pipefail` in scripts
- Quote variables: `"$var"` not `$var`
- Use `[[ ]]` not `[ ]` for conditionals
- Prefer `$(command)` over backticks
- Check command existence with `command -v` not `which`
