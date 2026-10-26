// Standalone untrusted native runtime probes, design 0028.
.arch armv8.1-a
.section .text.entry,"ax"
.global native_start
native_start:
.if {kind} < 4
    b context_entry
.else
    b runtime_main
.endif
.section .text.poll,"ax"
.global poll_start, poll_end
poll_start:
.if {kind} == 3
    ldr x18, [x17]
    cbz x18, poll_start
.else
    ldr x17, [x18]
    cbz x17, poll_start
.endif
poll_end:
    b context_captured
.section .runtime.data,"aw"
.balign 8
.global release, private_data
release: .quad 0
private_data: .quad 0
.global capture
.set capture, 0x1001ffc00
.section .text,"ax"
.global call_write, write_svc
call_write:
    mov x3, x2
    mov x2, x1
    mov x1, x0
    mov x0, #1
    mov x8, #1
write_svc:
    svc #0
    ret
.global call_exit, exit_svc
call_exit:
    mov x1, x0
    mov x0, #1
    mov x2, #0
    mov x3, #0
    mov x8, #2
exit_svc:
    svc #0
    ret
runtime_fail:
    mov x0, #201
    b call_exit
.global own_fault, fault
own_fault:
    adr x9, native_start
    msr nzcv, xzr
fault:
    str xzr, [x9]
    b runtime_fail
.global run_loop, loop_start, loop_end
run_loop:
    msr nzcv, xzr
loop_start:
    b loop_start
