#!/usr/bin/env bash
set -euo pipefail

sudo apt update
sudo apt install -y build-essential curl git llvm lld nasm xorriso qemu-system-x86
if ! command -v cargo >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
