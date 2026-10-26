# ARM synchronous fault recovery

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0007-arm-exceptions.md). Date: 2026-10-25.
Extends [ARM bootstrap](0002-arm64-bootstrap.md).
This is a fixed-site EL1 recovery mechanism and hardware protection test. It
is not general user-copy, demand paging, interrupt handling, EL0 isolation or
permission to resume arbitrary kernel faults.

## Entry and stack ownership

Retain the normal 64 KiB SP_EL1 stack. Add a separate 16 KiB exception stack
with its own 4 KiB guard page, entirely within the linker-owned image and the
existing 16 MiB mapping. Initialize SP_EL0 to its aligned top before installing
VBAR_EL1. Both stack guards are invalid leaves; include both in full mapping
checks. The allocator already excludes the complete image through
`__kernel_end`, which must now include this stack and guard.

The 2 KiB-aligned vector table retains sixteen 128-byte entries. Only current
EL using SP_ELx synchronous entry (offset 0x200) reaches the recovery handler.
Every other entry goes directly to the stack-free fatal path. IRQ/FIQ/SError
remain masked in this single-CPU profile. Lower-EL entries are not user entry
points and cannot resume execution.

The first handler instruction selects SP_EL0 before any memory access. Reserve
a 256-byte, 16-byte-aligned frame there and save x0 through x30 before using
any of them as scratch registers; the remaining eight bytes are padding.
SP_EL1 is never modified by the handler. No Rust call, allocator operation,
lock, FP/SIMD instruction or change to FPCR/FPSR occurs in this path.

A synchronous fault after the stack switch originates with SP_EL0 selected
and therefore reaches the current-EL/SP0 fatal vector instead of recursively
pushing another recovery frame. This also makes normal-stack exhaustion
independent of recovery entry storage. A corrupt emergency stack or unexpected
fault is fatal; no recovery claim is made for it. Keep the final return sequence
free of faulting memory accesses after restoring SP_EL0 and selecting SP_EL1.

## Recovery whitelist

A small immutable table or equivalent fixed comparisons in core assembly
bind each permitted fault instruction to a recovery label, expected fault
address and syndrome. No runtime caller can add entries or supply addresses.
For instruction abort, ELR is the attempted instruction address, not the BLR
that transferred control. All labels and targets belong to this image.

Require saved SPSR to describe AArch64 EL1h with DAIF masked. Match current-EL
exception class, 32-bit instruction length, exact fault status and relevant
read/write flags. Require valid FAR; reject external-abort, cache-maintenance
and stage-1-table-walk syndromes. Do not accept a broad class such as every
translation fault or every abort at an approximate PC range.

| Probe | Exact fault PC and FAR | Required class/status |
| --- | --- | --- |
| Null read | Dedicated LDR label; FAR zero | Current-EL Data Abort EC 0x25, IL=1, WnR=0, level-2 translation DFSC 0x06 |
| Normal guard read | Dedicated LDR label; FAR `__stack_guard` | EC 0x25, IL=1, WnR=0, level-3 translation DFSC 0x07 |
| Emergency guard read | Dedicated LDR label; FAR `__exception_stack_guard` | EC 0x25, IL=1, WnR=0, level-3 translation DFSC 0x07 |
| Read-only write | Dedicated STR label; FAR reserved assembly datum in rodata | EC 0x25, IL=1, WnR=1, level-3 permission DFSC 0x0f |
| NX execution | Dedicated rodata RET word is both ELR and FAR | Current-EL Instruction Abort EC 0x21, IL=1, level-3 permission IFSC 0x0f |

For Data Aborts, FnV, EA, CM and S1PTW must be zero. For Instruction Aborts,
FnV, EA and S1PTW must be zero. Ignore unrelated diagnostic ISS fields rather
than assuming one emulator's entire ESR value is universal. The exact levels
above follow the current fixed translation hierarchy; changing that hierarchy
requires updating this contract and tests, not broadening status acceptance.

On a complete match, write only the selected trusted recovery address to
ELR_EL1. Leave SPSR_EL1 intact. Restore all saved GPRs, restore the emergency
stack top, select SP_EL1 and ERET. The recovery label records success according
to its assembly function's ABI. Any unmatched PC, syndrome, FAR or saved mode
branches to the fatal path and cannot reach ERET.

