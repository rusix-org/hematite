# ARM SMP prerequisite audit and design review

Date: 2026-10-26. Independent AI source audit at `08cf6d2`; not human approval
or a complete soundness certification. No builds, guest execution or runtime
implementation performed by this reviewer.

## Existing source constraints

1. **Secondary entry cannot reuse cold boot.** `arm_start` selects one normal
   and one emergency stack, zeroes the complete BSS (including reserved stacks),
   initializes the UART, and writes global translation tables. Running it on
   another CPU would corrupt live objects and stacks. A distinct secondary
   entry must validate the admitted CPU identity before using private storage,
   install its own stacks/control registers and consume already-published
   immutable boot metadata without repeating global initialization.

2. **Execution authority currently describes one CPU.** Runner carries a
   non-Send/non-Sync marker and a global BUSY lease, but no hardware CPU ID.
   Four linker-owned worker stacks, the coordinator context and emergency stack
   cannot be used concurrently by two runners. Removing BUSY or constructing a
   second Runner alone is insufficient. Partition stack/context storage, make
   CPU affinity explicit, and validate that private authority is used only on
   its owning CPU. Captured/borrowed callback closures remain excluded.

3. **Interrupt state is global software storage.** `arm_preempt_state` contains
   the current coordinator/task pointers; timer state, IRQ counters and pressure
   flags are also global non-atomic assembly objects. Two CPU handlers would
   race and could restore another CPU's context. All CPU-local handler state and
   observations must be indexed by a validated CPU identity or a private CPU
   pointer established before IRQ enable. SP_EL0 is a banked register, but its
   current value points both CPUs to the same memory unless boot changes it.
   Synchronous exception diagnostics and guard checks also name CPU0's stacks.

4. **Current GIC setup/teardown affects the peer.** Both scoped timer and
   preemptive paths disable GICD_CTLR and clear interrupt banks during setup;
   teardown disables the distributor again. The distributor is shared, unlike
   each CPU interface and the banked SGI/PPI state. Initialize global state once
   with peers quiescent. Active CPU operations may configure/disable their local
   timer/PPI and CPU interface, but must not shut down peer delivery. Restore
   global diagnostic behavior only after independently established peer
   quiescence, not merely after a mailbox says callbacks returned.

5. **Frame and heap ownership remain CPU0-only.** Pool's Cell bitmap and
   diagnostics perform whole-ledger read/modify/write. Its construction latch
   prevents duplicate pools; it does not serialize later allocation. Sharing
   the pool across CPUs would permit overlapping frame ownership and zeroing of
   another owner's memory. Either preserve capability exclusion while the peer
   uses permanent storage, or introduce reviewed synchronization before sharing.
   Any allocator extension must keep payload destruction outside locked bitmap
   transactions and test exhaustion, reclamation and actual concurrent updates.

6. **Mapping reclamation needs a cross-CPU argument.** Mapper uses Cell slot
   state and `TLBI VMALLE1`, a local invalidation, before releasing backing.
   A shared root would let another CPU cache arena translations. Keeping
   Mapper !Sync alone does not invalidate those remote translations. Exclude
   mutable arenas from the secondary root and keep them unreachable, freeze
   changes for the complete shared-root lifetime, or implement acknowledged
   shootdown before reuse. Choose explicitly; never treat CPU halt or a software
   completion flag as an implicit TLB invalidation. Identity mappings used by
   both CPUs must remain immutable with matching shareability/cache attributes.

7. **Atomic primitives are not a scheduler migration proof.** Mutex already
   uses Acquire/Release with T:Send and non-Send/non-Sync guards; Event uses a
   nonwrapping atomic generation. They can protect explicitly shared static
   state when cache/coherency assumptions hold. Worker tokens, parked Event
   registrations, live stack locals and run records must remain CPU-affine;
   do not migrate those continuations merely because a payload mutex is Sync.
   A parked worker's CPU must keep scanning or receive a designed wake kick;
   a WFE/WFI idle path cannot be added without its own lost-wakeup proof.

8. **Remote completion must outlive every remote access.** A mailbox containing
   copied jobs/results needs exact ownership and release/acquire transitions.
   Reuse only after the remote CPU has left every callback/adapter/scheduler
   frame that could refer to the submitted storage. Startup failure, delayed
   entry, duplicate start and nonreturning callbacks must not permit stack or
   descriptor reuse. A timeout may terminate the VM, but must never return a
   safe error that reclaims memory still accessible by the secondary.

