# GNU Bash Port

This port targets the unmodified GNU Bash source release `bash-5.3.tar.gz`.
The source tarball and upstream patch files are fetched into `third_party`,
which is intentionally ignored by Git.

Fetch the source on Windows:

```powershell
.\ports\bash\fetch-bash.ps1
```

## Kernel Integration Status

- The kernel starts `/bin/bash` as the standard Linux ABI user task.
- The old Rust terminal fallback has been removed.
- Bash runs with the Linux/POSIX syscall personality.
- The syscall dispatcher selects Linux compatibility by task ABI, not by a
  hard-coded program name.
- The nkfs image builder automatically includes `build/user/bash.elf` at
  `/bin/bash` when a Bash port build produces it.
- QEMU serial verification reaches the real GNU Bash prompt:
  `bash-5.3#`.

## Compatibility Implemented So Far

The Linux compatibility path has basic support for:

- `read`, including blocking keyboard-backed stdin.
- `write` and `writev` to stdout/stderr.
- `open`, `openat`, `close`, `fstat`, `newfstatat`, and `lseek` for read-only
  nkfs files.
- `stat`, `readlink`, `brk`, `mmap`, `munmap`, `uname`, `getcwd`, `chdir`,
  `access`, `fcntl`, `ioctl(TCGETS)`, and `ioctl(TIOCGWINSZ)`.
- UID/GID/resuid/resgid and parent PID queries.
- Signal setup syscalls as no-op compatibility stubs.
- `arch_prctl`, `set_tid_address`, `set_robust_list`, `prlimit64`, and
  `getrandom` stubs.
- `gettimeofday`, `clock_gettime`, `fork`, `execve`, `wait4`, `exit`, and
  `exit_group`.

## Build Bash

The normal OS build calls the Bash port automatically when `third_party` does
not already contain a built Bash binary:

```powershell
.\scripts\build-limine.ps1
```

Manual fetch/build remains available on Windows:

```powershell
.\ports\bash\fetch-bash.ps1
.\ports\bash\build-bash.ps1
.\scripts\build-limine.ps1
```

The Bash binary is a static `x86_64-linux-musl` ELF linked at
`0x40000000`, so it fits the current nk userland loader. On a fresh checkout,
the scripts download upstream Bash and portable Zig into ignored `third_party`
storage as needed.

## Still Required

- A real process table beyond the current single-child `fork`/`execve`/`wait4`
  path.
- Pipes and descriptor duplication (`pipe`, `dup`, `dup2`, `dup3`).
- A real terminal device model with line discipline, termios, and job-control
  signal semantics.
- Per-process file-descriptor tables rather than the current single FD 3 shim.
- Larger and independently allocated user address spaces for real program
  images and heaps.
- More complete per-process address-space allocation beyond the current fixed
  task slots.

## Intended Build Output

The port build produces:

```text
build/user/bash.elf
```

The normal OS image build will then copy it into `build/nk-root.nkfs` as
`/bin/bash`; the kernel will start it as the default terminal process.
