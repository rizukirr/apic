# Flatpak (Flathub): io.github.rizukirr.apic

Source-build Flatpak of the `apic-gui` desktop app for
[Flathub](https://flathub.org/).

> The manifest targets the **`v0.3.2`** git tag — the first release that contains
> the `FLATPAK_ID` window-app-id fix. Cut that release before submitting to
> Flathub. To test before it exists, build the working tree (below).

## Regenerate the vendored crate sources (after a dependency bump)

```bash
python -m venv /tmp/fcg && /tmp/fcg/bin/pip install aiohttp toml
curl -sSL -o /tmp/fcg-gen.py \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
/tmp/fcg/bin/python /tmp/fcg-gen.py Cargo.lock -o packaging/flatpak/cargo-sources.json
```

## Build & run locally (working tree, before v0.3.2 exists)

Copy the manifest to `io.github.rizukirr.apic.local.yml` (gitignored) and replace
its `sources:` block so it builds the local checkout instead of the git tag:

```yaml
    sources:
      - type: dir
        path: ../..
        skip:
          - target
          - .git
      - cargo-sources.json
```

Then:

```bash
cd packaging/flatpak
flatpak-builder --user --install --force-clean --install-deps-from=flathub \
  build-dir io.github.rizukirr.apic.local.yml
flatpak run io.github.rizukirr.apic
```

## Submit to Flathub

After `v0.3.2` exists, fork `flathub/flathub`, add a branch named
`io.github.rizukirr.apic` containing the manifest + `cargo-sources.json` +
metainfo + desktop, and open a PR. Flathub CI builds and a reviewer approves.
