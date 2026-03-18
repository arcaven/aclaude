# Homebrew formula for aclaude (stable channel)
# Updated automatically by CI on tagged releases (v*)
# macOS only (arm64). Linux users: use install.sh or build from source.

class Aclaude < Formula
  desc "Opinionated wrapper for Claude Code with persona theming"
  homepage "https://github.com/arcaven/aclaude"
  url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-darwin-arm64"
  version "VERSION_PLACEHOLDER"
  sha256 "SHA256_ARM64_PLACEHOLDER"
  license "MIT"

  def install
    bin.install "aclaude-darwin-arm64" => "aclaude"
  end

  def caveats
    <<~EOS
      aclaude requires Claude Code CLI (claude) to be installed.
      See: https://docs.anthropic.com/en/docs/claude-code
    EOS
  end

  test do
    assert_match "aclaude", shell_output("#{bin}/aclaude --version 2>&1")
  end
end
