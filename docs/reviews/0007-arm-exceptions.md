# ARM exception recovery review

Date: 2026-10-25. Independent AI review of
[design 0007](../design/0007-arm-exceptions.md); not human approval or a
soundness certification. No builds or guest execution performed by reviewer.

## Design review

Ready for implementation. The contract gives recovery only to exact registered
instructions with the expected current-EL abort class, instruction length,
fault status, access direction and valid fault address. Saved mode must be
EL1h with masked DAIF. All mismatches terminate rather than returning through
an uncontrolled PC or state. The fixed mapping geometry justifies exact level
checks: null translation at level 2, guards at level 3, and permission faults
at level 3 for the read-only and NX targets.

The separate SP_EL0 stack is appropriate for this declared single-CPU kernel.
Architectural exception entry initially selects SP_EL1; the first handler
instruction switches to the preinitialized SP_EL0 before any memory access.
A fault after that switch originates in the current-EL/SP0 vector group, which
must remain stack-free and fatal. Saving all GPRs there leaves normal SP_EL1
untouched. Returning restores the emergency stack, selects SP_EL1 and uses
ERET with original SPSR and a trusted fixup ELR. The handler must contain no
SIMD, FP or Rust calls; that restriction preserves vector/FP state without
adding it to the exception frame.

The RO-write target must remain private mutable assembly storage, not a Rust
immutable object or live code. The NX target's valid RET instruction makes
unexpectedly permitted execution return a failed test safely. The NX probe
must separately preserve its caller LR across BLR. Context tests must capture
NZCV and GPRs before using them as comparison scratch, and restore their own
AAPCS state, including callee-saved vectors and FP control/status.

The two negative images are valuable independent checks: an unregistered BRK
with SP_EL1 inside the normal guard proves entry does not rely on that stack;
placing SP_EL0 so the first context store faults proves SP0 nested routing
reaches the fatal path. Both entry functions must be nonreturning and the
fatal path must never use a stack or call Rust. Require exact guest marker,
exit status and no successful inventory, not merely a nonzero emulator exit.
The nested profile must additionally emit exactly one `HMEXCEPTION NESTED`
from vector zero; the unknown-BRK profile must emit none. A shared fatal
marker alone would not distinguish successful SP0 routing from outer rejection.

## Source review

Reviewed `exceptions-aarch64.S`, `exceptions.rs`, the ARM boot/linker and
mapping changes, and fault-test integration. No blocking source finding.

- The 16 vector slots have the required 128-byte spacing. SPx synchronous
  entry selects SP0 before its first memory access; SP0 synchronous entry
  emits the distinct nested marker without stack use.
- The 256-byte frame saves/restores x0 through x30 at matching offsets. The
  six-entry table is bounded; `0xfe0007ff` checks EC, IL and relevant abort
  flags/status, while `0x3df`/`0x3c5` requires masked AArch64 EL1h. Recovery
  writes only the trusted ELR and leaves SPSR intact.
- The handler contains no SIMD/FP instruction or Rust call. The 1072-byte
  context fixture snapshots all GPRs before scratch use, captures NZCV before
  comparisons, verifies all 32 vectors and FP controls plus both stack
  pointers, and restores its caller's callee-saved GPR/vector and FP state.
  The NX wrapper preserves LR independently across BLR.
- Both guards are excluded from the leaf mappings. Emergency stack storage
  remains within the reserved image, and SP_EL0 is initialized before VBAR.
  No normal-stack access is introduced in recovery entry or fatal reporting.
- Negative entries are private, argument-free and nonreturning. UART polling
  is bounded; semihost failure exit uses no stack. The host requires exactly
  one fatal marker, exit 2, no HMTEST, successful launch and no timeout, plus
  the appropriate presence/absence of the SP0 nested marker.

This review is source-level evidence, not a build or runtime result. Runtime
claims belong to integration evidence, including the retained QEMU exception
trace. This is not general user-copy or resumable arbitrary kernel fault
handling. The whitelist deliberately does not test every possible wrong
syndrome dynamically; mismatch rejection is established here by source review.

## References checked

- [Arm Exception model, version 1.0](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Exception%20model.pdf?revision=a62f2bf2-b08a-4a4f-8cbe-38c67ddf4434):
  software saves GPRs, hardware records PSTATE/return PC, ERET restores state,
  and the initial exception stack is the target EL's stack.
- [Arm Cortex-A520 TRM](https://documentation-service.arm.com/static/672e33eac7fc0d1f211d8fd9):
  architectural ESR fault-status encodings for translation and permission levels.

Only architecture specifications were consulted; no Linux implementation was
used or copied. The source is not a change to the pinned toolchain or guest.
