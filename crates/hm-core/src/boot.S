// SAFETY: This entire entry shim executes only under the PVH boot contract.
// It owns linker-reserved BSS, tables and stack. No safe callers enter it.
// See docs/design/0001-bootstrap.md and its independent review.
.section .note.Xen,"a",@note
.balign 4
.long 4, 4, 18
.asciz "Xen"
.balign 4
.long start32

.section .text.boot,"ax"
.code32
.global start32
start32:
    cli
    cld
    mov esi, ebx                      // Preserve PVH start-info address.
    mov esp, offset __stack_top       // No stack use until BSS cleared.
    xor eax, eax
    mov edi, offset __bss_start
    mov ecx, offset __bss_end
    sub ecx, edi
    shr ecx, 2
    rep stosd

    xor eax, eax
    cpuid
    cmp eax, 7
    jb boot_fail
    mov eax, 1
    cpuid
    and edx, 0x07000000               // FXSR, SSE, SSE2.
    cmp edx, 0x07000000
    jne boot_fail
    mov eax, 7
    xor ecx, ecx
    cpuid
    and ebx, 0x00100080               // SMEP and SMAP.
    cmp ebx, 0x00100080
    jne boot_fail
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000001
    jb boot_fail
    mov eax, 0x80000001
    cpuid
    and edx, 0x20100000               // Long mode and NX.
    cmp edx, 0x20100000
    jne boot_fail

    mov eax, offset pdpt
    or eax, 3
    mov dword ptr [pml4], eax
    mov eax, offset page_directory
    or eax, 3
    mov dword ptr [pdpt], eax
    mov edi, offset page_directory
    mov eax, offset page_tables
    or eax, 3
    mov ecx, 8                        // Eight PTs map 16 MiB in 4 KiB pages.
1:
    mov [edi], eax
    add eax, 4096
    add edi, 8
    loop 1b

    mov edi, offset page_tables
    xor ecx, ecx
2:
    mov eax, ecx
    shl eax, 12
    test eax, eax
    jz 5f                            // Null page stays absent.
    cmp eax, offset __stack_guard
    je 5f
    mov edx, 0x80000000               // NX high dword by default.
    cmp eax, offset __text_start
    jb 4f
    cmp eax, offset __text_end
    jae 3f
    xor edx, edx                     // Text: RX, supervisor, no write.
    or eax, 1
    jmp 6f
3:
    cmp eax, offset __rodata_start
    jb 4f
    cmp eax, offset __rodata_end
    jae 4f
    or eax, 1                        // Rodata: R/NX.
    jmp 6f
4:
    or eax, 3                        // Other RAM: RW/NX.
    jmp 6f
5:
    xor eax, eax
    xor edx, edx
6:
    mov [edi], eax
    mov [edi + 4], edx
    add edi, 8
    inc ecx
    cmp ecx, 4096
    jb 2b

    lgdt [gdt_pointer]
    mov eax, 0x00300620               // PAE, OSFXSR, OSXMMEXCPT, SMEP, SMAP.
    mov cr4, eax
    mov eax, offset pml4
    mov cr3, eax
    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x900                    // LME and NXE.
    wrmsr
    mov eax, cr0
    and eax, 0xfffffff3              // Clear EM/TS.
    or eax, 0x80010023               // Paging, WP, PE, MP.
    mov cr0, eax
    .byte 0xea                       // Far jump to 64-bit code selector.
    .long long_mode
    .word 8

boot_fail:
    mov dx, 0xf4
    mov eax, 0x11
    out dx, eax
7:
    cli
    hlt
    jmp 7b

.code64
long_mode:
    mov ax, 16
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    lea rsp, [rip + __stack_top]
    xor ebp, ebp
    clac
    fninit
    ldmxcsr [rip + initial_mxcsr]
    mov edi, esi
    call {rust_entry}
    ud2

.section .rodata.boot,"a"
.balign 8
gdt:
    .quad 0
    .quad 0x00af9b000000ffff          // Accessed bit pre-set: no CPU write to RO GDT.
    .quad 0x00cf93000000ffff
gdt_pointer:
    .word 23
    .long gdt
initial_mxcsr:
    .long 0x1f80

.section .bss.tables,"aw",@nobits
.balign 4096
.global pml4, pdpt, page_directory, page_tables
pml4: .skip 4096
pdpt: .skip 4096
page_directory: .skip 4096
page_tables: .skip 32768
