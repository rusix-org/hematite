.section .text.arm_idle,"ax"
.balign 4
.global arm_idle_wfi, arm_idle_resume, arm_idle_end
.type arm_idle_wfi,%function
arm_idle_wfi:
    wfi
arm_idle_resume:
    isb
    mrs x0, cntpct_el0
    ret
arm_idle_end:
.size arm_idle_wfi, .-arm_idle_wfi
