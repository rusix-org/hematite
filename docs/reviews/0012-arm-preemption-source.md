# ARM preemption source review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit. This reviewer runs no
builds or guests; execution evidence belongs to the integration record.

## Hardware checks

The implementation must finish each acknowledged timer transaction before
resuming arbitrary worker code. Preserve the entire IAR token, use the configured
combined priority-drop/deactivation mode, and write EOIR exactly once for a real
acknowledged interrupt. Spurious IDs must not become scheduling events.
Source disable/rearm must synchronize before pending-PPI cleanup or EOI.

Every task switch must discard the previous local exclusive reservation.
ERET supplies this for ordinary asynchronous return, but the handler-to-
coordinator path deliberately resumes without ERET and needs explicit CLREX.
The cooperative paths must preserve the same property.

IRQ entry must retain full machine state in task-private storage and restore
the emergency-stack top before scheduler Rust resumes. No execution test can
replace checking every assembly offset and final scratch-register restoration.
The register probe must exercise caller-clobbered registers and full vector
widths across actual interrupts, not ordinary ABI-preserving helper calls alone.

## Primary references

Consulted 2026-10-26. No Linux implementation source consulted or copied.

- [Arm GICv2 architecture specification, IHI 0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=),
  sections 3.2 and 4.4: interrupt lifecycle, valid IAR/EOIR pairing, spurious
  IDs and combined versus split deactivation.
- [Arm Generic Timer, 102379 version 1.4](https://documentation-service.arm.com/static/66c4754f32f35b31ceb317ff),
  sections 4.1 and 4.5: counter ordering, timer enable/mask conditions and
  level-sensitive source handling before interrupt completion.
- [Arm software synchronization primitives, 110478 version 1.0](https://documentation-service.arm.com/static/68c223238a337a2bc6645c0a),
  section 3.2.4 only: exclusive-monitor handling on ERET and CLREX. Its Linux
  implementation-example sections were not consulted.

## Source inspection

Inspected `preempt.rs`, `preempt-aarch64.S`, `preempt-probe-aarch64.S`,
`preempt_checks.rs`, shared thread lease/stack hooks, timer routing, allocator
proof comments, safe preemption tests and platform integration. No blocking
source finding remains in the inspected implementation after the corrections
below. This is a source verdict; the required runtime inventory remains a
separate gate and no test pass is inferred here.

The private image is 800 bytes with compile-time field offsets matching all
assembly saves/restores. The IRQ first copies the 31 saved GPRs below the old
256-byte scratch frame, before vector saves overwrite that consumed frame.
The complete image then lives in task-owned storage. Both real and spurious
paths restore the canonical emergency-stack top before leaving it. Real IRQ
returns to a freshly saved coordinator continuation; asynchronous task dispatch
restores the image through ERET with its pointer register restored last.
No Rust references to mutable task records span callback execution.

First entry uses a constructed private image with valid stack and mode before
IRQ becomes enabled. Explicit yield captures its prior mask, masks before
state mutation and restores that mask only after its exact continuation resumes.
Completion masks immediately after the callback adapter's normal return and
publishes DONE only then. The coordinator wipes stacks only after all callbacks
complete and timer/controller quiescence is checked. Busy rejects reentrant
runs before touching task storage, and shared execution authority excludes the
cooperative runner. Handler publication derives from the coordinator's bounded
task index; safe clients cannot construct its pointer or change status.

The timer path uses explicit Group0/level configuration, exact-token EOIR,
source synchronization before completion, and fresh compare programming at
every dispatch. Per-return cleanup checks pending and active PPI state. Setup
rejects unsupported state before mutation; configuration readback failures use
teardown before returning TimerUnavailable. Counter arithmetic is checked against
the supported minimum counter width; a later exhausted counter domain terminates
instead of silently wrapping an expired deadline.

Safe workers receive only a function pointer's integer input and a private
borrowed context token. They cannot capture or obtain the BootPlatform's
allocator/mapper capabilities; IRQ code does not call those services. Existing
Cell accounting therefore remains coordinator-confined in this scope. Mutex
guards may survive suspension; the IRQ path never acquires their locks or calls
policy/destructors. This does not establish allocator access from workers or SMP.

Review requested an independent initial SP value in the register probe, GIC
configuration readbacks with rollback, early counter-domain rejection and an
explicit bounded-pointer authority comment. Those changes are present. Integration
also found that directly reading SP_EL1 at EL1 is invalid; the revised handler
uses a memory-free SPSel switch to observe the interrupted SP, then immediately
returns to the emergency stack. That correction is included in this inspection.

The register workload uses complementary polling-register sets to cover all
GPRs, all 128-bit vector registers and FP/NZCV state while timer counts advance.
Its SP comparison now uses a separately saved initial value. Safe workloads
require peer progress without yielding, preserve stack locals/destructors, and
check report counts against event causes. The mutex fixture forces peers to run
while a nonyielding owner retains an inconsistent protected value. These are
appropriate test constructions; actual execution, negative delivery/overflow,
boundary-expiry coverage and regression results must be recorded separately.

## Host harness and image wiring review

Inspected the positive/negative preemption harnesses, their failure-injection
tests, Cargo feature wiring, boot selection and Makefile commands. The positive
inventory remains separate from the unchanged cooperative inventory and runs at
64/128 MiB. Missing/duplicate cases, failed exits, timeouts and IRQ/ack/EOI count
mismatches fail. The disabled-delivery image runs the same first nonyielding
workload and requires a bounded killed guest with no IRQ/fatal/normal completion;
the overflow image requires a single guard-address translation abort followed
by the fatal exit, with no resumed fault. Feature precedence keeps combined
Clippy builds from accidentally executing several negative paths.

The requested follow-ups are now present and inspected: every exception-return
record must match EL1 to EL1, with malformed/EL0 rejection tests; the no-timer
entry PC must equal the tested ELF's unique `arm_preempt_start` symbol, with
missing/wrong-symbol rejection tests. No outstanding harness finding remains.

The final positive images also run the complete existing 32-case inventory in
the same guest after scheduling. The host requires that ordered second report
and adds its 16 diagnostic IRQs to the scheduler's independently reported count.
Expected synchronous protection probes must have complete EL1 fault/vector/return
transactions; unexpected or nested exceptions and incomplete returns fail.
This extends post-scheduling checks to actual memory, exception, timer and
cooperative operations rather than only read-only control observations.

## Boundary-pressure refinement review

Reviewed the design refinement before implementation and then inspected the
separate-feature `preempt_boundary` macro. Its three sites have a valid RUNNING
task and published coordinator continuation. It saves x9–x12 and NZCV in an
aligned 48-byte worker-stack frame, masks IRQ before flag updates, and restores
registers/flags/SP before the labelled unmask window. Completion's x0 result,
yield arguments, LR and other live registers are untouched. It checks the
IRQ-enabled entry precondition rather than silently changing a masked caller's
state; other DAIF bits are preserved. One-shot flags prevent repeated forced
expiry on resumption. No source blocker found in this diagnostic refinement.

The boundary trace helper resolves the three window ranges from the tested ELF
and requires actual interrupted PCs in each. Its caller also requires the full
bounded IRQ transaction validation. Merely programming an expired comparator
cannot make a missing delivery pass. Runtime observations remain separate.
