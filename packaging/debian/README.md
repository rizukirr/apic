# Launchpad PPA: apic-cli + apic-gui

One `debian/` tree that builds `apic` (CLI/TUI) and `apic-gui` (desktop GUI)
for Ubuntu, hosted on the Launchpad PPA `ppa:rizukirr/apic`.

The PPA must already exist on Launchpad with the `amd64` and `arm64`
architectures enabled, and the maintainer's GPG key must be registered on the
Launchpad account before any upload is attempted.

> **You are on CachyOS, which has no Debian packaging tooling.** Everything
> that needs `dpkg`, `debuild`, `lintian`, or `dput` runs inside a throwaway
> `ubuntu:26.04` podman container. Launchpad's own builders do the real
> compile from the uploaded source package.

## One-time setup

```bash
sudo pacman -S --needed podman

# On Launchpad: create ppa:rizukirr/apic with amd64 and arm64 enabled, and
# register the maintainer's GPG public key against the Launchpad account.
```

## Release steps

Run from the repo root.

1. Roll the orig tarball for the release tag:

   ```bash
   packaging/debian/mk-orig.sh 0.5.0 v0.5.0
   ```

2. Build and sign the source package in the container, with `~/.gnupg`
   bind-mounted so `debuild` can reach the maintainer's key:

   ```bash
   podman run --rm -v "$PWD/packaging/debian":/work -v "$HOME/.gnupg":/root/.gnupg -w /work/build/apic-0.5.0 ubuntu:26.04 bash -lc '
     set -e
     apt-get update >/dev/null
     DEBIAN_FRONTEND=noninteractive apt-get install -y devscripts dput >/dev/null
     debuild -S -sa -k"$GPG_KEY_ID"'
   ```

3. Upload the signed source changes file:

   ```bash
   dput ppa:rizukirr/apic packaging/debian/build/apic_0.5.0-1~resolute1_source.changes
   ```

`gpg --list-secret-keys --keyid-format=long` gives the value to substitute for
`$GPG_KEY_ID`. After the upload, confirm on the Launchpad build dashboard that
both the `amd64` and `arm64` builds reach a successful state.

**Upload status:** the first upload to `ppa:rizukirr/apic` is blocked until an
upstream `0.5.1` release exists. Tag `v0.5.0` declares `eframe 0.36` and
`rust-version 1.97`, which cannot build on Ubuntu 26.04. `0.5.1` must contain
the `eframe 0.35` downgrade before a source package is produced.

## Local test commands (no publish)

Confirm the payload before every release.

```bash
# 1. List the files each .deb installs.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  dpkg -c apic_0.5.0-1~resolute1_amd64.deb
  dpkg -c apic-gui_0.5.0-1~resolute1_amd64.deb'

# 2. Run lintian and record every tag it reports.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y lintian >/dev/null
  lintian --no-tag-display-limit *.changes || true'

# 3. Install into a clean container and smoke-test both binaries.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  set -e
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y ./apic_0.5.0-1~resolute1_amd64.deb ./apic-gui_0.5.0-1~resolute1_amd64.deb desktop-file-utils >/dev/null
  apic --version
  desktop-file-validate /usr/share/applications/apic-gui.desktop
  ldd /usr/bin/apic-gui | grep "not found" && exit 1 || true'
```

## Lintian tags observed

- `appstream-metadata-validation-failed` (`apic-gui`): the container's
  `appstreamcli` flags the bundled AppStream metainfo file; the metadata still
  installs and is read correctly by desktop environments, so the warning is
  accepted.
- `debug-file-with-no-debug-symbols` (`apic-dbgsym`, `apic-gui-dbgsym`): the
  release binaries are built without embedded debug info, so the generated
  `-dbgsym` packages carry empty debug files; harmless, accepted.
- `no-manual-page` (`apic`, `apic-gui`): neither binary ships a man page yet;
  cosmetic, accepted for this release.

## Bump for a new release

Edit only the `Version` in `packaging/debian/debian/changelog` (add a new
entry at the top). No other file needs to change; `mk-orig.sh` and the
container build steps read the version from the changelog and the tag you
pass them.