9. **Publication crosses an MMU/cache-state boundary.** CPU1's earliest entry
   cannot assume ordinary cached coherent Rust accesses. A release store alone
   does not prove visibility to an early uncached reader. Avoid mutable shared
   loads before coherent translation/cache setup, and explicitly publish/clean
   the boot tables/descriptor to the required point with appropriate barriers.
   Validate the PSCI context and CPU identity before selecting private storage;
   a late entry must not accidentally consume a newer mailbox generation.
   Review the exact cache-maintenance and startup control sequence separately.

10. **SMP evidence must distinguish CPUs.** Existing IRQ validators and control
    sentinels assume CPU0 and a single counter. Another CPU's ticks cannot be
    accepted as evidence that the tested CPU was preempted. Require per-CPU
    dispatch/context reports, CPU-local counter advancement and actual IRQ
    transactions on both hardware CPU IDs. Cross-CPU producer/consumer progress
    must run concurrently with both local preemptive schedulers, not merely
    before/after a secondary hello routine.

These are prerequisites for the new mechanism, not defects in the current
single-CPU profile. CPU1 hello output alone cannot satisfy the requested
simultaneous preemptive scheduling or Phase 1 allocator/concurrency exit gate.

## Design verdict

Reviewed [design 0014](../design/0014-arm-smp.md), including requested revisions.
**Ready for implementation at the design level; no blocking design finding
remains.** This is independent AI review of the contract, not runtime source
approval, execution evidence or authorization for unrelated backend changes.

The selected approach keeps every Task record, guarded worker/coordinator stack,
emergency stack and interrupt publication CPU-affine. The second root excludes
the mutable mapping arena while sharing only immutable identity/device tables.
This is a concrete ownership restriction that avoids a premature shootdown
claim; it does not make the current allocator concurrent or complete Phase 1.
Global GIC configuration remains CPU0-owned while CPU1 is OFF; only banked local
PPI/timer/interface state changes while both schedulers execute.

Requested startup clarification is present: both roots/shared descriptors are
constructed with MMU/cache off and completed by DSB before immutable use. CPU1
does not read mutable mailbox data before coherent mappings are established.
A later cache-enabled table-construction path requires new reviewed cache
maintenance rather than silently relying on release ordering. Admission checks
cover exact CPU identity, context generation and bounded topology.

The global lease precedes the OFF query and controller/mailbox changes. Copied
jobs and reports live in permanent session storage; phase publication, rather
than shared Rust references, transfers access. The peer's Rust adapter returns
normally before assembly publishes Returned. CPU0 additionally requires PSCI
OFF before reuse; delayed start, uncertain stop and nonreturning callbacks do
not authorize reclamation. Generation exhaustion cannot silently wrap.

Requested counter clarification is present: CPU0 cannot read plain CPU1 records
while the peer writes them. Reports are copied by their owner and consumed only
after join/OFF. Any live remote observation must be explicitly atomic. Register
sentinels require their own CPU's timer progress, and host traces attribute
transactions to each CPU. This prevents a peer's IRQs from masquerading as
local context-preservation evidence.

Implementation/source review must still inspect the concrete per-CPU layouts,
MPIDR/TPIDR validation, all entry and restoration paths, global/local GIC split,
mailbox aliasing/order and exact PSCI instructions. All prescribed concurrent
workloads, failure injections and same-guest post-join regressions remain
unexecuted requirements. A successful topology dump alone proves none of them.

## Primary sources

Consulted 2026-10-26, specifications only; no Linux implementation source used.

- [Arm GICv2 IHI 0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=):
  shared distributor, CPU interfaces and banked interrupt state require distinct
  initialization/ownership scopes.
- [Arm PSCI DEN 0022D](https://documentation-service.arm.com/static/5f905c71f86e16515cdc1fd0):
  CPU_ON supplies a physical entry point and context; startup state and
  CPU_OFF/AFFINITY_INFO lifecycle must be handled explicitly. A successful
  CPU_ON result is not proof that kernel initialization or a callback finished.

Exact pinned QEMU topology, PSCI conduit and startup observations belong in
the integration evidence; the manuals alone do not establish them.
