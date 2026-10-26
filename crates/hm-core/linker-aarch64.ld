OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(arm_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0x40080000;
    __text_start = .;
    .text : { KEEP(*(.text.boot)) *(.text .text.*) } :text
    . = ALIGN(4096);
    __text_end = .;
    __rodata_start = .;
    .rodata : { *(.rodata .rodata.*) } :rodata
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
        . += 65536;
        __stack_top = .;
    } :data
    __bss_end = .;
    __kernel_end = .;
    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
ASSERT(__kernel_end < 0x41000000, "bootstrap exceeds 16 MiB RAM mapping")
ASSERT((__text_start & 4095) == 0, "unaligned text")
ASSERT((__text_end & 4095) == 0, "unaligned text end")
ASSERT((__rodata_end & 4095) == 0, "unaligned rodata end")
ASSERT((__stack_guard & 4095) == 0, "unaligned guard")
ASSERT((__bss_end & 7) == 0, "BSS must contain whole doublewords")
