cask "apic-gui" do
  arch arm: "aarch64", intel: "x86_64"

  version "1.2.0"
  sha256 arm:   "263c1ae48287b653636ebcb5fdfa5e45f63e0e10cadd81578dbbb2612d2b6d98",
         intel: "b71c7eb3817121b16d72f317d891308b9fc77b1782eb21b10c2e09e7f4055777"

  url "https://github.com/rizukirr/apic/releases/download/v#{version}/apic-gui-v#{version}-#{arch}-apple-darwin.app.zip"
  name "apic"
  desc "Desktop GUI for designing and collaborating on API contracts"
  homepage "https://github.com/rizukirr/apic"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "apic.app"

  # The app is not signed or notarized, so clear the quarantine flag that
  # Gatekeeper would otherwise use to block the first launch.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/apic.app"]
  end

  zap trash: [
    "~/Library/Application Support/apic-gui",
    "~/Library/Saved Application State/com.rizukirr.apic-gui.savedState",
  ]
end
