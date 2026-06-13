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

ensure_limine_tool() {
  if [ -x "$LIMINE/limine" ]; then
    return
  fi
  if [ ! -f "$LIMINE/Makefile" ] || [ ! -f "$LIMINE/limine.c" ]; then
    echo "Limine host tool fehlt und kann nicht gebaut werden. Loesche third_party/limine und starte das Skript erneut." >&2
    exit 1
  fi
  make -C "$LIMINE" limine
}

if command -v rustup >/dev/null 2>&1; then
  rustup target add x86_64-unknown-none
fi

cargo build --release
ensure_limine_tool

rm -rf "$BUILD"
mkdir -p "$ISO_ROOT/boot" "$ISO_ROOT/EFI/BOOT"
mkdir -p "$BUILD/user"

find_rust_lld() {
  if command -v rust-lld >/dev/null 2>&1; then
    command -v rust-lld
    return
  fi

  local sysroot
  sysroot="$(rustc --print sysroot)"
  local host
  host="$(rustc -vV | awk '/^host:/ { print $2 }')"
  local candidates=(
    "$sysroot/bin/rust-lld"
    "$sysroot/lib/rustlib/$host/bin/rust-lld"
  )
  local candidate
  for candidate in "${candidates[@]}"; do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  echo "rust-lld fehlt. Installiere die Rust-Komponente mit: rustup component add llvm-tools-preview" >&2
  exit 1
}

RUST_LLD="$(find_rust_lld)"

build_user_program() {
  local name="$1"
  rustc \
    --edition=2021 \
    --crate-type=bin \
    --target=x86_64-unknown-none \
    -C panic=abort \
    -C relocation-model=static \
    -C code-model=small \
    -C "linker=$RUST_LLD" \
    "-Clink-arg=-T$ROOT/user/$name/linker.ld" \
    -o "$BUILD/user/$name.elf" \
    "$ROOT/user/$name/src/main.rs"
}

ensure_coreutils() {
  download_file() {
    local url="$1"
    local out="$2"
    local tmp="$out.download"
    rm -f "$tmp"
    curl -fL "$url" -o "$tmp"
    test -s "$tmp"
    mv -f "$tmp" "$out"
  }

  local version="0.9.0"
  local name="coreutils-$version-x86_64-unknown-linux-musl"
  local archive="$ROOT/third_party/$name.tar.gz"
  local source="$ROOT/third_party/$name"
  local url="https://github.com/uutils/coreutils/releases/download/$version/$name.tar.gz"

  mkdir -p "$ROOT/third_party" "$BUILD/user"
  if [ ! -f "$archive" ] || ! tar -tzf "$archive" >/dev/null 2>&1; then
    rm -f "$archive"
    download_file "$url" "$archive"
  fi
  if ! tar -tzf "$archive" >/dev/null 2>&1; then
    echo "Downloaded coreutils archive is invalid: $archive" >&2
    exit 1
  fi
  if [ ! -d "$source" ]; then
    rm -rf "$source"
    tar -xzf "$archive" -C "$ROOT/third_party"
  fi

  local binary=""
  if [ -f "$source/coreutils" ]; then
    binary="$source/coreutils"
  elif [ -f "$source/cat" ]; then
    binary="$source/cat"
  else
    binary="$(find "$source" -type f \( -name coreutils -o -name cat \) | head -1)"
  fi
  if [ -z "$binary" ]; then
    echo "No coreutils or cat binary found in $source" >&2
    exit 1
  fi
  cp "$binary" "$BUILD/user/coreutils.elf"
}

