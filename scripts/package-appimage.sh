#!/usr/bin/env bash
# Build an AppImage for Orange.
#
# Prerequisites (downloaded automatically if missing):
#   appimagetool (https://github.com/AppImage/AppImageKit)
#
# Output:
#   target/appimage/orange-<version>-x86_64.AppImage
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(awk -F'"' '/^version[[:space:]]*=/ { print $2; exit }' Cargo.toml)"
ARCH="${ARCH:-x86_64}"

echo "==> building Orange release binary"
cargo build --release --bin orange

OUT="$ROOT/target/appimage"
APPDIR="$OUT/orange.AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
         "$OUT"

# Binary
cp target/release/orange "$APPDIR/usr/bin/orange"
chmod +x "$APPDIR/usr/bin/orange"

# Desktop file (the AppDir root needs a copy for appimagetool)
cp packaging/linux/orange.desktop "$APPDIR/usr/share/applications/orange.desktop"
cp packaging/linux/orange.desktop "$APPDIR/orange.desktop"

# Icon (root + hicolor)
cp assets/icons/png/icon_256.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/orange.png"
cp assets/icons/png/icon_256.png "$APPDIR/orange.png"
cp assets/icons/png/icon_256.png "$APPDIR/.DirIcon"

# AppRun entry point
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/orange" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# Fetch appimagetool if missing
APPIMAGETOOL="$OUT/appimagetool"
if [ ! -x "$APPIMAGETOOL" ]; then
    echo "==> downloading appimagetool"
    curl -L -o "$APPIMAGETOOL" \
        "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$APPIMAGETOOL"
fi

OUTFILE="$OUT/orange-$VERSION-$ARCH.AppImage"
echo "==> running appimagetool"
ARCH="$ARCH" "$APPIMAGETOOL" "$APPDIR" "$OUTFILE"

echo "==> output: $OUTFILE"
