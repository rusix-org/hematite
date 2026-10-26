// SAFETY: Standalone untrusted EL0 fixtures; fixed ABI shims and deliberate
// faults never execute in kernel context. Captures use retained user stack.
.arch armv8.1-a
.section .text.entry,"ax"
.balign 4
.global native_start
native_start:
.if {case} == 29
    b register_entry
.else
    mov x1, #{case}
    b fixture_main
.endif

.global call_write, write_svc
call_write:
    mov x8, x1
    mov x1, x2
    mov x2, x3
    mov x3, x4
write_svc:
    svc #0
    ret
.global call_exit, exit_svc
call_exit:
    mov x3, x2
    mov x2, x1
    mov x1, x0
    mov x0, #1
    mov x8, #2
exit_svc:
    svc #0
    ret
fixture_fail:
    mov x0, #205
    mov x1, #0
    mov x2, #0
    b call_exit

.global deliberate_fault
 deliberate_fault:
    cmp x0, #8
    b.eq prepare_rxwrite
    cmp x0, #9
    b.eq prepare_rwexecute
    cmp x0, #10
    b.eq prepare_stackexecute
    cmp x0, #11
    b.eq prepare_kernelread
    cmp x0, #12
    b.eq prepare_kernelwrite
    cmp x0, #13
    b.eq prepare_uartread
    cmp x0, #14
    b.eq prepare_gicread
    cmp x0, #15
    b.eq prepare_cntpdisable
    cmp x0, #16
    b.eq prepare_daifmask
    cmp x0, #17
    b.eq prepare_stackguard
    cmp x0, #18
    b.eq prepare_loop
    cmp x0, #28
    b.eq prepare_svcimm
    cmp x0, #30
    b.eq prepare_badsp
    cmp x0, #31
    b.eq prepare_startupwrite
    cmp x0, #32
    b.eq prepare_semihost
    b fixture_fail
prepare_rxwrite:
    ldr x1, =native_start
fault_rxwrite:
    str xzr, [x1]
    b fixture_fail
prepare_rwexecute:
    ldr x1, =data
fault_rwexecute:
    br x1
    b fixture_fail
prepare_stackexecute:
    ldr x1, =0x1001ffff0
fault_stackexecute:
    br x1
    b fixture_fail
prepare_kernelread:
    ldr x1, =kernel_text
fault_kernelread:
    ldr x0, [x1]
    b fixture_fail
prepare_kernelwrite:
    ldr x1, =kernel_data
fault_kernelwrite:
    str xzr, [x1]
    b fixture_fail
prepare_uartread:
    ldr x1, =0x09000000
fault_uartread:
    ldr x0, [x1]
    b fixture_fail
prepare_gicread:
    ldr x1, =0x08000000
fault_gicread:
    ldr x0, [x1]
    b fixture_fail
prepare_cntpdisable:
    nop
fault_cntpdisable:
    msr cntp_ctl_el0, xzr
    b fixture_fail
prepare_daifmask:
    nop
fault_daifmask:
    msr daifset, #2
    b fixture_fail
prepare_stackguard:
    ldr x1, =0x1001ef000
fault_stackguard:
    str xzr, [x1]
    b fixture_fail
prepare_startupwrite:
    ldr x1, =0x100100038
fault_startupwrite:
    str xzr, [x1]
    b fixture_fail
prepare_semihost:
    nop
fault_semihost:
    hlt #0xf000
    b fixture_fail
prepare_svcimm:
    nop
fault_svcimm:
    svc #1
    b fixture_fail
prepare_loop:
.global loop_start, loop_end
loop_start:
    b loop_start
loop_end:
    b fixture_fail
prepare_badsp:
    mov x9, #-1
    mov sp, x9
    mov x0, #0
    mov x1, #0
    mov x2, #0
    b call_exit
.if {case} == 8
.global fault
.set fault, fault_rxwrite
.endif
.if {case} == 9
.global fault
.set fault, fault_rwexecute
.endif
.if {case} == 10
.global fault
.set fault, fault_stackexecute
.endif
.if {case} == 11
.global fault
.set fault, fault_kernelread
.endif
.if {case} == 12
.global fault
.set fault, fault_kernelwrite
.endif
.if {case} == 13
.global fault
.set fault, fault_uartread
.endif
.if {case} == 14
.global fault
.set fault, fault_gicread
.endif
.if {case} == 15
.global fault
.set fault, fault_cntpdisable
.endif
.if {case} == 16
.global fault
.set fault, fault_daifmask
.endif
.if {case} == 17
.global fault
.set fault, fault_stackguard
.endif
.if {case} == 28
.global fault
.set fault, fault_svcimm
.endif
.if {case} == 31
.global fault
.set fault, fault_startupwrite
.endif
.if {case} == 32
.global fault
.set fault, fault_semihost
.endif

