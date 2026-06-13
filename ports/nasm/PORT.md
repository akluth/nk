# NASM port

This port builds the official NASM source as a static `x86_64-linux-musl`
executable linked at `0x40000000`, matching nk's current Linux userland load
range.

The intended on-system workflow is:

```sh
nasm -f bin /home/root/hello.asm -o /home/root/hello
/home/root/hello
```

The example source emits a complete ELF64 executable as a flat binary, so no
separate linker is required yet.
