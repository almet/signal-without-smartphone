#!/usr/bin/env bash
#
# Package a built signal-setup binary into a Linux AppImage.
#
# Usage: make-appimage.sh <binary-path> <arch> <output-dir>
#
#   binary-path : path to the release binary (e.g. target/.../release/signal-setup)
#   arch        : AppImage arch tag, "x86_64" or "aarch64"
#   output-dir  : directory where the .AppImage is written
#
# Produces:
#   <output-dir>/Signal_Setup-<arch>.AppImage
#
# Downloads appimagetool for the given arch on first use. Intended for CI
# (Ubuntu runners); requires curl and standard coreutils.

set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "Usage: $0 <binary-path> <arch> <output-dir>" >&2
    exit 2
fi

BINARY="$1"
ARCH="$2"
OUTPUT_DIR="$3"

if [[ ! -x "$BINARY" ]]; then
    echo "error: binary not found or not executable: $BINARY" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LOGO_PNG="${REPO_ROOT}/crates/signal-setup/assets/logo.png"
if [[ ! -f "$LOGO_PNG" ]]; then
    echo "error: $LOGO_PNG not found" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT

# AppDir layout that appimagetool expects.
APPDIR="${STAGING}/Signal_Setup.AppDir"
mkdir -p "${APPDIR}/usr/bin"
cp "$BINARY" "${APPDIR}/usr/bin/signal-setup"
chmod +x "${APPDIR}/usr/bin/signal-setup"

# Icon: top-level (named after the desktop entry) plus the hicolor path.
cp "$LOGO_PNG" "${APPDIR}/signal-setup.png"
mkdir -p "${APPDIR}/usr/share/icons/hicolor/256x256/apps"
cp "$LOGO_PNG" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/signal-setup.png"

cat > "${APPDIR}/signal-setup.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Signal Setup
Comment=Register a Signal account without a smartphone
Exec=signal-setup
Icon=signal-setup
Categories=Network;Utility;
Terminal=false
DESKTOP

# AppRun launches the bundled binary.
cat > "${APPDIR}/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "${HERE}/usr/bin/signal-setup" "$@"
APPRUN
chmod +x "${APPDIR}/AppRun"

# Fetch appimagetool for this arch.
TOOL="${STAGING}/appimagetool"
TOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
curl --proto '=https' --tlsv1.2 -fsSL "$TOOL_URL" -o "$TOOL"
chmod +x "$TOOL"

OUT="${OUTPUT_DIR}/Signal_Setup-${ARCH}.AppImage"
rm -f "$OUT"
# --appimage-extract-and-run avoids needing FUSE on CI runners.
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"

echo "Built ${OUT}"
