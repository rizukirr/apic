# Flatpak (Flathub): io.github.rizukirr.apic

Source-build Flatpak of the `apic-gui` desktop app for
[Flathub](https://flathub.org/).

- **App id:** `io.github.rizukirr.apic`
- **Manifest:** `io.github.rizukirr.apic.yml`
- **Published repo:** https://github.com/flathub/io.github.rizukirr.apic
  (this is where version updates go — **not** the `flathub/flathub` monorepo)

## Update to a new version

Replace `vX.Y.Z` below with the release tag you are shipping.

### 1. Prep the release in this repo

- Add a `<release>` entry for the new version to
  `io.github.rizukirr.apic.metainfo.xml` (the build installs this file from the
  tagged commit, so it must be in the tag Flathub points at):

  ```xml
  <release version="X.Y.Z" date="YYYY-MM-DD"/>
  ```

- Regenerate `cargo-sources.json` **if `Cargo.lock` changed** since the last
  release (it vendors every crate for the offline sandbox build — a stale file
  makes the build fail):

  ```bash
  python -m venv /tmp/fcg && /tmp/fcg/bin/pip install tomlkit aiohttp
  curl -sSL -o /tmp/fcg-gen.py \
    https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
  /tmp/fcg/bin/python /tmp/fcg-gen.py Cargo.lock -o packaging/flatpak/cargo-sources.json
  ```

### 2. (Optional) Build & test locally

```bash
sudo pacman -S --needed flatpak flatpak-builder            # Arch / CachyOS
flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
```

Test the current working tree via a throwaway local manifest (gitignored via
`*.local.yml`) whose `sources:` block builds the local checkout:

```bash
cd packaging/flatpak
cp io.github.rizukirr.apic.yml io.github.rizukirr.apic.local.yml
```

Edit its `sources:` block to read:

```yaml
    sources:
      - type: dir
        path: ../..
        skip:
          - target
          - .git
      - cargo-sources.json
```

Build, install, and run (first run pulls the ~1.5 GB SDK and compiles eframe in
the sandbox — slow):

```bash
flatpak-builder --user --install --force-clean --install-deps-from=flathub \
  --state-dir=/tmp/apic-fp/state /tmp/apic-fp/build \
  io.github.rizukirr.apic.local.yml
flatpak run io.github.rizukirr.apic
```

### 3. Publish the update

Push the bump to the published Flathub repo — Flathub rebuilds automatically.

```bash
# clone your fork of the per-app repo (fork it on GitHub first if needed)
gh repo fork flathub/io.github.rizukirr.apic --clone=true
cd io.github.rizukirr.apic
git checkout -b update-X.Y.Z

# in io.github.rizukirr.apic.yml, bump the git source:
#     tag: vX.Y.Z
#     commit: <sha of the vX.Y.Z tag>   # keep it pinned to a commit
# and copy over the regenerated vendored sources:
cp ~/Projects/apic/packaging/flatpak/cargo-sources.json .

git add io.github.rizukirr.apic.yml cargo-sources.json
git commit -m "Update to X.Y.Z"
git push -u origin update-X.Y.Z

gh pr create --repo flathub/io.github.rizukirr.apic --base master --title "Update to X.Y.Z"
```

The Flathub buildbot builds the PR and comments a test-install command. When the
check is green, **merge it** — that publishes the update. (Or set up
`flatpak-external-data-checker` to open these update PRs automatically.)
