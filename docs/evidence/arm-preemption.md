# ARM preemption verification

Executed 2026-10-26 UTC on parent `7d5d8d4` plus this change. Final command:
`make test-all`, exit 0. [Full log](preemption-suite-final.log),
[source hashes](preemption-final/source-sha256.txt), expanded commands, ELF hashes,
serial output, compiler diagnostics and traces are archived in
[final evidence](preemption-final/). The pinned container uses Rust 1.99.0 and
QEMU 7.2.22, two host CPUs/4 GiB; guests use one emulated CPU and 64/128 MiB.

## Observed results

- Both architectures: formatting, Clippy, safety/repository checks, 40 host
  harness cases, all existing compiler contracts, 32 guest cases and monitor/
  rejection profiles passed. x86 has no preemptive runner; its regressions do
  not claim ARM scheduling execution.
- ARM 64/128 MiB: four strict scheduling cases passed, followed by all 32
  baseline checks in each same guest. Four workers require peer progress
  without yielding; tests verify stack locals, destructor counts, mixed yields,
  limit rejection, repeated runs, trace truncation and exact report accounting.
- Core checks force preemption of a mutex owner while its protected Cell pair
  is inconsistent, require peer contention/completion and nested-run Busy.
  Complementary assembly passes check every GPR, all full-width Q registers,
  FPCR/FPSR/NZCV and an independently saved SP across actual timer scheduling.
  Post-run observations check masked DAIF, canonical emergency SP and timer/GIC
  quiescence; ordinary memory/protection/thread/fault/timer tests then pass.
- Each ordinary scheduling image reported 93 scheduler IRQs. With the 16 later
  diagnostic IRQs, QEMU recorded exactly 109 IRQ/vector/acknowledgment/EOI
  transactions. These counts are observations, not fixed performance budgets.
- The separate forced-expiry image reported 118 scheduler IRQs and 134 total
  transactions. ELF-resolved windows contained eight start, four yield and
  eight completion interrupted PCs. Programming the timer alone cannot pass
  this check: the host requires observed delivery inside each range.
- With PPI delivery withheld, the same nonyielding workload reached the exact
  ELF worker-entry address and timed out at the two-second host bound, with no
  IRQ or normal completion. The separate oversized safe worker faulted in its
  own stack guard and exited through the required fatal path without resuming.

These are single-CPU emulator results. There is no SMP, userspace, wait/wake
queue, termination of nonreturning callbacks, worker allocator authority or
fairness/latency guarantee. [Independent AI source review](../reviews/0012-arm-preemption-source.md)
is not human review or a complete soundness audit. Remote CI has not run for
this change because public push approval is pending.

## Failures retained

[First compilation](preemption-build-first.log) rejected the PAN register spelling
and identified FFI/feature warnings; explicit architectural encoding and opaque
FFI pointers fixed them. The [first guest](preemption-first-failure/) took a nested
undefined-instruction exception at the first IRQ: SP_EL1 is not directly readable
at EL1. The corrected handler observes the interrupted SP through a masked,
memory-free SPSel switch and returns immediately to the emergency stack.
[First full suite](preemption-suite-first.log) rejected an unused overflow helper
in the combined-feature lint build; its configuration now matches boot precedence.

Intermediate successful runs are summarized in the retained second/third suite
logs. The final suite includes later boundary checks, post-run regressions and
stricter trace/entry validation; only its archived source manifest identifies
the complete verified implementation.
