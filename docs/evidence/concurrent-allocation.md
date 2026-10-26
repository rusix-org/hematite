# Concurrent allocator verification

Executed 2026-10-26 UTC on parent `9d6522b` plus this change. `make test-all`
exited 0 both before and after strengthening deterministic peer reuse.
[Final complete output](allocation-full-suite-attempt2.log),
[source identities](allocation-final/source-sha256.txt), commands, ELF hashes,
compiler diagnostics and raw guest traces are archived in
[final evidence](allocation-final/). Existing pinned Rust 1.99.0/QEMU 7.2.22
container/resource limits apply. Concurrent guests use two CPUs, TCG
`thread=multi`, and either 64 or 128 MiB RAM. No physical hardware was modified.

## Runtime results

All eleven [SMP cases](allocation-final/aarch64/smp/result.json) pass at both
memory sizes, followed by the full 39-case baseline after confirmed CPU1 OFF.
The three new cases use the actual boot Pool, not an isolated test allocator.
Each retains a coordinator-owned neighbour; a deliberate forgotten frame stays
unavailable across all cases. Their identical accounting at both memory sizes:

| Case | Free before/after | Successful allocations | Peak tracked pages | Destructor checks |
| --- | ---: | ---: | ---: | ---: |
| Mixed frame/block isolation | 3314 / 3314 | 1024 | 28 | 0 |
| Two complete exhaustion cycles | 3314 / 3314 | 6628 | 3314 | 16 |
| Typed values and destructor reentry | 3314 / 3314 | 1280 | 2 | 512 |

Peak counts cover the independent diagnostic page ledger, not all typed-value
backing pages. In particular the reentry ledger tracks destructor allocations.
The host requires exact inventory, restored capacity, allocation/drop counts,
appropriate peak bounds and placement of each metrics record before its session.

Eight workers retain and dirty disjoint frames/blocks, including multi-page and
over-aligned blocks, through yields and forced timer preemption. Four paired handshakes require
CPU1 to allocate its next round after CPU0 releases a frame while CPU0 still
retains and subsequently rechecks its block; all eight completion counters must
reach 64. Sequential whole-CPU execution cannot satisfy this protocol. The ledger
checks live page overlap and reservations independently of allocator metadata.
It is cleared immediately before owner drop, allowing valid subsequent reuse.
Exhaustion retains up to 512 actual Frame owners per worker until all workers
finish allocating. Barriers separate exact zero-free checks, repeated public
allocation failures, reclamation and the next cycle. Existing live/leaked pages
cannot be claimed. Typed tests cover borrowed values, Cell values, alignment,
ZST destruction, failed-value preservation and destructors that allocate while
their own storage remains occupied. No allocator metadata lock spans user code.

Both IRQ-enabled workers and the already-masked coordinator check exact DAIF
restoration. The coordinator tests invalid alignment and an unavailable valid
full-arena request; workers exercise ordinary allocation, drops and exhaustion.
Metadata-lock inspection verifies that IRQs are masked inside the critical
section. The optional forced-pending-timer hook was not added or claimed.

Independent trace accounting matches all reported local interrupts plus the
post-join baseline: 169 CPU0 / 147 CPU1 transactions at 64 MiB, and 168 / 145
at 128 MiB. IRQ/vector/ack/EOI counts agree separately for each CPU. Counts are
observations of these runs, not fixed timing requirements. All prior withheld
timer, peer overflow, blocked-retention, lifecycle and single-CPU negative
profiles pass unchanged.

## Compiler, stack and regression evidence

Both targets pass formatting, Clippy, structural safety checks, 75 host harness
unit tests, 39 baseline cases and existing monitor/protection checks. Actual
core clients verify 30 frame/heap boundaries per architecture, 22 ARM mapping
boundaries and 28 ARM thread/Event/scoped-access boundaries. Positive sharing
clients compile; invalid typed-owner traits, aliases, lifetimes, construction,
cloning and scoped Pool/Frame/byte escapes are rejected. The private mapping
capability remains non-Send/non-Sync. Individual owner transfer between CPUs
is compile-tested only; the workloads allocate/drop each owner on its own CPU.
All 55 result-bearing JSON files in the final archive report success.

The [emitted assembly](allocation-stack/hematite.s),
[exact emission command](allocation-stack/command.json) and
[identity record](allocation-stack/input.json) belong to the same ELF hash as
the final SMP guests. Independent [stack review](../reviews/0020-concurrent-allocation.md)
accounts for the complete 17600-byte exhaustion function frame, entry/caller
frames, helper calls, yield and register-sentinel storage. Fixed normal paths
conservatively remain below 20 KiB within 64 KiB guarded worker stacks. Timer
save images use separate emergency stacks. Actual guarded execution also passes.
This inspection does not bound arbitrary future callbacks or recursive drops.

## Review and scope

[Design 0020](../design/0020-concurrent-allocation.md) was independently reviewed
before implementation; source review found no remaining blocking issue. Review
corrected a probe wrapper that repeated the scoped lifetime signature: it now
directly reexports the actual accessor, so a weakened implementation cannot be
hidden by the wrapper. Masked failure-path checks were also added before the
first guest. Both [initial](allocation-first-build.log) and
[corrected](allocation-second-build.log) builds succeeded; the
[first guest](allocation-first-boot/result.json) and
[first full suite](allocation-full-suite-attempt1.log) passed. A final requirement
audit strengthened peer reuse from permitted interleaving to the required
handshake. That safe-test refinement was independently reviewed, rebuilt with
[exact completion checks](allocation-churn-final-build.log), and passed the
complete suite again. The [earlier results](allocation-first-suite/) and
[earlier assembly](allocation-pre-churn-stack/) are retained. No failed
execution was discarded. Review is independent AI review, not human approval
or complete soundness certification.

This establishes concurrent page-backed ownership in the declared emulator
profile. It does not establish a scalable/small-object allocator, bounded lock
wait time, native ARM memory-ordering proof, cross-CPU mappings/TLB shootdowns,
DMA ownership, userspace or the complete published Phase 1 gate. Remote CI
remains at the earlier published milestone; this change has not been pushed.
