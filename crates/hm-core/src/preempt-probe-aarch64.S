// SAFETY: Private diagnostic preserves AAPCS caller state and uses only its
// own bounded stack plus the scheduler's read-only IRQ counter. Two passes
// keep polling scratch separate from every register's tested sentinel.
.arch armv8.1-a
.section .text.preempt_probe,"ax"
.global arm_preempt_context_probe
arm_preempt_context_probe:
    sub sp, sp, #1104
    add x1, sp, #800
    stp x18, x19, [x1, #0]
    stp x20, x21, [x1, #16]
    stp x22, x23, [x1, #32]
    stp x24, x25, [x1, #48]
    stp x26, x27, [x1, #64]
    stp x28, x29, [x1, #80]
    str x30, [sp, #896]
    stp q8, q9, [sp, #912]
    stp q10, q11, [sp, #944]
    stp q12, q13, [sp, #976]
    stp q14, q15, [sp, #1008]
    mrs x1, fpcr
    str x1, [sp, #1040]
    mrs x1, fpsr
    str x1, [sp, #1048]
    str x0, [sp, #1056]
    mov x1, sp
    str x1, [sp, #1072]
    str xzr, [sp, #1064]
    mov x1, #1
    str x1, [sp, #1080]
    b .Lprobe_pass0
.Lprobe_pass0:
    ldr x17, [sp, #1056]
    and x17, x17, #1
    lsl x17, x17, #22
    msr fpcr, x17
    lsl x17, x17, #5
    msr fpsr, x17
    ldr x17, [sp, #1056]
    and x17, x17, #3
    lsl x17, x17, #28
    msr nzcv, x17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #32
    dup v0.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #33
    dup v1.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #34
    dup v2.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #35
    dup v3.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #36
    dup v4.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #37
    dup v5.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #38
    dup v6.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #39
    dup v7.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #40
    dup v8.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #41
    dup v9.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #42
    dup v10.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #43
    dup v11.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #44
    dup v12.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #45
    dup v13.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #46
    dup v14.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #47
    dup v15.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #48
    dup v16.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #49
    dup v17.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #50
    dup v18.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #51
    dup v19.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #52
    dup v20.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #53
    dup v21.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #54
    dup v22.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #55
    dup v23.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #56
    dup v24.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #57
    dup v25.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #58
    dup v26.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #59
    dup v27.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #60
    dup v28.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #61
    dup v29.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #62
    dup v30.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #63
    dup v31.16b, w17
    ldr x0, [sp, #1056]
    lsl x0, x0, #8
    add x0, x0, #256
    ldr x1, [sp, #1056]
    lsl x1, x1, #8
    add x1, x1, #257
    ldr x2, [sp, #1056]
    lsl x2, x2, #8
    add x2, x2, #258
    ldr x3, [sp, #1056]
    lsl x3, x3, #8
    add x3, x3, #259
    ldr x4, [sp, #1056]
    lsl x4, x4, #8
    add x4, x4, #260
    ldr x5, [sp, #1056]
    lsl x5, x5, #8
    add x5, x5, #261
    ldr x6, [sp, #1056]
    lsl x6, x6, #8
    add x6, x6, #262
    ldr x7, [sp, #1056]
    lsl x7, x7, #8
    add x7, x7, #263
    ldr x8, [sp, #1056]
    lsl x8, x8, #8
    add x8, x8, #264
    ldr x9, [sp, #1056]
    lsl x9, x9, #8
    add x9, x9, #265
    ldr x10, [sp, #1056]
    lsl x10, x10, #8
    add x10, x10, #266
    ldr x11, [sp, #1056]
    lsl x11, x11, #8
    add x11, x11, #267
    ldr x12, [sp, #1056]
    lsl x12, x12, #8
    add x12, x12, #268
    ldr x13, [sp, #1056]
    lsl x13, x13, #8
    add x13, x13, #269
    ldr x14, [sp, #1056]
    lsl x14, x14, #8
    add x14, x14, #270
    ldr x15, [sp, #1056]
    lsl x15, x15, #8
    add x15, x15, #271
    ldr x16, [sp, #1056]
    lsl x16, x16, #8
    add x16, x16, #272
    ldr x18, [sp, #1056]
    lsl x18, x18, #8
    add x18, x18, #274
    ldr x19, [sp, #1056]
    lsl x19, x19, #8
    add x19, x19, #275
    ldr x20, [sp, #1056]
    lsl x20, x20, #8
    add x20, x20, #276
    ldr x21, [sp, #1056]
    lsl x21, x21, #8
    add x21, x21, #277
    ldr x22, [sp, #1056]
    lsl x22, x22, #8
    add x22, x22, #278
    ldr x23, [sp, #1056]
    lsl x23, x23, #8
    add x23, x23, #279
    ldr x24, [sp, #1056]
    lsl x24, x24, #8
    add x24, x24, #280
    ldr x25, [sp, #1056]
    lsl x25, x25, #8
    add x25, x25, #281
    ldr x26, [sp, #1056]
    lsl x26, x26, #8
    add x26, x26, #282
    ldr x27, [sp, #1056]
    lsl x27, x27, #8
    add x27, x27, #283
    ldr x28, [sp, #1056]
    lsl x28, x28, #8
    add x28, x28, #284
    ldr x29, [sp, #1056]
    lsl x29, x29, #8
    add x29, x29, #285
    ldr x30, [sp, #1056]
    lsl x30, x30, #8
    add x30, x30, #286
    ldr x17, [sp, #1056]
    lsl x17, x17, #8
    add x17, x17, #273
    ldr x0, =arm_preempt_irq_count
    ldr x2, [x0]
    add x2, x2, #8
.Lprobe_poll0:
    ldr x1, [x0]
    sub x1, x1, x2
    tbnz x1, #63, .Lprobe_poll0
    b .Lprobe_snapshot
.Lprobe_pass1:
    ldr x17, [sp, #1056]
    and x17, x17, #1
    lsl x17, x17, #22
    msr fpcr, x17
    lsl x17, x17, #5
    msr fpsr, x17
    ldr x17, [sp, #1056]
    and x17, x17, #3
    lsl x17, x17, #28
    msr nzcv, x17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #32
    dup v0.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #33
    dup v1.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #34
    dup v2.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #35
    dup v3.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #36
    dup v4.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #37
    dup v5.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #38
    dup v6.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #39
    dup v7.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #40
    dup v8.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #41
    dup v9.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #42
    dup v10.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #43
    dup v11.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #44
    dup v12.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #45
    dup v13.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #46
    dup v14.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #47
    dup v15.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #48
    dup v16.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #49
    dup v17.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #50
    dup v18.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #51
    dup v19.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #52
    dup v20.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #53
    dup v21.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #54
    dup v22.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #55
    dup v23.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #56
    dup v24.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #57
    dup v25.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #58
    dup v26.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #59
    dup v27.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #60
    dup v28.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #61
    dup v29.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #62
    dup v30.16b, w17
    ldr x17, [sp, #1056]
    lsl x17, x17, #5
    add x17, x17, #63
    dup v31.16b, w17
    ldr x0, [sp, #1056]
    lsl x0, x0, #8
    add x0, x0, #256
    ldr x1, [sp, #1056]
    lsl x1, x1, #8
    add x1, x1, #257
    ldr x2, [sp, #1056]
    lsl x2, x2, #8
    add x2, x2, #258
    ldr x3, [sp, #1056]
    lsl x3, x3, #8
    add x3, x3, #259
    ldr x4, [sp, #1056]
    lsl x4, x4, #8
    add x4, x4, #260
    ldr x5, [sp, #1056]
    lsl x5, x5, #8
    add x5, x5, #261
    ldr x6, [sp, #1056]
    lsl x6, x6, #8
    add x6, x6, #262
    ldr x7, [sp, #1056]
    lsl x7, x7, #8
    add x7, x7, #263
    ldr x8, [sp, #1056]
    lsl x8, x8, #8
    add x8, x8, #264
    ldr x9, [sp, #1056]
    lsl x9, x9, #8
    add x9, x9, #265
    ldr x10, [sp, #1056]
    lsl x10, x10, #8
    add x10, x10, #266
    ldr x11, [sp, #1056]
    lsl x11, x11, #8
    add x11, x11, #267
    ldr x12, [sp, #1056]
    lsl x12, x12, #8
    add x12, x12, #268
    ldr x13, [sp, #1056]
    lsl x13, x13, #8
    add x13, x13, #269
    ldr x14, [sp, #1056]
    lsl x14, x14, #8
    add x14, x14, #270
    ldr x15, [sp, #1056]
    lsl x15, x15, #8
    add x15, x15, #271
    ldr x16, [sp, #1056]
    lsl x16, x16, #8
    add x16, x16, #272
    ldr x18, [sp, #1056]
    lsl x18, x18, #8
    add x18, x18, #274
    ldr x19, [sp, #1056]
    lsl x19, x19, #8
    add x19, x19, #275
    ldr x20, [sp, #1056]
    lsl x20, x20, #8
    add x20, x20, #276
    ldr x21, [sp, #1056]
    lsl x21, x21, #8
    add x21, x21, #277
    ldr x22, [sp, #1056]
    lsl x22, x22, #8
    add x22, x22, #278
    ldr x23, [sp, #1056]
    lsl x23, x23, #8
    add x23, x23, #279
    ldr x24, [sp, #1056]
    lsl x24, x24, #8
    add x24, x24, #280
    ldr x25, [sp, #1056]
    lsl x25, x25, #8
    add x25, x25, #281
    ldr x26, [sp, #1056]
    lsl x26, x26, #8
    add x26, x26, #282
    ldr x27, [sp, #1056]
    lsl x27, x27, #8
    add x27, x27, #283
    ldr x28, [sp, #1056]
    lsl x28, x28, #8
    add x28, x28, #284
    ldr x29, [sp, #1056]
    lsl x29, x29, #8
    add x29, x29, #285
    ldr x30, [sp, #1056]
    lsl x30, x30, #8
    add x30, x30, #286
    ldr x17, [sp, #1056]
    lsl x17, x17, #8
    add x17, x17, #273
    ldr x3, =arm_preempt_irq_count
    ldr x5, [x3]
    add x5, x5, #8
.Lprobe_poll1:
    ldr x4, [x3]
    sub x4, x4, x5
    tbnz x4, #63, .Lprobe_poll1
    b .Lprobe_snapshot
.Lprobe_snapshot:
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
    mov x0, sp
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
    mrs x0, nzcv
    str x0, [sp, #768]
    mrs x0, fpcr
    str x0, [sp, #776]
    mrs x0, fpsr
    str x0, [sp, #784]
    ldr x0, [sp, #1056]
    lsl x0, x0, #8
    add x0, x0, #256
    mov x1, #0
    mov x2, sp
    ldr x6, [sp, #1064]
.Lprobe_check_gpr:
    cbnz x6, .Lprobe_skip_second
    cmp x1, #3
    b.lo .Lprobe_next_gpr
    b .Lprobe_compare_gpr
.Lprobe_skip_second:
    cmp x1, #3
    b.lo .Lprobe_compare_gpr
    cmp x1, #6
    b.lo .Lprobe_next_gpr
.Lprobe_compare_gpr:
    ldr x3, [x2, x1, lsl #3]
    add x4, x0, x1
    cmp x3, x4
    b.ne .Lprobe_bad
.Lprobe_next_gpr:
    add x1, x1, #1
    cmp x1, #31
    b.lo .Lprobe_check_gpr
    ldr x0, [sp, #248]
    ldr x1, [sp, #1072]
    cmp x0, x1
    b.ne .Lprobe_bad
    ldr x0, [sp, #1056]
    and x1, x0, #3
    lsl x1, x1, #28
    ldr x2, [sp, #768]
    cmp x1, x2
    b.ne .Lprobe_bad
    and x1, x0, #1
    lsl x1, x1, #22
    ldr x2, [sp, #776]
    cmp x1, x2
    b.ne .Lprobe_bad
    lsl x1, x1, #5
    ldr x2, [sp, #784]
    cmp x1, x2
    b.ne .Lprobe_bad
    lsl x0, x0, #5
    add x0, x0, #32
    ldr x5, =0x0101010101010101
    add x2, sp, #256
    mov x1, #0
.Lprobe_vectors:
    add x3, x0, x1
    and x3, x3, #255
    mul x3, x3, x5
    ldp x4, x7, [x2], #16
    cmp x3, x4
    b.ne .Lprobe_bad
    cmp x3, x7
    b.ne .Lprobe_bad
    add x1, x1, #1
    cmp x1, #32
    b.lo .Lprobe_vectors
    cbnz x6, .Lprobe_finish
    mov x6, #1
    str x6, [sp, #1064]
    b .Lprobe_pass1
.Lprobe_bad:
    str xzr, [sp, #1080]
.Lprobe_finish:
    ldr x1, [sp, #1040]
    msr fpcr, x1
    ldr x1, [sp, #1048]
    msr fpsr, x1
    ldp q8, q9, [sp, #912]
    ldp q10, q11, [sp, #944]
    ldp q12, q13, [sp, #976]
    ldp q14, q15, [sp, #1008]
    add x1, sp, #800
    ldp x18, x19, [x1, #0]
    ldp x20, x21, [x1, #16]
    ldp x22, x23, [x1, #32]
    ldp x24, x25, [x1, #48]
    ldp x26, x27, [x1, #64]
    ldp x28, x29, [x1, #80]
    ldr x30, [sp, #896]
    ldr x0, [sp, #1080]
    add sp, sp, #1104
    ret
    .ltorg
