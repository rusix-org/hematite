// SAFETY: Reviewed fixed-site EL1 exception contract in design 0007. Boot
// owns both stacks and mappings; only the exact table entries may recover.
.arch armv8.1-a
.section .text.exceptions,"ax"
.balign 2048
.global arm_vectors
arm_vectors:
    b arm_nested_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_sync_exception
    .space 124
    b arm_timer_irq
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124
    b arm_exception_fatal
    .space 124


// Select the separately owned emergency stack before any memory access. A
// fault after this switch uses the SP0 vector, whose path never touches a stack.
arm_sync_exception:
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
    mrs x0, spsr_el1
    mov x1, #0x3df              // AArch64 mode bits plus DAIF.
    and x0, x0, x1
    cmp x0, #0x3c5              // EL1h, all asynchronous classes masked.
    b.ne arm_exception_fatal
    mrs x0, elr_el1
    mrs x1, esr_el1
    ldr x3, =0xfe0007ff         // EC, IL and relevant low abort syndrome.
    and x1, x1, x3
    mrs x2, far_el1
    ldr x3, =arm_fault_fixups
    mov x4, #6
.Lfind_fixup:
    ldp x5, x6, [x3]           // Exact fault PC and trusted resume PC.
    cmp x0, x5
    b.ne .Lnext_fixup
    ldp x7, x8, [x3, #16]      // Exact FAR and masked ESR.
    cmp x2, x7
    b.ne arm_exception_fatal
    cmp x1, x8
    b.ne arm_exception_fatal
    msr elr_el1, x6
    // Leave SPSR intact: ERET restores the interrupted NZCV/DAIF/PAN/mode.

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
.Lnext_fixup:
    add x3, x3, #32
    subs x4, x4, #1
    b.ne .Lfind_fixup
    b arm_exception_fatal

// Fatal reporting must also work with either stack inside a guard. No calls,
// stack accesses or unbounded UART polling appear on this path.
arm_nested_fatal:
    ldr x0, =arm_nested_message
    b .Lfatal_print
.global arm_exception_fatal
arm_exception_fatal:
.if {secondary}
    mrs x0, mpidr_el1
    ldr x1, =0xff00ffffff
    and x0, x0, x1
    cbnz x0, arm_secondary_failure
.endif
    ldr x0, =arm_exception_message
.Lfatal_print:
    mov x1, #0x09000000
.Lfatal_character:
    ldrb w2, [x0], #1
    cbz w2, .Lfatal_exit
    mov x4, #0xffff
.Lfatal_wait:
    ldr w3, [x1, #0x18]
    tbz w3, #5, .Lfatal_send
    subs x4, x4, #1
    b.ne .Lfatal_wait
    b .Lfatal_exit
.Lfatal_send:
    str w2, [x1]
    b .Lfatal_character
.Lfatal_exit:
    b arm_exception_fail

.global arm_probe_null
arm_probe_null:
    mov x0, #0
.Lnull_fault:
    ldr x1, [x0]
    mov x0, #0
    ret
.Lnull_resume:
    mov x0, #1
    ret

.global arm_probe_stack_guard
arm_probe_stack_guard:
    ldr x0, =__stack_guard
.Lstack_fault:
    ldr x1, [x0]
    mov x0, #0
    ret
.Lstack_resume:
    mov x0, #1
    ret

.global arm_probe_exception_guard
arm_probe_exception_guard:
    ldr x0, =__exception_stack_guard
.Lexception_guard_fault:
    ldr x1, [x0]
    mov x0, #0
    ret
.Lexception_guard_resume:
    mov x0, #1
    ret

.global arm_probe_read_only
arm_probe_read_only:
    ldr x0, =arm_probe_ro_datum
    mov w1, #0
.Lread_only_fault:
    str w1, [x0]
    mov x0, #0
    ret
.Lread_only_resume:
    ldr w1, [x0]
    ldr w2, =0x12345678
    cmp w1, w2
    cset x0, eq
    ret

.global arm_probe_no_execute
arm_probe_no_execute:
    stp x29, x30, [sp, #-16]!
    ldr x9, =arm_probe_nx_target
    blr x9
    mov x0, #0                 // Missing protection reaches the valid RET.
    b .Lno_execute_finish
.Lno_execute_resume:
    mov x0, #1
.Lno_execute_finish:
    ldp x29, x30, [sp], #16
    ret
    .ltorg

// Fixed 1072-byte normal-stack fixture. Post-fault GPRs occupy 0..248,
// NZCV 248, Q0..Q31 256..768, FPCR/FPSR/SP 768/776/784. Saved caller
// FPCR/FPSR/SP occupy 792/800/808; x18..x30 816..920, q8..q15 928..1056,
// and expected/observed SP_EL0 1056/1064. Only the probe, never the handler,
// executes SIMD instructions. The public ABI is restored independently.
.global arm_probe_context
arm_probe_context:
    sub sp, sp, #1072
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
    mrs x16, fpcr
    str x16, [sp, #792]
    eor x16, x16, #0x400000    // Toggle only the defined rounding-mode bit.
    msr fpcr, x16
    mrs x16, fpsr
    str x16, [sp, #800]
    eor x16, x16, #0x8000000   // Toggle only the defined cumulative QC bit.
    msr fpsr, x16
    mov x16, sp
    str x16, [sp, #808]
    mrs x16, sp_el0
    str x16, [sp, #1056]
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
    mov x16, #0xa0000000
    msr nzcv, x16
    mov x0, #0
    mov x1, #257
    mov x2, #258
    mov x3, #259
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
.Lcontext_fault:
    ldr x0, [x0]
    b .Lcontext_fail
.Lcontext_resume:
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
    mrs x16, nzcv              // Capture before comparisons change flags.
    str x16, [sp, #248]
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
    mrs x16, fpcr
    str x16, [sp, #768]
    mrs x16, fpsr
    str x16, [sp, #776]
    mov x16, sp
    str x16, [sp, #784]
    mrs x16, sp_el0
    str x16, [sp, #1064]

    ldr x3, [sp]
    cbnz x3, .Lcontext_fail
    add x0, sp, #8
    mov x1, #0x101
    mov x2, #30
.Lcheck_gprs:
    ldr x3, [x0], #8
    cmp x3, x1
    b.ne .Lcontext_fail
    add x1, x1, #1
    subs x2, x2, #1
    b.ne .Lcheck_gprs
    ldr x0, [sp, #248]
    mov x1, #0xa0000000
    cmp x0, x1
    b.ne .Lcontext_fail
    ldr x0, [sp, #768]
    ldr x1, [sp, #792]
    eor x1, x1, #0x400000
    cmp x0, x1
    b.ne .Lcontext_fail
    ldr x0, [sp, #776]
    ldr x1, [sp, #800]
    eor x1, x1, #0x8000000
    cmp x0, x1
    b.ne .Lcontext_fail
    ldr x0, [sp, #784]
    ldr x1, [sp, #808]
    cmp x0, x1
    b.ne .Lcontext_fail
    ldr x0, [sp, #1064]
    ldr x1, [sp, #1056]
    cmp x0, x1
    b.ne .Lcontext_fail
    add x0, sp, #256
    ldr x1, =0x2020202020202020
    ldr x4, =0x0101010101010101
    mov x5, #32
.Lcheck_simd:
    ldp x2, x3, [x0], #16
    cmp x2, x1
    b.ne .Lcontext_fail
    cmp x3, x1
    b.ne .Lcontext_fail
    add x1, x1, x4
    subs x5, x5, #1
    b.ne .Lcheck_simd
    mov x0, #1
    b .Lcontext_finish
.Lcontext_fail:
    mov x0, #0
.Lcontext_finish:
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
    add sp, sp, #1072
    ret
    .ltorg

// Only nonreturning negative-profile wrappers call these entries. The first
// proves recovery entry does not store to the corrupted normal stack; the
// second forces the handler's first store into its own guard (SP0 fatal route).
.global arm_probe_unexpected
arm_probe_unexpected:
    ldr x0, =__stack_guard
    mov sp, x0
    brk #0
    b arm_exception_fatal

.global arm_probe_nested
arm_probe_nested:
    ldr x0, =__exception_stack_guard
    add x0, x0, #256
    msr sp_el0, x0
    brk #0
    b arm_exception_fatal
    .ltorg

.section .rodata.exceptions,"a"
.balign 8
arm_fault_fixups:
    .quad .Lnull_fault, .Lnull_resume, 0, 0x96000006
    .quad .Lstack_fault, .Lstack_resume, __stack_guard, 0x96000007
    .quad .Lexception_guard_fault, .Lexception_guard_resume, __exception_stack_guard, 0x96000007
    .quad .Lread_only_fault, .Lread_only_resume, arm_probe_ro_datum, 0x9600004f
    .quad arm_probe_nx_target, .Lno_execute_resume, arm_probe_nx_target, 0x8600000f
    .quad .Lcontext_fault, .Lcontext_resume, 0, 0x96000006
arm_exception_message: .asciz "HMEXCEPTION FATAL\n"
arm_nested_message: .asciz "HMEXCEPTION NESTED\nHMEXCEPTION FATAL\n"
.balign 4
// Private assembly probe storage, never a Rust immutable object or live code.
arm_probe_ro_datum: .word 0x12345678
arm_probe_nx_target: .word 0xd65f03c0  // RET: safe failure if PXN is missing.

