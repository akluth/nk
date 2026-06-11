# nk

`nk` is a tiny x86-64 operating system written in Rust. It boots through the
Limine bootloader on BIOS and UEFI systems, brings up a small microkernel-style
core, starts Ring 3 tasks, and loads user programs as ELF files from a small
read-only nkfs root filesystem disk.

## Current Status

- Boots in QEMU through Limine/BIOS.
- Initializes GDT/TSS, IDT, PIC, and the PIT timer.
- Builds a dedicated userland page-table root.
- Starts Ring 3 tasks through `iretq` and saved trapframes.
- Uses timer interrupt trapframes as scheduler context.
- Builds a read-only nkfs root disk containing `/bin/bash`, `/bin/gui`,
  `/bin/taskview`, uutils Coreutils command aliases, plus small data files.
- Reads that nkfs disk through a first ATA PIO block-device path.
- Parses userland ELF files from the nkfs root disk and starts them as
  Ring 3 tasks.
- Provides minimal GUI syscalls for clearing the screen, drawing rectangles,
  and drawing scaled bitmap text.
- Boots a real static GNU Bash 5.3 `/bin/bash` as the default user process and
  renders its terminal output directly through a minimal kernel framebuffer
  console.
- Keeps the userland GUI compositor and task viewer as optional nkfs-loaded
  programs; they are not started automatically during boot.
- Makes the Rust uutils Coreutils multicall binary available through `/bin`
  aliases such as `/bin/cat`, `/bin/ls`, `/bin/wc`, and `/bin/sha256sum`.
- Selects Linux/POSIX syscall handling by task ABI, so future Linux-compatible
  user programs are not tied to hard-coded kernel task names.
- Includes a GNU Bash port under `ports/bash`; the port fetches upstream Bash
  5.3 sources into ignored `third_party` storage and builds a static
  `x86_64-linux-musl` ELF linked at `0x40000000`.
- Uses a generated monospace bitmap font for GUI text.
- Tracks a generic focused user-task slot so userland can build taskbar/window
  switching without making the kernel depend on specific GUI programs.
- Delivers PS/2 keyboard input through IRQ1 and a small `read_key` syscall.
- Delivers PS/2 mouse input through IRQ12 and a small `read_mouse` syscall.
- Bash can start Coreutils commands on demand through the minimal
  `fork`/`execve`/`wait4` path; `cat hello.txt` reads `/hello.txt` from the
  nkfs root disk.

## Architecture

- `src/main.rs`: kernel entry and bootstrap sequence.
- `src/gdt.rs`: GDT, kernel/user segments, TSS, and kernel stacks.
- `src/interrupts.rs`: IDT, exception diagnostics, PIC/PIT setup, timer IRQs,
  keyboard/mouse IRQs, trapframe scheduling, and the `int 0x80` syscall
  boundary.
- `src/memory.rs`: user page-table creation, user image pages, user stacks, and
  kernel-only framebuffer mapping for syscall handlers.
- `src/scheduler.rs`: minimal kernel scheduler plus trapframe-based user task
  scheduling.
- `src/ata.rs`: first ATA PIO sector reader for the QEMU root disk.
- `src/nkfs.rs`: tiny read-only nkfs reader with UNIX-like absolute paths.
- `src/userland.rs`: address-space model, ELF loader, task frame setup, CR3
  switch, and Ring 3 entry.
- `src/services.rs`: kernel-side framebuffer service used by GUI syscalls.
- `src/mouse.rs`: tiny PS/2 mouse packet decoder.
- `src/linux_abi.rs`: Linux/POSIX syscall compatibility path for Linux ABI
  user tasks, including basic file I/O, keyboard-backed stdin, `writev`,
  `openat`, `stat`, `fstat`, `lseek`, `brk`, `mmap`, `readlink`, `uname`,
  `getcwd`, `access`, `fcntl`, `ioctl`, UID/GID queries, signal setup stubs,
  time syscalls, and exit syscalls.
- `src/font.rs`: generated fixed-size monospace bitmap font.
- `src/framebuffer.rs`: low-level pixel and rectangle drawing.
- `src/limine.rs`: Limine framebuffer, HHDM, and kernel address requests.
- `src/pci.rs` and `src/virtio.rs`: PCI scan, Virtio capability discovery, and
  early queue memory setup.
