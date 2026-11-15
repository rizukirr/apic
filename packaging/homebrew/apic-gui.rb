cask "apic-gui" do
  arch arm: "aarch64", intel: "x86_64"

  version "1.1.0"
  sha256 arm:   "27cac52f525fed59f4ff309dc4c0a8eaa5fbd1262382c4bd2a7faf4c11e3d089",
         intel: "d4315f47b3def4734cd86e4fffbc7947a36c3d7f3649acdf209aef76f50642db"

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
