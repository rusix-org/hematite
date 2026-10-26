// SAFETY: Direct-ELF QEMU virt entry at EL1 with MMU/caches off; all storage
// belongs to this image. The reviewed bootstrap establishes the Rust ABI.
.arch armv8.1-a
.section .text.boot,"ax"
.global arm_start
arm_start:
    msr daifset, #15
    mov x19, x0
    mrs x0, CurrentEL
    cmp x0, #4
    b.ne arm_exception_fail
    mrs x0, mpidr_el1
    ldr x1, ={cpu_affinity_mask}
    tst x0, x1
    b.ne arm_exception_fail          // Reject peers before any shared stack/BSS write.
    msr spsel, #1
    ldr x0, =__stack_top
    mov sp, x0
    ldr x0, =__exception_stack_top
    msr sp_el0, x0
    ldr x0, =arm_vectors
    msr vbar_el1, x0
    isb
    mrs x0, sctlr_el1
    ldr x1, =0x1005
    tst x0, x1                      // Require M/C/I off, not a warm boot.
    b.ne arm_exception_fail
    ldr x0, =__bss_start
    ldr x1, =__bss_end
1:
    cmp x0, x1
    b.hs 2f
    str xzr, [x0], #8
    b 1b
2:
    ldr x0, =arm_cpu_records
    msr tpidr_el1, x0
    isb
    add x0, x0, #{cpu_stride}
    ldr x1, ={cpu_canary}
    mov x2, #{cpu_words}
