# ARM CPU-local state review

Date: 2026-10-26. Independent AI review; not human approval or a complete
soundness certification. Reviewer performed no builds or guest execution.

## Design verdict

Reviewed [design 0015](../design/0015-arm-cpu-local-state.md) before runtime
implementation. **Ready for the scoped single-CPU implementation.**

The proposed 128-byte record has nonoverlapping timer, preemption, counter,
diagnostic and identity fields. Rust layout assertions and assembly constant
substitution must bind every offset to that layout. Boot rejects nonzero full
MPIDR affinity before shared stack/BSS access, then installs the exact trusted
record base after BSS initialization and before any IRQ can be enabled.
Private Rust validation checks the base before dereferencing it; observations
mask IRQ and restore the caller's DAIF. Runner checks CPU association before
either execution lease and retains its non-Send/non-Sync marker.

The inactive record is initialized to nonzero canaries and checked after actual
timer/preemption mutations. Bad-base checks exercise the validation predicate
without installing an invalid architectural base. These checks establish
single-CPU address separation only. Existing actual-type compiler probes must
continue rejecting Runner transfer/sharing and construction by safe clients.

Source review requirements: all 31 interrupted GPRs must be saved before
TPIDR routing scratch use; every counter/sentinel path must select the local
record; no plain live remote observations are permitted. The final integration
must put canary checks after real IRQ activity, including the boundary image.

Global GIC distributor setup/stop, BUSY, stacks, allocator and mappings remain
single-CPU mechanisms. This change cannot authorize CPU1 activation or support
an SMP execution claim. The broader requirements in design 0014 remain open.

## Source verdict

Reviewed the implementation against parent `0f5492d`, including `cpu_local.rs`,
boot, timer, preemption and sentinel assembly, Runner boundaries, platform hooks
and the actual-type compiler harness integration. **No blocking source finding
remains for this single-CPU prerequisite.** This is source approval only;
build, compiler probes and guest evidence remain the integrating agent's gate.

- The affinity mask includes Aff0, Aff1, Aff2 and Aff3 while excluding other
  MPIDR bits. Boot checks it before selecting shared stacks, uses a stack-free
  fatal path on rejection, and sets TPIDR after clearing BSS. No other code
  writes TPIDR and no secondary startup path was added.
- The two records are 64-byte aligned with asserted 128-byte stride. Every
  assembly base replacement uses the original scratch register and asserted
  field offset. IRQ entry still stores all 31 GPRs before obtaining TPIDR;
  interrupted full-image and restore sequences remain unchanged. Both
  complementary register sentinels poll their local counter. The old singleton
  software state symbols and allocations are removed.
- Rust validates exact base and full affinity before scalar identity access.
  All identity/counter/canary reads are inside an IRQ-mask window with complete
  DAIF restoration. Neither asm boundary declares `nomem`/`readonly`, so the
  compiler must honor its memory effects. Private observation closures do not
  expose references, callbacks or potentially panicking indexing. Requested
  correction to declare assembly-mutated storage as `static mut` is present;
  addressing remains raw and creates no Rust reference.
- Runner rejects absent or foreign stored identities at both public execution
  methods before acquiring BUSY. Private rejection fixtures precede successful
  normal and nested-Busy tests. Its marker remains non-Send/non-Sync, and the
  compiler harness includes the real new module and supplies the new private
  field when testing construction rejection.
- Invalid base fixtures cover null, misalignment and the inactive slot;
  affinity fixtures cover all four affinity levels. Cold boot seeds sixteen
  nonzero inactive-slot words. Baseline checks run after the real timer test;
  preemption checks run after scheduler/context tests, including the boundary
  feature. Checking these canaries never reads a live peer's record: CPU1
  remains inactive throughout the admitted execution profile.

No review execution results are claimed. Required runtime checks include the
full existing positive/negative suites, repeated monitor tests, actual-type
rejections and the new canary/affinity case. This source change leaves global
GIC shutdown and shared stacks intact and is explicitly **not SMP-ready**.
