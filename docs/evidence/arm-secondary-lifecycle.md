# ARM secondary lifecycle verification

Executed 2026-10-26 UTC on parent `6193bb7` plus this change. Final
`make test-all` exited 0. [Full suite output](secondary-lifecycle-suite-second.log),
[source hashes](secondary-lifecycle-final/source-sha256.txt), commands, ELF
identities, compiler results and raw serial/exception traces are archived in
[final evidence](secondary-lifecycle-final/). Existing pinned Rust/QEMU container
and resource limits apply. Two-CPU runs explicitly use TCG `thread=multi`.

Both targets passed formatting, Clippy, safety/repository checks, 53 host harness
tests, 39 baseline guest cases and all existing compiler/monitor profiles. ARM
also passed every existing scheduling, boundary, negative and IRQ accounting
profile. Four new actual-type clients reject secondary Runner Send, Sync, Clone
and safe construction (23 thread/event/secondary clients total).

[Five new profiles](secondary-lifecycle-final/aarch64/secondary/result.json) pass:

- At both 128 and 64 MiB, CPU0 admits generations 1 and 2. CPU1 starts through
  PSCI, installs its own root and stacks, runs a fixed Rust callback, returns
  normally, publishes Returned in assembly, and invokes CPU_OFF. CPU0 observes
  both Returned and firmware OFF before consuming the report or reusing storage.
  Runtime PSCI_VERSION is 65537 (1.1); CPU_ON64, CPU_OFF and AFFINITY_INFO64 feature
  queries return zero. Each CPU_ON returns zero. Both session reports have mask
  255 and measured CPU identity 1; root and SP match independent ELF symbols.
  Each trace contains exactly two CPU1 HVC events. CPU0 HVC polling counts are
  recorded but not assumed fixed. The complete 39-case baseline passes after
  shutdown in each same guest.
- The same image with one CPU explicitly rejects topology and exits 1 before
  any HVC or baseline. Missing CPU1 cannot silently skip the lifecycle test.
- A mismatched context generation reaches the mapped secondary adapter and
  exits 3 on CPU1. Wrong mailbox phase instead has generic failure code 2.
- The simulated CPU_OFF-return path exits 4 on CPU1 after normal Rust return
  and Returned publication. It deliberately bypasses the final CPU_OFF HVC;
  this tests the fatal path, not a real firmware malfunction. Neither negative
  image emits a successful session or reaches baseline.

CPU1 observations cover MPIDR/TPIDR, root, coordinator/emergency stacks,
EL/DAIF/PAN/MMU/cache/FP controls, and hardware AT/PAR translations: a usable
stack page translates identically; the coordinator guard and owned arena reject
at levels 3 and 1 respectively. These are translation queries, not actual guard
overflow or recoverable faults. CPU1 uses dedicated fatal-only vectors. It does
not run the thread scheduler, access Pool/Mapper capabilities, or emit serial
reports. Shared kernel identity RAM remains mapped as documented in design0017.

Independent AI [lifecycle review](../reviews/0018-arm-secondary-lifecycle.md)
and [GIC amendment review](../reviews/0016-arm-gic-ownership.md) approved design
before implementation and source before final verification. This is not human
review, a complete soundness audit, native ARM memory-ordering evidence or the
simultaneous preemptive execution gate in design0014.

## Failures and diagnostic correction

The [first build](secondary-lifecycle-first-build.log) passed. The
[first guest](secondary-lifecycle-first-guest.log) completed both lifecycle
sessions but failed the old post-shutdown GIC ownership diagnostic. Its
[register trace](secondary-lifecycle-final/investigation/secondary-gic-trace/serial-and-gic.log)
showed an ineffective disabled-SPI32 pending seed. The independently reviewed
diagnostic amendment establishes CPU0 targeting on multiple-interface GICs,
preserves it through local cycles, then restores the original byte after global
cleanup. Single-interface RAZ/WI targeting retains implicit CPU0 behavior.
The [corrected trace](secondary-lifecycle-final/investigation/secondary-gic-trace-fixed/serial-and-gic.log)
shows original target 0, target 1, pending seed 1 preserved through both cycles,
pending clear 0, and restored target 0, followed by a passing baseline. Production
local setup/teardown still never changes shared target bytes.

A separate halted-QEMU monitor `xp` inspection terminated the emulator with
signal 11 and produced no target value. Its command/output and
[failed result](secondary-gic-reset-result.json) are retained; no register claim
relies on that attempt. The running guest's corrected trace supplies the value.
The [first full suite](secondary-lifecycle-suite-first.log) stopped before guests
because two existing safety comments were separated from declarations by cfg
attributes. Moving those comments adjacent satisfied the structural checker;
the complete second suite passed. Remote publication remains pending approval;
no public push was attempted.
