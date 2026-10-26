// SAFETY: Design 0009. Only the fixed assembly window unmasks IRQ; IRQ
// entry never calls Rust, touches borrowed Rust memory, or modifies mappings.
.arch armv8.1-a
.section .text.timer,"ax"
.global arm_timer_probe, arm_timer_timeout_probe
arm_timer_probe:
    mov x0, #1
    b .Ltimer_common
arm_timer_timeout_probe:
    mov x0, #0
.Ltimer_common:
    sub sp, sp, #1120
    str x0, [sp, #1072]         // Fixed entry mode: allow/withhold timer PPI.
    str xzr, [sp, #1080]        // Observation mask, not shared with IRQ.
    str x18, [sp, #816]
    str x19, [sp, #824]
    str x20, [sp, #832]
    str x21, [sp, #840]
    str x22, [sp, #848]
    str x23, [sp, #856]
    str x24, [sp, #864]
    str x25, [sp, #872]
    str x26, [sp, #880]
    str x27, [sp, #888]
    str x28, [sp, #896]
    str x29, [sp, #904]
    str x30, [sp, #912]
    str q8, [sp, #928]
    str q9, [sp, #944]
    str q10, [sp, #960]
    str q11, [sp, #976]
    str q12, [sp, #992]
    str q13, [sp, #1008]
    str q14, [sp, #1024]
    str q15, [sp, #1040]
    mrs x0, fpcr
    str x0, [sp, #792]
    mrs x0, fpsr
    str x0, [sp, #800]
    mov x0, sp
    str x0, [sp, #808]
    mrs x0, sp_el0
    str x0, [sp, #1056]
    mrs x0, daif
    str x0, [sp, #1088]
    and x1, x0, #0x3c0
    cmp x1, #0x3c0
    b.ne .Ltimer_restore

    ldr x0, =0x08000000
    ldr w2, [x0, #4]            // GICD_TYPER.SecurityExtn must be absent.
    tbnz w2, #10, .Ltimer_restore
    ldr x1, =0x08010000
    ldr w3, [x1, #0xfc]         // GICC_IIDR.Architecture == GICv2.
    ubfx w3, w3, #16, #4
    cmp w3, #2
    b.ne .Ltimer_restore
    str wzr, [x0]              // No interrupt group forwarding during setup.
    str wzr, [x1]
    mov x3, #2
    msr cntp_ctl_el0, x3        // Disable and mask the source first.
    isb
    and w2, w2, #31
    add w2, w2, #1             // At most32 banks; all registers within page.
    add x4, x0, #0x180
    add x5, x0, #0x280
    mov w3, #-1
.Ltimer_clear_banks:
    str w3, [x4], #4
    str w3, [x5], #4
    subs w2, w2, #1
    b.ne .Ltimer_clear_banks
    ldr w2, [x0, #0x80]
    bic w2, w2, #0x40000000    // Explicit Group0 ownership, not a reset guess.
    str w2, [x0, #0x80]
    ldr w2, [x0, #0x80]
    tbnz w2, #30, .Ltimer_cleanup
    ldr w2, [x0, #0xc04]
    bic w2, w2, #0x20000000    // PPI30 level-sensitive.
    str w2, [x0, #0xc04]
    ldr w2, [x0, #0xc04]
    tbnz w2, #29, .Ltimer_cleanup
    ldr w2, [x0, #0x41c]
    mov w3, #0x80
    bfi w2, w3, #16, #8        // ID30 priority byte.
    str w2, [x0, #0x41c]
    mov w2, #0xff
    str w2, [x1, #4]           // CPU priority mask.
    str wzr, [x1, #8]          // Binary point0.
    mov w2, #1
    str w2, [x1]               // Enable, EOImode=0 (combined deactivate).
    str w2, [x0]
    dsb sy

    mrs x1, cntfrq_el0
    cmp x1, #1000
    b.lo .Ltimer_cleanup
    ldr x2, =1000000000
    cmp x1, x2
    b.hi .Ltimer_cleanup
    isb
    mrs x2, cntpct_el0
    ldr x3, =0x00ffffffffffffff
    sub x3, x3, x1             // Reserve1second before minimum56-bit wrap.
    cmp x2, x3
    b.hi .Ltimer_cleanup
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_timer}
    str xzr, [x0]
    str xzr, [x0, #32]
    mov x3, #1000
    udiv x3, x1, x3            // Nonzero1ms interval.
    str x3, [x0, #8]
    add x4, x2, x3
    msr cntp_cval_el0, x4
    mov x3, #10
    udiv x3, x1, x3
    add x2, x2, x3
    str x2, [x0, #24]          //100ms delivery deadline.
    mov x2, #1
    str x2, [x0, #16]          // Handler authority active only in this call.
    ldr x0, =0x08000000
    ldr x2, [sp, #1072]
    cbz x2, .Ltimer_no_enable
    mov w2, #0x40000000
    str w2, [x0, #0x100]       // Enable only the physical timer PPI.
.Ltimer_no_enable:
    mov x0, #1
    msr cntp_ctl_el0, x0
    dsb sy
    isb

    ldr x0, [sp, #792]
    eor x0, x0, #0x400000
    msr fpcr, x0
    ldr x0, [sp, #800]
    eor x0, x0, #0x8000000
    msr fpsr, x0
    movi v0.16b, #32
    movi v1.16b, #33
    movi v2.16b, #34
    movi v3.16b, #35
    movi v4.16b, #36
    movi v5.16b, #37
    movi v6.16b, #38
    movi v7.16b, #39
    movi v8.16b, #40
    movi v9.16b, #41
    movi v10.16b, #42
    movi v11.16b, #43
    movi v12.16b, #44
    movi v13.16b, #45
    movi v14.16b, #46
    movi v15.16b, #47
    movi v16.16b, #48
    movi v17.16b, #49
    movi v18.16b, #50
    movi v19.16b, #51
    movi v20.16b, #52
    movi v21.16b, #53
    movi v22.16b, #54
    movi v23.16b, #55
    movi v24.16b, #56
    movi v25.16b, #57
    movi v26.16b, #58
    movi v27.16b, #59
    movi v28.16b, #60
    movi v29.16b, #61
    movi v30.16b, #62
    movi v31.16b, #63
    mov x4, #260
    mov x5, #261
    mov x6, #262
    mov x7, #263
    mov x8, #264
    mov x9, #265
    mov x10, #266
    mov x11, #267
    mov x12, #268
    mov x13, #269
    mov x14, #270
    mov x15, #271
    mov x16, #272
    mov x17, #273
    mov x18, #274
    mov x19, #275
    mov x20, #276
    mov x21, #277
    mov x22, #278
    mov x23, #279
    mov x24, #280
    mov x25, #281
    mov x26, #282
    mov x27, #283
    mov x28, #284
    mov x29, #285
    mov x30, #286
    mov x0, #0xa0000000
    msr nzcv, x0
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_timer}
    mov x3, #0x1000000         // Secondary bound if counter stops advancing.
.Ltimer_window_begin:
    msr daifclr, #2
.Ltimer_poll:
    ldr x1, [x0, #32]
    cbnz x1, .Ltimer_poll_failure
    ldr x1, [x0]
    sub x1, x1, #8
    cbz x1, .Ltimer_delivered
    isb
    mrs x1, cntpct_el0
    ldr x2, [x0, #24]
    sub x1, x1, x2
    tbz x1, #63, .Ltimer_deadline
    sub x3, x3, #1
    cbnz x3, .Ltimer_poll
.Ltimer_poll_failure:
    mov x2, #0
    b .Ltimer_mask
.Ltimer_deadline:
    mov x2, #8
    b .Ltimer_mask
.Ltimer_delivered:
    mov x2, #1
.Ltimer_mask:
    msr daifset, #2
.Ltimer_window_end:
    isb
    str x2, [sp, #1080]
    // Snapshot before checking instructions change registers or NZCV.
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    mrs x0, nzcv
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
    mrs x0, fpcr
    str x0, [sp, #768]
    mrs x0, fpsr
    str x0, [sp, #776]
    mov x0, sp
    str x0, [sp, #784]
    mrs x0, sp_el0
    str x0, [sp, #1064]
    add x0, sp, #32
    mov x1, #260
    mov x2, #27
.Ltimer_check_gpr:
    ldr x3, [x0], #8
    cmp x3, x1
    b.ne .Ltimer_cleanup
    add x1, x1, #1
    subs x2, x2, #1
    b.ne .Ltimer_check_gpr
    ldr x0, [sp, #248]
    mov x1, #0xa0000000
    cmp x0, x1
    b.ne .Ltimer_cleanup
    ldr x0, [sp, #768]
    ldr x1, [sp, #792]
    eor x1, x1, #0x400000
    cmp x0, x1
    b.ne .Ltimer_cleanup
    ldr x0, [sp, #776]
    ldr x1, [sp, #800]
    eor x1, x1, #0x8000000
    cmp x0, x1
    b.ne .Ltimer_cleanup
    ldr x0, [sp, #784]
    ldr x1, [sp, #808]
    cmp x0, x1
    b.ne .Ltimer_cleanup
    ldr x0, [sp, #1064]
    ldr x1, [sp, #1056]
    cmp x0, x1
    b.ne .Ltimer_cleanup
    add x0, sp, #256
    ldr x1, =0x2020202020202020
    ldr x4, =0x0101010101010101
    mov x5, #32
.Ltimer_check_simd:
    ldp x2, x3, [x0], #16
    cmp x2, x1
    b.ne .Ltimer_cleanup
    cmp x3, x1
    b.ne .Ltimer_cleanup
    add x1, x1, x4
    subs x5, x5, #1
    b.ne .Ltimer_check_simd
    ldr x0, [sp, #1080]
    orr x0, x0, #2
    str x0, [sp, #1080]

.Ltimer_cleanup:
    msr daifset, #2
    mov x0, #2
    msr cntp_ctl_el0, x0
    isb
    ldr x0, =0x08000000
    mov w2, #0x40000000
    str w2, [x0, #0x180]
    dsb sy
    str w2, [x0, #0x280]       // Source is disabled before pending clear.
    str wzr, [x0]
    ldr x1, =0x08010000
    str wzr, [x1]
    dsb sy
    isb
    mrs x3, tpidr_el1
    add x3, x3, #{cpu_timer}
    ldr x4, [x3]
    str x4, [sp, #1104]        // No subsequent delivery may change this.
    ldr x4, [x3, #8]
    cbz x4, .Ltimer_no_quiescence
    isb
    mrs x5, cntpct_el0
    ldr x6, =0x00ffffffffffffff
    sub x6, x6, x4
    cmp x5, x6
    b.hi .Ltimer_no_quiescence
    add x5, x5, x4
    mov x6, #0x1000000
.Ltimer_quiet_wait:
    isb
    mrs x7, cntpct_el0
    cmp x7, x5
    b.hs .Ltimer_check_quiet
    subs x6, x6, #1
    b.ne .Ltimer_quiet_wait
    b .Ltimer_no_quiescence
.Ltimer_check_quiet:
    ldr w2, [x0, #0x300]       // Do not fake deactivation with ICACTIVER.
    tbnz w2, #30, .Ltimer_no_quiescence
    ldr w2, [x0, #0x200]
    tbnz w2, #30, .Ltimer_no_quiescence
    ldr w2, [x0, #0x100]
    tbnz w2, #30, .Ltimer_no_quiescence
    ldr w2, [x0]
    cbnz w2, .Ltimer_no_quiescence
    ldr w2, [x1]
    cbnz w2, .Ltimer_no_quiescence
    mrs x2, cntp_ctl_el0
    and x2, x2, #3
    cmp x2, #2
    b.ne .Ltimer_no_quiescence
    mrs x2, daif
    ldr x4, [sp, #1088]
    cmp x2, x4
    b.ne .Ltimer_no_quiescence
    mrs x2, sp_el0
    ldr x4, [sp, #1056]
    cmp x2, x4
    b.ne .Ltimer_no_quiescence
    ldr x2, [x3]
    ldr x4, [sp, #1104]
    cmp x2, x4
    b.ne .Ltimer_no_quiescence
    ldr x4, [sp, #1072]
    cbnz x4, .Ltimer_quiet_observed
    cbnz x2, .Ltimer_no_quiescence // Disabled route must deliver exactlyzero.
.Ltimer_quiet_observed:
    ldr x2, [sp, #1080]
    orr x2, x2, #4
    str x2, [sp, #1080]
.Ltimer_no_quiescence:
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_timer}
    str xzr, [x0, #16]        // End handler authority only after shutdown.
.Ltimer_restore:
    ldr x0, [sp, #1080]
    ldr x16, [sp, #792]
    msr fpcr, x16
    ldr x16, [sp, #800]
    msr fpsr, x16
    ldr q8, [sp, #928]
    ldr q9, [sp, #944]
    ldr q10, [sp, #960]
    ldr q11, [sp, #976]
    ldr q12, [sp, #992]
    ldr q13, [sp, #1008]
    ldr q14, [sp, #1024]
    ldr q15, [sp, #1040]
    ldr x18, [sp, #816]
    ldr x19, [sp, #824]
    ldr x20, [sp, #832]
    ldr x21, [sp, #840]
    ldr x22, [sp, #848]
    ldr x23, [sp, #856]
    ldr x24, [sp, #864]
    ldr x25, [sp, #872]
    ldr x26, [sp, #880]
    ldr x27, [sp, #888]
    ldr x28, [sp, #896]
    ldr x29, [sp, #904]
    ldr x30, [sp, #912]
    add sp, sp, #1120
    ret
    .ltorg

.global arm_timer_irq
arm_timer_irq:
    msr spsel, #0
    sub sp, sp, #256
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_preempt}
    ldr x1, [x0, #16]
    cbnz x1, arm_preempt_irq
    mrs x0, spsr_el1
    mov x1, #0x3df
    and x0, x0, x1
    cmp x0, #0x345             // EL1h with only IRQ unmasked.
    b.ne arm_exception_fatal
    mrs x0, tpidr_el1
    add x0, x0, #{cpu_timer}
    ldr x1, [x0, #16]
    cmp x1, #1
    b.ne arm_exception_fatal
    mrs x1, elr_el1
    ldr x2, =.Ltimer_window_begin
    cmp x1, x2
    b.lo arm_exception_fatal
    ldr x2, =.Ltimer_window_end
    cmp x1, x2
    b.hs arm_exception_fatal
    ldr x0, =0x08010000
    ldr w1, [x0, #0xc]        // Retain complete acknowledged IAR for EOI.
    and w2, w1, #0x3ff
    cmp w2, #30
    b.eq .Ltimer_irq_real
    cmp w2, #1020
    b.hs .Ltimer_irq_restore   // Special IDs do not have an EOI operation.
    b arm_exception_fatal
.Ltimer_irq_real:
    mrs x2, cntp_ctl_el0
    tbz x2, #2, arm_exception_fatal
    mrs x2, tpidr_el1
    add x2, x2, #{cpu_timer}
    ldr x3, [x2]
    cmp x3, #8
    b.hs arm_exception_fatal
    add x3, x3, #1
    str x3, [x2]
    cmp x3, #8
    b.eq .Ltimer_irq_disable
    ldr x4, [x2, #8]
    isb
    mrs x5, cntpct_el0
    ldr x6, =0x00ffffffffffffff
    sub x6, x6, x4
    cmp x5, x6
    b.hi .Ltimer_irq_bad_counter
    add x5, x5, x4
    msr cntp_cval_el0, x5
    isb                       // Rearm deasserts the level before EOI.
    b .Ltimer_irq_eoi
.Ltimer_irq_bad_counter:
    mov x3, #1
    str x3, [x2, #32]
.Ltimer_irq_disable:
    mov x3, #2
    msr cntp_ctl_el0, x3
    isb
.Ltimer_irq_eoi:
    dsb sy
    str w1, [x0, #0x10]
    dsb sy
.Ltimer_irq_restore:
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #256
    msr spsel, #1
    eret
    .ltorg
