# ARM scoped timer interrupt delivery

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0009-arm-timer-irq.md).
Date: 2026-10-26. Extends [exception entry](0007-arm-exceptions.md).

Implement actual ARM generic-timer IRQ delivery as the interrupt foundation for
later scheduling. This increment deliberately enables IRQ only inside a bounded
assembly operation. All Rust execution remains DAIF-masked; no allocator,
mapping operation, callback, context switch or scheduler runs from an IRQ.
It is not preemption or SMP support.

## Fixed platform and mappings

Pin QEMU to `virt,virtualization=off,secure=off,gic-version=2`, one CPU, existing
EL1 direct-ELF boot. Capture the pinned QEMU DTB to verify GIC distributor
`0x08000000`, CPU interface `0x08010000` and non-secure physical timer PPI 14
(architectural interrupt ID 30). Runtime support remains this fixed profile,
not arbitrary firmware discovery. Check GICD_TYPER.SecurityExtn is zero and
GICC_IIDR reports GICv2 before enabling anything. An unsupported controller
returns failed observations without enabling IRQ.

Add one permanent device L3 table under device L2 index 64. Map only its leaf 0
and leaf 16, covering the first 4 KiB of GICD/GICC respectively, as EL1 RW,
Device-nGnRnE, PXN/UXN. Their accessed registers all fit those pages. Keep every
other leaf invalid, preserve the UART table at L2 index 72, and extend full
BootChecks accordingly. The table remains inside the linker-reserved image.
No public API accepts MMIO addresses, interrupt IDs or arbitrary callbacks.

## GIC and timer ownership

The core exclusively owns this controller and its timer in the fixed profile.
Initialization and teardown occur with DAIF masked. Disable distributor and CPU
interface first. Read the implemented interrupt-bank count from GICD_TYPER;
bound it by the architectural register area and disable/clear pending interrupts
in every implemented bank. SGIs that cannot be disabled are never generated.
Do not rely on reset enable bits. PPI30 is configured level-sensitive, priority
0x80; no SPI targeting is needed for a banked PPI. Explicitly clear and read
back GICD_IGROUPR0 bit30 while disabled to establish Group0 routing. Set GICC_PMR=0xff,
GICC_BPR=0 and GICC_CTLR=1 (EOImode=0). With SecurityExtn=0, GICD_CTLR=1 enables
Group0. Interrupt grouping can exist without Security Extensions. Do not
assume non-secure Group1 behavior on a
security-enabled controller; that controller is outside this profile.

Use CNTPCT_EL0/CNTFRQ_EL0 and the EL1 physical timer CNTP_CVAL_EL0/CNTP_CTL_EL0.
Reject zero or unreasonable frequency before arithmetic. Use a fixed interval
derived from frequency (about 1 ms), minimum one counter tick, and request
eight interrupts. All state used by IRQ (remaining count, delivered count,
interval, active flag) lives in private assembly storage; Rust never forms a
reference to it. Reset and inspect it only while IRQ is masked. Counter reads
that order observations use ISB. Timer deadline addition and timeout arithmetic
must have explicit wrapping/bounded semantics. Accept frequency in
`1000..=1_000_000_000` Hz and initial counter at most `(1<<56)-1 - frequency`.
Validate the same 56-bit bound before every later compare/quiet-deadline
addition; a stalled host may advance counter time arbitrarily. Invalid rearm
sets a private failure flag and disables the timer before acknowledgment;
invalid quiet-deadline construction fails the observation with IRQ masked.
Reject near-wrap setup rather than making a long-lived clock claim.

Enable only PPI30 in GICD_ISENABLER0. Program a future physical compare value,
set CNTP_CTL_EL0 ENABLE=1/IMASK=0, synchronize, then unmask IRQ inside assembly.
FIQ, SError and debug remain masked. The assembly polling window checks the
delivered count and a counter-based deadline (at most one second), plus a fixed
iteration cap for a broken/stopped counter. It must not use unbounded WFI.

## IRQ entry and return

Only vector offset 0x280 (current EL, SPx, IRQ) routes to the new handler. Its
first instruction selects SP_EL0 and then saves all x0-x30 in the existing
256-byte emergency frame, before clobbering any general register. It makes no
Rust/FP/SIMD call or instruction. SP_EL1 is untouched. The hardware masks IRQ
on exception entry and this handler never unmasks it. A fault with SP0 selected
continues to use the existing fatal path; no nested recovery is attempted.

Require saved SPSR masked mode/DAIF bits to describe EL1h with only IRQ unmasked
(`SPSR & 0x3df == 0x345`), a live private active flag, and saved ELR inside the
fixed polling window, including the address of its final IRQ-mask instruction
(an IRQ may arrive immediately before that instruction). The range ends after
the mask instruction. This bounds the new asynchronous entry contract. Leave
ELR/SPSR unchanged on normal return, restore all GPRs and SP_EL0, select SP1,
and ERET. Existing synchronous recovery still requires all DAIF masked.

