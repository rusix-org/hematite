// SAFETY: Design 0023. CPU0/BUSY/peer-OFF, exact admitted controls and
// retained roots/context. Only fixed reserved kernel storage is accessed while
// the process root is active. Every normal trap restores before returning.
.arch armv8.1-a
.section .text.process,"ax"
.balign 2048
.global arm_process_vectors
arm_process_vectors:
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_sync
    .space 124
    b arm_process_irq
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124
    b arm_process_kernel_fatal
    .space 124

.global arm_process_slice
.type arm_process_slice,%function
arm_process_slice:
    ldr x16, =__exception_stack_top
    sub x16, x16, #{FRAME_SIZE}
    str x18, [x16, #{K_GPRS}]
    str x19, [x16, #({K_GPRS}+8)]
    str x20, [x16, #({K_GPRS}+16)]
    str x21, [x16, #({K_GPRS}+24)]
    str x22, [x16, #({K_GPRS}+32)]
    str x23, [x16, #({K_GPRS}+40)]
    str x24, [x16, #({K_GPRS}+48)]
    str x25, [x16, #({K_GPRS}+56)]
    str x26, [x16, #({K_GPRS}+64)]
    str x27, [x16, #({K_GPRS}+72)]
    str x28, [x16, #({K_GPRS}+80)]
    str x29, [x16, #({K_GPRS}+88)]
    str x30, [x16, #({K_GPRS}+96)]
    str q8, [x16, #{K_Q}]
    str q9, [x16, #({K_Q}+16)]
    str q10, [x16, #({K_Q}+32)]
    str q11, [x16, #({K_Q}+48)]
    str q12, [x16, #({K_Q}+64)]
    str q13, [x16, #({K_Q}+80)]
    str q14, [x16, #({K_Q}+96)]
    str q15, [x16, #({K_Q}+112)]
    mrs x17, ttbr0_el1
    str x17, [x16, #{K_TTBR0}]
    mrs x17, ttbr1_el1
    str x17, [x16, #{K_TTBR1}]
    mrs x17, par_el1
    str x17, [x16, #{K_PAR}]
    mrs x17, vbar_el1
    str x17, [x16, #{K_VBAR}]
    mrs x17, sp_el0
    str x17, [x16, #{K_SP0}]
    mrs x17, daif
    str x17, [x16, #{K_DAIF}]
    mrs x17, elr_el1
    str x17, [x16, #{K_ELR}]
    mrs x17, spsr_el1
    str x17, [x16, #{K_SPSR}]
    mrs x17, esr_el1
    str x17, [x16, #{K_ESR}]
    mrs x17, far_el1
    str x17, [x16, #{K_FAR}]
    mrs x17, cntkctl_el1
    str x17, [x16, #{K_CNTKCTL}]
    mrs x17, cpacr_el1
    str x17, [x16, #{K_CPACR}]
    mrs x17, tpidr_el0
    str x17, [x16, #{K_TLS}]
    mrs x17, tpidrro_el0
    str x17, [x16, #{K_TLSRO}]
    mrs x17, cntp_cval_el0
    str x17, [x16, #{K_CVAL}]
    mrs x17, cntp_ctl_el0
    and x17, x17, #3
    str x17, [x16, #{K_CTL}]
    mov x17, sp
    str x17, [x16, #{K_SP}]
    str x1, [x16, #{K_CONTEXT}]
    mrs x17, fpcr
    str x17, [x16, #{K_FPCR}]
    mrs x17, fpsr
    str x17, [x16, #{K_FPSR}]
    mrs x17, tcr_el1
    str x17, [x16, #{K_TCR}]
    mrs x17, mair_el1
    str x17, [x16, #{K_MAIR}]
    str x2, [x16, #{DEADLINE}]
    str x0, [x16, #{ROOT}]
    // Admission restricts DIT to0/1 and SSBS to0/2. Never access absent fields.
    mov x3, #0
    str xzr, [x16, #{K_DIT}]
    str xzr, [x16, #{K_SSBS}]
    mrs x17, id_aa64pfr0_el1
    ubfx x17, x17, #48, #4
    cbz x17, .Lprocess_no_dit_save
    mrs x17, S3_3_C4_C2_5
    str x17, [x16, #{K_DIT}]
    orr x3, x3, #1
.Lprocess_no_dit_save:
    mrs x17, id_aa64pfr1_el1
    ubfx x17, x17, #4, #4
    cbz x17, .Lprocess_no_ssbs_save
    mrs x17, S3_3_C4_C2_6
    str x17, [x16, #{K_SSBS}]
    orr x3, x3, #2
.Lprocess_no_ssbs_save:
    str x3, [x16, #{FEATURES}]
    mov x17, #1
    str x17, [x16, #{ACTIVE}]
    mov x3, #0
.Lprocess_copy_in:
    ldr x4, [x1, x3]
    str x4, [x16, x3]
    add x3, x3, #8
    cmp x3, #{USER_SIZE}
    b.lo .Lprocess_copy_in
    str xzr, [x16, #{USER_ESR}]
    str xzr, [x16, #{USER_FAR}]
    str xzr, [x16, #{USER_KIND}]
    str xzr, [x16, #{USER_IAR}]
    str xzr, [x16, #{USER_FAR_VALID}]
    ldr x5, =0x08000000
    ldr x6, =0x08010000
    ldr w7, [x5, #0]
    str x7, [x16, #{GICD_CTL}]
    ldr w7, [x6, #0]
    str x7, [x16, #{GICC_CTL}]
    ldr w7, [x6, #4]
    str x7, [x16, #{GIC_PMR}]
    ldr w7, [x6, #8]
    str x7, [x16, #{GIC_BPR}]
    ldr w7, [x5, #128]
    str x7, [x16, #{GIC_GROUP}]
    ldr w7, [x5, #3076]
    str x7, [x16, #{GIC_CONFIG}]
    ldr w7, [x5, #1052]
    str x7, [x16, #{GIC_PRIORITY}]
    ldr w7, [x5, #256]
    str x7, [x16, #{GIC_ENABLE}]
    ldr w7, [x5, #0x80]
    bic w7, w7, #0x40000000
    str w7, [x5, #0x80]
    dsb sy
    ldr w8, [x5, #0x80]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr w7, [x5, #0xc04]
    bic w7, w7, #0x20000000
    str w7, [x5, #0xc04]
    dsb sy
    ldr w8, [x5, #0xc04]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr w7, [x5, #0x41c]
    mov w8, #0x80
    bfi w7, w8, #16, #8
    str w7, [x5, #0x41c]
    dsb sy
    ldr w8, [x5, #0x41c]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    mov w7, #0xff
    str w7, [x6, #4]
    dsb sy
    ldr w8, [x6, #4]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    str wzr, [x6, #8]
    dsb sy
    ldr w8, [x6, #8]
    cbnz w8, arm_process_kernel_fatal
    mov w7, #1
    str w7, [x6]
    str w7, [x5]
    dsb sy
    ldr w8, [x6]
    cmp w8, #1
    b.ne arm_process_kernel_fatal
    ldr w8, [x5]
    cmp w8, #1
    b.ne arm_process_kernel_fatal
    mov w7, #0x40000000
    str w7, [x5, #0x100]
    dsb sy
    ldr w8, [x5, #0x100]
    tbz w8, #30, arm_process_kernel_fatal
    ldr x7, [x16, #{DEADLINE}]
    msr cntp_cval_el0, x7
    isb
    mrs x8, cntp_cval_el0
    cmp x8, x7
    b.ne arm_process_kernel_fatal
    dsb sy
    isb
    ldr x7, =arm_process_vectors
    msr vbar_el1, x7
    isb
    mov sp, x16
    msr cntkctl_el1, xzr
    mov x7, #0x300000
    msr cpacr_el1, x7
    isb
    ldr x7, [sp, #{USER_SP}]
    msr sp_el0, x7
    ldr x7, [sp, #{ROOT}]
    dsb sy
    tlbi vmalle1
    dsb sy
    isb
    msr ttbr0_el1, x7
    isb
    tlbi vmalle1
    dsb sy
    isb
    ldr x7, [sp, #{USER_PC}]
    msr elr_el1, x7
    ldr x7, [sp, #{USER_PSTATE}]
    and x7, x7, #0xf0000000
    ldr x8, =0x00400340
    orr x7, x7, x8
    msr spsr_el1, x7
    ldr x7, [sp, #{USER_FPCR}]
    msr fpcr, x7
    ldr x7, [sp, #{USER_FPSR}]
    msr fpsr, x7
    ldr x7, [sp, #{USER_TLS}]
    msr tpidr_el0, x7
    msr tpidrro_el0, xzr
    mov x7, #1
    msr cntp_ctl_el0, x7
    dsb sy
    isb
    ldr q0, [sp, #{USER_Q}]
    ldr q1, [sp, #({USER_Q}+16)]
    ldr q2, [sp, #({USER_Q}+32)]
    ldr q3, [sp, #({USER_Q}+48)]
    ldr q4, [sp, #({USER_Q}+64)]
    ldr q5, [sp, #({USER_Q}+80)]
    ldr q6, [sp, #({USER_Q}+96)]
    ldr q7, [sp, #({USER_Q}+112)]
    ldr q8, [sp, #({USER_Q}+128)]
    ldr q9, [sp, #({USER_Q}+144)]
    ldr q10, [sp, #({USER_Q}+160)]
    ldr q11, [sp, #({USER_Q}+176)]
    ldr q12, [sp, #({USER_Q}+192)]
    ldr q13, [sp, #({USER_Q}+208)]
    ldr q14, [sp, #({USER_Q}+224)]
    ldr q15, [sp, #({USER_Q}+240)]
    ldr q16, [sp, #({USER_Q}+256)]
    ldr q17, [sp, #({USER_Q}+272)]
    ldr q18, [sp, #({USER_Q}+288)]
    ldr q19, [sp, #({USER_Q}+304)]
    ldr q20, [sp, #({USER_Q}+320)]
    ldr q21, [sp, #({USER_Q}+336)]
    ldr q22, [sp, #({USER_Q}+352)]
    ldr q23, [sp, #({USER_Q}+368)]
    ldr q24, [sp, #({USER_Q}+384)]
    ldr q25, [sp, #({USER_Q}+400)]
    ldr q26, [sp, #({USER_Q}+416)]
    ldr q27, [sp, #({USER_Q}+432)]
    ldr q28, [sp, #({USER_Q}+448)]
    ldr q29, [sp, #({USER_Q}+464)]
    ldr q30, [sp, #({USER_Q}+480)]
    ldr q31, [sp, #({USER_Q}+496)]
    ldr x0, [sp, #{USER_GPRS}]
    ldr x1, [sp, #({USER_GPRS}+8)]
    ldr x2, [sp, #({USER_GPRS}+16)]
    ldr x3, [sp, #({USER_GPRS}+24)]
    ldr x4, [sp, #({USER_GPRS}+32)]
    ldr x5, [sp, #({USER_GPRS}+40)]
    ldr x6, [sp, #({USER_GPRS}+48)]
    ldr x7, [sp, #({USER_GPRS}+56)]
    ldr x8, [sp, #({USER_GPRS}+64)]
    ldr x9, [sp, #({USER_GPRS}+72)]
    ldr x10, [sp, #({USER_GPRS}+80)]
    ldr x11, [sp, #({USER_GPRS}+88)]
    ldr x12, [sp, #({USER_GPRS}+96)]
    ldr x13, [sp, #({USER_GPRS}+104)]
    ldr x14, [sp, #({USER_GPRS}+112)]
    ldr x15, [sp, #({USER_GPRS}+120)]
    ldr x16, [sp, #({USER_GPRS}+128)]
    ldr x17, [sp, #({USER_GPRS}+136)]
    ldr x18, [sp, #({USER_GPRS}+144)]
    ldr x19, [sp, #({USER_GPRS}+152)]
    ldr x20, [sp, #({USER_GPRS}+160)]
    ldr x21, [sp, #({USER_GPRS}+168)]
    ldr x22, [sp, #({USER_GPRS}+176)]
    ldr x23, [sp, #({USER_GPRS}+184)]
    ldr x24, [sp, #({USER_GPRS}+192)]
    ldr x25, [sp, #({USER_GPRS}+200)]
    ldr x26, [sp, #({USER_GPRS}+208)]
    ldr x27, [sp, #({USER_GPRS}+216)]
    ldr x28, [sp, #({USER_GPRS}+224)]
    ldr x29, [sp, #({USER_GPRS}+232)]
    ldr x30, [sp, #({USER_GPRS}+240)]
    eret
.size arm_process_slice,.-arm_process_slice

arm_process_sync:
    str x0, [sp, #{USER_GPRS}]
    str x1, [sp, #({USER_GPRS}+8)]
    str x2, [sp, #({USER_GPRS}+16)]
    str x3, [sp, #({USER_GPRS}+24)]
    str x4, [sp, #({USER_GPRS}+32)]
    str x5, [sp, #({USER_GPRS}+40)]
    str x6, [sp, #({USER_GPRS}+48)]
    str x7, [sp, #({USER_GPRS}+56)]
    str x8, [sp, #({USER_GPRS}+64)]
    str x9, [sp, #({USER_GPRS}+72)]
    str x10, [sp, #({USER_GPRS}+80)]
    str x11, [sp, #({USER_GPRS}+88)]
    str x12, [sp, #({USER_GPRS}+96)]
    str x13, [sp, #({USER_GPRS}+104)]
    str x14, [sp, #({USER_GPRS}+112)]
    str x15, [sp, #({USER_GPRS}+120)]
    str x16, [sp, #({USER_GPRS}+128)]
    str x17, [sp, #({USER_GPRS}+136)]
    str x18, [sp, #({USER_GPRS}+144)]
    str x19, [sp, #({USER_GPRS}+152)]
    str x20, [sp, #({USER_GPRS}+160)]
    str x21, [sp, #({USER_GPRS}+168)]
    str x22, [sp, #({USER_GPRS}+176)]
    str x23, [sp, #({USER_GPRS}+184)]
    str x24, [sp, #({USER_GPRS}+192)]
    str x25, [sp, #({USER_GPRS}+200)]
    str x26, [sp, #({USER_GPRS}+208)]
    str x27, [sp, #({USER_GPRS}+216)]
    str x28, [sp, #({USER_GPRS}+224)]
    str x29, [sp, #({USER_GPRS}+232)]
    str x30, [sp, #({USER_GPRS}+240)]
    mov x0, #1
    str x0, [sp, #{USER_KIND}]
    b .Lprocess_trap

arm_process_irq:
    str x0, [sp, #{USER_GPRS}]
    str x1, [sp, #({USER_GPRS}+8)]
    str x2, [sp, #({USER_GPRS}+16)]
    str x3, [sp, #({USER_GPRS}+24)]
    str x4, [sp, #({USER_GPRS}+32)]
    str x5, [sp, #({USER_GPRS}+40)]
    str x6, [sp, #({USER_GPRS}+48)]
    str x7, [sp, #({USER_GPRS}+56)]
    str x8, [sp, #({USER_GPRS}+64)]
    str x9, [sp, #({USER_GPRS}+72)]
    str x10, [sp, #({USER_GPRS}+80)]
    str x11, [sp, #({USER_GPRS}+88)]
    str x12, [sp, #({USER_GPRS}+96)]
    str x13, [sp, #({USER_GPRS}+104)]
    str x14, [sp, #({USER_GPRS}+112)]
    str x15, [sp, #({USER_GPRS}+120)]
    str x16, [sp, #({USER_GPRS}+128)]
    str x17, [sp, #({USER_GPRS}+136)]
    str x18, [sp, #({USER_GPRS}+144)]
    str x19, [sp, #({USER_GPRS}+152)]
    str x20, [sp, #({USER_GPRS}+160)]
    str x21, [sp, #({USER_GPRS}+168)]
    str x22, [sp, #({USER_GPRS}+176)]
    str x23, [sp, #({USER_GPRS}+184)]
    str x24, [sp, #({USER_GPRS}+192)]
    str x25, [sp, #({USER_GPRS}+200)]
    str x26, [sp, #({USER_GPRS}+208)]
    str x27, [sp, #({USER_GPRS}+216)]
    str x28, [sp, #({USER_GPRS}+224)]
    str x29, [sp, #({USER_GPRS}+232)]
    str x30, [sp, #({USER_GPRS}+240)]
    mov x0, #3
    str x0, [sp, #{USER_KIND}]
    b .Lprocess_trap
.Lprocess_trap:
    msr daifset, #15
    ldr x0, [sp, #{ACTIVE}]
    cmp x0, #1
    b.ne arm_process_kernel_fatal
    mrs x0, sp_el0
    str x0, [sp, #{USER_SP}]
    mrs x0, elr_el1
    str x0, [sp, #{USER_PC}]
    mrs x0, spsr_el1
    str x0, [sp, #{USER_PSTATE}]
    and x1, x0, #31
    cbnz x1, arm_process_kernel_fatal
    str q0, [sp, #{USER_Q}]
    str q1, [sp, #({USER_Q}+16)]
    str q2, [sp, #({USER_Q}+32)]
    str q3, [sp, #({USER_Q}+48)]
    str q4, [sp, #({USER_Q}+64)]
    str q5, [sp, #({USER_Q}+80)]
    str q6, [sp, #({USER_Q}+96)]
    str q7, [sp, #({USER_Q}+112)]
    str q8, [sp, #({USER_Q}+128)]
    str q9, [sp, #({USER_Q}+144)]
    str q10, [sp, #({USER_Q}+160)]
    str q11, [sp, #({USER_Q}+176)]
    str q12, [sp, #({USER_Q}+192)]
    str q13, [sp, #({USER_Q}+208)]
    str q14, [sp, #({USER_Q}+224)]
    str q15, [sp, #({USER_Q}+240)]
    str q16, [sp, #({USER_Q}+256)]
    str q17, [sp, #({USER_Q}+272)]
    str q18, [sp, #({USER_Q}+288)]
    str q19, [sp, #({USER_Q}+304)]
    str q20, [sp, #({USER_Q}+320)]
    str q21, [sp, #({USER_Q}+336)]
    str q22, [sp, #({USER_Q}+352)]
    str q23, [sp, #({USER_Q}+368)]
    str q24, [sp, #({USER_Q}+384)]
    str q25, [sp, #({USER_Q}+400)]
    str q26, [sp, #({USER_Q}+416)]
    str q27, [sp, #({USER_Q}+432)]
    str q28, [sp, #({USER_Q}+448)]
    str q29, [sp, #({USER_Q}+464)]
    str q30, [sp, #({USER_Q}+480)]
    str q31, [sp, #({USER_Q}+496)]
    mrs x0, fpcr
    str x0, [sp, #{USER_FPCR}]
    mrs x0, fpsr
    str x0, [sp, #{USER_FPSR}]
    mrs x0, tpidr_el0
    str x0, [sp, #{USER_TLS}]
    ldr x0, [sp, #{USER_KIND}]
    cmp x0, #3
    b.eq .Lprocess_irq_capture
    mrs x0, esr_el1
    str x0, [sp, #{USER_ESR}]
    lsr x1, x0, #26
    cmp x1, #0x15
    b.ne .Lprocess_fault
    tbz x0, #25, .Lprocess_fault
    and x2, x0, #0xffff
    cbnz x2, .Lprocess_fault
    b .Lprocess_quiesce
.Lprocess_fault:
    mov x2, #2
    str x2, [sp, #{USER_KIND}]
    cmp x1, #0x20
    b.eq .Lprocess_far
    cmp x1, #0x24
    b.ne .Lprocess_quiesce
.Lprocess_far:
    tbnz x0, #10, .Lprocess_quiesce
    mrs x2, far_el1
    str x2, [sp, #{USER_FAR}]
    mov x2, #1
    str x2, [sp, #{USER_FAR_VALID}]
    b .Lprocess_quiesce
.Lprocess_irq_capture:
    ldr x0, =0x08010000
    ldr w1, [x0, #0xc]
    str x1, [sp, #{USER_IAR}]
    and w2, w1, #0x3ff
    cmp w2, #30
    b.eq .Lprocess_timer_irq
    cmp w2, #1020
    b.lo arm_process_kernel_fatal
    mov x2, #4
    str x2, [sp, #{USER_KIND}]
    b .Lprocess_quiesce
.Lprocess_timer_irq:
    mrs x2, cntp_ctl_el0
    str x2, [sp, #{TIMER_STATUS}]
    tbz x2, #2, arm_process_kernel_fatal
    isb
    mrs x2, cntpct_el0
    str x2, [sp, #{TRAP_COUNT}]
    ldr x3, [sp, #{DEADLINE}]
    cmp x2, x3
    b.lo arm_process_kernel_fatal
.Lprocess_quiesce:
    ldr x0, [sp, #{USER_KIND}]
    str x0, [sp, #{RETURNED_KIND}]
    mov x0, #2
    msr cntp_ctl_el0, x0
    isb
    ldr x5, =0x08000000
    ldr x6, =0x08010000
    mov w7, #0x40000000
    str w7, [x5, #0x180]
    dsb sy
    ldr x0, [sp, #{USER_KIND}]
    cmp x0, #3
    b.ne .Lprocess_no_eoi
    ldr w0, [sp, #{USER_IAR}]
    str w0, [x6, #0x10]
    dsb sy
.Lprocess_no_eoi:
    str w7, [x5, #0x280]
    dsb sy
    ldr w0, [x5, #0x100]
    tbnz w0, #30, arm_process_kernel_fatal
    ldr w0, [x5, #0x200]
    tbnz w0, #30, arm_process_kernel_fatal
    ldr w0, [x5, #0x300]
    tbnz w0, #30, arm_process_kernel_fatal
    str wzr, [x6]
    str wzr, [x5]
    dsb sy
    ldr x7, [sp, #{GIC_GROUP}]
    str w7, [x5, #128]
    dsb sy
    ldr w8, [x5, #128]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GIC_CONFIG}]
    str w7, [x5, #3076]
    dsb sy
    ldr w8, [x5, #3076]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GIC_PRIORITY}]
    str w7, [x5, #1052]
    dsb sy
    ldr w8, [x5, #1052]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GIC_PMR}]
    str w7, [x6, #4]
    dsb sy
    ldr w8, [x6, #4]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GIC_BPR}]
    str w7, [x6, #8]
    dsb sy
    ldr w8, [x6, #8]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GICC_CTL}]
    str w7, [x6, #0]
    dsb sy
    ldr w8, [x6, #0]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{GICD_CTL}]
    str w7, [x5, #0]
    dsb sy
    ldr w8, [x5, #0]
    cmp w8, w7
    b.ne arm_process_kernel_fatal
    ldr w7, [x5, #0x100]
    ldr w8, [sp, #{GIC_ENABLE}]
    cmp w7, w8
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{K_CVAL}]
    msr cntp_cval_el0, x7
    ldr x7, [sp, #{K_CTL}]
    msr cntp_ctl_el0, x7
    isb
    mrs x8, cntp_ctl_el0
    and x8, x8, #3
    cmp x8, x7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{K_CVAL}]
    mrs x8, cntp_cval_el0
    cmp x8, x7
    b.ne arm_process_kernel_fatal
    ldr x7, [sp, #{K_TTBR0}]
    dsb sy
    tlbi vmalle1
    dsb sy
    isb
    msr ttbr0_el1, x7
    isb
    tlbi vmalle1
    dsb sy
    isb
    // Context destination may name the omitted owned arena: copy only now.
    ldr x1, [sp, #{K_CONTEXT}]
    mov x2, #0
    mov x16, sp
.Lprocess_copy_out:
    ldr x3, [x16, x2]
    str x3, [x1, x2]
    add x2, x2, #8
    cmp x2, #{USER_SIZE}
    b.lo .Lprocess_copy_out
    str xzr, [x16, #{ACTIVE}]
    ldr x3, [x16, #{FEATURES}]
    tbz x3, #0, .Lprocess_no_dit_restore
    ldr x17, [x16, #{K_DIT}]
    msr S3_3_C4_C2_5, x17
.Lprocess_no_dit_restore:
    tbz x3, #1, .Lprocess_no_ssbs_restore
    ldr x17, [x16, #{K_SSBS}]
    msr S3_3_C4_C2_6, x17
.Lprocess_no_ssbs_restore:
    isb
    ldr x17, [x16, #{K_PAR}]
    msr par_el1, x17
    ldr x17, [x16, #{K_SP0}]
    msr sp_el0, x17
    ldr x17, [x16, #{K_DAIF}]
    msr daif, x17
    ldr x17, [x16, #{K_ELR}]
    msr elr_el1, x17
    ldr x17, [x16, #{K_SPSR}]
    msr spsr_el1, x17
    ldr x17, [x16, #{K_ESR}]
    msr esr_el1, x17
    ldr x17, [x16, #{K_FAR}]
    msr far_el1, x17
    ldr x17, [x16, #{K_CNTKCTL}]
    msr cntkctl_el1, x17
    ldr x17, [x16, #{K_CPACR}]
    msr cpacr_el1, x17
    ldr x17, [x16, #{K_TLS}]
    msr tpidr_el0, x17
    ldr x17, [x16, #{K_TLSRO}]
    msr tpidrro_el0, x17
    ldr x17, [x16, #{K_FPCR}]
    msr fpcr, x17
    ldr x17, [x16, #{K_FPSR}]
    msr fpsr, x17
    isb
    ldr q8, [x16, #{K_Q}]
    ldr q9, [x16, #({K_Q}+16)]
    ldr q10, [x16, #({K_Q}+32)]
    ldr q11, [x16, #({K_Q}+48)]
    ldr q12, [x16, #({K_Q}+64)]
    ldr q13, [x16, #({K_Q}+80)]
    ldr q14, [x16, #({K_Q}+96)]
    ldr q15, [x16, #({K_Q}+112)]
    ldr x18, [x16, #{K_GPRS}]
    ldr x19, [x16, #({K_GPRS}+8)]
    ldr x20, [x16, #({K_GPRS}+16)]
    ldr x21, [x16, #({K_GPRS}+24)]
    ldr x22, [x16, #({K_GPRS}+32)]
    ldr x23, [x16, #({K_GPRS}+40)]
    ldr x24, [x16, #({K_GPRS}+48)]
    ldr x25, [x16, #({K_GPRS}+56)]
    ldr x26, [x16, #({K_GPRS}+64)]
    ldr x27, [x16, #({K_GPRS}+72)]
    ldr x28, [x16, #({K_GPRS}+80)]
    ldr x29, [x16, #({K_GPRS}+88)]
    ldr x30, [x16, #({K_GPRS}+96)]
    ldr x0, [x16, #{RETURNED_KIND}]
    ldr x17, [x16, #{K_SP}]
    mov sp, x17
    ldr x17, [x16, #{K_VBAR}]
    msr vbar_el1, x17
    isb
    ret

.global arm_process_kernel_fatal
arm_process_kernel_fatal:
    msr daifset, #15
    ldr x0, =.Lprocess_fatal_message
    mov x1, #0x09000000
.Lprocess_fatal_char:
    ldrb w2, [x0], #1
    cbz w2, .Lprocess_fatal_exit
    mov x3, #0xffff
.Lprocess_fatal_poll:
    ldr w4, [x1, #0x18]
    tbz w4, #5, .Lprocess_fatal_send
    subs x3, x3, #1
    b.ne .Lprocess_fatal_poll
    b .Lprocess_fatal_exit
.Lprocess_fatal_send:
    str w2, [x1]
    b .Lprocess_fatal_char
.Lprocess_fatal_exit:
    ldr x1, =.Lprocess_failure_block
    mov x0, #0x20
    hlt #0xf000
.Lprocess_terminal:
    wfe
    b .Lprocess_terminal
    .ltorg
.section .rodata.process,"a"
.Lprocess_fatal_message:
    .asciz "HMPROC KERNEL FAILURE\n"
.balign 8
.Lprocess_failure_block:
    .quad 0x20026, 1
