# Homebrew formula for aclaude (stable channel)
# Updated automatically by CI on tagged releases (v*)

class Aclaude < Formula
  desc "BYOA agent CLI with persona theming"
  homepage "https://github.com/arcaven/aclaude"
  version "VERSION_PLACEHOLDER"
  license "MIT"

  if Hardware::CPU.arm?
    url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-darwin-arm64"
    sha256 "SHA256_ARM64_PLACEHOLDER"
  else
    url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-darwin-amd64"
    sha256 "SHA256_AMD64_PLACEHOLDER"
  end

  # Linux support
  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-linux-arm64"
      sha256 "SHA256_LINUX_ARM64_PLACEHOLDER"
    else
      url "https://github.com/arcaven/aclaude/releases/download/TAG_PLACEHOLDER/aclaude-linux-amd64"
      sha256 "SHA256_LINUX_AMD64_PLACEHOLDER"
    end
  end

  def install
    cpu = Hardware::CPU.arm? ? "arm64" : "amd64"
    os = OS.mac? ? "darwin" : "linux"
    binary = "aclaude-#{os}-#{cpu}"
    bin.install binary => "aclaude"
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
