#!/bin/sh
# aclaude installer — downloads the latest release from GitHub and installs it.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/arcaven/aclaude/main/install.sh | sh -s -- --alpha
set -euo pipefail

GITHUB_OWNER="arcaven"
GITHUB_REPO="aclaude"
INSTALL_DIR="$HOME/.local/share/aclaude/versions"
SYMLINK_DIR="$HOME/.local/bin"

# Parse arguments
CHANNEL="stable"
for arg in "$@"; do
  case "$arg" in
    --alpha) CHANNEL="alpha" ;;
    --help|-h)
      echo "Usage: install.sh [--alpha]"
      echo ""
      echo "  --alpha    Install the alpha channel (aclaude-a)"
      echo "  (default)  Install the stable channel (aclaude)"
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
  # Get latest non-prerelease
  RELEASE_INFO="$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest" 2>/dev/null)" || {
    echo "Failed to fetch latest release. Is the repo public?"
    exit 1
  }
  TAG="$(echo "$RELEASE_INFO" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  VERSION="${TAG#v}"
else
  # Get latest prerelease
  RELEASE_INFO="$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases" 2>/dev/null)" || {
    echo "Failed to fetch releases. Is the repo public?"
    exit 1
  }
  # Find first prerelease tag
  TAG="$(echo "$RELEASE_INFO" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  VERSION="$TAG"
fi

if [[ -z "$TAG" ]]; then
  echo "No ${CHANNEL} release found."
  exit 1
fi

echo "Latest ${CHANNEL} version: ${VERSION} (${TAG})"

# Download
DOWNLOAD_URL="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${TAG}/${ASSET_NAME}"
VERSION_DIR="${INSTALL_DIR}/${VERSION}"
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
echo "Run '${BIN_NAME} --version' to verify."
