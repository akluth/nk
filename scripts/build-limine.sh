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

build_user_program() {
  local name="$1"
  rustc \
    --edition=2021 \
    --crate-type=bin \
    --target=x86_64-unknown-none \
    -C panic=abort \
    -C relocation-model=static \
    -C code-model=small \
    -C linker=rust-lld \
    "-Clink-arg=-T$ROOT/user/$name/linker.ld" \
    -o "$BUILD/user/$name.elf" \
    "$ROOT/user/$name/src/main.rs"
}

build_c_user_program() {
  local name="$1"
  clang \
    --target=x86_64-unknown-none \
    -std=gnu89 \
    -ffreestanding \
    -fno-builtin \
    -fno-stack-protector \
    -mno-red-zone \
    -nostdlib \
    -c "$ROOT/user/$name/src/$name.c" \
    -o "$BUILD/user/$name.o"
  rust-lld \
    -T "$ROOT/user/$name/linker.ld" \
    -o "$BUILD/user/$name.elf" \
    "$BUILD/user/$name.o"
}

build_user_program gui
build_user_program shell
build_user_program taskview
build_c_user_program cat
python3 "$ROOT/scripts/make-fat32.py" "$BUILD/nk-apps.fat32" "$BUILD/user/gui.elf" "$BUILD/user/shell.elf" "$BUILD/user/taskview.elf" "$BUILD/user/cat.elf" "$ROOT/apps/HELLO.TXT"

cp "$ROOT/target/x86_64-unknown-none/release/nk" "$ISO_ROOT/boot/nk"
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
  qemu-system-x86_64 -M pc -m 256M -boot d -cdrom "$BUILD/nk.iso" \
    -drive "file=$BUILD/nk-apps.fat32,format=raw,if=ide,index=0,media=disk"
fi
