ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x0000000040120000;

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :text

    . = ALIGN(4K);

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(4K);

    .data : ALIGN(4K) {
        *(.data .data.*)
    } :data

    .bss : ALIGN(4K) {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.comment)
        *(.note .note.*)
        *(.eh_frame .eh_frame.*)
    }
}
