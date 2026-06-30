# Flatpak (Flathub): io.github.rizukirr.apic

Source-build Flatpak of the `apic-gui` desktop app for
[Flathub](https://flathub.org/).

- **App id:** `io.github.rizukirr.apic`
- **Manifest:** `io.github.rizukirr.apic.yml` (builds the `v0.3.6` tag; `apic-gui`
  picks up `FLATPAK_ID` as its window id, added in `0.3.2`)
- **Status:** submitted to Flathub — https://github.com/flathub/flathub/pull/9041
  (Flathub builds + reviews; merge = published)

## 1. Prerequisites (Arch / CachyOS)

```bash
sudo pacman -S --needed flatpak flatpak-builder
# the user-level installation needs the flathub remote (the build uses --user):
flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo
```

## 2. Build & test locally

Flathub builds the `v0.3.6` git tag. To test the *current working tree* instead,
make a throwaway local manifest (gitignored via `*.local.yml`) whose `sources:`
block builds the local checkout:

```bash
cd packaging/flatpak
cp io.github.rizukirr.apic.yml io.github.rizukirr.apic.local.yml
```

Then edit `io.github.rizukirr.apic.local.yml` so the `sources:` block reads:

```yaml
    sources:
      - type: dir
        path: ../..
        skip:
          - target
          - .git
      - cargo-sources.json
```

Build, install, and run it (first run downloads the ~1.5 GB SDK and compiles
eframe in the sandbox — slow):

```bash
flatpak-builder --user --install --force-clean --install-deps-from=flathub \
  --state-dir=/tmp/apic-fp/state /tmp/apic-fp/build \
  io.github.rizukirr.apic.local.yml
flatpak run io.github.rizukirr.apic
```

## 3. Regenerate the vendored crate sources (after a dependency bump)

`cargo-sources.json` vendors every crate so the sandboxed build can run offline.
Regenerate it whenever `Cargo.lock` changes:

```bash
python -m venv /tmp/fcg && /tmp/fcg/bin/pip install tomlkit aiohttp
curl -sSL -o /tmp/fcg-gen.py \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
/tmp/fcg/bin/python /tmp/fcg-gen.py Cargo.lock -o packaging/flatpak/cargo-sources.json
```

## 4. Submit to Flathub (first time)

```bash
# fork + clone flathub/flathub
gh repo fork flathub/flathub --clone=true --default-branch-only
cd flathub

# branch named exactly the app id, based on the empty new-pr branch
git checkout -b io.github.rizukirr.apic upstream/new-pr

# add ONLY the manifest + vendored sources at the repo root
cp ~/Projects/apic/packaging/flatpak/io.github.rizukirr.apic.yml .
cp ~/Projects/apic/packaging/flatpak/cargo-sources.json .
# (recommended) pin the git source to a commit, not just the tag:
#   under sources: -> type: git, add   commit: <sha of the vX.Y.Z tag>

git add io.github.rizukirr.apic.yml cargo-sources.json
git commit -m "Add io.github.rizukirr.apic"
git push -u origin io.github.rizukirr.apic

# PR against the new-pr branch (NOT master)
gh pr create --repo flathub/flathub --base new-pr \
  --head <you>:io.github.rizukirr.apic --title "Add io.github.rizukirr.apic"
```

Then watch the PR: the Flathub bot builds it and comments; respond to any
reviewer requests by pushing to the same branch on your fork.

## 5. Update after a new release (once published)

Once merged, the app lives at `flathub/io.github.rizukirr.apic`. To ship a new
version: clone that repo, bump the `tag:`/`commit:` in the manifest, regenerate
`cargo-sources.json` (step 3) if dependencies changed, commit, and push — Flathub
rebuilds automatically. (Or set up `flatpak-external-data-checker` to open those
update PRs for you.)
