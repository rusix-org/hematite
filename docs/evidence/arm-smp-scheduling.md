# Concurrent ARM scheduling verification

Executed 2026-10-26 UTC on parent `0fa4fae` plus this change. Final
`make test-all` exited 0. [Full output](smp-full-suite-attempt2.log),
[source hashes](smp-final/source-sha256.txt), exact commands, ELF identities,
compiler results and raw serial/exception traces are archived in
[final evidence](smp-final/). The pinned Rust 1.99.0/QEMU 7.2.22 container uses
two host CPUs and 4 GiB; concurrent guests use two CPUs with TCG `thread=multi`.
This is emulator evidence, not native ARM memory-ordering verification.

Both targets passed formatting, Clippy, safety/repository checks, 71 host harness
tests, 39 baseline guest cases, compiler boundaries and monitor profiles. ARM
also passed every prior preemption, wait, boundary, lifecycle and negative
profile. The 23 thread/event/secondary compiler clients now compile the actual
core modules with SMP enabled. All 55 result-bearing JSON files in the final
archive report success (46 ARM, 9 x86).

## Concurrent execution

[Five SMP profiles](smp-final/aarch64/smp/result.json) pass. At both 64 and
128 MiB, eight sessions use generations 1 through 8, confirmed CPU1 shutdown
before reuse, and complete the 39-case baseline afterward in the same guest:

- Nonyielding workers require second-worker progress on both CPUs, then verify
  complete register preservation against their own timer counters. Sequential
  whole-CPU execution cannot satisfy the barriers.
- Eight mutex workers force initial contention while the owner holds an
  inconsistent protected payload. They finish 16000 checked updates, matching
  checksum and eight destructor witnesses, including owner preemption.
- Eight mixed workers each force timer preemption, then yield 100 times. The
  bounded 64-entry report explicitly records truncation.
- Event tests cover notification before waiting, a forced notification between
  final snapshot and registration, broadcast to four blocked peer workers,
  and unrelated notification that must not wake the target waiter. Exact
  notification increments and expected park counts are checked.
- Oversized lists reject before admission; an empty session then completes.

Guest checks validate every admitted worker's actual CPU/task identity and SP
against its exact guarded stack. Host checks independently compare reported
roots/coordinator/worker-zero addresses with ELF symbols. Each positive profile
records exactly 55 CPU0 and 38 CPU1 IRQ/vector/ack/EOI transactions; CPU0's total
includes 16 interrupts from the subsequent baseline. CPU1 invokes CPU_OFF eight
times. The trace parser attributes entries only through CPU-tagged exception
blocks and GIC events; untagged ERET lines never supply CPU identity.

## Deliberate failure profiles

- With only CPU1's timer withheld, its first worker publishes its real stack
  address, but the second cannot run. CPU0 records 1712 interrupts and CPU1
  records zero. The host kills the live guest after two seconds; no successful
  session, peer shutdown or baseline is accepted.
- Both CPUs block on callback-local Events with no notifier. The coordinator
  reports both blocked only after atomic observations agree. The host kills the
  guest after two seconds, with no faults, shutdown or success report. This
  verifies retained frames/borrows, not CPU idle.
- A real 128 KiB Rust frame on CPU1 faults in its worker-zero guard and exits
  with code 5. The trace records FAR 1076424608 and ELR 1075874720; the latter
  falls within the actual ELF probe range [1075874700, 1075874828). The host
  requires the CPU1 data-abort syndrome, guard address, vector and terminal exit.
  Missing probe ranges and wrong PCs are rejected by harness unit fixtures.

## Review, failures and scope

Independent AI [review 0019](../reviews/0019-arm-smp-scheduling.md) approved the
[design](../design/0019-arm-smp-scheduling.md) before implementation and source
before execution. This is not human review or a complete soundness audit.
The [first build](smp-first-build.log) and [first guest](smp-first-boot/result.json)
passed. The [first full suite](smp-full-suite-attempt1.log) stopped at Clippy's
range-loop warning. The equivalent iterator initialization was independently
reviewed; the complete second suite passed without weakening any gate.

This satisfies the scoped simultaneous scheduling contract in design 0014.
Pool/Mapper remain CPU0-only, stacks and task counts are fixed, and workers
cannot capture caller borrows. There is no task migration, dynamic stack
allocation, cancellation, CPU idle or userspace execution. Concurrent allocator
stress and general resource ownership remain necessary for the full Phase 1
gate. All changes remain local; no new remote CI result is claimed.
