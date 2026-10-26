// SAFETY: Private caller retains audited roots and copies bounded buffers to
// checked common RW identity storage. CPU0 owns BUSY and exceptions are masked,
// peer is OFF. No changed mapping is dereferenced; no Rust runs until restored.
.section .text.arm_space_probe,"ax"
.balign 16
.global arm_space_probe
.type arm_space_probe, %function
arm_space_probe:
    mrs x5, ttbr0_el1
    mrs x6, par_el1
    mov x7, #0
.Lspace_pass:
    mov x8, #0
.Lspace_query:
    cmp x8, x3
    b.hs .Lspace_done_pass
    add x12, x1, x8, lsl #4
    ldp x9, x10, [x12]
    cmp x10, #0
    b.eq .Lspace_e0r
    cmp x10, #1
    b.eq .Lspace_e0w
    cmp x10, #2
    b.eq .Lspace_e1r
    at s1e1w, x9
    b .Lspace_result
.Lspace_e0r:
    at s1e0r, x9
    b .Lspace_result
.Lspace_e0w:
    at s1e0w, x9
    b .Lspace_result
.Lspace_e1r:
    at s1e1r, x9
.Lspace_result:
    isb
    mrs x11, par_el1
    add x13, x8, x8, lsl #1
    add x13, x2, x13, lsl #3
    str x11, [x13, x7, lsl #3]
    add x8, x8, #1
    b .Lspace_query
.Lspace_done_pass:
    cmp x7, #2
    b.eq .Lspace_finish
    cmp x7, #0
    csel x14, x0, x5, eq
    // Drain preceding observations/accesses and invalidate the previous root.
    dsb sy
    tlbi vmalle1
    dsb sy
    isb
    msr ttbr0_el1, x14
    isb
    // Includes global translations and refills made before root replacement.
    tlbi vmalle1
    dsb sy
    isb
    mrs x15, ttbr0_el1
    str x15, [x4, x7, lsl #3]
    add x7, x7, #1
    b .Lspace_pass
.Lspace_finish:
    msr par_el1, x6
    isb
    ret
.size arm_space_probe, .-arm_space_probe
