#!/usr/bin/env bash
#
# Package a built signal-setup binary into Signal Setup.app and a .dmg.
#
# Usage: make-app.sh <binary-path> <version> <output-dir>
#
#   binary-path : path to the release binary (e.g. target/.../release/signal-setup)
#   version     : version string for CFBundleShortVersionString (e.g. 3.0.2)
#   output-dir  : directory where Signal Setup.app and *.dmg will be written
#
# Produces:
#   <output-dir>/Signal Setup.app          -- ad-hoc codesigned .app bundle
#   <output-dir>/signal-setup-macos-<arch>.dmg  -- compressed disk image
#
# Arch is auto-detected from the input binary via `lipo -archs`.
#
# Requires (all preinstalled on macOS): codesign, hdiutil, iconutil, sips, lipo,
# plutil, sed.

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "Usage: $0 <binary-path> <version> <output-dir>" >&2
    exit 2
fi

BINARY="$1"
VERSION="$2"
OUTPUT_DIR="$3"

if [[ ! -x "$BINARY" ]]; then
    echo "error: binary not found or not executable: $BINARY" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOGO_PNG="${REPO_ROOT}/assets/logo.png"
INFO_PLIST_TEMPLATE="${REPO_ROOT}/packaging/macos/Info.plist"

if [[ ! -f "$LOGO_PNG" ]]; then
    echo "error: $LOGO_PNG not found" >&2
    exit 1
fi
if [[ ! -f "$INFO_PLIST_TEMPLATE" ]]; then
    echo "error: $INFO_PLIST_TEMPLATE not found" >&2
    exit 1
fi

# Map lipo's arch names to the asset-name suffix we ship to users.
ARCH="$(lipo -archs "$BINARY" | tr -d '[:space:]')"
case "$ARCH" in
    arm64)   ASSET_ARCH="silicon" ;;
    x86_64)  ASSET_ARCH="intel" ;;
    *)
        echo "error: unsupported binary architecture: $ARCH" >&2
        exit 1
        ;;
esac

mkdir -p "$OUTPUT_DIR"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

APP="${STAGING}/Signal Setup.app"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"

# --- Binary ---------------------------------------------------------------
cp "$BINARY" "${APP}/Contents/MacOS/signal-setup"
chmod +x "${APP}/Contents/MacOS/signal-setup"

# --- Info.plist (substitute @VERSION@) ------------------------------------
sed "s/@VERSION@/${VERSION}/g" "$INFO_PLIST_TEMPLATE" > "${APP}/Contents/Info.plist"
plutil -lint "${APP}/Contents/Info.plist" >/dev/null

# --- Icon (AppIcon.icns from 1024×1024 PNG) -------------------------------
ICONSET="${STAGING}/AppIcon.iconset"
mkdir -p "$ICONSET"
# Apple's required iconset members. Each entry is "size@scale outname".
generate_icon() {
    local size="$1" out="$2"
    sips -z "$size" "$size" "$LOGO_PNG" --out "${ICONSET}/${out}" >/dev/null
}
generate_icon   16 icon_16x16.png
generate_icon   32 icon_16x16@2x.png
generate_icon   32 icon_32x32.png
generate_icon   64 icon_32x32@2x.png
generate_icon  128 icon_128x128.png
generate_icon  256 icon_128x128@2x.png
generate_icon  256 icon_256x256.png
generate_icon  512 icon_256x256@2x.png
generate_icon  512 icon_512x512.png
generate_icon 1024 icon_512x512@2x.png
iconutil -c icns "$ICONSET" -o "${APP}/Contents/Resources/AppIcon.icns"

# --- Ad-hoc codesign ------------------------------------------------------
# Without a signature, Apple Silicon refuses to launch the binary at all
# ('damaged' / 'can't be opened'). The ad-hoc identity (-s -) attaches a
# valid self-signature; users still see Gatekeeper's 'unidentified
# developer' prompt the first time (right-click → Open to accept).
codesign --force --deep --sign - --timestamp=none "$APP"
codesign --verify --strict --verbose=2 "$APP"

# --- Move .app out of the staging dir before building the DMG -------------
FINAL_APP="${OUTPUT_DIR}/Signal Setup.app"
rm -rf "$FINAL_APP"
mv "$APP" "$FINAL_APP"

# --- DMG with drag-to-Applications shortcut -------------------------------
# Build the DMG from a fresh staging dir that contains only the .app and a
# symlink to /Applications — gives users the familiar drag-and-drop UX.
DMG_STAGING="${STAGING}/dmg"
mkdir -p "$DMG_STAGING"
cp -R "$FINAL_APP" "${DMG_STAGING}/Signal Setup.app"
ln -s /Applications "${DMG_STAGING}/Applications"

DMG_PATH="${OUTPUT_DIR}/signal-setup-macos-${ASSET_ARCH}.dmg"
rm -f "$DMG_PATH"
hdiutil create \
    -volname "Signal Setup" \
    -srcfolder "$DMG_STAGING" \
    -ov \
    -format UDZO \
    "$DMG_PATH" >/dev/null

echo "Built ${FINAL_APP}"
echo "Built ${DMG_PATH}"
