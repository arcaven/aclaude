# aclaude — top-level build orchestrator

default:
  @just --list

# Build the CLI
build:
  cd cli && npm run build

# Run CLI in dev mode (pass args after --)
dev *args:
  cd cli && npx tsx src/index.ts {{args}}

# Run tests
test:
  cd cli && npx vitest run

# Run tests in watch mode
test-watch:
  cd cli && npx vitest

# Lint
lint:
  cd cli && npx eslint src/

# Start tmux session
start:
  tmux/start-session.sh

# List available personas
persona-list:
  cd cli && npx tsx src/index.ts persona list

# Show a specific persona
persona-show name:
  cd cli && npx tsx src/index.ts persona show {{name}}

# Show a persona role with portrait (Kitty/Ghostty)
persona-portrait theme agent="dev" position="top" size="large":
  cd cli && npx tsx src/index.ts persona show {{theme}} --agent {{agent}} --portrait --portrait-position {{position}} --portrait-size {{size}}

# Show resolved config
config:
  cd cli && npx tsx src/index.ts config
