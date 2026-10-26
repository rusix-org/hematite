# ARM scoped timer IRQ review

Date: 2026-10-26. Independent AI review of
[design 0009](../design/0009-arm-timer-irq.md); not human approval or a complete
soundness audit. Existing exception assembly, frame pool and mapping authority
were inspected alongside the primary architecture references below.

## Design review

Ready for implementation after the counter, whitelist and trace clarifications
below. No blocking design finding remains.

The scoped assembly window is a suitable first interrupt mechanism. IRQs never
interrupt Rust code, and the handler neither calls Rust nor accesses allocator,
mapping or caller-borrowed memory. Existing Cell-based ownership therefore
remains sequential. Earlier blanket masked-interrupt documentation must be
narrowed explicitly; this design does not permit future general callbacks or
preemption without another ownership/context review.

The fixed no-security GICv2 contract avoids ambiguous secure/non-secure group
views. The PPI is enabled only after explicit controller initialization and a
future physical-timer compare. A valid acknowledge is paired with its exact
EOI value after servicing the level source; special IDs are never EOI targets.
EOImode remains zero. Teardown disables the source before clearing pending
state, and observes active state rather than hiding an unmatched acknowledgment
with forced deactivation. No controller address or interrupt ID comes from safe
policy.

The IRQ entry must switch to the independent SP_EL0 stack before saving all
GPRs. The current-EL SPx IRQ vector alone gains this route; faults after switching
to SP0 remain fatal. No nested IRQ, FP/SIMD instruction, Rust call or scheduler
state exists in the handler. Keeping ELR/SPSR intact and restoring all saved
registers preserves interrupted state. The probe separately preserves its
public calling convention, and runtime context claims accurately exclude the
four scratch registers used by its polling loop.

The ELR whitelist must cover the first IRQ-mask instruction itself: an IRQ can
arrive immediately before that instruction executes. Every route out of the
window masks IRQ before entering ordinary teardown or returning to Rust.
Counter deadlines and iteration caps apply to both the delivery window and
the post-shutdown observation; a stopped counter must not hang either loop.
Near-wrap setup needs an explicit conservative bound compatible with the
architecture's minimum 56-bit counter width, rather than assuming 64-bit
wrapping subtraction describes every supported counter.

The amended design specifies frequency bounds, a 56-bit near-wrap guard, capped
delivery and quiet loops, the inclusive mask-instruction boundary and honest
trace scope. Compare/deadline additions must also remain bounded after a host
stall: validate before rearming rather than relying on elapsed code execution
always finishing within the nominal budget. The additional disabled-PPI entry
is valuable: it must reach the counter deadline with zero delivered IRQs and
complete teardown, distinguishing timeout handling from an untested fallback.

## Source review

Reviewed `timer-aarch64.S`, `timer.rs`, the IRQ vector change, GIC bootstrap
pages and complete mapping checks, narrowed frame/mapper constructor contracts,
and the safe policy timer case. No build or guest execution was performed by
this reviewer; runtime claims belong to integration evidence.

- The handler's first instruction selects SP_EL0 and all 31 GPRs are saved
  before scratch use. Its fixed 256-byte frame matches the restore sequence.
  SPSR requires EL1h with only IRQ unmasked, the private active flag is checked,
  and the ELR range includes the final mask instruction. ELR/SPSR are unchanged
  on return; no FP/SIMD or Rust operation occurs in the handler.
- The complete IAR value remains in a register that the rearm path does not
  overwrite. Each accepted ID30 is serviced and deasserted before exactly one
  EOI, with ordering before return. Special IDs receive no EOI; other real IDs
  terminate. Rearm additions revalidate the conservative counter bound, and a
  failed bound disables the source and communicates failure to the polling
  operation rather than continuing with a wrapped compare.
- The polling window uses non-flag-setting arithmetic and only x0-x3 as
  scratch. Context snapshots occur before comparisons; x4-x30, all Q registers,
  FP controls, NZCV and both stack pointers are checked. The independent caller
  ABI restore reloads x18-x30, q8-q15 and FP controls from the normal stack.
- Every post-initialization failure path reaches masked cleanup. Cleanup
  disables/masks the timer, disables the PPI/controllers and clears pending
  state; it checks active state rather than forcibly deactivating it. The
  post-shutdown observation has its own counter and iteration bounds. The
  withheld-PPI path distinguishes reaching the counter deadline from exhausting
  its iteration cap and additionally requires zero IRQs.
- Assembly-private state is never represented by Rust references. The safe
  test keeps a live exclusive mapped-byte borrow across both probe pairs and
  checks contents/accounting afterward. Synchronous fault checks run after the
  timer case to exercise restoration of the former exception contract.

One source/documentation correction was requested and verified fixed: GICv2 supports interrupt
grouping even without Security Extensions. CTLR=1 enables Group0, not an
architecture-wide sole group. The implementation now clears and reads back
GICD_IGROUPR0 bit30 while disabled, rejects a nonzero readback, and the design
wording describes Group0 accurately. No remaining source finding blocks this
increment. Passing integration evidence is still required independently.

## Primary references checked

- [Arm GICv2 architecture IHI0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=),
  sections 3.2 and 4.4: acknowledgment, deactivation and special IDs; section
  4.3: level-sensitive pending state and disabling before configuration.
- [Arm Generic Timer guide ARM062-1010708621-30](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Generic%20Timer.pdf?revision=c710e7a7-9f52-4901-8c9d-91b19f44f9c7),
  sections 2 and 3: minimum counter width, physical counter/timer registers,
  ordered counter observations and timer-output control.
- [QEMU virt documentation](https://www.qemu.org/docs/master/system/arm/virt.html):
  explicit GIC version selection and versioned platform scope. The fixed
  address/PPI contract also requires the pinned guest DTB evidence.

No Linux implementation source informed this review.
