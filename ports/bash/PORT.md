# GNU Bash Port

This port targets the unmodified GNU Bash source release `bash-5.3.tar.gz`.
The source tarball and upstream patch files are fetched into `third_party`,
which is intentionally ignored by Git.

Fetch the source on Windows:

```powershell
.\ports\bash\fetch-bash.ps1
```

## Kernel Integration Status

- The kernel now prefers `BASH.ELF` for user task slot 1.
- If `BASH.ELF` is absent from the FAT32 application disk, the old Rust
  terminal remains only as a temporary fallback.
- Slot 1 runs with the Linux/POSIX syscall personality when `BASH.ELF` exists.
- The syscall dispatcher selects Linux compatibility by task ABI, not by a
  hard-coded program name.
- The FAT32 image builder automatically includes `build/user/bash.elf` when a
  Bash port build produces it.

## Compatibility Implemented So Far

The Linux compatibility path has basic support for:

- `read`, including blocking keyboard-backed stdin.
- `write` to stdout/stderr.
- `open`, `openat`, `close`, `fstat`, `newfstatat`, and `lseek` for FAT32 files.
- `brk`, `uname`, `getcwd`, `chdir`, `access`, `ioctl(TIOCGWINSZ)`.
- UID/GID and parent PID queries.
- Signal setup syscalls as no-op compatibility stubs.
- `arch_prctl`, `set_tid_address`, `set_robust_list`, `prlimit64`, and
  `getrandom` stubs.
- `exit` and `exit_group`.

## Still Required Before Bash Can Run

Bash is not linked or running yet. The blockers are structural:

- `fork`, `execve`, `wait4`/`waitpid`, and process reaping.
- Pipes and descriptor duplication (`pipe`, `dup`, `dup2`, `dup3`).
- A real terminal device model with line discipline, termios, and job-control
  signal semantics.
- Per-process file-descriptor tables rather than the current single FD 3 shim.
- argv/envp/auxv construction on the initial user stack.
- Larger and independently allocated user address spaces for real program
  images and heaps.
- Either a static libc target for nk or enough Linux ABI coverage to run a
  static Linux Bash binary.

## Intended Build Output

When the port is ready, the build should produce:

```text
build/user/bash.elf
```

The normal OS image build will then copy it into `build/nk-apps.fat32` as
`BASH.ELF`; the kernel will boot it in slot 1 instead of the fallback terminal.
