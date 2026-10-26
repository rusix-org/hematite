.arch armv8.1-a
.section .text.entry,"ax"
.global native_start
.type native_start,%function
native_start:
    mov x1, #{app}
    bl app_main
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

.global deliberate_fault
.type deliberate_fault,%function
deliberate_fault:
    cmp x0, #2
    b.ne loop_start
    adr x9, native_start
.global fault
fault:
    str xzr, [x9]
    b .
.global loop_start
loop_start:
    b loop_start
.global loop_end
loop_end:
.size deliberate_fault,.-deliberate_fault
