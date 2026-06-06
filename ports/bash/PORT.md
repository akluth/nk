# GNU Bash Port

This port targets the unmodified GNU Bash source release `bash-5.3.tar.gz`.
The source tarball and upstream patch files are fetched into `third_party`,
which is intentionally ignored by Git.

Fetch the source on Windows:

```powershell
.\ports\bash\fetch-bash.ps1
```

## Kernel Integration Status

- The kernel now prefers `BASH.ELF` as the primary user task.
- If `BASH.ELF` is absent from the FAT32 application disk, the old Rust
  terminal remains only as a temporary fallback.
- Bash runs with the Linux/POSIX syscall personality.
- The syscall dispatcher selects Linux compatibility by task ABI, not by a
  hard-coded program name.
- The FAT32 image builder automatically includes `build/user/bash.elf` when a
  Bash port build produces it.
- QEMU serial verification reaches the real GNU Bash prompt:
  `bash-5.3#`.

## Compatibility Implemented So Far

The Linux compatibility path has basic support for:

- `read`, including blocking keyboard-backed stdin.
- `write` and `writev` to stdout/stderr.
- `open`, `openat`, `close`, `fstat`, `newfstatat`, and `lseek` for FAT32 files.
- `stat`, `readlink`, `brk`, `mmap`, `munmap`, `uname`, `getcwd`, `chdir`,
  `access`, `fcntl`, `ioctl(TCGETS)`, and `ioctl(TIOCGWINSZ)`.
- UID/GID/resuid/resgid and parent PID queries.
- Signal setup syscalls as no-op compatibility stubs.
- `arch_prctl`, `set_tid_address`, `set_robust_list`, `prlimit64`, and
  `getrandom` stubs.
- `gettimeofday`, `clock_gettime`, `wait4`, `exit`, and `exit_group`.

## Build Bash

Fetch and build the port on Windows:

```powershell
.\ports\bash\fetch-bash.ps1
.\ports\bash\build-bash.ps1
.\scripts\build-limine.ps1
```

The Bash binary is a static `x86_64-linux-musl` ELF linked at
`0x40000000`, so it fits the current nk userland loader.

## Still Required

- `fork`, `execve`, `wait4`/`waitpid`, and process reaping.
- Pipes and descriptor duplication (`pipe`, `dup`, `dup2`, `dup3`).
- A real terminal device model with line discipline, termios, and job-control
  signal semantics.
- Per-process file-descriptor tables rather than the current single FD 3 shim.
- Larger and independently allocated user address spaces for real program
  images and heaps.
- Restore GUI, task viewer, and Bash as concurrent isolated user tasks. The
  current Bash milestone boots Bash as the primary user process because the
  older single-image address space cannot safely host all large ELF programs at
  the same virtual base yet.

## Intended Build Output

The port build produces:

```text
build/user/bash.elf
```

The normal OS image build will then copy it into `build/nk-apps.fat32` as
`BASH.ELF`; the kernel will boot it in slot 1 instead of the fallback terminal.
