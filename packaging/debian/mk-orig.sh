#!/usr/bin/env bash
# Roll the Debian orig tarball for a release.
#
# Launchpad builders have no network access, so every crate dependency is
# vendored into the tarball and .cargo/config.toml is extended to point cargo
# at it.
#
# Run from anywhere in the repo: packaging/debian/mk-orig.sh 0.5.0 [git-ref]
set -euo pipefail

ver="${1:?usage: mk-orig.sh <version> [git-ref], e.g. mk-orig.sh 0.5.0 v0.5.0}"
# The ref defaults to HEAD so the packaging can be verified before a release is
# tagged. A real upload must name the release tag that it ships.
ref="${2:-HEAD}"

root="$(git rev-parse --show-toplevel)"
out="$root/packaging/debian"
work="$out/build/apic-$ver"

rm -rf "$out/build"
mkdir -p "$work"
git -C "$root" archive "$ref" | tar -x -C "$work"

# --versioned-dirs keeps the directory names stable across runs so the tarball
# is reproducible for a given Cargo.lock.
( cd "$work" && cargo vendor --versioned-dirs --locked vendor >/dev/null )

# Append the vendor redirect to the tracked .cargo/config.toml rather than
# overwrite it: that file already carries the Windows crt-static block.
cat >> "$work/.cargo/config.toml" <<'CARGOCFG'

# Added by packaging/debian/mk-orig.sh for the offline Launchpad build.
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CARGOCFG

# A vendor tree whose checksum manifests name files that are not on disk fails
# only once the package build reaches that crate, twenty minutes in and one
# crate at a time. Check every crate here instead, where it costs a second.
python3 - "$work/vendor" <<'CHECK'
import json, pathlib, sys

root = pathlib.Path(sys.argv[1])
missing = []
crates = 0
for manifest in sorted(root.glob("*/.cargo-checksum.json")):
    crates += 1
    for name in json.loads(manifest.read_text()).get("files", {}):
        if not (manifest.parent / name).exists():
            missing.append(f"{manifest.parent.name}: {name}")

if missing:
    print(f"vendor tree inconsistent, {len(missing)} checksummed files absent:", file=sys.stderr)
    for line in missing[:10]:
        print(f"  {line}", file=sys.stderr)
    sys.exit(1)

print(f"vendor tree consistent across {crates} crates")
CHECK

tar czf "$out/apic_$ver.orig.tar.gz" -C "$out/build" "apic-$ver"
echo "wrote $out/apic_$ver.orig.tar.gz"
