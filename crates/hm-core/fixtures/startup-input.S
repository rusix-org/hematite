.arch armv8.1-a
.section .text.entry,"ax"
.global native_start
.type native_start,%function
native_start:
    mov x1, #{probe}
    bl probe_main
    b .
.size native_start,.-native_start

.section .text,"ax"
.global call_write
.type call_write,%function
call_write:
    mov x3, x2
    mov x2, x1
    mov x1, x0
    mov x0, #1
    mov x8, #1
.global write_svc
write_svc:
    svc #0
    ret
.size call_write,.-call_write

.global call_newline
.type call_newline,%function
call_newline:
    mov x1, x0
    adr x2, newline
    mov x3, #1
    mov x0, #1
    mov x8, #1
.global newline_svc
newline_svc:
    svc #0
    ret
.size call_newline,.-call_newline

.global call_exit
.type call_exit,%function
call_exit:
    mov x1, x0
    mov x0, #1
    mov x2, #0
    mov x3, #0
    mov x8, #2
.global exit_svc
exit_svc:
    svc #0
    ret
.size call_exit,.-call_exit

.global probe_end
.type probe_end,%function
probe_end:
    mov x9, #0x58
    movk x9, #0x10, lsl #16
    movk x9, #1, lsl #32
    cmp x0, #5
    b.eq write_payload
    cmp x0, #6
    b.eq execute_payload
    msr nzcv, xzr
    b loop_start
write_payload:
    msr nzcv, xzr
.global fault_write
fault_write:
    strb wzr, [x9]
    b .
execute_payload:
    msr nzcv, xzr
.global branch_payload
branch_payload:
    br x9
.global loop_start
loop_start:
    b loop_start
.global loop_end
loop_end:
.size probe_end,.-probe_end
