#!/usr/bin/env bash
# Build a Debian package via cargo-deb.
#
# Prerequisites:
#   cargo install cargo-deb
# Output:
#   target/debian/orange_<version>_<arch>.deb
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo-deb >/dev/null 2>&1; then
    echo "cargo-deb not installed; install with: cargo install cargo-deb" >&2
    exit 1
fi

echo "==> building Orange release binary"
cargo build --release --bin orange --bin orange-grep

echo "==> running cargo deb"
cargo deb -p orange-app --no-build

echo "==> output:"
ls -la target/debian/*.deb
