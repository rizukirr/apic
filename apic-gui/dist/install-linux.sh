#!/usr/bin/env bash
# Installs apic-gui into the per-user XDG locations so it shows up in the
# application launcher with the apic icon. No root required. Re-run to update;
# pass --uninstall to remove.
set -euo pipefail

APP=apic-gui
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
ICON_DIR="$DATA_HOME/icons/hicolor/256x256/apps"
APPS_DIR="$DATA_HOME/applications"
DESKTOP_FILE="$APPS_DIR/$APP.desktop"

refresh() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS_DIR" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -f -t "$DATA_HOME/icons/hicolor" 2>/dev/null || true
}

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -fv "$BIN_DIR/$APP" "$ICON_DIR/$APP.png" "$DESKTOP_FILE"
    refresh
    echo "Removed $APP."
    exit 0
fi

# Locate the binary: next to this script (release tarball), the repo build dir,
# or already on PATH.
find_bin() {
    local c
    for c in "$SCRIPT_DIR/$APP" \
             "$SCRIPT_DIR/../../target/release/$APP" \
             "$SCRIPT_DIR/../../target/debug/$APP"; do
        [[ -x "$c" ]] && { echo "$c"; return 0; }
    done
    command -v "$APP" >/dev/null 2>&1 && { command -v "$APP"; return 0; }
    return 1
}

# Locate the icon: next to this script (tarball) or in the crate assets (repo).
find_icon() {
    local c
    for c in "$SCRIPT_DIR/icon.png" "$SCRIPT_DIR/../assets/icon.png"; do
        [[ -f "$c" ]] && { echo "$c"; return 0; }
    done
    return 1
}

BIN="$(find_bin)" || {
    echo "error: $APP binary not found. Build it first:" >&2
    echo "       cargo build --release -p apic-gui" >&2
    exit 1
}
ICON="$(find_icon)" || {
    echo "error: icon.png not found next to the installer or in apic-gui/assets/" >&2
    exit 1
}

mkdir -p "$BIN_DIR" "$ICON_DIR" "$APPS_DIR"
install -m 755 "$BIN" "$BIN_DIR/$APP"
install -m 644 "$ICON" "$ICON_DIR/$APP.png"

# Absolute Exec path so the launcher works regardless of PATH.
cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=apic
GenericName=API Contract Explorer
Comment=Browse and edit Git-friendly API contracts
Exec=$BIN_DIR/$APP
Icon=$APP
Terminal=false
Categories=Development;
Keywords=api;contract;rest;json;
StartupWMClass=$APP
EOF
chmod 644 "$DESKTOP_FILE"

refresh

echo "Installed:"
echo "  binary  -> $BIN_DIR/$APP"
echo "  icon    -> $ICON_DIR/$APP.png"
echo "  desktop -> $DESKTOP_FILE"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "note: $BIN_DIR is not on your PATH; add it to launch 'apic-gui' from a terminal." ;;
esac
echo "It should now appear in your application launcher as 'apic'."
