# Concurrent allocation review

Date: 2026-10-26. Independent AI review, not human approval or complete
soundness certification. This reviewer performs no builds or guest execution.

## Design verdict

Reviewed [design 0020](../design/0020-concurrent-allocation.md) against the
existing frame/heap, mapping and joined SMP implementations at parent
`9d6522b`. **Approved for the specified implementation.** Source and execution
verification remain separate gates; no concurrent allocator is claimed yet.

The design extends the actual singleton boot Pool rather than constructing an
unrelated test allocator. Its mutable bitmap and diagnostic state share one
metadata lock. IRQ exclusion begins before acquisition and ends after Release
unlock, preventing local preemption from stranding a lock owner. Metadata
operations are bounded and callback-free; the linear contiguous-run scan avoids
quadratic IRQ-disabled search. Zeroing and typed initialization occur after a
transactional claim but before owner publication, and destructors run before
reclamation with no allocator lock held. Stats is a coherent observation, not
authority to allocate or reclaim.

Pool sharing relies on immutable matching coherent identity mappings on every
admitted CPU. Frame/Block transfer does not create aliases: their private owner
is still unique and mutable access requires an exclusive borrow. KernelBox
requires separate conditional Send and Sync implementations tied to T; a raw
pointer is not evidence for either trait. Pool and value lifetimes, padding
privacy, permanent construction latch and forgotten-owner leaks are preserved.

An important existing boundary is tightened before widening owner transfer:
Mapper's Cell fields make it non-Sync but do not make the Mapper itself
non-Send. The explicit nontransferable marker prevents movement of local page
table authority; MappedPage and BootPlatform must retain that restriction.
CPU1's absent mutable arena mapping and the consuming Frame-to-mapping protocol
permit allocator sharing without introducing remote TLB invalidation claims.

Legacy diagnostics cannot restore whole snapshots or reclaim from observations.
They may report failure when concurrent activity invalidates exact-count
expectations, but must remain memory-safe, synchronize once-only leak state and
release only their own claims. The new exhaustion gate instead uses actual
owners and explicit quiescent phases to establish exact totals and reclamation.

The scoped runner retains the borrowed Pool until both local callback sets
return and CPU1 reaches Returned plus firmware OFF. Publication is private,
under BUSY, and cannot be replaced by a rejected nested session. AtomicPtr
preserves the actual pointer; the worker-only higher-ranked accessor lends it
only for a live callback invocation. Its result type cannot carry that fresh
borrow outward. No public guard can be forgotten to end the lifetime proof.
Join precedes unpublication and lease release; uncertain live-peer failure is
terminal. Non-pool and inactive-worker access must fail without dereference.

The verification plan requires actual retained owners on both CPUs, mixed
frame/block overlap detection, exact public-API exhaustion, cross-CPU churn,
typed destructor reentry and restoration of both masked and enabled IRQ state.
Compiler clients distinguish permitted transfer from forbidden aliases, value
traits, scope escape and mapping authority. Runtime cross-CPU transfer of an
individual owner is explicitly not inferred from compile-only Send/Sync tests.
The eight bounded owner arrays require conservative compiled stack accounting;
their collection size alone does not establish the entire worker frame fits.

Implementation review must specifically inspect every mutable metadata access,
guard destruction order, assembly compiler effects, all early-error publication
paths, linear scan alignment arithmetic, and trait bounds. Fixed diagnostics
must not wait on a condition requiring the masked lock owner's local scheduler.
All prior allocator/mapping/SMP gates and the new concurrent tests remain required.

## Source verdict

Reviewed the frozen allocator, Mapper marker, scoped runner publication and
actual-type clients, plus the safe concurrent allocation workloads and SMP
integration. **No blocking core source finding remains.** Compiler and guest
execution are still separate gates.

- Metadata uses one AtomicBool/UnsafeCell boundary. All bitmap and leak-flag
  mutations acquire its private guard; immutable Plan reads need no lock.
  The guard masks before acquisition, cannot transfer across CPUs and performs
  Release unlock before restoring state. Its Deref/DerefMut references cannot
  outlive the guard. Both assembly directions retain compiler memory effects.
- Frame/run claims are transactional. The run scan visits each candidate page
  once, remembering the earliest aligned start within a free eligible run;
  committing marks at most MAX_FRAMES bits. No prior bitmap snapshot is ever
  restored. Allocation initialization and all typed destructor calls occur
  outside the guard. Error paths release the guard through ordinary Drop.
- Leak-test reservation and retry are synchronized. A legacy diagnostic that
  encounters a formerly occupied snapshot bit records its newly acquired bit
  in its own ledger before reporting failure, so cleanup neither loses that
  claim nor releases a foreign allocation. Concurrent exact-count assertions
  may fail safely; the dedicated workload supplies stronger barriers.
- Pool/Frame/Block obtain the intended auto traits. KernelBox's manual Send
  and Sync implementations independently require T: Send and T: Sync. Mapper's
  explicit raw-pointer PhantomData keeps it and borrowed mappings nontransferable.
- ACTIVE_POOL publication follows all recoverable admission failures, under
  BUSY with CPU1 OFF. The borrowed reference remains valid until local callbacks
  finish and peer Returned/OFF is confirmed. Clearing precedes lease release.
  Ordinary sessions use null; rejected nesting cannot change the publication.
  The worker-only accessor checks active worker state before Acquire-loading
  the pointer and invoking its higher-ranked closure. An invocation cannot
  outlive the callback whose completion is required by the join.
