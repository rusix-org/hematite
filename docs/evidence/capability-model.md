# Capability model verification

Executed 2026-10-26 UTC (2026-10-25 local), using `make test-all` on parent
commit `419e5ad` plus this change. Exact tested source hashes are preserved in
[source-sha256.txt](capability-final/source-sha256.txt). The pinned container
uses Rust 1.99.0 and QEMU 7.2.22; raw per-target environment, commands, kernel
hashes, serial output and results are in [capability-final/](capability-final/).

Both ARM64 and x86_64 passed:

- Formatting, Clippy with warnings denied, repository and safety hygiene.
- All 12 host parser tests and all 14 required guest cases.
- CPU feature rejection profiles and interactive monitor/recovery checks.
- Five compiler probes: valid client accepted; cross-instance tokens, escaping
  handles, private generation mutation and model cloning rejected with the
  expected diagnostic category.

The five new guest cases cover authority/attenuation, budgets and table limits,
resource lifetimes, slot generation retirement and all 24 permutations of
revoke/release/cancel/complete. Review additions exercise root release with a
live delegate, stale operation completion after reuse and create rejection
when grants are full. Model size is checked at no more than 16 KiB. Both release
guest suites execute on guarded 64 KiB stacks; this is not a general stack-depth
proof for future callers or optimization settings.

The first attempt failed Clippy's collapsible-if lint before guest execution;
its log is [capability-suite.log](capability-suite.log). The corrected initial
suite is [capability-suite-2.log](capability-suite-2.log), and the final reviewed
suite is [capability-suite-final.log](capability-suite-final.log).

Independent AI design/source review identified no implementation defect and
requested the additional cases above. This is not human review or a soundness
certification. No unsafe or hardware code changed. Owner labels and backend
completion are trusted model inputs; this proves no userspace authentication,
DMA quiescence, SMP safety, Linux compatibility or performance advantage.
Remote CI for this change has not been run.
