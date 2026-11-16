# COPR: apic-cli + apic-gui

One RPM spec that builds prebuilt **`apic-cli`** (CLI) and **`apic-gui`** (desktop GUI) for Fedora, hosted on [COPR](https://copr.fedorainfracloud.org/).

> **You are on Arch / CachyOS, which has no `rpmbuild`/`dnf`.** Everything that needs RPM tooling runs inside a throwaway **Fedora `podman` container**. COPR's own servers do the real package build. You only hand them a source RPM (`.src.rpm`).

## One-time setup (already done, for reference)

```bash
# tooling on Arch
sudo pacman -S --needed copr-cli podman

# COPR API token: save the block from https://copr.fedorainfracloud.org/api/
#   into ~/.config/copr   (then: copr-cli whoami  -> prints your username)

# create the COPR project ONCE (it already exists as rizukirr/apic, do NOT repeat):
copr-cli create apic \
  --chroot fedora-43-x86_64 --chroot fedora-43-aarch64 \
  --chroot fedora-44-x86_64 --chroot fedora-44-aarch64 \
  --chroot fedora-rawhide-x86_64 --chroot fedora-rawhide-aarch64 \
  --description "Git-friendly API contracts: apic CLI and apic-gui desktop GUI." \
  --instruction "dnf copr enable rizukirr/apic && dnf install apic-cli apic-gui"
```

### Optional: a builder image

Every run below installs `rpm-build` and `rpmdevtools` first, which can take a long time on a slow mirror. Build an image with them once and reuse it:

```bash
podman build -t apic-rpm - <<'EOF'
FROM fedora:latest
RUN dnf -y install rpm-build rpmdevtools && dnf clean all
EOF
```

Then replace `fedora:latest` with `apic-rpm` in the commands below and drop the `dnf -y install` line. Rebuild the image when a new Fedora release comes out.

## Publish a release to COPR

After bumping the spec (see the last section), run:

```bash
./packaging/copr/release.sh
```

It checks that the top `%changelog` entry matches `Version:`, builds the source RPM in a container (using the `apic-rpm` image when it exists), submits it with `copr-cli build`, and deletes the local `.src.rpm`. The manual steps it runs are below.

Run every command below **from the repo root**. The container mounts `packaging/copr` as `/work`, so the mount path has to point at the folder that holds `apic.spec`. If it does not, the run fails after the `dnf` step with `cp: cannot stat 'apic.spec': No such file or directory`.

```bash
# 1. Build the source RPM inside a Fedora container (no rpmbuild on Arch).
#    This drops apic-cli-<ver>-1.fc*.src.rpm into packaging/copr/.
podman run --rm -v "$(pwd)/packaging/copr":/work -w /work fedora:latest bash -lc '
  set -e
  dnf -y install rpm-build rpmdevtools >/dev/null
  rpmdev-setuptree
  cp apic.spec ~/rpmbuild/SPECS/
  cd ~/rpmbuild/SPECS
  spectool -g -R apic.spec
  rpmbuild -bs apic.spec
  cp ~/rpmbuild/SRPMS/*.src.rpm /work/'

# 2. Submit that .src.rpm to the existing COPR project (NO `create` needed).
copr-cli build apic packaging/copr/apic-cli-*.src.rpm

# 3. (optional) clean up the local .src.rpm afterwards
rm -f packaging/copr/*.src.rpm
```

Delete any older `.src.rpm` in `packaging/copr` before step 1, so the glob in step 2 only matches the new one.

`copr-cli build` blocks and streams the build log. It builds both `apic-cli` and `apic-gui` for every chroot. Watch it on the web dashboard at <https://copr.fedorainfracloud.org/coprs/rizukirr/apic/builds/> too.

Once green, users install with:

```bash
sudo dnf copr enable rizukirr/apic
sudo dnf install apic-cli apic-gui
```

## Test the spec locally (optional, no publish)

Builds the binary RPMs in a container and lists their contents, which is handy after editing the spec. Run from the repo root:

```bash
podman run --rm -v "$(pwd)/packaging/copr":/work -w /work fedora:latest bash -lc '
  set -e
  dnf -y install rpm-build rpmdevtools >/dev/null
  rpmdev-setuptree
  cp apic.spec ~/rpmbuild/SPECS/
  cd ~/rpmbuild/SPECS
  spectool -g -R apic.spec
  rpmbuild -bb apic.spec
  echo "=== apic-cli payload ==="; rpm -qlp ~/rpmbuild/RPMS/*/apic-cli-*.rpm
  echo "=== apic-gui payload ==="; rpm -qlp ~/rpmbuild/RPMS/*/apic-gui-*.rpm
'
```

## Bump for a new release

In `apic.spec`, set `Version:` to the new number, keep `Release: 1%{?dist}`, and add a `%changelog` entry at the top. The `Source` URLs use `%{version}`, so no other edits are needed. Then run the publish steps above.
