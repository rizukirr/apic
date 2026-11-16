#!/usr/bin/env bash
# Build the source RPM in a Fedora container and submit it to COPR (rizukirr/apic).
# Run after bumping Version: and %changelog in apic.spec. Works from any folder.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")"

version=$(awk '/^Version:/ {print $2; exit}' apic.spec)
echo "==> apic.spec version: $version"

if ! grep -A1 '^%changelog' apic.spec | grep -q -- "- $version-"; then
  echo "error: the top %changelog entry is not for $version, update apic.spec first" >&2
  exit 1
fi

copr-cli whoami >/dev/null

# Reuse the apic-rpm builder image when it exists (see README), else install the tools each run.
if podman image exists apic-rpm; then
  image=apic-rpm
  install=true
else
  image=fedora:latest
  install='dnf -y install rpm-build rpmdevtools >/dev/null'
fi

rm -f ./*.src.rpm

echo "==> building source RPM in $image"
podman run --rm -v "$PWD":/work -w /work "$image" bash -lc "
  set -e
  $install
  rpmdev-setuptree
  cp apic.spec ~/rpmbuild/SPECS/
  cd ~/rpmbuild/SPECS
  spectool -g -R apic.spec
  rpmbuild -bs apic.spec
  cp ~/rpmbuild/SRPMS/*.src.rpm /work/"

srpm=$(ls apic-cli-"$version"-*.src.rpm)
echo "==> submitting $srpm to COPR"
copr-cli build apic "$srpm"

rm -f "$srpm"
echo "==> done: https://copr.fedorainfracloud.org/coprs/rizukirr/apic/builds/"
