# Homebrew formula for aclaude-a (alpha channel)
# Updated automatically by CI on every push to main

class AclaudeA < Formula
  desc "BYOA agent CLI with persona theming (alpha channel)"
  homepage "https://github.com/arcaven/aclaude"
  version "VERSION_PLACEHOLDER"
  license "MIT"

  if Hardware::CPU.arm?
    url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-a-darwin-arm64"
    sha256 "SHA256_ARM64_PLACEHOLDER"
  else
    url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-a-darwin-amd64"
    sha256 "SHA256_AMD64_PLACEHOLDER"
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-a-linux-arm64"
      sha256 "SHA256_LINUX_ARM64_PLACEHOLDER"
    else
      url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-a-linux-amd64"
      sha256 "SHA256_LINUX_AMD64_PLACEHOLDER"
    end
  end

  def install
    cpu = Hardware::CPU.arm? ? "arm64" : "amd64"
    os = OS.mac? ? "darwin" : "linux"
    binary = "aclaude-a-#{os}-#{cpu}"
    bin.install binary => "aclaude-a"
  end

  def caveats
    <<~EOS
      aclaude-a is the alpha channel of aclaude. Updates on every push to main.
      For the stable channel, use: brew install arcaven/tap/aclaude

      Requires Claude Code CLI (claude) to be installed.
      See: https://docs.anthropic.com/en/docs/claude-code
    EOS
  end

  test do
    assert_match "aclaude", shell_output("#{bin}/aclaude-a --version 2>&1")
  end
end
