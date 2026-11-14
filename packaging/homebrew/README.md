# Homebrew: rizukirr/tap

A formula for the `apic` CLI (macOS and Linux) and a cask for the `apic-gui` app (macOS), both pulling the prebuilt binaries from the GitHub release.

They ship from a personal tap, not homebrew-core or homebrew-cask. homebrew-core expects a project with more stars and forks than apic has today, and homebrew-cask no longer accepts apps that are not signed and notarized.

Users install with:

```bash
brew install rizukirr/tap/apic        # CLI
brew install --cask rizukirr/tap/apic-gui   # GUI, macOS only
```

## Create the tap (first time only)

The tap is a public GitHub repo named `homebrew-tap`. Homebrew maps `rizukirr/tap` to `github.com/rizukirr/homebrew-tap`.

```bash
gh repo create rizukirr/homebrew-tap --public --description "Homebrew tap for apic"
git clone git@github.com:rizukirr/homebrew-tap.git
mkdir -p homebrew-tap/Formula homebrew-tap/Casks
cp apic.rb homebrew-tap/Formula/
cp apic-gui.rb homebrew-tap/Casks/
cd homebrew-tap
git add . && git commit -m "apic 1.0.0" && git push
```

## Test locally

On a Mac (or Linux for the formula) with Homebrew installed:

```bash
brew tap rizukirr/tap
brew install rizukirr/tap/apic && apic --version
brew test rizukirr/tap/apic
brew audit --strict --online rizukirr/tap/apic
brew install --cask rizukirr/tap/apic-gui
brew audit --cask --strict --online rizukirr/tap/apic-gui
```

## Bump for a new release

After a new vX.Y.Z release exists, update `version` and every `sha256` in both files. The hashes come from the `.sha256` files on the release:

```bash
gh release download vX.Y.Z -R rizukirr/apic -p '*apple-darwin*.sha256' -p '*linux-gnu.sha256' -D /tmp/apic-sums
cat /tmp/apic-sums/*.sha256 | grep -E 'apic-v|\.app\.zip'
```

`apic.rb` uses the four `apic-vX.Y.Z-<target>.tar.gz` hashes. `apic-gui.rb` uses the two `apic-gui-vX.Y.Z-<target>.app.zip` hashes. Copy both files into the tap and push.