// Save before scratch use. SP is already the private 1024-byte capture frame.
.macro capture initial
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
.if \initial
    add x1, sp, #1024
.else
    mov x1, sp
.endif
    str x1, [sp, #248]
    mrs x1, nzcv
    str x1, [sp, #256]
    mrs x1, fpcr
    str x1, [sp, #264]
    stp q0, q1, [sp, #272]
    stp q2, q3, [sp, #304]
    stp q4, q5, [sp, #336]
    stp q6, q7, [sp, #368]
    stp q8, q9, [sp, #400]
    stp q10, q11, [sp, #432]
    stp q12, q13, [sp, #464]
    stp q14, q15, [sp, #496]
    stp q16, q17, [sp, #528]
    stp q18, q19, [sp, #560]
    stp q20, q21, [sp, #592]
    stp q22, q23, [sp, #624]
    stp q24, q25, [sp, #656]
    stp q26, q27, [sp, #688]
    stp q28, q29, [sp, #720]
    stp q30, q31, [sp, #752]
    mrs x1, fpsr
    str x1, [sp, #784]
    mrs x1, tpidr_el0
    str x1, [sp, #792]
    mrs x1, tpidrro_el0
    str x1, [sp, #800]
    mrs x1, S3_3_C4_C2_5 // DIT: fixed QEMU max fixture profile.
    str x1, [sp, #808]
    mrs x1, S3_3_C4_C2_6 // SSBS2 direct access in the admitted max profile.
    str x1, [sp, #816]
    str xzr, [sp, #824]
    str xzr, [sp, #832]
    str xzr, [sp, #840]
    str xzr, [sp, #848]
    str xzr, [sp, #856]
    str xzr, [sp, #864]
    str xzr, [sp, #872]
    str xzr, [sp, #880]
    str xzr, [sp, #888]
    str xzr, [sp, #896]
    str xzr, [sp, #904]
    str xzr, [sp, #912]
    str xzr, [sp, #920]
    str xzr, [sp, #928]
    str xzr, [sp, #936]
    str xzr, [sp, #944]
    str xzr, [sp, #952]
    str xzr, [sp, #960]
    str xzr, [sp, #968]
    str xzr, [sp, #976]
    str xzr, [sp, #984]
    str xzr, [sp, #992]
    str xzr, [sp, #1000]
    str xzr, [sp, #1008]
    str xzr, [sp, #1016]
.endm
.macro sentinels stage, flags
    movz x2, #0x3000, lsl #48
    movk x2, #((\stage << 8) | 2)
    movz x3, #0x3000, lsl #48
    movk x3, #((\stage << 8) | 3)
    movz x4, #0x3000, lsl #48
    movk x4, #((\stage << 8) | 4)
    movz x5, #0x3000, lsl #48
    movk x5, #((\stage << 8) | 5)
    movz x6, #0x3000, lsl #48
    movk x6, #((\stage << 8) | 6)
    movz x7, #0x3000, lsl #48
    movk x7, #((\stage << 8) | 7)
    movz x8, #0x3000, lsl #48
    movk x8, #((\stage << 8) | 8)
    movz x9, #0x3000, lsl #48
    movk x9, #((\stage << 8) | 9)
    movz x10, #0x3000, lsl #48
    movk x10, #((\stage << 8) | 10)
    movz x11, #0x3000, lsl #48
    movk x11, #((\stage << 8) | 11)
    movz x12, #0x3000, lsl #48
    movk x12, #((\stage << 8) | 12)
    movz x13, #0x3000, lsl #48
    movk x13, #((\stage << 8) | 13)
    movz x14, #0x3000, lsl #48
    movk x14, #((\stage << 8) | 14)
    movz x15, #0x3000, lsl #48
    movk x15, #((\stage << 8) | 15)
    movz x16, #0x3000, lsl #48
    movk x16, #((\stage << 8) | 16)
    movz x17, #0x3000, lsl #48
    movk x17, #((\stage << 8) | 17)
    movz x18, #0x3000, lsl #48
    movk x18, #((\stage << 8) | 18)
    movz x19, #0x3000, lsl #48
    movk x19, #((\stage << 8) | 19)
    movz x20, #0x3000, lsl #48
    movk x20, #((\stage << 8) | 20)
    movz x21, #0x3000, lsl #48
    movk x21, #((\stage << 8) | 21)
    movz x22, #0x3000, lsl #48
    movk x22, #((\stage << 8) | 22)
    movz x23, #0x3000, lsl #48
    movk x23, #((\stage << 8) | 23)
    movz x24, #0x3000, lsl #48
    movk x24, #((\stage << 8) | 24)
    movz x25, #0x3000, lsl #48
    movk x25, #((\stage << 8) | 25)
    movz x26, #0x3000, lsl #48
    movk x26, #((\stage << 8) | 26)
    movz x27, #0x3000, lsl #48
    movk x27, #((\stage << 8) | 27)
    movz x28, #0x3000, lsl #48
    movk x28, #((\stage << 8) | 28)
    movz x29, #0x3000, lsl #48
    movk x29, #((\stage << 8) | 29)
    movz x30, #0x3000, lsl #48
    movk x30, #((\stage << 8) | 30)
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 0)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 0)
    fmov d0, x0
    ins v0.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 1)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 1)
    fmov d1, x0
    ins v1.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 2)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 2)
    fmov d2, x0
    ins v2.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 3)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 3)
    fmov d3, x0
    ins v3.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 4)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 4)
    fmov d4, x0
    ins v4.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 5)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 5)
    fmov d5, x0
    ins v5.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 6)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 6)
    fmov d6, x0
    ins v6.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 7)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 7)
    fmov d7, x0
    ins v7.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 8)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 8)
    fmov d8, x0
    ins v8.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 9)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 9)
    fmov d9, x0
    ins v9.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 10)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 10)
    fmov d10, x0
    ins v10.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 11)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 11)
    fmov d11, x0
    ins v11.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 12)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 12)
    fmov d12, x0
    ins v12.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 13)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 13)
    fmov d13, x0
    ins v13.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 14)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 14)
    fmov d14, x0
    ins v14.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 15)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 15)
    fmov d15, x0
    ins v15.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 16)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 16)
    fmov d16, x0
    ins v16.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 17)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 17)
    fmov d17, x0
    ins v17.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 18)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 18)
    fmov d18, x0
    ins v18.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 19)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 19)
    fmov d19, x0
    ins v19.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 20)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 20)
    fmov d20, x0
    ins v20.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 21)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 21)
    fmov d21, x0
    ins v21.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 22)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 22)
    fmov d22, x0
    ins v22.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 23)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 23)
    fmov d23, x0
    ins v23.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 24)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 24)
    fmov d24, x0
    ins v24.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 25)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 25)
    fmov d25, x0
    ins v25.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 26)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 26)
    fmov d26, x0
    ins v26.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 27)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 27)
    fmov d27, x0
    ins v27.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 28)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 28)
    fmov d28, x0
    ins v28.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 29)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 29)
    fmov d29, x0
    ins v29.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 30)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 30)
    fmov d30, x0
    ins v30.d[1], x1
    movz x0, #0x1000, lsl #48
    movk x0, #((\stage << 8) | 31)
    movz x1, #0x2000, lsl #48
    movk x1, #((\stage << 8) | 31)
    fmov d31, x0
    ins v31.d[1], x1
    mov x0, #(\stage << 22)
    msr fpcr, x0
    mov x0, #0x08000000
    add x0, x0, #(1 << (\stage - 1))
    msr fpsr, x0
    movz x0, #0x1234, lsl #48
    movk x0, #\stage
    msr tpidr_el0, x0
    mov x0, #0x01000000
    msr S3_3_C4_C2_5, x0
    mov x0, #0x1000
    msr S3_3_C4_C2_6, x0
    mov x2, #-1
    mov x3, #0
    mov x8, #1
    ldr x0, =0x100100018
    ldr x1, [x0]
    mov x0, #\flags
    msr nzcv, x0
    mov x0, #1
.endm
register_entry:
    sub sp, sp, #1024
    capture 1
    mov x0, sp
    mov x1, #0
    bl check_registers
    cbz x0, fixture_fail
    sentinels 1, 0xa0000000
.global register_svc_1
register_svc_1:
    svc #0
    capture 0
    mov x0, sp
    mov x1, #1
    bl check_registers
    cbz x0, fixture_fail
    sentinels 2, 0x50000000
.global register_svc_2
register_svc_2:
    svc #0
    capture 0
    mov x0, sp
    mov x1, #2
    bl check_registers
    cbz x0, fixture_fail
    sentinels 3, 0xf0000000
.global register_svc_3
register_svc_3:
    svc #0
    capture 0
    mov x0, sp
    mov x1, #3
    bl check_registers
    cbz x0, fixture_fail
    mov x0, #0
    mov x1, #0
    mov x2, #0
    b call_exit
.ltorg
