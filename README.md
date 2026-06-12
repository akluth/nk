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
- Starts `/bin/init` as the first Ring 3 process; the current init starts the
  fast native `/bin/nsh` shell as the default terminal process.
- Keeps a real static GNU Bash 5.3 `/bin/bash` available on the root disk for
  optional execution as the Linux/POSIX ABI matures.
- Keeps the userland GUI compositor and task viewer as optional nkfs-loaded
  programs; they are not started automatically during boot.
- Makes the Rust uutils Coreutils multicall binary available through `/bin`
  aliases such as `/bin/cat`, `/bin/ls`, `/bin/wc`, and `/bin/sha256sum`.
- Selects Linux/POSIX syscall handling by task ABI, so future Linux-compatible
  user programs are not tied to hard-coded kernel task names.
- Includes a GNU Bash port under `ports/bash`; the port fetches upstream Bash
  5.3 sources into ignored `third_party` storage and builds a static
  `x86_64-linux-musl` ELF linked at `0x40000000`.
- Loads a Spleen 12x24 PSF2 monospace font from `/etc/font.psf` on the nkfs
  root disk instead of compiling the font bitmap into the kernel.
- Tracks a generic focused user-task slot so userland can build taskbar/window
  switching without making the kernel depend on specific GUI programs.
- Delivers PS/2 keyboard input through IRQ1 and a small `read_key` syscall.
- Delivers PS/2 mouse input through IRQ12 and a small `read_mouse` syscall.
- `nsh` can start Coreutils commands on demand through the minimal
  `fork`/`execve`/`wait4` path; examples such as `ls /bin`, `echo ok`, and
  `cat /hello.txt` run against the nkfs root disk.

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
- `src/font.rs`: small PSF2 font loader used by the framebuffer console.
- `src/framebuffer.rs`: low-level pixel and rectangle drawing.
- `src/limine.rs`: Limine framebuffer, HHDM, and kernel address requests.
- `src/pci.rs` and `src/virtio.rs`: PCI scan, Virtio capability discovery, and
  early queue memory setup.
- `user/gui/src/main.rs`: optional separate no_std Rust GUI/compositor
  executable, startable from the shell as `gui` or `/bin/gui`.
- `user/gui/linker.ld`: GUI ELF linker script.
- `user/taskview/src/main.rs`: separate no_std Rust task viewer executable.
- `user/taskview/linker.ld`: task viewer ELF linker script.
- `ports/bash/`: fetch/build glue for the real GNU Bash port.
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
- `build/user/init.elf`: first userland process started by the kernel.
- `build/user/nsh.elf`: native shell started by init as the standard terminal.
- `build/user/bash.elf`: GNU Bash executable; the normal build fetches/builds
  it on demand and copies it to the root disk as `/bin/bash`.
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

Already done:

- Ring 3 user tasks, TSS/GDT/IDT setup, timer interrupts, and trapframe-based
  preemptive scheduling.
- Isolated user page-table roots for the current user process table.
- A first init process (`/bin/init`) that starts the default native shell
  (`/bin/nsh`) instead of making the kernel depend on a shell implementation.
- A read-only nkfs root disk with UNIX-like absolute paths, `/bin`, `/etc`,
  `/home/root`, `/hello.txt`, `/bin/init`, `/bin/nsh`, `/bin/bash`, `/bin/gui`,
  `/bin/taskview`, and uutils Coreutils aliases.
- ELF loading from the nkfs root disk for native no_std programs and static
  Linux ABI programs.
- Optional separate GUI and task viewer binaries loaded from `/bin`; they are
  no longer mandatory boot-time kernel payloads.
- Rust uutils Coreutils integration with command aliases in `/bin`.
- A minimal Linux/POSIX ABI path with enough file, directory, process, memory,
  and terminal syscalls to run useful static userland tools.
- Minimal `fork`/`execve`/`wait4` support so the shell can launch external
  programs on demand.
- A larger reusable user process table with dynamic PID allocation, parent PID
  tracking, PID-specific `wait4`, zombie reaping, and per-task Linux ABI state
  for CWD, file descriptors, `brk`, `mmap`, and stdout limiting.
- User process descriptors are allocated from kernel-managed physical frames
  during boot instead of being embedded directly in the static scheduler object.
- User memory is backed by a kernel-managed 4 KiB page pool instead of fixed
  per-slot image/stack byte arrays; ELF segments, stacks, `brk`, `mmap`, and
  `fork` copies allocate and map per-task pages on demand.
- The user page allocator is fed by Limine's memory map through a simple
  physical frame freelist, with HHDM-backed frame zeroing/copying and explicit
  user-pointer copies for native syscalls.
- PS/2 keyboard and mouse IRQ paths with small user-facing input syscalls.
- A dynamically loaded Spleen 12x24 PSF2 font at `/etc/font.psf`.
- A framebuffer terminal with incremental row/cell redraws instead of full
  screen redraw on every character.
- Early PCI/Virtio discovery and queue-memory scaffolding.

Still useful next:

- Replace the remaining compile-time user process capacity with a growable
  descriptor table and dynamically allocated page-table roots.
- Expand the Linux/POSIX ABI with pipes, descriptor duplication, `poll`/`select`,
  signals, termios/TTY handling, process groups, and job-control semantics.
- Add proper argv/envp/auxv setup for Linux ABI program startup.
- Move from the current framebuffer console path toward a proper TTY/console
  subsystem so Bash can become the default shell again without special cases.
- Add writable filesystem support or a second writable layer on top of nkfs.
- Move the root block backend from ATA PIO to Virtio block on `q35`.
- Complete Virtio input/block drivers beyond discovery and queue setup.
- Give GUI applications private window buffers and route them through a real
  compositor/window manager instead of shared framebuffer drawing.
- Split GUI/input/filesystem services behind explicit IPC/capability boundaries
  so the kernel stays small and userland services can evolve independently.
- Add broader syscall/error-path tests and boot-time smoke tests for common
  userland commands such as `ls`, `cat`, `echo`, and `pwd`.