- `user/gui/src/main.rs`: optional separate no_std Rust GUI/compositor
  executable, startable from Bash as `gui` or `/bin/gui`.
- `user/gui/linker.ld`: GUI ELF linker script.
- `user/taskview/src/main.rs`: separate no_std Rust task viewer executable.
- `user/taskview/linker.ld`: task viewer ELF linker script.
- `ports/bash/`: staging notes and fetch script for the real GNU Bash port.
- `ports/coreutils/`: fetch/build glue for the Rust uutils Coreutils port and
  the command alias list installed into `/bin` on the nkfs root disk.
- `scripts/mkfs-nkfs.py`: host-side nkfs image builder.

## Install Tools

Windows with an administrator PowerShell:

```powershell
.\scripts\install-tools-admin.ps1
```

Linux/WSL/Ubuntu:

```bash
./scripts/install-tools-ubuntu.sh
```

The Windows installer uses Chocolatey, which requires access to
`C:\ProgramData`.

## Build

Windows:

```powershell
.\scripts\build-limine.ps1
```

Linux/WSL:

```bash
./scripts/build-limine.sh
```

The build script creates both:

- `target/x86_64-unknown-none/release/nk`: the kernel.
- `build/user/gui.elf`: optional separate userland GUI executable.
- `build/user/taskview.elf`: optional separate userland task viewer executable.
- `build/user/coreutils.elf`: the Rust uutils Coreutils multicall executable.
- `build/user/bash.elf`: GNU Bash executable; the normal build fetches/builds
  it on demand, copies it to the root disk as `/bin/bash`, and starts it as the
  standard terminal process.
- `build/nk-root.nkfs`: the read-only nkfs root disk containing `/bin`,
  `/etc`, `/home/root`, user programs, and small data files.

The ISO only contains the kernel and bootloader files. User programs are loaded
from the nkfs root disk at runtime.

Building Coreutils requires network access on the first build to download the
official uutils `x86_64-unknown-linux-musl` release asset into ignored
`third_party` storage. Building Bash requires network access on the first
build, MSYS2 `make`, MSYS2 `gcc` for host build tools, and portable Zig for the
static Musl target; the build script downloads Zig and Bash sources into
ignored `third_party` storage when needed. See `ports/bash/PORT.md`.

## Run in QEMU

Windows:

```powershell
.\scripts\build-limine.ps1 -Run
```

The BIOS run path uses QEMU's `pc` machine with an IDE disk because the current
block reader is the intentionally small ATA PIO path. The nkfs root disk is
attached automatically.

UEFI test with edk2/OVMF from the QEMU installation:

```powershell
.\scripts\build-limine.ps1 -Run -Uefi
```

Linux/WSL:

```bash
./scripts/build-limine.sh run
```

## Virtio Smoke Test

With a Virtio block device and Virtio keyboard, the serial log prints detected
Virtio PCI devices:

```powershell
$disk = "$PWD\build\virtio-test.img"
[IO.File]::WriteAllBytes($disk, (New-Object byte[] 1048576))
& "C:\Program Files\qemu\qemu-system-x86_64.exe" `
  -M q35 -m 256M -cdrom .\build\nk.iso `
  -drive "file=$disk,format=raw,if=none,id=vd0" `
  -device virtio-blk-pci,drive=vd0 `
  -device virtio-keyboard-pci `
  -serial stdio
```

## Next Useful Steps

- Add a real compositor/window manager so applications submit private window
  buffers instead of drawing directly into the shared framebuffer.
- Extend the current single-child `fork`/`execve`/`wait4` implementation into a
  real process table with multiple children and reaping.
- Add pipes, descriptor duplication, signals, termios/TTY handling, and
  job-control semantics for Bash and other real Linux/POSIX programs.
- Load a real PSF/SSFN font from the root disk instead of compiling the generated
  bitmap table into the kernel.
- Add dirty-rectangle or double-buffered drawing to remove the remaining direct
  framebuffer redraw artifacts.
- Replace the fixed user image buffer with per-process page allocation.
- Replace the interim C runtime shim with enough Linux process ABI support to
  execute unmodified static Linux binaries.
- Add argv/envp/auxv setup on the initial user stack.
- Split GUI syscalls into a proper capability-checked IPC protocol.
- Move the nkfs block backend from ATA PIO to Virtio block on `q35`.
- Move more kernel-side services behind explicit userland/server boundaries.
