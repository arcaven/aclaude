#!/bin/bash
# aclaude installer — downloads the latest release from GitHub and installs it.
#
# Install:
#   curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | bash -s -- --alpha
#
# Uninstall:
#   curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | bash -s -- --uninstall
#
set -euo pipefail

GITHUB_OWNER="arcaven"
GITHUB_REPO="aclaude"
INSTALL_DIR="$HOME/.local/share/aclaude"
VERSIONS_DIR="$INSTALL_DIR/versions"
SYMLINK_DIR="$HOME/.local/bin"

# Parse arguments
CHANNEL="stable"
UNINSTALL=false
for arg in "$@"; do
  case "$arg" in
    --alpha) CHANNEL="alpha" ;;
    --uninstall) UNINSTALL=true ;;
    --help|-h)
      echo "Usage: install.sh [--alpha] [--uninstall]"
      echo ""
      echo "  --alpha      Install the alpha channel (aclaude-a)"
      echo "  --uninstall  Remove aclaude and all versions"
      echo "  (default)    Install the stable channel (aclaude)"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg"
      exit 1
      ;;
  esac
done

# Determine binary name from channel
if [[ "$CHANNEL" = "alpha" ]]; then
  BIN_NAME="aclaude-a"
else
  BIN_NAME="aclaude"
fi

# --- Uninstall ---
if [[ "$UNINSTALL" = true ]]; then
  echo "Uninstalling aclaude..."
  echo ""

  # Remove symlinks
  for name in aclaude aclaude-a; do
    link="$SYMLINK_DIR/$name"
    if [[ -L "$link" ]] || [[ -f "$link" ]]; then
      echo "  Removing $link"
      rm -f "$link"
    fi
  done

  # Remove versions and data
  if [[ -d "$INSTALL_DIR" ]]; then
    echo "  Removing $INSTALL_DIR"
    rm -rf "$INSTALL_DIR"
  fi

  # Note: config is preserved
  echo ""
  echo "aclaude removed."
  echo ""
  echo "Config preserved at ~/.config/aclaude/ (delete manually if unwanted)."
  echo "Portraits preserved at ~/.local/share/aclaude/portraits/ (if they existed,"
  echo "they were removed with the data directory above)."
  echo ""
  echo "If installed via Homebrew, also run:"
  echo "  brew uninstall aclaude aclaude-a"
  exit 0
fi

# --- Install ---

# Detect platform
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  darwin) PLATFORM="darwin" ;;
  linux)  PLATFORM="linux" ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH_NAME="arm64" ;;
  x86_64|amd64)  ARCH_NAME="amd64" ;;
  *)
    echo "Unsupported architecture: $ARCH"
    exit 1
    ;;
esac

ASSET_NAME="${BIN_NAME}-${PLATFORM}-${ARCH_NAME}"

# Get latest release info
echo "Checking for latest ${CHANNEL} release..."

if [[ "$CHANNEL" = "stable" ]]; then
  # Get latest non-prerelease via /releases/latest
  RELEASE_JSON="$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest" 2>/dev/null)" || {
    echo "No stable release found."
    echo ""
    echo "If this is a new project, only alpha releases may exist."
    echo "Try: install.sh --alpha"
    exit 1
  }
  # Parse tag_name from the single release object
  TAG="$(echo "$RELEASE_JSON" | sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p' | head -1)"
  VERSION="${TAG#v}"
else
  # Get releases list and find first prerelease
  # Use per_page=20 to limit response size — we only need the latest prerelease
  RELEASES_JSON="$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases?per_page=20" 2>/dev/null)" || {
    echo "Failed to fetch releases. Is the repo public?"
    exit 1
  }

  # Find the first release where prerelease is true and tag starts with alpha-
  # Parse line by line: track tag_name and prerelease fields, emit when both match
  TAG=""
  current_tag=""
  while IFS= read -r line; do
    case "$line" in
      *'"tag_name"'*)
        current_tag="$(echo "$line" | sed 's/.*"tag_name" *: *"\([^"]*\)".*/\1/')"
        ;;
      *'"prerelease"'*true*)
        if [[ -n "$current_tag" ]] && [[ "$current_tag" == alpha-* ]]; then
          TAG="$current_tag"
          break
        fi
        ;;
      *'"prerelease"'*false*)
        current_tag=""
        ;;
    esac
  done <<< "$RELEASES_JSON"

  VERSION="$TAG"
fi

if [[ -z "$TAG" ]]; then
  echo "No ${CHANNEL} release found."
  exit 1
fi

echo "Latest ${CHANNEL} version: ${VERSION} (${TAG})"

# Check for existing brew installation
if command -v brew >/dev/null 2>&1; then
  if brew list --formula "$BIN_NAME" >/dev/null 2>&1; then
    echo ""
    echo "Warning: ${BIN_NAME} is also installed via Homebrew."
    echo "Having both may cause confusion — see 'which ${BIN_NAME}' to check"
    echo "which version runs. Consider using only one install method."
    echo ""
  fi
fi

# Download
DOWNLOAD_URL="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${TAG}/${ASSET_NAME}"
VERSION_DIR="${VERSIONS_DIR}/${VERSION}"
BINARY_PATH="${VERSION_DIR}/${BIN_NAME}"

mkdir -p "$VERSION_DIR"

echo "Downloading ${ASSET_NAME}..."
curl -fsSL -o "$BINARY_PATH" "$DOWNLOAD_URL" || {
  echo "Download failed. Asset may not exist for this platform."
  echo "URL: ${DOWNLOAD_URL}"
  exit 1
}

chmod +x "$BINARY_PATH"

# Create symlink
mkdir -p "$SYMLINK_DIR"
SYMLINK_PATH="${SYMLINK_DIR}/${BIN_NAME}"

# Atomic symlink rotation
TMP_LINK="${SYMLINK_PATH}.tmp.$$"
ln -sf "$BINARY_PATH" "$TMP_LINK"
mv -f "$TMP_LINK" "$SYMLINK_PATH"

echo ""
echo "${BIN_NAME} ${VERSION} installed to ${SYMLINK_PATH}"

# Check PATH
case ":${PATH}:" in
  *":${SYMLINK_DIR}:"*) ;;
  *)
    echo ""
    echo "Add ${SYMLINK_DIR} to your PATH:"
    echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
    echo ""
    echo "Then restart your shell or run:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac

echo ""
echo "Requires: Claude Code CLI (claude)"
echo "  https://docs.anthropic.com/en/docs/claude-code"
echo ""
echo "Auth: uses your Claude Code credentials, or set ANTHROPIC_API_KEY."
echo ""
echo "To uninstall:"
echo "  curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | bash -s -- --uninstall"
echo ""
echo "Run '${BIN_NAME} --version' to verify."
