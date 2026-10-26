OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(native_start)
PHDRS {
    entry PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
}
SECTIONS {
    . = 0x100000000;
    .entry : { KEEP(*(.text.entry)) } :entry
    . = 0x100000100;
    .poll : { KEEP(*(.text.poll)) } :entry
    . = 0x100002000;
    .data : { KEEP(*(.runtime.data)) *(.data .data.*) *(.got .got.*) } :data
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) } :data
    . = 0x100010000;
    .text : { *(.text .text.*) } :text
    . = ALIGN(4096);
    .rodata : { KEEP(*(.runtime.rodata)) *(.rodata .rodata.*) } :rodata
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr) }
}
ASSERT(native_start == 0x100000000, "entry moved")
ASSERT(poll_start == 0x100000100, "poll moved")
ASSERT(poll_end == 0x100000108, "poll interval changed")
ASSERT(release == 0x100002000, "release moved")
ASSERT(private_data == 0x100002008, "private data moved")
