# COPR: apic-cli + apic-gui

One RPM spec that builds prebuilt **`apic-cli`** (CLI/TUI) and **`apic-gui`**
(desktop GUI) for Fedora, hosted on [COPR](https://copr.fedorainfracloud.org/).

## Test locally (Fedora container)

```bash
cd packaging/copr
podman run --rm -v "$(pwd)":/work -w /work fedora:latest bash -lc '
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

## Publish to COPR

Needs a Fedora account and an API token in `~/.config/copr`
(https://copr.fedorainfracloud.org/api/).

```bash
# one-time: create the project (or via the web UI)
copr-cli create apic \
  --chroot fedora-41-x86_64 --chroot fedora-41-aarch64

# build a source RPM, then submit it
cd packaging/copr
rpmbuild -bs apic.spec   # produces an .src.rpm (do this in a Fedora env/container)
copr-cli build apic ~/rpmbuild/SRPMS/apic-cli-0.3.1-1.*.src.rpm
```

## Bump for a new release

Edit `Version`, reset `Release` to `1%{?dist}`, add a `%changelog` entry, then
rebuild and submit.
