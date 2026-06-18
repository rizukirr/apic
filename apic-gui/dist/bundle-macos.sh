#!/usr/bin/env bash
# Assembles apic.app from an already-built apic-gui binary, so the GUI is a
# double-clickable macOS application with the apic icon. The zipping step
# (ditto) is macOS-only and lives in the release workflow; this script just
# lays out the bundle and can run anywhere for inspection.
#
# Usage: bundle-macos.sh <target-triple> <version>
#   e.g. bundle-macos.sh aarch64-apple-darwin 0.3.0
set -euo pipefail

TARGET="${1:?target triple required (e.g. aarch64-apple-darwin)}"
VERSION="${2:?version required (e.g. 0.3.0)}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/$TARGET/release/apic-gui"
ICNS="$ROOT/apic-gui/assets/icon.icns"

[[ -x "$BIN" ]] || { echo "error: binary not found: $BIN (build it first)" >&2; exit 1; }
[[ -f "$ICNS" ]] || { echo "error: icon not found: $ICNS" >&2; exit 1; }

APP="apic.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/apic-gui"
chmod +x "$APP/Contents/MacOS/apic-gui"
cp "$ICNS" "$APP/Contents/Resources/apic.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                <string>apic</string>
    <key>CFBundleDisplayName</key>         <string>apic</string>
    <key>CFBundleIdentifier</key>          <string>com.rizukirr.apic-gui</string>
    <key>CFBundleExecutable</key>          <string>apic-gui</string>
    <key>CFBundleIconFile</key>            <string>apic.icns</string>
    <key>CFBundleVersion</key>             <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>  <string>$VERSION</string>
    <key>CFBundlePackageType</key>         <string>APPL</string>
    <key>LSMinimumSystemVersion</key>      <string>10.13</string>
    <key>NSHighResolutionCapable</key>     <true/>
</dict>
</plist>
PLIST

echo "Assembled $APP (version $VERSION, $TARGET)"
