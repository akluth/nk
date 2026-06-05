#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
ISO_ROOT="$BUILD/iso_root"
LIMINE="$ROOT/third_party/limine"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo fehlt. Installiere Rust zuerst: https://rustup.rs/" >&2
  exit 1
fi

if [ ! -d "$LIMINE" ]; then
  mkdir -p "$ROOT/third_party"
  git clone --depth 1 --branch v9.x-binary https://github.com/limine-bootloader/limine.git "$LIMINE"
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-unknown-none
fi

cargo build --release

rm -rf "$BUILD"
mkdir -p "$ISO_ROOT/boot" "$ISO_ROOT/EFI/BOOT"
mkdir -p "$BUILD/user"

rustc \
  --edition=2021 \
  --crate-type=bin \
  --target=x86_64-unknown-none \
  -C panic=abort \
  -C relocation-model=static \
  -C code-model=small \
  -C linker=rust-lld \
  "-Clink-arg=-T$ROOT/user/gui/linker.ld" \
  -o "$BUILD/user/gui.elf" \
  "$ROOT/user/gui/src/main.rs"

cp "$ROOT/target/x86_64-unknown-none/release/nk" "$ISO_ROOT/boot/nk"
cp "$BUILD/user/gui.elf" "$ISO_ROOT/boot/gui.elf"
cp "$ROOT/limine.conf" "$ISO_ROOT/boot/limine.conf"
cp "$LIMINE/limine-bios.sys" "$LIMINE/limine-bios-cd.bin" "$LIMINE/limine-uefi-cd.bin" "$ISO_ROOT/"
cp "$LIMINE/BOOTX64.EFI" "$ISO_ROOT/EFI/BOOT/BOOTX64.EFI"
cp "$LIMINE/BOOTIA32.EFI" "$ISO_ROOT/EFI/BOOT/BOOTIA32.EFI"

xorriso -as mkisofs \
  -b limine-bios-cd.bin \
  -no-emul-boot \
  -boot-load-size 4 \
  -boot-info-table \
  --efi-boot limine-uefi-cd.bin \
  -efi-boot-part \
  --efi-boot-image \
  --protective-msdos-label \
  "$ISO_ROOT" \
  -o "$BUILD/nk.iso"

"$LIMINE/limine" bios-install "$BUILD/nk.iso"

if [ "${1:-}" = "run" ]; then
  qemu-system-x86_64 -M q35 -m 256M -cdrom "$BUILD/nk.iso"
fi
