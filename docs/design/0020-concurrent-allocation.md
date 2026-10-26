# Concurrent frame and heap ownership

Status: independently reviewed and implemented locally, 2026-10-26; see
[review 0020](../reviews/0020-concurrent-allocation.md) and
[execution evidence](../evidence/concurrent-allocation.md). Extends designs [0005](0005-frame-allocation.md),
[0006](0006-fallible-heap.md) and [0019](0019-arm-smp-scheduling.md). Retain the
actual boot Pool and its physical-memory proof; do not introduce a disconnected
allocator merely for concurrency tests.

## Ownership and sharing

Pool keeps its immutable Plan and permanent singleton construction latch.
Replace its Cell bitmap and diagnostic flags with private synchronized metadata.
Successful allocation removes an eligible frame/run from availability before
releasing the metadata lock. Zeroing then happens outside that lock and before
publishing its unique owner. A preempted initializer still owns its marked run;
no peer may allocate it. Release occurs only after all byte borrows and typed
values have ended. Reuse follows Acquire/Release synchronization through the
same metadata lock. No path restores an earlier bitmap snapshot.

The boot proof now covers every admitted CPU: eligible physical pages have
permanent matching normal-memory identity mappings and coherent access, with
no aliases providing independent byte ownership, devices or userspace. The
secondary root already shares these immutable identity mappings. Mapping arena
exclusion on CPU1 does not exclude the allocator's identity RAM. Reservations,
including every CPU's stacks, roots and records, remain excluded. The latch
cannot be reset by Pool drop or by forgetting owners.

Pool becomes Send + Sync through its synchronized state and immutable Plan.
Frame and Block can derive Send + Sync from their fields: a unique movable
owner permits mutable access only through its exclusive borrow, and shared
byte borrows expose initialized u8 memory. KernelBox needs explicitly justified
Send when T: Send and Sync when T: Sync because its private raw pointer prevents
auto derivation. PhantomData<T> and the pool borrow remain; borrowed values need
not become static. No unconditional typed-owner sharing, raw constructors,
Clone, reset, or allocator-borrow erasure is added. Forgetting an owner leaks its
occupied bits, preserving safety and existing accounting semantics.

Mapper currently uses Cell and is implicitly Send despite being !Sync. Add an
explicit non-Send/non-Sync marker before relaxing frame-owner traits. Mapper
and MappedPage remain confined to CPU0; moving Frame into a mapping still removes
identity byte access, and unmap completes local invalidation before returning
Frame or releasing its bit. No remote arena access or shootdown is introduced.
BootPlatform itself must not become a transferable source of mapping authority.

## Private metadata critical sections

A private allocator-only lock owns the bitmap and once-only diagnostic state.
It may reuse Mutex::try_lock internally, but must not expose an ordinary
cooperative mutex guard or arbitrary lock callback. Before attempting acquisition,
save local interrupt state and mask IRQs. Acquire the lock with Acquire ordering;
unlock with Release ordering before restoring exactly the prior IRQ state.
Mask/restore assembly retains compiler memory effects so metadata accesses cannot
move outside interrupt exclusion.
The private guard is non-Send/non-Sync and cannot escape allocator code. ARM
uses its existing DAIF privilege contract; any x86 implementation preserves IF
without introducing a real x86 physical allocator or claiming runtime coverage.

Masking before acquisition prevents a local timer from dispatching a worker
that would spin behind its preempted lock holder. A peer holding the lock also
has IRQs masked and executes only finite metadata operations. The wait itself
may be unbounded under contention; this is not a fairness or real-time claim.
There are no allocator calls from IRQ handlers, faults, scheduler internals or
panic output. No lock operation yields, invokes callbacks, performs serial I/O,
zeroes memory, creates/drops T, or acquires another lock while metadata is held.
All exits, including invalid layouts and exhaustion, release the lock and restore
prior interrupt state. No public IRQ-mask capability is exposed.

Claims, run claims, release, run release and stats use the same lock. Layout
validation and checked page rounding can precede acquisition. Run claims validate
all eligible free pages before marking any, and errors leave metadata unchanged.
Use one bounded linear scan of the at-most MAX_FRAMES arena, tracking the current
consecutive eligible/free run and testing aligned candidate/end containment in
that run. Do not retain the existing nested candidate/run rescans, whose worst
case approaches MAX_FRAMES squared with IRQs masked. Record the actual section
bound; no numeric latency or fairness promise follows without measurement.
Stats takes one coherent metadata snapshot but is immediately stale after unlock;
it never promises a reservation. Zeroing, pointer initialization, reads/writes
through owners and drop_in_place happen outside metadata protection. A destructor
may allocate and drop another owner while its own backing allocation stays live.

Legacy diagnostics remain memory-safe if called concurrently, even though their
exact-count assertions may legitimately fail under external allocation. Each
uses only its locally owned ledger and releases only bits it actually claimed;
never clear or overwrite another operation's bits. Atomically reserve each
once-only leak test under metadata protection, then allocate outside the lock;
on allocation failure allow retry without losing the reservation protocol.
Snapshot observations do not authorize reclamation. Do not hold the metadata
lock over an entire exhaustion diagnostic or over public allocate calls.
The dedicated concurrent gate supplies explicit quiescent barriers for exact
accounting, rather than relying on incidental timing in those legacy checks.

## Scoped access from SMP workers

