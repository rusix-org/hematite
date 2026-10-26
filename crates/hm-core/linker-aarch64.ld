OUTPUT_FORMAT(elf64-littleaarch64)
ENTRY(arm_start)
PHDRS {
    text PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data PT_LOAD FLAGS(6);
}
SECTIONS {
    . = 0x40200000;
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
        __exception_stack_guard = .;
        . += 4096;
        . += 16384;
        __exception_stack_top = .;
        __thread0_guard = .;
        . += 4096 + 65536;
        __thread0_top = .;
        __thread1_guard = .;
        . += 4096 + 65536;
        __thread1_top = .;
        __thread2_guard = .;
        . += 4096 + 65536;
        __thread2_top = .;
        __thread3_guard = .;
        . += 4096 + 65536;
        __thread3_top = .;
        __secondary_stack_guard = .;
        . += 4096 + 65536;
        __secondary_stack_top = .;
        __secondary_exception_stack_guard = .;
        . += 4096 + 16384;
        __secondary_exception_stack_top = .;
        __secondary_thread0_guard = .;
        . += 4096 + 65536;
        __secondary_thread0_top = .;
        __secondary_thread1_guard = .;
        . += 4096 + 65536;
        __secondary_thread1_top = .;
        __secondary_thread2_guard = .;
        . += 4096 + 65536;
        __secondary_thread2_top = .;
        __secondary_thread3_guard = .;
        . += 4096 + 65536;
        __secondary_thread3_top = .;
    } :data
    __bss_end = .;
    __kernel_end = .;
    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
ASSERT(__kernel_end < 0x41000000, "bootstrap exceeds 16 MiB RAM mapping")
ASSERT(__text_start == 0x40200000, "reserve 2 MiB before kernel for DTB")
ASSERT((__text_start & 4095) == 0, "unaligned text")
ASSERT((__text_end & 4095) == 0, "unaligned text end")
ASSERT((__rodata_end & 4095) == 0, "unaligned rodata end")
ASSERT((__stack_guard & 4095) == 0, "unaligned guard")
ASSERT((__exception_stack_guard & 4095) == 0, "unaligned exception guard")
ASSERT((__exception_stack_top & 15) == 0, "unaligned exception stack")
ASSERT((__bss_end & 7) == 0, "BSS must contain whole doublewords")
ASSERT((__thread0_guard & 4095) == 0, "unaligned worker guard")
ASSERT((__thread3_top & 4095) == 0, "unaligned worker stack top")
ASSERT((__secondary_stack_guard & 4095) == 0, "unaligned secondary stack guard")
ASSERT((__secondary_stack_top & 4095) == 0, "unaligned secondary stack top")
ASSERT(__secondary_stack_top - __secondary_stack_guard == 69632, "wrong secondary stack size")
ASSERT((__secondary_exception_stack_guard & 4095) == 0, "unaligned secondary exception_stack guard")
ASSERT((__secondary_exception_stack_top & 4095) == 0, "unaligned secondary exception_stack top")
ASSERT(__secondary_exception_stack_top - __secondary_exception_stack_guard == 20480, "wrong secondary exception_stack size")
ASSERT((__secondary_thread0_guard & 4095) == 0, "unaligned secondary thread0 guard")
ASSERT((__secondary_thread0_top & 4095) == 0, "unaligned secondary thread0 top")
ASSERT(__secondary_thread0_top - __secondary_thread0_guard == 69632, "wrong secondary thread0 size")
ASSERT((__secondary_thread1_guard & 4095) == 0, "unaligned secondary thread1 guard")
ASSERT((__secondary_thread1_top & 4095) == 0, "unaligned secondary thread1 top")
ASSERT(__secondary_thread1_top - __secondary_thread1_guard == 69632, "wrong secondary thread1 size")
ASSERT((__secondary_thread2_guard & 4095) == 0, "unaligned secondary thread2 guard")
ASSERT((__secondary_thread2_top & 4095) == 0, "unaligned secondary thread2 top")
ASSERT(__secondary_thread2_top - __secondary_thread2_guard == 69632, "wrong secondary thread2 size")
ASSERT((__secondary_thread3_guard & 4095) == 0, "unaligned secondary thread3 guard")
ASSERT((__secondary_thread3_top & 4095) == 0, "unaligned secondary thread3 top")
ASSERT(__secondary_thread3_top - __secondary_thread3_guard == 69632, "wrong secondary thread3 size")
ASSERT(__secondary_stack_guard == __thread3_top, "secondary overlaps CPU0 stacks")
ASSERT(__secondary_exception_stack_guard == __secondary_stack_top, "secondary stacks not adjacent")
ASSERT(__secondary_thread0_guard == __secondary_exception_stack_top, "secondary stacks not adjacent")
ASSERT(__secondary_thread1_guard == __secondary_thread0_top, "secondary stacks not adjacent")
ASSERT(__secondary_thread2_guard == __secondary_thread1_top, "secondary stacks not adjacent")
ASSERT(__secondary_thread3_guard == __secondary_thread2_top, "secondary stacks not adjacent")
ASSERT(__secondary_thread3_top == __kernel_end, "secondary stacks outside kernel reservation")
ASSERT((arm_secondary_root & 4095) == 0, "unaligned secondary root")
ASSERT(arm_secondary_root + 4096 <= __stack_guard, "secondary root overlaps stacks")
