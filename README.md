# nk

`nk` is a tiny x86-64 operating system written in Rust. It boots through the
Limine bootloader on BIOS and UEFI systems, brings up a small microkernel-style
core, starts Ring 3 tasks, and loads user programs as ELF files from a small
FAT32 application disk.

## Current Status

- Boots in QEMU through Limine/BIOS.
- Initializes GDT/TSS, IDT, PIC, and the PIT timer.
- Builds a dedicated userland page-table root.
- Starts Ring 3 tasks through `iretq` and saved trapframes.
- Uses timer interrupt trapframes as scheduler context.
- Builds a FAT32 application disk containing `GUI.ELF`, `SHELL.ELF`, and
  `TASKVIEW.ELF`, `CAT.ELF`, plus small data files.
- Reads that FAT32 disk through a first ATA PIO block-device path.
- Parses GUI, shell, and task viewer ELF files in the kernel and starts them as
  Ring 3 tasks.
- Provides minimal GUI syscalls for clearing the screen, drawing rectangles,
  and drawing scaled bitmap text.
- Starts a passive userland GUI ELF that paints the desktop background.
- Starts a second userland shell ELF with a single interactive shell window.
- Starts a third userland task viewer ELF that displays the running user tasks.
- Starts a fourth userland C `cat` ELF based on the original V7 UNIX `cat.c`
  program body and prints a FAT32 file through a small Linux-like syscall
  compatibility layer.
- Uses a generated monospace bitmap font for GUI text.
- Tracks a generic focused user-task slot so userland can build taskbar/window
  switching without making the kernel depend on specific GUI programs.
- Delivers PS/2 keyboard input through IRQ1 and a small `read_key` syscall.
- Delivers PS/2 mouse input through IRQ12 and a small `read_mouse` syscall.
- Supports the shell commands `version`, `cat`, and `shutdown`.
- The `cat` command starts the separate `CAT.ELF` user task on demand; that
  program reads `/HELLO.TXT` from the FAT32 application disk.

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
- `src/ata.rs`: first ATA PIO sector reader for the QEMU FAT32 application disk.
- `src/fat32.rs`: tiny FAT32 reader for 8.3 root-directory files.
- `src/userland.rs`: address-space model, ELF loader, task frame setup, CR3
  switch, and Ring 3 entry.
- `src/services.rs`: kernel-side framebuffer service used by GUI syscalls.
- `src/mouse.rs`: tiny PS/2 mouse packet decoder.
- `src/linux_abi.rs`: first Linux-like syscall compatibility path for the C
  `cat` process, including basic file I/O, `openat`, `fstat`, `lseek`, `brk`,
  `uname`, and exit syscalls.
- `src/font.rs`: generated fixed-size monospace bitmap font.
- `src/framebuffer.rs`: low-level pixel and rectangle drawing.
- `src/limine.rs`: Limine framebuffer, HHDM, and kernel address requests.
- `src/pci.rs` and `src/virtio.rs`: PCI scan, Virtio capability discovery, and
  early queue memory setup.
- `user/gui/src/main.rs`: separate no_std Rust GUI executable.
- `user/gui/linker.ld`: GUI ELF linker script.
- `user/shell/src/main.rs`: separate no_std Rust shell executable.
- `user/shell/linker.ld`: shell ELF linker script.
- `user/taskview/src/main.rs`: separate no_std Rust task viewer executable.
- `user/taskview/linker.ld`: task viewer ELF linker script.
- `user/cat/src/cat.c`: separate C `cat` executable using the V7 UNIX `cat.c`
  program body with a tiny local runtime.
- `user/cat/linker.ld`: cat ELF linker script.

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
- `build/user/gui.elf`: the separate userland GUI executable.
- `build/user/shell.elf`: the separate userland shell executable.
- `build/user/taskview.elf`: the separate userland task viewer executable.
- `build/user/cat.elf`: the separate userland C cat executable.
- `build/nk-apps.fat32`: the FAT32 disk image containing the user programs.

The ISO only contains the kernel and bootloader files. User programs are loaded
from the FAT32 application disk at runtime.

Building `cat.elf` requires `clang` and `rust-lld` because it is a C userland
program rather than a Rust executable.

## Run in QEMU

Windows:

```powershell
.\scripts\build-limine.ps1 -Run
```

The BIOS run path uses QEMU's `pc` machine with an IDE disk because the current
block reader is the intentionally small ATA PIO path. The app disk is attached
automatically.

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

- Add a compositor/window manager so GUI and shell windows no longer draw
  directly into the shared framebuffer.
- Replace the temporary Rust mini-shell with a real port of GNU Bash once the
  POSIX process model is ready.
- Add `fork`, `execve`, `waitpid`, pipes, signals, termios/TTY handling, and
  argv/envp/auxv setup for Bash and other real Linux/POSIX programs.
- Load a real PSF/SSFN font from the app disk instead of compiling the generated
  bitmap table into the kernel.
- Add dirty-rectangle or double-buffered drawing to remove the remaining direct
  framebuffer redraw artifacts.
- Replace the fixed user image buffer with per-process page allocation.
- Replace the interim C runtime shim with enough Linux process ABI support to
  execute unmodified static Linux binaries.
- Add argv/envp/auxv setup on the initial user stack.
- Split GUI syscalls into a proper capability-checked IPC protocol.
- Move the FAT32 block backend from ATA PIO to Virtio block on `q35`.
- Move more kernel-side services behind explicit userland/server boundaries.
