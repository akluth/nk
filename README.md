# nk

`nk` is a tiny x86-64 operating system written in Rust. It boots through the
Limine bootloader on BIOS and UEFI systems, brings up a small microkernel-style
core, starts Ring 3 tasks, and loads user programs as ELF files from a small
writable nkfs root filesystem disk.

## Current Status

- Boots in QEMU through Limine/BIOS.
- Initializes GDT/TSS, IDT, PIC, and the PIT timer.
- Builds a dedicated userland page-table root.
- Starts Ring 3 tasks through `iretq` and saved trapframes.
- Uses timer interrupt trapframes as scheduler context.
- Builds a nkfs root disk containing `/bin/bash`, `/bin/gui`, `/bin/taskview`,
  `/bin/nasm`, uutils Coreutils command aliases, plus small data files.
- Provides persistent writable regular files on the nkfs root disk through the
  Virtio block backend; user programs can create and overwrite files under
  existing directories such as `/home/root`.
- Reads and writes the nkfs disk through the Virtio block-device path, with ATA
  PIO retained only as a legacy read fallback.
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
- Includes a NASM 2.16.03 port under `ports/nasm`; the port fetches upstream
  NASM sources into ignored `third_party` storage and builds a static
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
- `nsh` has a tiny `edit <path>` command for creating text files on nkfs, and
  `/bin/nasm` can assemble `/home/root/hello.asm` into a new persistent
  executable that immediately runs in Ring 3.

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
- `src/nkfs.rs`: tiny writable nkfs implementation with UNIX-like absolute
  paths, persistent regular-file creation, truncation, writes, directory-entry
  extension, and on-disk inode/superblock updates.
- `src/userland.rs`: address-space model, ELF loader, task frame setup, CR3
  switch, and Ring 3 entry.
- `src/services.rs`: kernel-side framebuffer service used by GUI syscalls.
- `src/mouse.rs`: tiny PS/2 mouse packet decoder.
- `src/linux_abi.rs`: Linux/POSIX syscall compatibility path for Linux ABI
  user tasks, including basic file I/O, keyboard-backed stdin, `writev`,
  `pwrite64`, `openat`, `stat`, `fstat`, `lseek`, `truncate`, `ftruncate`,
  `brk`, file-backed and anonymous `mmap`, `readlink`, `uname`, `getcwd`,
  `access`, `fcntl`, `ioctl`, resource-limit queries, UID/GID queries, signal
  setup stubs, time syscalls, and exit syscalls.
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
- `ports/nasm/`: fetch/build glue for the real NASM port.
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
- `build/user/nasm.elf`: NASM executable; the normal build fetches/builds it
  on demand and copies it to the root disk as `/bin/nasm`.
- `build/nk-root.nkfs`: the nkfs root disk containing `/bin`, `/etc`,
  `/home/root`, user programs, and small data files.

The ISO only contains the kernel and bootloader files. User programs are loaded
from the nkfs root disk at runtime.

Building Coreutils requires network access on the first build to download the
official uutils `x86_64-unknown-linux-musl` release asset into ignored
`third_party` storage. Building Bash requires network access on the first
build, MSYS2 `make`, MSYS2 `gcc` for host build tools, and portable Zig for the
static Musl target; the build script downloads Zig and Bash sources into
ignored `third_party` storage when needed. See `ports/bash/PORT.md`.
Building NASM also requires network access on the first build to download the
official NASM source archive into ignored `third_party` storage. See
`ports/nasm/PORT.md`.

## Assemble in nk

After booting, `nsh` can assemble and run a tiny ELF program entirely inside the
OS:

```text
# nasm -f bin /home/root/hello.asm -o /home/root/hello
# /home/root/hello
hello from nasm on nk
```

You can create or replace small source files with `edit`:

```text
# edit /home/root/test.asm
; finish the file with a single dot on its own line
.
saved
```

Files written this way are persisted to the Virtio-backed nkfs root disk. A
two-boot smoke test verifies that `/home/root/hello`, generated by NASM in one
boot, is still visible and executable after rebooting with the same disk image.

## Run in QEMU

Windows:

```powershell
.\scripts\build-limine.ps1 -Run
```

The BIOS run path uses QEMU's `pc` machine and attaches the nkfs root disk as a
Virtio block device. The kernel can still fall back to ATA PIO when booted with
an older manual QEMU command, but the standard run path is Virtio-only.

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
- A nkfs root disk with UNIX-like absolute paths, `/bin`, `/etc`,
  `/home/root`, `/hello.txt`, `/bin/init`, `/bin/nsh`, `/bin/bash`, `/bin/gui`,
  `/bin/taskview`, `/bin/nasm`, and uutils Coreutils aliases.
- Persistent writable regular files on nkfs, including `open(O_CREAT)`,
  `write`, `writev`, `pwrite64`, `truncate`, `ftruncate`, `unlink`,
  `unlinkat`, directory listing integration, file-backed `mmap`, on-disk inode
  updates, superblock free-block tracking, and Virtio block writes.
- ELF loading from the nkfs root disk for native no_std programs and static
  Linux ABI programs.
- Optional separate GUI and task viewer binaries loaded from `/bin`; they are
  no longer mandatory boot-time kernel payloads.
- Rust uutils Coreutils integration with command aliases in `/bin`.
- Real NASM integration as `/bin/nasm`, with an in-OS workflow that assembles
  `/home/root/hello.asm` into `/home/root/hello` and executes the result.
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
- A root block-device layer in front of nkfs, with Virtio block probing,
  ATA PIO fallback support, no boot-time Coreutils image preload, and cached
  nkfs superblock state after mount.
- The legacy Virtio block path now uses per-request slots, per-slot DMA buffers,
  used-ring completion scanning from the IRQ path, and a robust synchronous
  wait fallback for early kernel/syscall reads.
- Virtio and block backend state is encapsulated behind `UnsafeCell` driver
  globals instead of direct `static mut` references, keeping the kernel build
  free of Rust 2024 unsafe-reference warnings.
- Kernel panic diagnostics are printed to serial and the framebuffer boot log
  instead of silently halting.

Still useful next:

- Replace the remaining compile-time user process capacity with a growable
  descriptor table and dynamically allocated page-table roots.
- Expand the Linux/POSIX ABI with pipes, descriptor duplication, `poll`/`select`,
  signals, termios/TTY handling, process groups, and job-control semantics.
- Add proper argv/envp/auxv setup for Linux ABI program startup.
- Move from the current framebuffer console path toward a proper TTY/console
  subsystem so Bash can become the default shell again without special cases.
- Add full directory mutation (`mkdir`, `rmdir`, `rename`), free-space reuse,
  and evolve nkfs toward a journaled or copy-on-write root filesystem.
- Complete Virtio input drivers beyond discovery and queue setup.
- Give GUI applications private window buffers and route them through a real
  compositor/window manager instead of shared framebuffer drawing.
- Split GUI/input/filesystem services behind explicit IPC/capability boundaries
  so the kernel stays small and userland services can evolve independently.
- Add broader syscall/error-path tests and boot-time smoke tests for common
  userland commands such as `ls`, `cat`, `echo`, and `pwd`.
