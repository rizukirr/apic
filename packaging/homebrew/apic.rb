class Apic < Formula
  desc "Git-friendly CLI for designing and collaborating on API contracts"
  homepage "https://github.com/rizukirr/apic"
  version "1.2.0"
  license "GPL-3.0-only"

  on_macos do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "d4d8ad18e30dcc791452bcf557bb8ff4f9ba3a87d448da46700827be3af0b85e"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "baf126de78dc8cf2bf4d1937c4ee8ec6b335932330e3b1c85b494512a2588802"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "6afdd19dd51f692bdbb8c38203db6a200a5b7e5114a865fd02121bcbe0f1bf0e"
    end
    on_intel do
      url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "344e93cb58daef1343207a969f5d30059ff0c1b8456322d8995ca3d054f2229b"
    end
  end

  def install
    bin.install "apic"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/apic --version")
  end
end