Read GICC_IAR once and retain its complete value. Accept timer ID30; IDs
1020..1023 are special/spurious and must not receive EOIR. A spurious IRQ may
return without counting progress. Every other real ID is fatal. Timer handling
validates that its count is below eight and the timer condition is asserted,
increments once, then reprograms a future compare value for another tick or
disables/masks the timer after the eighth. Synchronize source deassertion before
writing the exact acknowledged IAR value to GICC_EOIR. With EOImode=0 this both
drops priority and deactivates. Complete MMIO ordering with DSB before ERET.

The handler cannot access pool/mapper Cell state or borrowed Rust data. Narrow
the earlier ownership documentation to the actual maintained condition:
all Rust operations remain masked, and the only scoped asynchronous handler
touches its private timer/controller state. This does not authorize general
interrupt callbacks, reentrancy, concurrent allocation or mapping updates.

## Context observation and teardown

The probe is AAPCS64-correct, saving/restoring its own callee-preserved registers
and FP controls. Seed x4-x30 and all 32 Q registers with distinct known patterns;
the wait loop uses x0-x3 only. Seed NZCV and avoid flag-setting instructions in
the polling loop (CBZ/TBZ and non-S subtraction suffice). Capture NZCV before
comparison. Check x4-x30, Q0-Q31, FPCR/FPSR and both stack pointers after IRQ
delivery. Do not claim a runtime sentinel test of x0-x3: those are polling
scratch. Their preservation is covered by reviewed handler save/restore.

Every nonfatal exit, including timeout and context failure, masks IRQ before
leaving the polling region, masks/disables CNTP, disables PPI30, disables GICC
and GICD, clears PPI pending state after deassertion and synchronizes. Require
PPI30 inactive and not pending, timer disabled/masked, controller disabled,
DAIF restored and emergency SP at its original top before returning to Rust.
Never clear a genuinely active interrupt as a substitute for matching IAR/EOIR.
Keep an unexpected active state as failure. Reset the private active flag last.

After shutdown, a counter wait with its own fixed iteration cap and IRQ still masked must show the
delivered count unchanged; then inspect pending/active state again. Repeat the
entire operation to establish that teardown permits reuse without an old tick.
Also expose a second private fixed assembly entry that withholds PPI enable but
otherwise exercises the same timer/window/cleanup path. Require zero delivered
IRQs, reaching the counter deadline (rather than success or the iteration cap),
and complete quiescence. This deliberately tests the no-delivery timeout route.
Use a 100 ms deadline for both entries, with a separate fixed iteration cap;
the normal eight 1 ms ticks should complete comfortably inside that bound.

The safe wrapper returns `TimerChecks { delivery, context, quiescent, timeout }`, each
a boolean observation, through `Platform::timer_checks() -> Option<TimerChecks>`.
The timeout observation is true only when the disabled-delivery entry reaches
its expected deadline with zero ticks and clean teardown. Policy repeats the
pair twice. No arbitrary durations or user addresses
enter this API. x86 returns None explicitly.

## Verification and limits

Add a strict timer contract case requiring all four observations, both calls,
and unchanged frame/mapping counts plus protection checks. Existing ARM fault
tests must still pass after timer shutdown, proving DAIF and stack restoration.
Run the full ARM/x86 suites, ARM64/128 MiB monitor profiles and repeated `test`
commands. Keep QEMU interrupt traces: actual IRQ entries at vector offset 0x280
must agree with guest observations. `-d int` alone does not show acknowledged
interrupt IDs; enable the relevant GIC trace event if separately asserting eight
acknowledgments per probe. Missing delivery must fail by a bounded guest timeout or existing
host deadline, never silently pass. The poll loop is a diagnostic, not an idle
strategy or performance measurement.

This provides an interrupt entry, timer rearming and acknowledgment mechanism
for subsequent thread scheduling. Enabling IRQ during Rust, saving/restoring
threads, general handlers or SMP requires another reviewed ownership/context
design. Native hardware, timer precision and latency remain unmeasured.

## Primary sources

Consulted 2026-10-26. No Linux implementation source used or copied.

- [Arm GICv2 architecture, IHI0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=):
  register interfaces, SecurityExtn, level interrupts, acknowledgment and EOI.
- [Arm Generic Timer guide, ARM062-1010708621-30](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Generic%20Timer.pdf?revision=c710e7a7-9f52-4901-8c9d-91b19f44f9c7):
  physical counter, frequency and timer controls, ordering counter reads.
- [QEMU virt documentation](https://www.qemu.org/docs/master/system/arm/virt):
  explicit gic-version=2 selection. Exact geometry is checked against the pinned
  QEMU7.2.22 DTB rather than inferred from moving documentation.
- Exception entry and AAPCS64 primary references are recorded in
  [design0007](0007-arm-exceptions.md).
