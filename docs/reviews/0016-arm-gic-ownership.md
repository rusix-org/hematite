# ARM GIC ownership review

Date: 2026-10-26. Independent AI design/source review, not human approval or a
complete soundness certification. Reviewer performs no builds or guest runs.

## Design verdict

Reviewed [design 0016](../design/0016-arm-gic-ownership.md) before runtime edits.
**Ready for the scoped single-CPU implementation.**

The global session owns distributor forwarding and shared SPI bank mutation
under BUSY with CPU1 off. Local setup/stop own GICC, CNTP and banked PPI30
configuration; they must not disable forwarding or clear shared SPI state.
No operation clears active state to manufacture quiescence. CPU0/TPIDR and
masked-coordinator checks protect session admission. Local setup explicitly
rejects other enabled PPIs and unexpected pending/active bank0 state without
requiring read-only enabled SGI bits to be zero.

Requested diagnostic strengthening is present: seed pending SPI32 only after
validating that a SPI bank exists and the interrupt is disabled, then require
its pending state and global forwarding to survive two local setup/stop cycles.
Disabled SPI32 cannot deliver an IRQ; global cleanup removes the fixture.
Readback of forwarding alone would miss accidental all-bank pending clears.

Setup failure before mutation returns without losing the lease contract.
After global start, local failure must quiesce local state before global stop;
cleanup failure is fatal, never authority to reclaim live contexts. Implementation
review must check register liveness, bounded bank loops, rollback before full
publication, and normal callback completion before teardown/reclamation.

The existing timer diagnostics retain whole-GIC ownership on the sole masked
CPU0 coordinator. They are explicitly excluded from future concurrent sessions.
Private secondary stacks, startup, mappings and join/OFF authority remain open
requirements; this design does not make the kernel SMP-ready.

## Source verdict

Reviewed the implementation against parent `1a12080`: session/local assembly,
Rust session lifetime and diagnostic, platform hook and strict test inventory.
**No blocking source finding remains for this single-CPU prerequisite.**

- Shared start checks masked execution, inactive software publications, GIC
  profile, disabled interfaces, unexpected bank0 state and every SPI active
  bank before mutation. The SPI loop count is the bounded nonzero
  ITLinesNumber; loops begin at bank1 and stay within the mapped distributor
  page. Only session operations write shared forwarding or SPI banks in the
  preemption implementation. The separate exclusive timer diagnostic remains
  the documented exception.
- Local setup preserves the original interval register and changes only GICC,
  CNTP and banked PPI30 group/config/priority/enable/pending state. No shared
  bank reset remains. Local stop checks the timer interrupt is inactive,
  nonpending and disabled, disables GICC, and requires shared forwarding still
  enabled before withdrawing local publication. Existing acknowledgment/EOI
  handling remains the only active interrupt completion path.
- Ordinary session-start rejection precedes mutation. Setup rollback calls
  local stop only; the Rust owner then performs idempotent local cleanup and
  global stop before releasing BUSY. Cleanup requires no successfully
  initialized interval or task pointer. Hardware quiescence/readback failures
  terminate instead of returning with uncertain authority. Successful work
  completes callbacks before local/global teardown, stack clearing and lease
  release. Empty work remains free of controller changes.
- New assembly entries use caller-saved registers; nested cleanup preserves
  LR with an aligned frame. IRQ save/restore, sentinel register tests and
  context image paths are unchanged. Session-stop bank loops rely on the
  previously admitted immutable GIC profile and verify disabled, nonpending,
  inactive SPI state after cleanup.
- The diagnostic requires validated CPU0, masked DAIF and BUSY. It seeds only
  disabled SPI32, confirms pending readback, performs two local lifecycle
  cycles, and requires both pending SPI32 and forwarding to survive each.
  A duplicate setup must reject during the first setup's active publication;
  source inspection confirms that branch precedes mutation. Local cleanup is
  unconditional even when a diagnostic assertion fails. Global cleanup removes
  the fixture before releasing the lease. No task or IRQ is dispatched by it.

The diagnostic verifies control/ownership separation, not actual cross-CPU
interrupt delivery. Full regression execution, compiler checks and exact
timer/preemption traces remain the integrating agent's verification gate.
No build or guest result was produced by this reviewer. CPU1 must remain off;
the unchanged timer diagnostic and absent secondary lifetime/stack machinery
preclude any SMP-ready claim.
