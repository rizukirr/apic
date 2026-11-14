class Apic < Formula
  desc "Git-friendly CLI for designing and collaborating on API contracts"
  homepage "https://github.com/rizukirr/apic"
  version "1.0.0"
  license "GPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "801ededd4c2580789f5b5f8b1f56b55b00d7c237c491b2e92f1dc417cd29f697"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "83d89ecf08add9f0b84ca4b1a32374a25f2df4e0181352b6088a484bc9ccefe6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "d2eecb80e52730c9a6a6db582be3720323b459372aed102844492a752e61f1b4"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "c7684c280f78b97d4771273c47efae39e6fc09b49cc39d906aa50ee513d0efdd"
    end
  end

  def install
    bin.install "apic"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/apic --version")
  end
end
