#!/usr/bin/env bash
# Build an RPM package via cargo-generate-rpm.
#
# Prerequisites:
#   cargo install cargo-generate-rpm
# Output:
#   target/generate-rpm/orange-<version>-1.<arch>.rpm
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo-generate-rpm >/dev/null 2>&1; then
    echo "cargo-generate-rpm not installed; install with: cargo install cargo-generate-rpm" >&2
    exit 1
fi

echo "==> building Orange release binary"
cargo build --release --bin orange --bin orange-grep

echo "==> running cargo generate-rpm"
cargo generate-rpm -p crates/orange-app

echo "==> output:"
ls -la target/generate-rpm/*.rpm