.Lseed_inactive_cpu:
    str x1, [x0], #8
    subs x2, x2, #1
    b.ne .Lseed_inactive_cpu
    bl arm_uart_init
    mrs x0, id_aa64mmfr0_el1
    ubfx x0, x0, #28, #4            // 4 KiB granule supported (0 or LPA2=1).
    cmp x0, #1
    b.hi arm_exception_fail
    mrs x0, id_aa64mmfr1_el1
    ubfx x0, x0, #20, #4
    cbz x0, arm_reject_pan

    ldr x0, =arm_root
    ldr x1, =arm_device_l2
    orr x1, x1, #3
    str x1, [x0]
    ldr x1, =arm_ram_l2
    orr x1, x1, #3
    str x1, [x0, #8]
    // CPU1's future root shares only immutable device/identity subtrees.
    ldr x2, =arm_secondary_root
    ldr x1, =arm_device_l2
    orr x1, x1, #3
    str x1, [x2]
    ldr x1, =arm_ram_l2
    orr x1, x1, #3
    str x1, [x2, #8]              // Entry2 remains zero: no owned arena.
    ldr x1, =arm_owned_l2
    orr x1, x1, #3
    str x1, [x0, #16]              // Owned data arena at VA 0x80000000.
    ldr x0, =arm_owned_l2
    ldr x1, =arm_owned_l3
    orr x1, x1, #3
    str x1, [x0]                  // Leaves remain invalid until owned map.
    ldr x0, =arm_device_l2
    ldr x1, =arm_gic_l3
    orr x1, x1, #3
    str x1, [x0, #512]             // GIC MMIO lies in 0x08000000 / 2 MiB ==64.
    ldr x2, =arm_gic_l3
    ldr x1, =0x0060000008000407    // GICD first4KiB, Device EL1 RW PXN/UXN.
    str x1, [x2]
    ldr x1, =0x0060000008010407    // GICC first4KiB; no virtualization page.
    str x1, [x2, #128]
    ldr x1, =arm_device_l3
    orr x1, x1, #3
    str x1, [x0, #576]             // 0x09000000 / 2 MiB == 72.
    ldr x0, =arm_device_l3
    ldr x1, =0x0060000009000407    // PL011 only: Device, AF, EL1 RW, PXN/UXN.
    str x1, [x0]
    ldr x0, =arm_ram_l2
    ldr x1, =arm_ram_l3
    mov x2, #8
3:
    orr x3, x1, #3
    str x3, [x0], #8
    add x1, x1, #4096
    subs x2, x2, #1
    b.ne 3b

    ldr x6, =arm_ram_l3
    mov x3, #0x40000000
    mov x2, #4096
    ldr x7, =__text_start
    ldr x8, =__text_end
    ldr x9, =__rodata_start
    ldr x10, =__rodata_end
    ldr x11, =__stack_guard
    ldr x12, =__exception_stack_guard
    ldr x13, =__thread0_guard
    ldr x14, =__thread1_guard
    ldr x15, =__thread2_guard
    ldr x16, =__thread3_guard
4:
    cmp x3, x11
    b.eq 8f
    cmp x3, x12
    b.eq 8f
    cmp x3, x13
    b.eq 8f
    cmp x3, x14
    b.eq 8f
    cmp x3, x15
    b.eq 8f
    cmp x3, x16
    b.eq 8f
    cmp x3, x7
    b.lo 7f
    cmp x3, x8
    b.hs 5f
    ldr x4, =0x0040000000000783    // EL1 RX, read-only, UXN, normal WB.
    b 9f
5:
    cmp x3, x9
    b.lo 7f
    cmp x3, x10
    b.hs 7f
    ldr x4, =0x0060000000000783    // EL1 RO, PXN/UXN.
    b 9f
7:
    ldr x4, =0x0060000000000703    // EL1 RW, PXN/UXN.
    b 9f
8:
    str xzr, [x6], #8
    b 10f
9:
    orr x4, x4, x3
    str x4, [x6], #8
10:
    add x3, x3, #4096
    subs x2, x2, #1
    b.ne 4b

    // Secondary guard edits happen only here, before caches/MMU and DSB SY.
    ldr x0, =arm_ram_l3
    mov x2, #0x40000000
    ldr x1, =__secondary_stack_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]
    ldr x1, =__secondary_exception_stack_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]
    ldr x1, =__secondary_thread0_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]
    ldr x1, =__secondary_thread1_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]
    ldr x1, =__secondary_thread2_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]
    ldr x1, =__secondary_thread3_guard
    sub x1, x1, x2
    lsr x1, x1, #12
    str xzr, [x0, x1, lsl #3]

    dsb sy
    mov x0, #0xff                  // Attr0 WB; Attr1 Device-nGnRnE=0.
    msr mair_el1, x0
    ldr x0, =0x80993519            // 39-bit TTBR0, 4 KiB, WB/ISH, TTBR1 off.
    msr tcr_el1, x0
    ldr x0, =arm_root
    msr ttbr0_el1, x0
    msr ttbr1_el1, xzr
    isb
    tlbi vmalle1
    dsb sy
    isb
    ic iallu
    dsb sy
    isb
    ldr x0, =0x3058180d            // RES1, M/C/I/WXN/SA; SPAN=0, little endian.
    msr sctlr_el1, x0
    isb
    msr pan, #1
    mov x0, #0x300000
    msr cpacr_el1, x0
    isb
    msr fpcr, xzr
    msr fpsr, xzr
    mov x0, x19
    bl {rust_entry}
    b arm_exception_fail

arm_reject_pan:
    ldr x0, =pan_message
    mov x1, #0x09000000
11:
    ldrb w2, [x0], #1
    cbz w2, 13f
    mov x4, #0xffff
12:
    ldr w3, [x1, #0x18]
    tbz w3, #5, 14f
    subs x4, x4, #1
    b.ne 12b
    b arm_exception_fail
14:
    str w2, [x1]
    b 11b
13:
    ldr x1, =reject_block
    b arm_semihost_exit

.global arm_exception_fail
arm_exception_fail:
    ldr x1, =failure_block
arm_semihost_exit:
    mov x0, #0x20
    hlt #0xf000
15:
    wfi
    b 15b

arm_uart_init:
    mov x0, #0x09000000
    str wzr, [x0, #0x30]
    mov w1, #0x7ff
    str w1, [x0, #0x44]
    mov w1, #13
    str w1, [x0, #0x24]
    mov w1, #1
    str w1, [x0, #0x28]
    mov w1, #0x70
    str w1, [x0, #0x2c]
    mov w1, #0x301
    str w1, [x0, #0x30]
    ret
    .ltorg

.section .rodata.boot,"a"
.balign 8
reject_block: .quad 0x20026, 1
failure_block: .quad 0x20026, 2
pan_message: .asciz "HMBOOT REJECT PAN\n"

.section .bss.tables,"aw",@nobits
.balign 4096
.global arm_root, arm_device_l2, arm_device_l3, arm_ram_l2, arm_ram_l3
.global arm_owned_l2, arm_owned_l3
arm_root: .skip 4096
.global arm_secondary_root
arm_secondary_root: .skip 4096
arm_device_l2: .skip 4096
arm_device_l3: .skip 4096
.global arm_gic_l3
arm_gic_l3: .skip 4096
arm_ram_l2: .skip 4096
arm_ram_l3: .skip 32768
arm_owned_l2: .skip 4096
arm_owned_l3: .skip 4096
