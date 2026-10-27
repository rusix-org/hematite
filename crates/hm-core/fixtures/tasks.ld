OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(native_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0x100000000;
    .text : { KEEP(*(.text.entry)) *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : { KEEP(*(.fixture.rodata)) *(.rodata .rodata.*) } :rodata
    . = ALIGN(4096);
    .data : { KEEP(*(.fixture.data)) *(.data .data.*) *(.got .got.*) } :data
    .bss (NOLOAD) : { KEEP(*(.fixture.bss)) *(.bss .bss.*) *(COMMON) } :data
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