## Probes and language boundary

Expose only fixed, bounded no-argument probe functions to Rust. Their unsafe
extern call sites explain stack/mapping/EL1 preconditions; safe policy receives
boolean observations, never arbitrary memory access. Assembly probes follow
the AAPCS64 call contract, including x19–x30 and required vector preservation.
The NX probe saves its caller's LR before BLR and restores it on both paths.
The null and guard probes read only; the RO-write datum is private assembly
storage, not Rust immutable data or live code. If protection unexpectedly
permits an operation, the probe returns failure rather than declaring recovery.

The NX target contains a valid RET instruction loaded with the image. If PXN
fails, it returns through the probe's normal failure path instead of executing
arbitrary data. No page-table mutation, permission relaxation or dynamically
written executable code is introduced.

A context probe seeds and checks representative caller/callee GPRs, NZCV,
FP/SIMD state and both stack pointers across a registered fault. Its checking
instructions must not destroy an observation before capturing it. Preserve
its own caller ABI independently of what the exception handler preserves.
The handler preserves FP/SIMD state by never touching it; future Rust calls,
FP use, scheduling or nested recoverable exceptions require a new context design.

## Fatal behavior and evidence

The runtime fatal vector emits exactly `HMEXCEPTION FATAL` as a complete serial
line using bounded, stack-free UART operations, then exits via the existing
semihost failure status 2. Early bootstrap validation may retain its existing
stack-free failure route without claiming initialized serial output.

Build two separate ARM negative-profile images after normal initialization.
The `unhandled-fault-test` profile enters nonreturning assembly, puts SP_EL1
inside `__stack_guard` and executes an unregistered BRK. Recovery entry must
still save context on its independent emergency stack before rejecting BRK.
The `nested-fault-test` profile sets SP_EL0 to
`__exception_stack_guard + 256`, retains the normal SP_EL1 and executes BRK.
The handler's 256-byte frame then lies in the emergency guard, so its first
store faults with SP_EL0 selected and must enter the SP0 fatal vector without
recursion. The SP0 synchronous fatal vector emits a distinct
`HMEXCEPTION NESTED` line before the common fatal line, so the host can verify
that the second fault actually reached that vector rather than merely seeing
outer BRK rejection. Neither assembly entry may return to Rust after corrupting a stack.
When both features are selected, choose the nested profile deterministically.

Require exactly one nested marker for the nested image and none for the
unregistered-fault image. For each image, the host requires the fatal line, exit 2, no
HMTEST records and completion within the deadline. An emulator launch
failure cannot count as a guest rejection. These establish both normal-stack
independence and failure containment for the emergency stack. All existing CPU-rejection, parser,
allocator, heap and monitor tests remain required on both architectures.

ARM guest checks must actually induce the five protected accesses and verify
recovery, exercise context preservation, repeat probes to establish emergency
stack reuse and retain full mapping checks including both guards. Test normal
64/128 MiB profiles and preserve exact source/ELF identities, commands, serial
logs and any exception traces. x86 reports the absence of recovery support
explicitly; existing x86 checks do not imply these faults were exercised there.
The core result has six boolean observations: `null_read`, `stack_guard`,
`exception_guard`, `read_only`, `no_execute` and `context`.

This provides a reviewed base for later fault-aware operations. Real user-copy
still needs address-space authority, mapping lifetime synchronization, fault
containment and partial-copy semantics; none follows from these fixed probes.

## Provenance

Consulted 2026-10-25; no Linux implementation source consulted or copied.

- [Arm AArch64 Exception Model, 102412_0103_02_en](https://documentation-service.arm.com/static/67ac57fb091bfc3e0a9479cc), sections 5.1–5.2:
  vector grouping, stack selection and exception return state. The architecture
  selects the target EL stack on entry; this design deliberately switches to
  its preinitialized emergency SP_EL0 before saving GPRs.
- [Arm ESR_EL1 register reference](https://support.arm.com/documentation/111107/2026-09/AArch64-Registers/ESR-EL1--Exception-Syndrome-Register--EL1-):
  exception class, instruction length and abort syndrome fields. The source is
  a register specification, not a kernel implementation or new toolchain pin.
- [AAPCS64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst):
  the existing compiled-code ABI remains binding on diagnostic assembly calls.