loop_end:
// Capture the explicit 104-word wire before clobbering any saved GPR.
.macro save_capture initial
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
    add x3, sp, #264
    umov x1, v0.d[0]
    umov x2, v0.d[1]
    stp x1, x2, [x3, #0]
    umov x1, v1.d[0]
    umov x2, v1.d[1]
    stp x1, x2, [x3, #16]
    umov x1, v2.d[0]
    umov x2, v2.d[1]
    stp x1, x2, [x3, #32]
    umov x1, v3.d[0]
    umov x2, v3.d[1]
    stp x1, x2, [x3, #48]
    umov x1, v4.d[0]
    umov x2, v4.d[1]
    stp x1, x2, [x3, #64]
    umov x1, v5.d[0]
    umov x2, v5.d[1]
    stp x1, x2, [x3, #80]
    umov x1, v6.d[0]
    umov x2, v6.d[1]
    stp x1, x2, [x3, #96]
    umov x1, v7.d[0]
    umov x2, v7.d[1]
    stp x1, x2, [x3, #112]
    umov x1, v8.d[0]
    umov x2, v8.d[1]
    stp x1, x2, [x3, #128]
    umov x1, v9.d[0]
    umov x2, v9.d[1]
    stp x1, x2, [x3, #144]
    umov x1, v10.d[0]
    umov x2, v10.d[1]
    stp x1, x2, [x3, #160]
    umov x1, v11.d[0]
    umov x2, v11.d[1]
    stp x1, x2, [x3, #176]
    umov x1, v12.d[0]
    umov x2, v12.d[1]
    stp x1, x2, [x3, #192]
    umov x1, v13.d[0]
    umov x2, v13.d[1]
    stp x1, x2, [x3, #208]
    umov x1, v14.d[0]
    umov x2, v14.d[1]
    stp x1, x2, [x3, #224]
    umov x1, v15.d[0]
    umov x2, v15.d[1]
    stp x1, x2, [x3, #240]
    umov x1, v16.d[0]
    umov x2, v16.d[1]
    stp x1, x2, [x3, #256]
    umov x1, v17.d[0]
    umov x2, v17.d[1]
    stp x1, x2, [x3, #272]
    umov x1, v18.d[0]
    umov x2, v18.d[1]
    stp x1, x2, [x3, #288]
    umov x1, v19.d[0]
    umov x2, v19.d[1]
    stp x1, x2, [x3, #304]
    umov x1, v20.d[0]
    umov x2, v20.d[1]
    stp x1, x2, [x3, #320]
    umov x1, v21.d[0]
    umov x2, v21.d[1]
    stp x1, x2, [x3, #336]
    umov x1, v22.d[0]
    umov x2, v22.d[1]
    stp x1, x2, [x3, #352]
    umov x1, v23.d[0]
    umov x2, v23.d[1]
    stp x1, x2, [x3, #368]
    umov x1, v24.d[0]
    umov x2, v24.d[1]
    stp x1, x2, [x3, #384]
    umov x1, v25.d[0]
    umov x2, v25.d[1]
    stp x1, x2, [x3, #400]
    umov x1, v26.d[0]
    umov x2, v26.d[1]
    stp x1, x2, [x3, #416]
    umov x1, v27.d[0]
    umov x2, v27.d[1]
    stp x1, x2, [x3, #432]
    umov x1, v28.d[0]
    umov x2, v28.d[1]
    stp x1, x2, [x3, #448]
    umov x1, v29.d[0]
    umov x2, v29.d[1]
    stp x1, x2, [x3, #464]
    umov x1, v30.d[0]
    umov x2, v30.d[1]
    stp x1, x2, [x3, #480]
    umov x1, v31.d[0]
    umov x2, v31.d[1]
    stp x1, x2, [x3, #496]
    mrs x1, fpcr
    str x1, [sp, #776]
    mrs x1, fpsr
    str x1, [sp, #784]
    mrs x1, tpidr_el0
    str x1, [sp, #792]
    mrs x1, tpidrro_el0
    str x1, [sp, #800]
    mrs x1, S3_3_C4_C2_5
    str x1, [sp, #808]
    mrs x1, S3_3_C4_C2_6
    str x1, [sp, #816]
    str xzr, [sp, #824]
.endm
context_entry:
    sub sp, sp, #1024
    save_capture 1
    mov x0, sp
    bl context_initial
    cbz x0, runtime_fail
    ldr x0, =private_data
    mov x1, #(({kind} & 1) + 1)
    str x1, [x0]
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 0)
    fmov d0, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 0)
    ins v0.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 1)
    fmov d1, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 1)
    ins v1.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 2)
    fmov d2, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 2)
    ins v2.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 3)
    fmov d3, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 3)
    ins v3.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 4)
    fmov d4, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 4)
    ins v4.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 5)
    fmov d5, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 5)
    ins v5.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 6)
    fmov d6, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 6)
    ins v6.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 7)
    fmov d7, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 7)
    ins v7.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 8)
    fmov d8, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 8)
    ins v8.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 9)
    fmov d9, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 9)
    ins v9.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 10)
    fmov d10, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 10)
    ins v10.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 11)
    fmov d11, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 11)
    ins v11.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 12)
    fmov d12, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 12)
    ins v12.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 13)
    fmov d13, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 13)
    ins v13.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 14)
    fmov d14, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 14)
    ins v14.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 15)
    fmov d15, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 15)
    ins v15.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 16)
    fmov d16, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 16)
    ins v16.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 17)
    fmov d17, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 17)
    ins v17.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 18)
    fmov d18, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 18)
    ins v18.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 19)
    fmov d19, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 19)
    ins v19.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 20)
    fmov d20, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 20)
    ins v20.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 21)
    fmov d21, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 21)
    ins v21.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 22)
    fmov d22, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 22)
    ins v22.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 23)
    fmov d23, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 23)
    ins v23.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 24)
    fmov d24, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 24)
    ins v24.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 25)
    fmov d25, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 25)
    ins v25.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 26)
    fmov d26, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 26)
    ins v26.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 27)
    fmov d27, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 27)
    ins v27.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 28)
    fmov d28, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 28)
    ins v28.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 29)
    fmov d29, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 29)
    ins v29.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 30)
    fmov d30, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 30)
    ins v30.d[1], x1
    movz x0, #0x2222, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 31)
    fmov d31, x0
    movz x1, #0x3333, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 31)
    ins v31.d[1], x1
    mov x0, #((({kind} & 1) + 1) << 22)
    msr fpcr, x0
    mov x0, #0x08000000
    add x0, x0, #(({kind} & 1) + 1)
    msr fpsr, x0
    movz x0, #0x4444, lsl #48
    movk x0, #(({kind} & 1) + 1)
    msr tpidr_el0, x0
    mov x0, #0x01000000
.global context_set_dit
context_set_dit:
    msr S3_3_C4_C2_5, x0
    mov x0, #0x1000
.global context_set_ssbs
context_set_ssbs:
    msr S3_3_C4_C2_6, x0
.if ({kind} & 1) == 0
    mov x0, #0xa0000000
.else
    mov x0, #0x50000000
