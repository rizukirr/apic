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

Everything is one script. It reads the version from
`packaging/debian/debian/changelog`, so nothing is passed in and the tarball,
the build directory and the `.changes` file cannot drift apart. Run from
anywhere in the repo:

```bash
packaging/debian/release.sh          # builds the tag matching the changelog
packaging/debian/release.sh HEAD     # dry run against the working tree
```

It rolls the vendored orig tarball, copies `debian/` into the build tree,
builds and signs the source package inside a throwaway `ubuntu:26.04` podman
container with `~/.gnupg` bind-mounted, then prompts before running `dput`.
Answering anything but `y` leaves the signed `.changes` in
`packaging/debian/build/` to upload by hand later:

```bash
dput ppa:rizukirr/apic packaging/debian/build/apic_0.5.1-1~resolute1_source.changes
```

The `ppa:` shortcut needs dput-ng. Classic `dput` 1.x cannot expand it and
needs a matching stanza in `~/.dput.cf` instead.

Signing uses the fingerprint registered on
`launchpad.net/~rizukirr/+editpgpkeys`, hardcoded in the script and overridable
with `GPG_KEY_ID=...`. `gpg --list-secret-keys --keyid-format=long` prints it.
After the upload, confirm on the Launchpad build dashboard that both the
`amd64` and `arm64` builds reach a successful state.

### The orig tarball is immutable

Once Launchpad accepts an upstream version, every later Debian revision must
reference the exact same tarball bytes. A re-vendored tree is not byte
reproducible, so regenerating it is rejected with `orig.tar.gz already exists
... but uploaded version has different contents`.

`release.sh` handles this from the changelog revision. Revision `-1` builds the
tarball from the tag and passes `-sa`. Revision `-2` and later reuse
`packaging/debian/apic_<version>.orig.tar.gz` as the source tree and pass
`-sd`, and the script stops with an error if that file is absent. Do not delete
an accepted tarball, and if it is gone, download the exact accepted one from
Launchpad rather than rebuilding it.

**Upload status:** unblocked. Tag `v0.5.0` declared `eframe 0.36` and
`rust-version 1.97`, which cannot build on Ubuntu 26.04. Tag `v0.5.1` carries
the `eframe 0.35` downgrade and `rust-version 1.92`, so it is the first tag a
source package can be produced from.

## Local test commands (no publish)

Confirm the payload before every release.

```bash
# 1. List the files each .deb installs.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  dpkg -c apic_0.5.1-1~resolute1_amd64.deb
  dpkg -c apic-gui_0.5.1-1~resolute1_amd64.deb'

# 2. Run lintian and record every tag it reports.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y lintian >/dev/null
  lintian --no-tag-display-limit *.changes || true'

# 3. Install into a clean container and smoke-test both binaries.
podman run --rm -v "$PWD/packaging/debian/build":/work -w /work ubuntu:26.04 bash -lc '
  set -e
  apt-get update >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y ./apic_0.5.1-1~resolute1_amd64.deb ./apic-gui_0.5.1-1~resolute1_amd64.deb desktop-file-utils >/dev/null
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

Add the new entry at the top of `packaging/debian/debian/changelog` and run
`release.sh`. That is the whole procedure: the version lives in the changelog
only, and the script derives the tarball name, the build directory, the git
tag it archives (`v<upstream version>`, override by passing a ref) and the
`.changes` filename from it.

Bumping only the Debian revision, a packaging fix with no upstream change,
requires the accepted orig tarball to still be present, see **The orig tarball
is immutable** above.