ensure_bash_program() {
  download_file() {
    local url="$1"
    local out="$2"
    local tmp="$out.download"
    rm -f "$tmp"
    curl -fL "$url" -o "$tmp"
    test -s "$tmp"
    mv -f "$tmp" "$out"
  }

  local source="$ROOT/third_party/bash-5.3"
  local bash_bin="$source/bash"
  local tools="$ROOT/third_party/tools"
  local zig_version="0.15.2"
  local zig_dir="$tools/zig-x86_64-linux-$zig_version"
  local zig="$zig_dir/zig"

  mkdir -p "$ROOT/third_party" "$tools"
  if [ ! -f "$source/configure" ]; then
    local archive="$ROOT/third_party/bash-5.3.tar.gz"
    if [ ! -f "$archive" ] || ! tar -tzf "$archive" >/dev/null 2>&1; then
      rm -f "$archive"
      download_file "https://ftp.gnu.org/gnu/bash/bash-5.3.tar.gz" "$archive"
    fi
    if ! tar -tzf "$archive" >/dev/null 2>&1; then
      echo "Downloaded Bash archive is invalid: $archive" >&2
      exit 1
    fi
    rm -rf "$source"
    tar -xzf "$archive" -C "$ROOT/third_party"
    if [ ! -f "$source/configure" ]; then
      echo "Extracted Bash source is incomplete: $source" >&2
      exit 1
    fi
  fi

  if [ ! -x "$zig" ]; then
    local zig_archive="$tools/zig-x86_64-linux-$zig_version.tar.xz"
    if [ ! -f "$zig_archive" ] || ! tar -tf "$zig_archive" >/dev/null 2>&1; then
      rm -f "$zig_archive"
      download_file "https://ziglang.org/download/$zig_version/zig-x86_64-linux-$zig_version.tar.xz" "$zig_archive"
    fi
    if ! tar -tf "$zig_archive" >/dev/null 2>&1; then
      echo "Downloaded Zig archive is invalid: $zig_archive" >&2
      exit 1
    fi
    tar -xf "$zig_archive" -C "$tools"
  fi

  if [ ! -f "$bash_bin" ]; then
    (
      cd "$source"
      make distclean >/dev/null 2>&1 || true
      CC="$zig cc -target x86_64-linux-musl -static" \
      LD="$zig cc -target x86_64-linux-musl -static" \
      AR="$zig ar" \
      RANLIB="$zig ranlib" \
      CC_FOR_BUILD="gcc" \
      CFLAGS_FOR_BUILD="-g -DCROSS_COMPILING -std=gnu17" \
      CFLAGS="-Os -std=gnu89" \
      LDFLAGS="-Wl,--image-base=0x40000000" \
      ./configure --host=x86_64-linux-musl --build=x86_64-pc-linux-gnu \
        --enable-static-link --disable-nls --without-bash-malloc --disable-threads \
        --disable-readline --disable-history --disable-job-control \
        --disable-help-builtin --disable-progcomp --disable-alias \
        --disable-array-variables --disable-brace-expansion \
        --disable-directory-stack --disable-dparen-arithmetic \
        --disable-process-substitution --disable-net-redirections \
        --disable-coprocesses --disable-command-timing --disable-select \
        --disable-mem-scramble
      make -j2
    )
  fi

  cp "$bash_bin" "$BUILD/user/bash.elf"
}

ensure_nasm_program() {
  "$ROOT/ports/nasm/build-nasm.sh"
}

build_user_program gui
build_user_program taskview
build_user_program init
build_user_program nsh
build_user_program nkedit
ensure_coreutils
ensure_bash_program
ensure_nasm_program

app_files=(
  "$BUILD/user/gui.elf=/bin/gui"
  "$BUILD/user/gui.elf=/GUI.elf"
  "$BUILD/user/taskview.elf=/bin/taskview"
  "$BUILD/user/taskview.elf=/bin/taskviewer"
  "$BUILD/user/init.elf=/bin/init"
  "$BUILD/user/nsh.elf=/bin/nsh"
  "$BUILD/user/nkedit.elf=/bin/nkedit"
  "$BUILD/user/bash.elf=/bin/bash"
  "$BUILD/user/nasm.elf=/bin/nasm"
)
while read -r app alias; do
  if [ -z "${app:-}" ] || [[ "$app" = \#* ]]; then
    continue
  fi
  app_files+=("$BUILD/user/coreutils.elf=/bin/$app")
done < "$ROOT/ports/coreutils/coreutils-apps.txt"
app_files+=("$ROOT/apps/HELLO.TXT=/hello.txt")
app_files+=("$ROOT/apps/HELLO.TXT=/HELLO.TXT")
app_files+=("$ROOT/apps/font.psf=/etc/font.psf")
app_files+=("$ROOT/apps/hello.asm=/home/root/hello.asm")
app_files+=("$ROOT/apps/hello.asm=/hello.asm")
python3 "$ROOT/scripts/mkfs-nkfs.py" "$BUILD/nk-root.nkfs" "${app_files[@]}"

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
    -drive "file=$BUILD/nk-root.nkfs,format=raw,if=none,id=rootdisk" \
    -device "virtio-blk-pci,drive=rootdisk,disable-modern=on"
fi
