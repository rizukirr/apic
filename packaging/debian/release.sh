#!/usr/bin/env bash
# Build, sign and upload the apic source package to ppa:rizukirr/apic.
#
# Run from anywhere in the repo:
#   packaging/debian/release.sh            # builds the tag matching the changelog
#   packaging/debian/release.sh HEAD       # dry run against the working tree
#
# The version is read from debian/changelog and never passed in, so the
# tarball, the build directory and the .changes file cannot drift apart.
#
# CachyOS has no Debian tooling, so everything that needs dpkg runs inside a
# throwaway ubuntu:26.04 podman container. Launchpad's builders do the real
# compile from the source package this uploads.
set -euo pipefail

# The OpenPGP key registered and confirmed on launchpad.net/~rizukirr/+editpgpkeys.
# The full fingerprint is unambiguous; `gpg --local-user` fails loudly on a
# missing key rather than falling back to some other default secret key.
GPG_KEY_ID="${GPG_KEY_ID:-0918EF57B66E6636BD2AA90449026E8CED45A563}"
PPA="ppa:rizukirr/apic"
IMAGE="ubuntu:26.04"

root="$(git rev-parse --show-toplevel)"
out="$root/packaging/debian"
changelog="$out/debian/changelog"

# 0.5.1-1~resolute1 -> full, upstream 0.5.1, revision 1~resolute1, rev_num 1
full="$(sed -n '1s/^[^(]*(\([^)]*\)).*/\1/p' "$changelog")"
[ -n "$full" ] || { echo "error: cannot parse a version from $changelog" >&2; exit 1; }
upstream="${full%%-*}"
revision="${full#*-}"
rev_num="${revision%%[!0-9]*}"

ref="${1:-v$upstream}"
git -C "$root" rev-parse --verify --quiet "$ref^{commit}" >/dev/null \
  || { echo "error: git ref '$ref' does not exist" >&2; exit 1; }

work="$out/build/apic-$upstream"
orig="$out/apic_$upstream.orig.tar.gz"
changes="$out/build/apic_${full}_source.changes"

echo "==> apic $full (upstream $upstream, revision $revision) from $ref"

rm -rf "$out/build"
mkdir -p "$work"

# The orig tarball is IMMUTABLE on Launchpad: once an upstream version is
# accepted, every later Debian revision must reference the exact same bytes.
# A re-vendored tree is not byte-reproducible, so regenerating it would be
# rejected with 'orig.tar.gz already exists ... but uploaded version has
# different contents'. Rev 1 builds it, later revs reuse the accepted one.
if [ "$rev_num" = "1" ]; then
  echo "==> revision 1: building the orig tarball from $ref"
  git -C "$root" archive "$ref" | tar -x -C "$work"

  # Launchpad builders have no network access, so every crate dependency is
  # vendored into the tarball. --versioned-dirs keeps the directory names
  # stable across runs so the tarball is reproducible for a given Cargo.lock.
  ( cd "$work" && cargo vendor --versioned-dirs --locked vendor >/dev/null )

  # Append the vendor redirect to the tracked .cargo/config.toml rather than
  # overwrite it: that file already carries the Windows crt-static block.
  cat >> "$work/.cargo/config.toml" <<'CARGOCFG'

# Added by packaging/debian/release.sh for the offline Launchpad build.
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CARGOCFG

  # A vendor tree whose checksum manifests name files that are not on disk
  # fails only once the package build reaches that crate, twenty minutes in and
  # one crate at a time. Check every crate here instead, where it costs a second.
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

  tar czf "$orig" -C "$out/build" "apic-$upstream"
  echo "==> wrote $orig"
  orig_flag="-sa"
else
  if [ ! -f "$orig" ]; then
    echo "error: revision $rev_num needs the already-accepted orig tarball at" >&2
    echo "       $orig" >&2
    echo "       It is immutable on Launchpad. Download the exact accepted" >&2
    echo "       tarball and put it there before re-running." >&2
    exit 1
  fi
  echo "==> revision $rev_num: reusing the accepted orig tarball"
  tar xzf "$orig" -C "$out/build"
  orig_flag="-sd"
fi

# dpkg-source looks for the orig tarball in the PARENT of the build tree, so it
# has to sit in build/ next to apic-<version>/. The retained copy stays one
# level up in packaging/debian/, out of reach of the `rm -rf build` above, so a
# later Debian revision can still reuse the accepted bytes.
cp "$orig" "$out/build/"

# debuild needs debian/ at the root of the build tree. The tracked copy lives
# at packaging/debian/debian, and it is packaging, never part of the orig, so
# it is copied in after the tarball is rolled.
cp -a "$out/debian" "$work/debian"

# gpg here runs with use-keyboxd, so the public keyring is a sqlite database
# owned by the host keyboxd daemon. Bind-mounting ~/.gnupg alone makes the
# container spawn a SECOND keyboxd against that same database and the two
# deadlock ("database_open ... waiting for lock", then a timeout). Mounting the
# host socket directory makes the container a client of the host daemons
# instead, which is also why it cannot raise its own pinentry: unlock the key
# here first, and the container signs against the warm agent cache.
echo "==> unlocking the signing key, the container signs through this agent"
: | gpg --local-user "$GPG_KEY_ID" --clearsign --output /dev/null \
  || { echo "error: could not unlock $GPG_KEY_ID on the host" >&2; exit 1; }

echo "==> building and signing the source package in $IMAGE"
# A bare `[ -t 0 ] && tty_flags=(-it)` would abort the script under `set -e`
# whenever stdin is not a terminal, because the AND-list then returns 1.
tty_flags=()
if [ -t 0 ]; then
  tty_flags=(-it)
fi
podman run --rm "${tty_flags[@]}" \
  -v "$out":/work \
  -v "$HOME/.gnupg":/root/.gnupg \
  -v "/run/user/$(id -u)/gnupg":/run/user/0/gnupg \
  -e "GPG_KEY_ID=$GPG_KEY_ID" \
  -e "ORIG_FLAG=$orig_flag" \
  -w "/work/build/apic-$upstream" \
  "$IMAGE" bash -lc '
    set -e
    apt-get update >/dev/null
    DEBIAN_FRONTEND=noninteractive apt-get install -y \
      devscripts debhelper dpkg-dev fakeroot gnupg >/dev/null
    # -d skips the build-dependency check: this is a source-only build and the
    # container deliberately has no Rust toolchain, Launchpad compiles it.
    debuild -S "$ORIG_FLAG" -d -k"$GPG_KEY_ID"'

[ -f "$changes" ] || { echo "error: expected $changes, the build produced none" >&2; exit 1; }
echo "==> built $changes"

echo
echo "==> upload with:"
echo "  dput $PPA $changes"
echo
reply=""
read -r -p "Upload now? [y/N] " reply || true
case "$reply" in
  [Yy]*)
    dput "$PPA" "$changes"
    echo "==> uploaded, check the build status at"
    echo "    https://launchpad.net/~rizukirr/+archive/ubuntu/apic/+packages"
    ;;
  *)
    echo "==> skipped the upload"
    ;;
esac
