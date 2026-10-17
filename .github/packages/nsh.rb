# The formula of longbridge/homebrew-tap, written by packages.yml for each
# published release of longbridge/nsh. The whole distribution
# goes in libexec, since nsh finds its runtimes next to bin/nsh.
class Nsh < Formula
  desc "Native UI from TSX modules: the nsh command line and its prebuilt runtimes"
  homepage "https://nsh.dev"
  version "${NSH_VERSION}"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/${NSH_REPO}/releases/download/v${NSH_VERSION}/nsh-macos-arm64.tar.gz"
      sha256 "${NSH_MACOS_ARM64}"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/${NSH_REPO}/releases/download/v${NSH_VERSION}/nsh-linux-x86_64.tar.gz"
      sha256 "${NSH_LINUX_X86_64}"
    end
  end

  def install
    libexec.install Dir["*"]
    bin.install_symlink libexec/"bin/nsh"
  end

  test do
    assert_match "nsh #{version}", shell_output("#{bin}/nsh --version")
  end
end