- Review corrected a compiler-test weakness: a wrapper originally repeated
  the higher-ranked signature and could hide future weakening of the actual
  accessor. The wrapper now reexports the actual function directly. Scope
  escape clients therefore test the implementation's signature itself.
- The safe workloads retain overlapping lifetimes on all eight workers, dirty
  and recheck frame/block contents through yields and forced preemption, keep a
  coordinator-owned neighbour and deliberate forgotten frame unavailable, and
  track live claimed pages independently. Exhaustion retains actual owners in
  bounded arrays until every worker has stopped allocating; only then are
  exact free counts and repeated failure results checked. Separate barriers
  keep reclamation and subsequent cycles from racing those observations.
  Typed fixtures include borrowed values, Cell values, over-alignment, ZSTs
  and destructors allocating while their own storage remains occupied.

The parent added and this reviewer inspected ordinary allocation failure with
the coordinator already IRQ-masked: invalid alignment and an unavailable
full-arena request must preserve DAIF 0x3c0 and accounting. This complements
enabled-worker failure and successful private lock checks. No remaining source
finding blocks the execution gate. Exact allocator metrics, per-CPU IRQ traces
and all prior regression gates remain required. No compiler or QEMU process
was run by this reviewer.

### Deterministic cross-CPU reuse refinement

Final requirement audit identified that initial simultaneous retention plus
free-running churn did not deterministically establish the specified remote
allocation-after-release sequence. Approved the following safe-test refinement
before its implementation: each CPU0 worker pairs with the same task ID on CPU1.
After releasing its frame in round r, CPU0 publishes r+1 while retaining its
block. Except in the final round, it waits for CPU1's allocation publication
r+2 before inspecting and dropping that retained block. Before each round after
zero, CPU1 waits for the corresponding release count, then allocates/registers
and checks its new owners before publishing its allocation count. Waits yield
with IRQs enabled. The last-round exemption avoids awaiting a nonexistent round.

Release/Acquire counters, reset before the joined session, establish that the
remote allocation follows frame release while CPU0's neighbour block remains
live. Final counters must equal the full churn count for all four pairs. This
tests ordered cross-CPU reuse and overlapping owner lifetimes; it does not claim
two threads can enter the exclusive metadata section simultaneously. Revised
source was independently checked: validated task IDs index four disjoint pairs,
publication follows successful registration/zero checks, CPU0's retained block
is rechecked after the remote publication, final-round waiting is omitted, and
all eight counters must finish at CHURN. Arrays reset only outside a live joined
session. **The refinement source is approved.** Revised execution must pass
before citing this stronger result.

## Compiled worker stack inspection

Inspected the parent's emitted assembly
`target/aarch64/smp-test/aarch64-unknown-none/release/deps/hematite-896b8e4f044bddca.s`
and read-only disassembly of its linked `hematite` ELF. The parent archives the
assembly and compiler invocation with final evidence. This is inspection of
the pinned compiled image, not a general Rust stack-bound proof.

The largest worker helper, `allocation_checks::exhaust_owned`, reserves 17600
bytes, including its real owner collection, temporary initialization storage
and spill slots. Its prologue probes four successive 4096-byte decrements and
the final 1104 bytes, in addition to its 112-byte register save. The emitted
CFI maximum agrees with that explicit SP arithmetic. `exhaustion_worker`
reserves 96 bytes and restores them before tail-branching to this helper;
counting both remains conservative. `isolation_worker` reserves 288 bytes and
`reentry_worker` 336 bytes; the over-aligned heap value does not introduce an
unseen 8192-byte local reservation in the inspected image. Reinspection after
the deterministic churn refinement confirms these same frame sizes; the emitted
assembly includes both CHURN_RELEASED and CHURN_ALLOCATED state accesses.

The common `preempt::entry` frame is 48 bytes. Reachable ordinary allocation
helpers are small: `Pool::allocate_heap` reserves 48 bytes, the live-page
registration helper 16, and typed reentry drop glue 64. The relevant drop glue
and allocation helpers do not recurse. The indirect calls in these fixed
workloads are the admitted Token's CPU/id/yield methods; CPU/id/yield Rust
methods add no frame, and `arm_preempt_yield` adds 16 bytes before switching to
the separate coordinator stack. The register sentinel reserves 1104 bytes.
The linked memcpy/memset/memcmp routines use no stack or external callees.
Hardware timer entry uses the separate emergency stack, so its saved image is
not added to the worker reservation.

Even conservatively adding the largest observed worker/helper frames and the
sentinel together keeps these normal callback paths below 20 KiB, with ample
space within each 64 KiB usable worker stack. The actual guarded two-CPU
workloads remain the execution check. This inspection does not bound arbitrary
future callbacks, recursive destructors, or changed compiler output; those
require renewed inspection or runtime stack-limit enforcement.

## DAIF synchronization reference

The mask/restore instructions do not need an added ISB solely for the PSTATE
mask change: Arm's employee response citing ARMv8 C5.1.3 states that these
PSTATE mask writes take effect in program order without extra synchronization.
This does not remove the required compiler memory effects or atomic ordering.
[Arm explanation](https://community.arm.com/forums/f/architectures-and-processors-forum/45735/does-msr-daif-require-isb-instruction-if-no-why),
consulted 2026-10-26. Search results incidentally included Linux snippets; no
Linux implementation page was opened or used for this review or copied.
