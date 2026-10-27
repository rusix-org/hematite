.arch armv8.1-a
.section .text.entry,"ax"
.global native_start
.type native_start,%function
native_start:
    mov x1, #{task}
    bl task_main
    b .
.size native_start,.-native_start
.section .text,"ax"
.macro three_argument name, number, label
.global \name
.type \name,%function
\name:
    mov x3, x2
    mov x2, x1
    mov x1, x0
    mov x0, #1
    mov x8, #\number
.global \label
\label:
    svc #0
    ret
.size \name,.-\name
.endm
three_argument call_write,1,write_svc
three_argument call_spawn,3,spawn_svc
.global call_wait
.type call_wait,%function
call_wait:
    mov x2, x1
    mov x1, x0
    mov x3, #0
    mov x0, #1
    mov x8, #4
.global wait_svc
wait_svc:
    svc #0
    ret
.size call_wait,.-call_wait
.macro one_argument name, number, label
.global \name
.type \name,%function
\name:
    mov x1, x0
    mov x2, #0
    mov x3, #0
    mov x0, #1
    mov x8, #\number
.global \label
\label:
    svc #0
    ret
.size \name,.-\name
.endm
one_argument call_cancel,5,cancel_svc
one_argument call_exit,2,exit_svc
.global worker_yield_four
.type worker_yield_four,%function
worker_yield_four:
    stp x19, x30, [sp, #-16]!
    mov x19, x0
.macro worker_yield label
    mov x0, #1
    mov x1, x19
    mov x2, #-1
    mov x3, #0
    mov x8, #1
.global \label
\label:
    svc #0
    cbnz x0, .Lyield_failed
    cbnz x1, .Lyield_failed
.endm
worker_yield worker_yield_1
worker_yield worker_yield_2
worker_yield worker_yield_3
worker_yield worker_yield_4
.global worker_yields_done
worker_yields_done:
    mov x0, #1
    ldp x19, x30, [sp], #16
    ret
.Lyield_failed:
    mov x0, #0
    ldp x19, x30, [sp], #16
    ret
.size worker_yield_four,.-worker_yield_four
.global copy_job
.type copy_job,%function
copy_job:
    cbz x2, .Lcopy_done
.global worker_copy_begin
worker_copy_begin:
    ldrb w3, [x0], #1
    strb w3, [x1], #1
    subs x2, x2, #1
    b.ne worker_copy_begin
.Lcopy_done:
    ret
.size copy_job,.-copy_job
.global overwrite_job
.type overwrite_job,%function
overwrite_job:
    cbz x1, .Loverwrite_done
    mov w2, #0xa5
.global parent_overwrite
parent_overwrite:
    strb w2, [x0], #1
    subs x1, x1, #1
    b.ne parent_overwrite
.Loverwrite_done:
    ret
.size overwrite_job,.-overwrite_job
.global run_loop
.type run_loop,%function
run_loop:
    msr nzcv, xzr
.global loop_start
loop_start:
    b loop_start
.global loop_end
loop_end:
.size run_loop,.-run_loop
