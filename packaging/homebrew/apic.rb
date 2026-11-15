class Apic < Formula
  desc "Git-friendly CLI for designing and collaborating on API contracts"
  homepage "https://github.com/rizukirr/apic"
  version "1.1.0"
  license "GPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "a980d61f19584561f3d799a0b31a3c193d4c6e0fdbe0f57c84253136d03b2fec"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "9999925dfeb25b5b06c0ce3b80809d75ac268a12cb8e79abd7065abfab288c7a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "9e747377a93aa8e6996e92c4313cb0c9b10c373092a587a9b3dc58ebb6d1680b"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "8faf5ff3049197a0926728cef20a6a1f41d3749adb6ac30d6a48e11872a80143"
    end
  end

  def install
    bin.install "apic"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/apic --version")
  end
end
