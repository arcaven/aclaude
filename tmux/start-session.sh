#!/usr/bin/env bash
# aclaude tmux session launcher
# Starts a tmux session with aclaude in the main pane

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SOCKET_NAME="${ACLAUDE_TMUX__SOCKET:-ac}"
SESSION_NAME="aclaude"
LAYOUT="${ACLAUDE_TMUX__LAYOUT:-bottom}"

# Check dependencies
command -v tmux >/dev/null 2>&1 || { echo "tmux is required but not installed."; exit 1; }

# Kill existing session on this socket if it exists
if tmux -L "$SOCKET_NAME" has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Session '$SESSION_NAME' already exists on socket '$SOCKET_NAME'."
  echo "Attaching..."
  exec tmux -L "$SOCKET_NAME" attach-session -t "$SESSION_NAME"
fi

# Create session
tmux -L "$SOCKET_NAME" new-session -d -s "$SESSION_NAME" -c "$PROJECT_ROOT"

# Configure status line to read from cache files
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status on
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-interval 5
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-style "bg=colour235,fg=colour245"
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-left-length 50
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-right-length 60

# Status reads from cache files if they exist
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-left \
  "#(cat .aclaude/tmux-status-left 2>/dev/null || echo '#[fg=colour67]aclaude#[default]')"
tmux -L "$SOCKET_NAME" set-option -t "$SESSION_NAME" status-right \
  "#(cat .aclaude/tmux-status-right 2>/dev/null || echo '')"

# Launch aclaude in the main pane
tmux -L "$SOCKET_NAME" send-keys -t "$SESSION_NAME" "cd $PROJECT_ROOT && just dev" Enter

# Attach
exec tmux -L "$SOCKET_NAME" attach-session -t "$SESSION_NAME"
