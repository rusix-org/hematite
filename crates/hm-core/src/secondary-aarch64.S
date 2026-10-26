// SAFETY: Dedicated CPU1 lifecycle entry; design 0018. No mutable shared loads
// until coherent mappings are active. All peer exceptions are terminal.
.arch armv8.1-a
.section .text.secondary,"ax"
.global arm_secondary_start
arm_secondary_start:
    msr daifset, #15
    mov x19, x0
    mrs x0, CurrentEL
    cmp x0, #4
    b.ne arm_secondary_failure
    mrs x0, mpidr_el1
    ldr x1, ={affinity_mask}
    and x0, x0, x1
    cmp x0, #1
    b.ne arm_secondary_failure
    cbz x19, arm_secondary_failure
    mrs x0, sctlr_el1
    ldr x1, =0x1005
    tst x0, x1
    b.ne arm_secondary_failure
    msr spsel, #1
    ldr x0, =__secondary_stack_top
    mov sp, x0
    ldr x0, =__secondary_exception_stack_top
    msr sp_el0, x0
    ldr x0, =arm_secondary_vectors
    msr vbar_el1, x0
    isb
    mov x0, #0xff
    msr mair_el1, x0
    ldr x0, =0x80993519
    msr tcr_el1, x0
    ldr x0, =arm_secondary_root
    msr ttbr0_el1, x0
    msr ttbr1_el1, xzr
    isb
    tlbi vmalle1
    dsb sy
    isb
    ic iallu
    dsb sy
    isb
    ldr x0, =0x3058180d
    msr sctlr_el1, x0
    isb
    msr pan, #1
    mov x0, #0x300000
    msr cpacr_el1, x0
    isb
    msr fpcr, xzr
    msr fpsr, xzr
    ldr x0, =arm_cpu_records
    add x0, x0, #{cpu_stride}
    msr tpidr_el1, x0
    isb
    bl arm_secondary_quiet
    // Reserve a real assembly caller frame so observed SP is inside usable RAM.
    stp x29, x30, [sp, #-16]!
    mov x0, x19
    bl {adapter}
    ldp x29, x30, [sp], #16
    // Every Rust frame has returned. Teardown and publication are assembly-only.
    bl arm_secondary_quiet
    ldr x1, ={mailbox}
    ldar x0, [x1]
    cmp x0, #3
    b.ne arm_secondary_failure
    mov x0, #4
    stlr x0, [x1]
    dsb sy
.if {off_return}
    b arm_secondary_off_return
.else
    ldr x0, =0x84000002
    mov x1, #0
    mov x2, #0
    mov x3, #0
    hvc #0
    b arm_secondary_failure
.endif

// Only this CPU's timer/PPI/GICC; never changes shared distributor forwarding.
.global arm_secondary_quiet
arm_secondary_quiet:
    mov x0, #2
    msr cntp_ctl_el0, x0
    isb
    ldr x0, =0x08010000
    str wzr, [x0]
    ldr x1, =0x08000000
    mov w2, #0x40000000
    str w2, [x1, #0x180]
    dsb sy
    str w2, [x1, #0x280]
    dsb sy
    mrs x2, cntp_ctl_el0
    and x2, x2, #3
    cmp x2, #2
    b.ne arm_secondary_failure
    ldr w2, [x0]
    cbnz w2, arm_secondary_failure
    ldr w2, [x1, #0x100]
    tbnz w2, #30, arm_secondary_failure
    ldr w2, [x1, #0x200]
    tbnz w2, #30, arm_secondary_failure
    ldr w2, [x1, #0x300]
    tbnz w2, #30, arm_secondary_failure
    ret

.global arm_secondary_failure, arm_secondary_stale
arm_secondary_failure:
    ldr x1, =.Lsecondary_failed
    b .Lsecondary_exit
arm_secondary_stale:
    ldr x1, =.Lsecondary_stale
    b .Lsecondary_exit
arm_secondary_off_return:
    ldr x1, =.Lsecondary_off_return
.Lsecondary_exit:
    mov x0, #0x20
    hlt #0xf000
.Lsecondary_halt:
    wfi
    b .Lsecondary_halt
    .ltorg

.balign 2048
arm_secondary_vectors:
.rept 4
    b arm_secondary_failure
    .balign 128
.endr
.if {overflow}
    b arm_secondary_overflow_fault
.else
    b arm_secondary_failure
.endif
    .balign 128
.if {smp}
    b arm_timer_irq
.else
    b arm_secondary_failure
.endif
    .balign 128
.rept 10
    b arm_secondary_failure
    .balign 128
.endr

.if {overflow}
// Terminal-only negative: no stack access or register restoration on a guard.
arm_secondary_overflow_fault:
    mrs x0, esr_el1
    lsr x1, x0, #26
    cmp x1, #0x25
    b.ne arm_secondary_failure
    and x0, x0, #63
    cmp x0, #7
    b.ne arm_secondary_failure
    mrs x0, far_el1
    ldr x1, =__secondary_thread0_guard
    cmp x0, x1
    b.lo arm_secondary_failure
    add x1, x1, #4096
    cmp x0, x1
    b.hs arm_secondary_failure
    ldr x1, =.Lsecondary_overflow
    b .Lsecondary_exit
.endif
    .ltorg

.section .rodata.secondary,"a"
.balign 8
.Lsecondary_failed: .quad 0x20026, 2
.Lsecondary_stale: .quad 0x20026, 3
.Lsecondary_off_return: .quad 0x20026, 4

.Lsecondary_overflow: .quad 0x20026, 5
