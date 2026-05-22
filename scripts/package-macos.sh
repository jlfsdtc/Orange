#!/usr/bin/env bash
# Build Orange as a macOS .app bundle and DMG.
#
# Usage:
#   scripts/package-macos.sh [--no-dmg]
#
# Requires:
#   * Xcode command-line tools (cargo, hdiutil)
#   * The release binary built ahead of time, or run cargo build --release here
#
# Outputs (under target/packaging/macos/):
#   orange.app/   the bundle
#   orange-<version>.dmg
#
# Caveat: produces an unsigned bundle. Code signing and notarization are out
# of scope; consumer machines will need a right-click → Open or `xattr -dr
# com.apple.quarantine` workaround.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MAKE_DMG=1
for arg in "$@"; do
    case "$arg" in
        --no-dmg) MAKE_DMG=0 ;;
        *) echo "unknown arg: $arg" >&2; exit 2 ;;
    esac
done

VERSION="$(awk -F'"' '/^version[[:space:]]*=/ { print $2; exit }' Cargo.toml)"
if [ -z "$VERSION" ]; then
    echo "could not extract version from Cargo.toml" >&2
    exit 1
fi

echo "==> building release binary"
cargo build --release --bin orange

OUT="$ROOT/target/packaging/macos"
APP="$OUT/orange.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

echo "==> assembling bundle at $APP"
cp "target/release/orange" "$APP/Contents/MacOS/orange"
chmod +x "$APP/Contents/MacOS/orange"

cp "assets/icons/orange.icns" "$APP/Contents/Resources/orange.icns"

sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"
printf "APPL????" > "$APP/Contents/PkgInfo"

echo "==> sanity-checking bundle"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
file "$APP/Contents/MacOS/orange" | grep -q "Mach-O" || {
    echo "bundle binary is not a Mach-O executable" >&2
    exit 1
}

if [ "$MAKE_DMG" = "1" ]; then
    DMG="$OUT/orange-$VERSION.dmg"
    STAGE="$OUT/dmg-stage"
    rm -rf "$STAGE" "$DMG"
    mkdir -p "$STAGE"
    cp -R "$APP" "$STAGE/"
    ln -s /Applications "$STAGE/Applications"

    echo "==> creating DMG at $DMG"
    hdiutil create \
        -volname "Orange $VERSION" \
        -srcfolder "$STAGE" \
        -ov \
        -format UDZO \
        "$DMG" >/dev/null

    rm -rf "$STAGE"
    echo "==> wrote $DMG"
fi

echo "==> done. Open with: open \"$APP\""
