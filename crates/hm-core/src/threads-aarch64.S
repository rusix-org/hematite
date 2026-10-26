// SAFETY: Reviewed design0010. Private records and guarded stacks remain owned
// through each ordinary AAPCS64 switch; no live Rust frame is abandoned.
.arch armv8.1-a
.section .text.threads,"ax"
.global arm_context_switch, arm_thread_start, arm_thread_context_probe
arm_context_switch:
    stp x18, x19, [x0, #0]
    stp x20, x21, [x0, #16]
    stp x22, x23, [x0, #32]
    stp x24, x25, [x0, #48]
    stp x26, x27, [x0, #64]
    stp x28, x29, [x0, #80]
    str x30, [x0, #96]
    mov x2, sp
    str x2, [x0, #104]
    stp q8, q9, [x0, #112]
    stp q10, q11, [x0, #144]
    stp q12, q13, [x0, #176]
    stp q14, q15, [x0, #208]
    mrs x2, fpcr
    str x2, [x0, #240]
    mrs x2, fpsr
    str x2, [x0, #248]
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

arm_thread_start:
    mov x0, x21
    mov x1, x22
    bl {entry}
    // Rust callback and adapter have both returned normally. Only assembly
    // frames remain; marking Done cannot strand callback-owned Rust locals.
    str x0, [x19, #256]
    mov x0, #2
    str x0, [x19, #264]
    mov x0, x19
    mov x1, x20
    b arm_context_switch

arm_thread_context_probe:
    sub sp, sp, #208
    str x19, [sp, #0]
    str x20, [sp, #8]
    str x21, [sp, #16]
    str x22, [sp, #24]
    str x23, [sp, #32]
    str x24, [sp, #40]
    str x25, [sp, #48]
    str x26, [sp, #56]
    str x27, [sp, #64]
    str x28, [sp, #72]
    str x29, [sp, #80]
    str x30, [sp, #88]
    str d8, [sp, #96]
    str d9, [sp, #104]
    str d10, [sp, #112]
    str d11, [sp, #120]
    str d12, [sp, #128]
    str d13, [sp, #136]
    str d14, [sp, #144]
    str d15, [sp, #152]
    mrs x9, fpcr
    str x9, [sp, #160]
    mrs x9, fpsr
    str x9, [sp, #168]
    str x0, [sp, #176]
    str x1, [sp, #200]
    mov x9, sp
    str x9, [sp, #192]
    mov x9, #0x100
    add x9, x9, x1, lsl #8
    str x9, [sp, #184]
    add x19, x9, #19
    add x20, x9, #20
    add x21, x9, #21
    add x22, x9, #22
    add x23, x9, #23
    add x24, x9, #24
    add x25, x9, #25
    add x26, x9, #26
    add x27, x9, #27
    add x28, x9, #28
    add x29, x9, #29
    add x10, x9, #8
    fmov d8, x10
    add x10, x9, #9
    fmov d9, x10
    add x10, x9, #10
    fmov d10, x10
    add x10, x9, #11
    fmov d11, x10
    add x10, x9, #12
    fmov d12, x10
    add x10, x9, #13
    fmov d13, x10
    add x10, x9, #14
    fmov d14, x10
    add x10, x9, #15
    fmov d15, x10
    and x10, x1, #1
    lsl x11, x10, #22
    msr fpcr, x11
    lsl x11, x10, #27
    msr fpsr, x11
    ldr x0, [sp, #176]
    bl {probe_yield}
    mov x9, sp
    ldr x10, [sp, #192]
    cmp x9, x10
    b.ne .Lthread_probe_fail
    ldr x9, [sp, #184]
    add x10, x9, #19
    cmp x19, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #20
    cmp x20, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #21
    cmp x21, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #22
    cmp x22, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #23
    cmp x23, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #24
    cmp x24, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #25
    cmp x25, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #26
    cmp x26, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #27
    cmp x27, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #28
    cmp x28, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #29
    cmp x29, x10
    b.ne .Lthread_probe_fail
    add x10, x9, #8
    fmov x11, d8
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #9
    fmov x11, d9
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #10
    fmov x11, d10
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #11
    fmov x11, d11
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #12
    fmov x11, d12
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #13
    fmov x11, d13
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #14
    fmov x11, d14
    cmp x10, x11
    b.ne .Lthread_probe_fail
    add x10, x9, #15
    fmov x11, d15
    cmp x10, x11
    b.ne .Lthread_probe_fail
    ldr x9, [sp, #200]
    and x9, x9, #1
    lsl x10, x9, #22
    mrs x11, fpcr
    cmp x10, x11
    b.ne .Lthread_probe_fail
    lsl x10, x9, #27
    mrs x11, fpsr
    cmp x10, x11
    b.ne .Lthread_probe_fail
    mov x0, #1
    b .Lthread_probe_restore
.Lthread_probe_fail:
    mov x0, #0
.Lthread_probe_restore:
    ldr x9, [sp, #160]
    msr fpcr, x9
    ldr x9, [sp, #168]
    msr fpsr, x9
    ldr d8, [sp, #96]
    ldr d9, [sp, #104]
    ldr d10, [sp, #112]
    ldr d11, [sp, #120]
    ldr d12, [sp, #128]
    ldr d13, [sp, #136]
    ldr d14, [sp, #144]
    ldr d15, [sp, #152]
    ldr x19, [sp, #0]
    ldr x20, [sp, #8]
    ldr x21, [sp, #16]
    ldr x22, [sp, #24]
    ldr x23, [sp, #32]
    ldr x24, [sp, #40]
    ldr x25, [sp, #48]
    ldr x26, [sp, #56]
    ldr x27, [sp, #64]
    ldr x28, [sp, #72]
    ldr x29, [sp, #80]
    ldr x30, [sp, #88]
    add sp, sp, #208
    ret