.endif
    msr nzcv, x0
    movz x0, #0x1111, lsl #48
    movk x0, #(((({kind} & 1) + 1) << 8) | 0)
    movz x1, #0x1111, lsl #48
    movk x1, #(((({kind} & 1) + 1) << 8) | 1)
    movz x2, #0x1111, lsl #48
    movk x2, #(((({kind} & 1) + 1) << 8) | 2)
    movz x3, #0x1111, lsl #48
    movk x3, #(((({kind} & 1) + 1) << 8) | 3)
    movz x4, #0x1111, lsl #48
    movk x4, #(((({kind} & 1) + 1) << 8) | 4)
    movz x5, #0x1111, lsl #48
    movk x5, #(((({kind} & 1) + 1) << 8) | 5)
    movz x6, #0x1111, lsl #48
    movk x6, #(((({kind} & 1) + 1) << 8) | 6)
    movz x7, #0x1111, lsl #48
    movk x7, #(((({kind} & 1) + 1) << 8) | 7)
    movz x8, #0x1111, lsl #48
    movk x8, #(((({kind} & 1) + 1) << 8) | 8)
    movz x9, #0x1111, lsl #48
    movk x9, #(((({kind} & 1) + 1) << 8) | 9)
    movz x10, #0x1111, lsl #48
    movk x10, #(((({kind} & 1) + 1) << 8) | 10)
    movz x11, #0x1111, lsl #48
    movk x11, #(((({kind} & 1) + 1) << 8) | 11)
    movz x12, #0x1111, lsl #48
    movk x12, #(((({kind} & 1) + 1) << 8) | 12)
    movz x13, #0x1111, lsl #48
    movk x13, #(((({kind} & 1) + 1) << 8) | 13)
    movz x14, #0x1111, lsl #48
    movk x14, #(((({kind} & 1) + 1) << 8) | 14)
    movz x15, #0x1111, lsl #48
    movk x15, #(((({kind} & 1) + 1) << 8) | 15)
    movz x16, #0x1111, lsl #48
    movk x16, #(((({kind} & 1) + 1) << 8) | 16)
    movz x17, #0x1111, lsl #48
    movk x17, #(((({kind} & 1) + 1) << 8) | 17)
    movz x18, #0x1111, lsl #48
    movk x18, #(((({kind} & 1) + 1) << 8) | 18)
    movz x19, #0x1111, lsl #48
    movk x19, #(((({kind} & 1) + 1) << 8) | 19)
    movz x20, #0x1111, lsl #48
    movk x20, #(((({kind} & 1) + 1) << 8) | 20)
    movz x21, #0x1111, lsl #48
    movk x21, #(((({kind} & 1) + 1) << 8) | 21)
    movz x22, #0x1111, lsl #48
    movk x22, #(((({kind} & 1) + 1) << 8) | 22)
    movz x23, #0x1111, lsl #48
    movk x23, #(((({kind} & 1) + 1) << 8) | 23)
    movz x24, #0x1111, lsl #48
    movk x24, #(((({kind} & 1) + 1) << 8) | 24)
    movz x25, #0x1111, lsl #48
    movk x25, #(((({kind} & 1) + 1) << 8) | 25)
    movz x26, #0x1111, lsl #48
    movk x26, #(((({kind} & 1) + 1) << 8) | 26)
    movz x27, #0x1111, lsl #48
    movk x27, #(((({kind} & 1) + 1) << 8) | 27)
    movz x28, #0x1111, lsl #48
    movk x28, #(((({kind} & 1) + 1) << 8) | 28)
    movz x29, #0x1111, lsl #48
    movk x29, #(((({kind} & 1) + 1) << 8) | 29)
    movz x30, #0x1111, lsl #48
    movk x30, #(((({kind} & 1) + 1) << 8) | 30)
.if {kind} < 2
    mov x2, #-1
    mov x3, #0
    mov x8, #1
    ldr x0, =0x100100018
    ldr x1, [x0]
    mov x0, #1
.global context_svc
context_svc:
    svc #0
    b context_captured
.else
.if {kind} == 3
    movz x17, #0x2000
    movk x17, #1, lsl #32
.else
    movz x18, #0x2000
    movk x18, #1, lsl #32
.endif
    b poll_start
.global context_svc
context_svc:
    svc #0
.endif
context_captured:
    save_capture 0
    mov x0, sp
    b context_after
    .ltorg