Extend the private secondary runner with a scoped pool variant, conceptually
run_smp_with_pool(&mut self, pool: &Pool, cpu0_jobs, cpu1_jobs). The pool reference
is borrowed until both local callback sets return and CPU1 is confirmed
Returned plus firmware OFF. Existing run_smp without a pool retains its behavior.
Only this trusted coordinator publishes a raw pool address in a private AtomicPtr<Pool>
slot under the existing BUSY lease; it is null initially and for non-pool
sessions. Nested runner rejection must not replace an active publication.
Publish before worker admission, clear after
join and before releasing BUSY. Failures before admission never leave a slot
published. Any uncertain live-peer failure remains fatal, not a normal return.
No public RAII guard whose forgetting could release the enclosing borrow is used.

A worker-only safe accessor has the shape
with_pool<R>(f: impl for<'a> FnOnce(&'a Pool) -> R) -> Option<R>. It first validates
actual CPU-local active-worker admission and a non-null published slot. It then
invokes f with a borrow valid only for that invocation. The coordinator's joined
scope proves the pointer remains initialized and immovable for its duration;
normal callback completion cannot precede a live nested accessor call. CPU0 and
CPU1 observe publication through the existing admission release/acquire chain
and the slot's own Release/Acquire operations. No reference is forged as static.
The fixed R cannot depend on the fresh borrow lifetime: an owner, byte borrow or
Pool reference cannot escape in the result or be stored in a static container.
The accessor may encompass yields, waits and owner drops; metadata locks never
span f. Nested access lends the same shared Sync Pool and is safe. Forgetting an
allocation leaks pages, not a reference that can later be used after the scope.

Reject access from coordinators, inactive workers and after joined unpublication.
This is a scoped internal Rust capability, not a userspace ABI or a new global
allocator. Callback descriptors remain owned function pointers/u64 inputs.
General captured jobs, borrowed cross-CPU owner queues and task migration require
separate designs. The allocator's Send/Sync contracts are nevertheless real and
must be checked on the actual owner types, not substitutes. These trait tests
are compile-only evidence; runtime transfer of owners between CPUs is not claimed
by the scoped allocation workload.

## Required verification

Use the actual boot pool and public allocation/owner methods inside concurrent
preemptive callbacks on both CPUs at 64 and 128 MiB. Preserve all earlier SMP,
lifecycle, mapping, allocator, compiler and ARM/x86 regression gates.

- Force simultaneous retained frame and multi-page block allocations from all
  workers. An independent atomic live-page ledger rejects any overlap at page
  granularity, including across frame/block paths and excluded addresses. Fill
  every requested byte with a worker/iteration pattern, force timer preemption
  and cooperative yields while owners remain live, verify neighbours, then
  clear the diagnostic ledger immediately before dropping the unique owner.
  Keep ledger registration/removal ordered so it cannot reject legal reuse.
- Retain a bounded collection of actual owners across all eight worker stacks
  until exact pool exhaustion. Per-worker capacity together covers MAX_FRAMES;
  use at most 512 Option<Frame> slots per worker for the 4096-page arena,
  verify the compiled collection fits conservatively below its guarded 64 KiB limit, and
  use phase handshakes so a full local collection does not prevent other workers
  filling remaining pages. Once all workers stop allocating, sum retained
  owners, compare against the initial free count, require stats.free == 0 and
  repeated public frame/heap/typed requests to return exhaustion without mutation.
  Existing leaked/mapped/live pages remain unavailable. Release only test owners,
  then require exact pre-test accounting and reallocation zeroing. Repeat cycles.
- Concurrent mixed-size/aligned block churn validates all byte ranges, charged
  pages, fragmentation and ordinary failure without partial claims. Include
  one CPU releasing while the other allocates, with barriers that demonstrate
  actual overlap; sequential whole-CPU completion must not satisfy the case.
- Typed owners cover T: Send but !Sync, borrowed values, over-alignment and ZSTs,
  exactly-once destructors, intact values on failure, and destructor reentry that
  allocates while its own pages remain occupied. Such reentry must not deadlock.
- Explicitly verify IRQ state restoration for calls entered masked and enabled,
  normal and failure paths. Instrumented private diagnostics may force a pending
  timer while metadata is held: no local dispatch may occur before unlock, and
  delivery must occur after restoring enabled state. Avoid arbitrary callbacks
  under the metadata lock. Per-CPU scheduler reports and independent QEMU traces
  must still prove actual timer preemption on both CPUs.
- Compiler clients use actual core types: positive Pool/Frame/Block Send+Sync;
  KernelBox<Cell<_>> Send but rejected Sync; KernelBox of a non-Send type rejected
  Send; lifetime/borrow/constructor/Clone restrictions unchanged; Mapper,
  MappedPage and IRQ metadata guards remain nontransferable. A scoped-accessor
  client returning an owner or storing it beyond the closure must be rejected;
  an ordinary allocate/use/drop client must compile.

Record test inventory, bounded run times, actual allocation counts, interrupt
counts, source/ELF hashes, raw traces and failures. Measure critical-section
cost before claiming an allocator performance improvement. Passing these tests
establishes concurrent page-backed ownership, not a scalable allocator, small
object heap, lock fairness, DMA ownership, memory-ordering proof on native ARM,
userspace allocation or completion of every Phase 1 requirement.

## Provenance

No Linux implementation source is required or copied. Existing boot mapping,
IRQ and owner proofs are linked above. Rust's official Send, Sync and atomic
Ordering contracts inform the trait/order changes; the pinned compiler remains
1.99.0, independent of the version currently displayed by these moving pages:
[Send](https://doc.rust-lang.org/core/marker/trait.Send.html),
[Sync](https://doc.rust-lang.org/core/marker/trait.Sync.html),
[Ordering](https://doc.rust-lang.org/core/sync/atomic/enum.Ordering.html).
