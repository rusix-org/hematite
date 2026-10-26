OUTPUT_FORMAT(elf64-x86-64)
ENTRY(start32)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
    note PT_NOTE FLAGS(4);
}
SECTIONS {
    . = 0x100000;
    __text_start = .;
    .text : { KEEP(*(.text.boot)) *(.text .text.*) } :text
    . = ALIGN(4096);
    __text_end = .;
    __rodata_start = .;
    .rodata : { *(.rodata .rodata.*) } :rodata
    .note.Xen : ALIGN(4) { KEEP(*(.note.Xen)) } :rodata :note
    . = ALIGN(4096);
    __rodata_end = .;
    .data : { *(.data .data.*) *(.got .got.*) } :data
    . = ALIGN(4096);
    __bss_start = .;
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } :data
    . = ALIGN(4096);
    .stack (NOLOAD) : {
        __stack_guard = .;
        . += 4096;
        __stack_bottom = .;
        . += 65536;
        __stack_top = .;
    } :data
    __bss_end = .;
    __kernel_end = .;
    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
ASSERT(__kernel_end < 0x1000000, "bootstrap exceeds its 16 MiB mapping")
ASSERT((__text_start & 4095) == 0, "unaligned text")
ASSERT((__text_end & 4095) == 0, "unaligned text end")
ASSERT((__rodata_end & 4095) == 0, "unaligned rodata end")
ASSERT((__stack_guard & 4095) == 0, "unaligned stack guard")
ASSERT((__bss_end & 3) == 0, "BSS must consist of whole dwords")
