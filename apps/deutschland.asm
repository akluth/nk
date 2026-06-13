bits 64

base equ 0x40000000

ehdr:
    db 0x7f, "ELF", 2, 1, 1, 0
    times 8 db 0
    dw 2
    dw 0x3e
    dd 1
    dq base + _start
    dq phdr - $$
    dq 0
    dd 0
    dw ehdr_size
    dw phdr_size
    dw 1
    dw 0
    dw 0
    dw 0
ehdr_size equ $ - ehdr

phdr:
    dd 1
    dd 5
    dq 0
    dq base
    dq base
    dq file_size
    dq file_size
    dq 0x1000
phdr_size equ $ - phdr

_start:
    mov eax, 1
    mov edi, 1
    lea rsi, [rel message]
    mov edx, message_len
    syscall

    mov eax, 60
    xor edi, edi
    syscall

message db "hallo deutschland", 10
message_len equ $ - message
file_size equ $ - $$
