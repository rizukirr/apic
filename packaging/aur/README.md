# AUR: apic-bin

PKGBUILD for the [`apic-bin`](https://aur.archlinux.org/packages/apic-bin) AUR
package — prebuilt `apic` (CLI/TUI) and `apic-gui` (GUI) for Arch / CachyOS.

## Test locally

```bash
cd packaging/aur
updpkgsums                 # fill sha256sums_* from the release artifacts
makepkg -f --nodeps        # build apic-bin-<ver>-<rel>-<arch>.pkg.tar.zst
makepkg --printsrcinfo > .SRCINFO
namcap PKGBUILD *.pkg.tar.zst   # optional lint, if namcap is installed
```

Install the built package to smoke-test:

```bash
sudo pacman -U apic-bin-*.pkg.tar.zst
apic --version
apic-gui --desktop-entry   # (or just launch apic-gui from the app menu)
```

## Bump for a new release

```bash
# in packaging/aur/, after a new vX.Y.Z release exists:
sed -i 's/^pkgver=.*/pkgver=X.Y.Z/' PKGBUILD
sed -i 's/^pkgrel=.*/pkgrel=1/' PKGBUILD
updpkgsums
makepkg --printsrcinfo > .SRCINFO
```

## Publish to the AUR

```bash
git clone ssh://aur@aur.archlinux.org/apic-bin.git aur-apic-bin
cp PKGBUILD .SRCINFO aur-apic-bin/
cd aur-apic-bin
git add PKGBUILD .SRCINFO
git commit -m "apic-bin X.Y.Z"
git push
```

> First push only: confirm the name `apic-bin` is free at
> <https://aur.archlinux.org/packages/apic-bin>.
