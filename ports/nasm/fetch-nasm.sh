#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${1:-2.16.03}"
THIRD_PARTY="$ROOT/third_party"
ARCHIVE="$THIRD_PARTY/nasm-$VERSION.tar.xz"
SOURCE="$THIRD_PARTY/nasm-$VERSION"

mkdir -p "$THIRD_PARTY"
if [ ! -f "$ARCHIVE" ]; then
  curl -fL "https://www.nasm.us/pub/nasm/releasebuilds/$VERSION/nasm-$VERSION.tar.xz" -o "$ARCHIVE"
fi
if [ ! -d "$SOURCE" ]; then
  tar -xf "$ARCHIVE" -C "$THIRD_PARTY"
fi
test -f "$SOURCE/configure"
