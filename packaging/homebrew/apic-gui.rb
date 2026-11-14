cask "apic-gui" do
  arch arm: "aarch64", intel: "x86_64"

  version "1.0.0"
  sha256 arm:   "d8b5a8b28de48bcced5fc25819bff1b8a7c488206538cc69e1211bbf9f29b2fb",
         intel: "b15db49e7a94758b1367ccfe69fc43d2176601d426d0e034ffdfc56eba12a6c0"

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
