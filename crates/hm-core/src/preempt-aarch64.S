// SAFETY: Design 0012. All entries are private; context/image addresses
// are owned by the masked coordinator. IRQ never retains emergency frames.
.arch armv8.1-a
.section .text.preempt,"ax"

// Separate negative-pressure diagnostic; production expansion is empty.
.macro preempt_boundary mask, name
.if {boundary}
    stp x9, x10, [sp, #-48]!
    stp x11, x12, [sp, #16]
    mrs x9, daif
    msr daifset, #2
    isb
    mrs x10, nzcv
    str x10, [sp, #32]
    tbnz x9, #7, arm_exception_fatal
    mrs x11, tpidr_el1
    add x11, x11, #{cpu_boundary_flags}
    ldr x12, [x11]
    tst x12, #\mask
    b.ne .Lboundary_done\@
    orr x12, x12, #\mask
    str x12, [x11]
    msr cntp_cval_el0, xzr
    isb
    dsb sy
.Lboundary_done\@:
    ldr x10, [sp, #32]
    msr nzcv, x10
    ldp x11, x12, [sp, #16]
    ldp x9, x10, [sp], #48
    .global arm_preempt_boundary_\name\()_begin
    .global arm_preempt_boundary_\name\()_end
arm_preempt_boundary_\name\()_begin:
    msr daifclr, #2
    .rept 32
    nop
    .endr
arm_preempt_boundary_\name\()_end:
.endif
.endm

// SAFETY: Session-only entries require CPU0, BUSY, masked DAIF and CPU1 off.
// Shared distributor state is never accessed through the local lifecycle.
.global arm_gic_session_start, arm_gic_session_stop
.global arm_gic_probe_seed, arm_gic_probe_preserved
.global arm_gic_probe_target, arm_gic_probe_restore_target
arm_gic_session_start:
    mrs x0, daif
    cmp x0, #0x3c0
    b.ne .Lgic_start_fail
    mrs x0, tpidr_el1
    add x1, x0, #{cpu_preempt}
    ldr x2, [x1, #16]
    cbnz x2, .Lgic_start_fail
    add x1, x0, #{cpu_timer}
    ldr x2, [x1, #16]
    cbnz x2, .Lgic_start_fail
    ldr x0, =0x08000000
    ldr x1, =0x08010000
    ldr w2, [x0]
    cbnz w2, .Lgic_start_fail
    ldr w2, [x1]
    cbnz w2, .Lgic_start_fail
    ldr w2, [x0, #4]
    tbnz w2, #10, .Lgic_start_fail
    and w6, w2, #31             // Number of SPI banks, excludes bank0.
    cbz w6, .Lgic_start_fail    // Diagnostic SPI32 must exist.
    ldr w2, [x1, #0xfc]
    ubfx w2, w2, #16, #4
    cmp w2, #2
    b.ne .Lgic_start_fail
    ldr w2, [x0, #0x100]
    ldr w3, =0xbfff0000
    tst w2, w3
    b.ne .Lgic_start_fail
    ldr w2, [x0, #0x200]
    bic w2, w2, #0x40000000
    cbnz w2, .Lgic_start_fail
    ldr w2, [x0, #0x300]
    cbnz w2, .Lgic_start_fail
    add x3, x0, #0x304
    mov w4, w6
.Lgic_validate_spi:
    ldr w2, [x3], #4
    cbnz w2, .Lgic_start_fail
    subs w4, w4, #1
    b.ne .Lgic_validate_spi
    // All fallible preconditions passed; mutation begins with GICC disabled.
    add x3, x0, #0x184
    add x4, x0, #0x284
    mov w5, #-1
.Lgic_reset_spi:
    str w5, [x3], #4
    str w5, [x4], #4
    subs w6, w6, #1
    b.ne .Lgic_reset_spi
    dsb sy
    mov w2, #1
    str w2, [x0]
    dsb sy
    ldr w2, [x0]
    cmp w2, #1
    b.ne arm_exception_fatal
    mov x0, #1
    ret
.Lgic_start_fail:
    mov x0, #0
    ret

arm_gic_session_stop:
    ldr x0, =0x08000000
    ldr x1, =0x08010000
    ldr w2, [x1]
    cbnz w2, arm_exception_fatal
    ldr w2, [x0, #0x100]
    tbnz w2, #30, arm_exception_fatal
    ldr w2, [x0, #0x200]
    tbnz w2, #30, arm_exception_fatal
    ldr w2, [x0, #0x300]
    tbnz w2, #30, arm_exception_fatal
    str wzr, [x0]
    dsb sy
    ldr w2, [x0]
    cbnz w2, arm_exception_fatal
    ldr w6, [x0, #4]
    and w6, w6, #31
    add x3, x0, #0x284
    mov w4, w6
    mov w5, #-1
.Lgic_clear_pending:
    str w5, [x3], #4
    subs w4, w4, #1
    b.ne .Lgic_clear_pending
    dsb sy
    add x3, x0, #0x104
    add x4, x0, #0x204
    add x5, x0, #0x304
.Lgic_verify_quiet:
    ldr w2, [x3], #4
    cbnz w2, arm_exception_fatal
    ldr w2, [x4], #4
    cbnz w2, arm_exception_fatal
    ldr w2, [x5], #4
    cbnz w2, arm_exception_fatal
    subs w6, w6, #1
    b.ne .Lgic_verify_quiet
    mov x0, #1
    ret

// Lease-owned diagnostic target only; not part of local setup/teardown.
arm_gic_probe_target:
    ldr x1, =0x08000000
    ldr w2, [x1, #0x104]
    tbnz w2, #0, arm_exception_fatal
    ldr w2, [x1, #0x204]
    tbnz w2, #0, arm_exception_fatal
    ldr w2, [x1, #0x304]
    tbnz w2, #0, arm_exception_fatal
    ldrb w3, [x1, #0x820]         // Save only SPI32's target byte.
    ldr w2, [x1, #4]
    ubfx w2, w2, #5, #3          // CPUNumber: zero permits RAZ/WI targets.
    cbz w2, .Lgic_target_saved
    mov w2, #1
    strb w2, [x1, #0x820]
    dsb sy
    ldrb w2, [x1, #0x820]
    cmp w2, #1
    b.ne arm_exception_fatal
.Lgic_target_saved:
    mov w0, w3
    ret

arm_gic_probe_restore_target:
    cmp x0, #255
    b.hi arm_exception_fatal
    ldr x1, =0x08000000
    ldr w2, [x1]
    cbnz w2, arm_exception_fatal
    ldr w2, [x1, #0x104]
    tbnz w2, #0, arm_exception_fatal
    ldr w2, [x1, #0x204]
    tbnz w2, #0, arm_exception_fatal
    ldr w2, [x1, #0x304]
    tbnz w2, #0, arm_exception_fatal
    ldr w2, [x1, #4]
    ubfx w2, w2, #5, #3
    cbz w2, .Lgic_target_restored
    strb w0, [x1, #0x820]
    dsb sy
.Lgic_target_restored:
    ldrb w2, [x1, #0x820]
    cmp w2, w0
    b.ne arm_exception_fatal
    mov x0, #1
    ret

// Lease-owned diagnostic: SPI32 was cleared/disabled by session start.
arm_gic_probe_seed:
    ldr x0, =0x08000000
    ldr w1, [x0, #0x104]
    tbnz w1, #0, .Lgic_probe_fail
    ldr w1, [x0, #0x204]
    tbnz w1, #0, .Lgic_probe_fail
    mov w1, #1
    str w1, [x0, #0x204]
    dsb sy
arm_gic_probe_preserved:
    ldr x0, =0x08000000
    ldr w1, [x0, #4]
    ubfx w1, w1, #5, #3
    cbz w1, .Lgic_probe_target_ok
    ldrb w1, [x0, #0x820]
    cmp w1, #1
    b.ne .Lgic_probe_fail
.Lgic_probe_target_ok:
    ldr w1, [x0]
    cmp w1, #1
    b.ne .Lgic_probe_fail
    ldr w1, [x0, #0x104]
    tbnz w1, #0, .Lgic_probe_fail
    ldr w1, [x0, #0x204]
    tbz w1, #0, .Lgic_probe_fail
    mov x0, #1
    ret
.Lgic_probe_fail:
    mov x0, #0
    ret
    .ltorg

.global arm_preempt_setup, arm_preempt_stop, arm_preempt_dispatch
.global arm_preempt_yield, arm_preempt_start, arm_preempt_irq
.global arm_preempt_park
.global arm_preempt_wait_mask
arm_preempt_setup:
    mrs x0, daif
    cmp x0, #0x3c0
    b.ne .Lsetup_fail
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_preempt}
    ldr x1, [x0, #16]
    cbnz x1, .Lsetup_fail
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_timer}
    ldr x1, [x0, #16]
    cbnz x1, .Lsetup_fail
    ldr x0, =0x08000000
    ldr w2, [x0, #4]
    tbnz w2, #10, .Lsetup_fail
    ldr x1, =0x08010000
    ldr w3, [x1, #0xfc]
    ubfx w3, w3, #16, #4
    cmp w3, #2
    b.ne .Lsetup_fail
    mrs x6, cntfrq_el0
    cmp x6, #1000
    b.lo .Lsetup_fail
    ldr x3, =1000000000
    cmp x6, x3
    b.hi .Lsetup_fail
    mov x3, #1000
    udiv x6, x6, x3
    isb
    mrs x3, cntpct_el0
    ldr x4, =0x00ffffffffffffff
    sub x4, x4, x6
    cmp x3, x4
    b.hi .Lsetup_fail
    ldr w3, [x0]
    cmp w3, #1
    b.ne .Lsetup_fail
    // Other local sources must be absent; SGI enable bits may be RAO.
    ldr w3, [x0, #0x100]
    ldr w4, =0xbfff0000
    tst w3, w4
    b.ne .Lsetup_fail
    ldr w3, [x0, #0x200]
    bic w3, w3, #0x40000000
    cbnz w3, .Lsetup_fail
    ldr w3, [x0, #0x300]
    cbnz w3, .Lsetup_fail
    str wzr, [x1]
    mov x3, #2
    msr cntp_ctl_el0, x3
    isb
    mov w3, #0x40000000
    str w3, [x0, #0x180]          // Only this CPU's timer PPI30.
    dsb sy
    str w3, [x0, #0x280]
    dsb sy
    ldr w2, [x0, #0x80]
    bic w2, w2, #0x40000000
    str w2, [x0, #0x80]
    ldr w2, [x0, #0x80]
    tbnz w2, #30, .Lsetup_rollback
    ldr w2, [x0, #0xc04]
    bic w2, w2, #0x20000000
    str w2, [x0, #0xc04]
    ldr w2, [x0, #0xc04]
    tbnz w2, #29, .Lsetup_rollback
    ldr w2, [x0, #0x41c]
    mov w3, #0x80
    bfi w2, w3, #16, #8
    str w2, [x0, #0x41c]
    mov w2, #0xff
    str w2, [x1, #4]
    str wzr, [x1, #8]
    mov w2, #1
    str w2, [x1]
    dsb sy
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_preempt}
    str xzr, [x0]
    str xzr, [x0, #8]
    str x6, [x0, #24]
    mrs x3, tpidr_el1
    add x3, x3, #{cpu_boundary_flags}
    str xzr, [x3]
    str x2, [x0, #16]
    mov x0, x6
    ret
.Lsetup_rollback:
    stp x29, x30, [sp, #-16]!
    bl arm_preempt_stop
    ldp x29, x30, [sp], #16
.Lsetup_fail:
    mov x0, #0
    ret

// Masked, allocation-free per-return cleanup; no IRQ remains active here.
.Lquiet:
    mov x2, #2
    msr cntp_ctl_el0, x2
    isb
    ldr x0, =0x08000000
    mov w2, #0x40000000
    str w2, [x0, #0x180]
    dsb sy
    str w2, [x0, #0x280]
    dsb sy
    ldr w3, [x0, #0x300]
    tbnz w3, #30, arm_exception_fatal
    ldr w3, [x0, #0x200]
    tbnz w3, #30, arm_exception_fatal
    ret
arm_preempt_stop:
    stp x29, x30, [sp, #-16]!
    bl .Lquiet
    ldr x1, =0x08010000
    str wzr, [x1]
    dsb sy
    ldr w2, [x0]
    cmp w2, #1                    // Local stop must preserve shared forwarding.
    b.ne arm_exception_fatal
    ldr w2, [x1]
    cbnz w2, arm_exception_fatal
    ldr w2, [x0, #0x100]
    tbnz w2, #30, arm_exception_fatal
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_preempt}
    str xzr, [x0, #16]
    str xzr, [x0]
    str xzr, [x0, #8]
    mov x0, #1
    ldp x29, x30, [sp], #16
    ret

arm_preempt_dispatch:
    stp x29, x30, [sp, #-16]!
    adr x30, .Ldispatch_return
    stp x18, x19, [x0, #0]
    stp x20, x21, [x0, #16]
    stp x22, x23, [x0, #32]
    stp x24, x25, [x0, #48]
    stp x26, x27, [x0, #64]
    stp x28, x29, [x0, #80]
    str x30, [x0, #96]
    mov x3, sp
    str x3, [x0, #104]
    stp q8, q9, [x0, #112]
    stp q10, q11, [x0, #144]
    stp q12, q13, [x0, #176]
    stp q14, q15, [x0, #208]
    mrs x3, fpcr
    str x3, [x0, #240]
    mrs x3, fpsr
    str x3, [x0, #248]
    mrs x3, tpidr_el1
    add x3, x3, #{cpu_preempt}
    str x0, [x3]
    str x1, [x3, #8]
    mov x4, #1
    str x4, [x1, #264]
    // Fresh compare before publishing enabled source; checked minimum56bit wrap.
    isb
    mrs x4, cntpct_el0
    ldr x5, =0x00ffffffffffffff
    sub x5, x5, x2
    cmp x4, x5
    b.hi arm_exception_fatal
    add x4, x4, x2
    msr cntp_cval_el0, x4
    ldr x4, =0x08000000
    mov w5, #0x40000000
    // Build-time negative image withholds delivery, preserving the workload.
    mov x6, #{delivery}
    cbz x6, .Lwithhold
    str w5, [x4, #0x100]
.Lwithhold:
    mov x4, #1
    msr cntp_ctl_el0, x4
    dsb sy
    isb
    clrex
    ldr x3, [x1, #280]
    cmp x3, #1
    b.eq .Lrestore_cooperative
    cmp x3, #3
    b.eq .Lrestore_cooperative
    add x0, x1, #288
    b .Lrestore_image
.Ldispatch_return:
    bl .Lquiet
    ldp x29, x30, [sp], #16
    ret

arm_preempt_yield:
    preempt_boundary 2, yield
    mrs x9, daif
    msr daifset, #2
    isb
    stp x9, x30, [sp, #-16]!
    ldr x2, [x0, #272]
    adds x3, x2, #1
    csel x2, x2, x3, cs
    str x2, [x0, #272]
    mov x2, #1
    str x2, [x0, #280]
    str xzr, [x0, #264]
    mov x2, #2
    msr cntp_ctl_el0, x2
    isb
    clrex
    bl arm_context_switch
    ldp x9, x30, [sp], #16
    msr daif, x9
    isb
    ret
// Ordinary ABI call is a compiler barrier; return the actual prior mask.
arm_preempt_wait_mask:
    preempt_boundary 8, park
    mrs x0, daif
    msr daifset, #2
    isb
    ret

// Caller has masked IRQ, published BLOCKED and retained its Event borrow.
// Keep that state and its distinct counter; this is not a voluntary yield.
arm_preempt_park:
    stp x29, x30, [sp, #-16]!
.if {boundary}
    // The timer can become pending during registration but must not execute
    // an IRQ handler against a BLOCKED task. No unmask occurs in this window.
    mrs x2, daif
    tbz x2, #7, arm_exception_fatal
    mrs x2, tpidr_el1
    add x2, x2, #{cpu_boundary_flags}
    ldr x3, [x2]
    tbnz x3, #4, .Lpark_expiry_done
    orr x3, x3, #16
    str x3, [x2]
    msr cntp_cval_el0, xzr
    isb
    mrs x2, cntp_ctl_el0
    tbz x2, #2, arm_exception_fatal
    mrs x2, tpidr_el1
    add x2, x2, #{cpu_wait_expiries}
    ldr x3, [x2]
    adds x4, x3, #1
    csel x3, x3, x4, cs
    str x3, [x2]
.Lpark_expiry_done:
.endif
    mov x2, #2
    msr cntp_ctl_el0, x2
    isb
    clrex
    bl arm_context_switch
    ldp x29, x30, [sp], #16
    ret

arm_preempt_start:
    preempt_boundary 1, start
    mov x0, x21
    mov x1, x22
    bl {entry}
    preempt_boundary 4, complete
    msr daifset, #2
    isb
    str x0, [x19, #256]
    mov x0, #2
    str x0, [x19, #264]
    msr cntp_ctl_el0, x0
    isb
    mov x0, x19
    mov x1, x20
    clrex
    b arm_context_switch

// x1 points to a complete ordinary AAPCS context (host or explicit yield).
.Lrestore_cooperative:
    ldr x2, [x1, #240]
    msr fpcr, x2
    ldr x2, [x1, #248]
    msr fpsr, x2
    ldp q8, q9, [x1, #112]
    ldp q10, q11, [x1, #144]
    ldp q12, q13, [x1, #176]
    ldp q14, q15, [x1, #208]
    ldp x18, x19, [x1, #0]
    ldp x20, x21, [x1, #16]
    ldp x22, x23, [x1, #32]
    ldp x24, x25, [x1, #48]
    ldp x26, x27, [x1, #64]
    ldp x28, x29, [x1, #80]
    ldr x30, [x1, #96]
    ldr x2, [x1, #104]
    mov sp, x2
    ret

// Existing IRQ entry has already saved all31GPRs in a256byte SP_EL0 frame.
arm_preempt_irq:
    sub sp, sp, #544
    add x0, sp, #544
    mov x1, sp
    mov x2, #31
.Lcopy_gprs:
    ldr x3, [x0], #8
    str x3, [x1], #8
    subs x2, x2, #1
    b.ne .Lcopy_gprs
    msr spsel, #1
    mov x0, sp
    msr spsel, #0
    str x0, [sp, #248]
    stp q0, q1, [sp, #256]
    stp q2, q3, [sp, #288]
    stp q4, q5, [sp, #320]
    stp q6, q7, [sp, #352]
    stp q8, q9, [sp, #384]
    stp q10, q11, [sp, #416]
    stp q12, q13, [sp, #448]
    stp q14, q15, [sp, #480]
    stp q16, q17, [sp, #512]
    stp q18, q19, [sp, #544]
    stp q20, q21, [sp, #576]
    stp q22, q23, [sp, #608]
    stp q24, q25, [sp, #640]
    stp q26, q27, [sp, #672]
    stp q28, q29, [sp, #704]
    stp q30, q31, [sp, #736]
    mrs x0, elr_el1
    str x0, [sp, #768]
    mrs x0, spsr_el1
    str x0, [sp, #776]
    mrs x0, fpcr
    str x0, [sp, #784]
    mrs x0, fpsr
    str x0, [sp, #792]
    ldr x0, [sp, #776]
    mov x1, #0x3df
    and x0, x0, x1
    cmp x0, #0x345
    b.ne arm_exception_fatal
    mrs x4, tpidr_el1
    add x4, x4, #{cpu_preempt}
    // Coordinator publishes only a bounded admitted task pointer; safe policy
    // cannot write publication. Pointer+RUNNING validate that authority here.
    ldr x5, [x4, #8]
    cbz x5, arm_exception_fatal
    ldr x0, [x5, #264]
    cmp x0, #1
    b.ne arm_exception_fatal
    ldr x0, =0x08010000
    ldr w1, [x0, #0xc]
    and w2, w1, #0x3ff
    cmp w2, #30
    b.eq .Lreal_preemption
    cmp w2, #1020
    b.lo arm_exception_fatal
    // Spurious ID: no EOI, no scheduling, restore the exact interrupted image.
    mov x0, sp
    add sp, sp, #800
    msr spsel, #1
    b .Lrestore_image
.Lreal_preemption:
    mrs x6, tpidr_el1
    add x6, x6, #{cpu_irq_count}
    ldr x7, [x6]
    adds x8, x7, #1
    csel x7, x7, x8, cs
    str x7, [x6]
    mrs x2, cntp_ctl_el0
    tbz x2, #2, arm_exception_fatal
    mov x2, #2
    msr cntp_ctl_el0, x2
    isb
    dsb sy
    str w1, [x0, #0x10]
    dsb sy
    mov x0, sp
    add x1, x5, #288
    mov x2, #100
.Lcopy_image:
    ldr x3, [x0], #8
    str x3, [x1], #8
    subs x2, x2, #1
    b.ne .Lcopy_image
    ldr x0, [x5, #1088]
    adds x1, x0, #1
    csel x0, x0, x1, cs
    str x0, [x5, #1088]
    mov x0, #2
    str x0, [x5, #280]
    str xzr, [x5, #264]
    ldr x1, [x4]
    cbz x1, arm_exception_fatal
    add sp, sp, #800
    msr spsel, #1
    clrex
    b .Lrestore_cooperative

// x0 points to private saved image; no Rust/reference/stack use until ERET.
.Lrestore_image:
    mov x30, x0
    ldr x1, [x30, #248]
    mov sp, x1
    ldr x1, [x30, #768]
    msr elr_el1, x1
    ldr x1, [x30, #776]
    msr spsr_el1, x1
    ldr x1, [x30, #784]
    msr fpcr, x1
    ldr x1, [x30, #792]
    msr fpsr, x1
    ldp q0, q1, [x30, #256]
    ldp q2, q3, [x30, #288]
    ldp q4, q5, [x30, #320]
    ldp q6, q7, [x30, #352]
    ldp q8, q9, [x30, #384]
    ldp q10, q11, [x30, #416]
    ldp q12, q13, [x30, #448]
    ldp q14, q15, [x30, #480]
    ldp q16, q17, [x30, #512]
    ldp q18, q19, [x30, #544]
    ldp q20, q21, [x30, #576]
    ldp q22, q23, [x30, #608]
    ldp q24, q25, [x30, #640]
    ldp q26, q27, [x30, #672]
    ldp q28, q29, [x30, #704]
    ldp q30, q31, [x30, #736]
    ldp x0, x1, [x30, #0]
    ldp x2, x3, [x30, #16]
    ldp x4, x5, [x30, #32]
    ldp x6, x7, [x30, #48]
    ldp x8, x9, [x30, #64]
    ldp x10, x11, [x30, #80]
    ldp x12, x13, [x30, #96]
    ldp x14, x15, [x30, #112]
    ldp x16, x17, [x30, #128]
    ldp x18, x19, [x30, #144]
    ldp x20, x21, [x30, #160]
    ldp x22, x23, [x30, #176]
    ldp x24, x25, [x30, #192]
    ldp x26, x27, [x30, #208]
    ldp x28, x29, [x30, #224]
    ldr x30, [x30, #240]
    clrex
    eret
    .ltorg
