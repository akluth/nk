#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${1:-2.16.03}"
SOURCE="$ROOT/third_party/nasm-$VERSION"
BUILD="$ROOT/build/user"
TOOLS="$ROOT/third_party/tools"
ZIG_VERSION="0.15.2"
ZIG_DIR="$TOOLS/zig-x86_64-linux-$ZIG_VERSION"
ZIG="$ZIG_DIR/zig"
OUT="$BUILD/nasm.elf"
PORT_BINARY="$SOURCE/nasm"

mkdir -p "$BUILD"

if [ -f "$PORT_BINARY" ]; then
  cp "$PORT_BINARY" "$OUT"
  echo "NASM already built: $PORT_BINARY"
  exit 0
fi

if [ ! -f "$SOURCE/configure" ]; then
  "$ROOT/ports/nasm/fetch-nasm.sh" "$VERSION"
fi
if [ ! -x "$ZIG" ]; then
  mkdir -p "$TOOLS"
  ARCHIVE="$TOOLS/zig-x86_64-linux-$ZIG_VERSION.tar.xz"
  if [ ! -f "$ARCHIVE" ]; then
    curl -fL "https://ziglang.org/download/$ZIG_VERSION/zig-x86_64-linux-$ZIG_VERSION.tar.xz" -o "$ARCHIVE"
  fi
  tar -xf "$ARCHIVE" -C "$TOOLS"
fi

(
  cd "$SOURCE"
  make distclean >/dev/null 2>&1 || true
  CC="$ZIG cc -target x86_64-linux-musl -static -Wl,--image-base=0x40000000" \
  CFLAGS="-O2 -Wno-error=date-time" \
  ./configure --host=x86_64-linux-musl --prefix=/usr
  make -j2 nasm
)
cp "$SOURCE/nasm" "$OUT"
