# Kernel mutex ownership and cooperative contention

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0011-kernel-mutex.md).
Date: 2026-10-26. Extends [cooperative threads](0010-arm-kernel-threads.md).

Add an actual generic core mutex protecting shared Rust values, and exercise it
with the current kernel threads. This establishes a synchronization primitive
before preemption; it does not enable IRQ-time locking, scheduler wait queues,
guest SMP, priority inheritance or fairness guarantees.

## Public interface

The architecture-independent `hm-core` module owns `Mutex<T>` and its private
guard representation. Its storage is an `AtomicBool` plus `UnsafeCell<T>`; it
allocates no memory and can be constructed with `const fn new(value: T)`.
Start with sized `T`; unsizing, mapped guards and condition variables are later
work. Expose only:

- `try_lock(&self) -> Option<Guard<'_, T>>`, a single nonblocking attempt.
- `lock_cooperative<'a>(&'a self, context: &mut dyn ThreadContext) -> Guard<'a, T>`,
  retrying after a cooperative yield when acquisition fails.
- `get_mut(&mut self) -> &mut T`, using exclusive ownership of the mutex.
- `into_inner(self) -> T`, consuming the mutex without requiring acquisition.

The safe policy boundary has `mutex_api::OwnedMutex<T>` with a guard GAT bounded
by `DerefMut<Target = T>` and the same operations. `Platform` supplies
`type Mutex<T>: OwnedMutex<T>` and `new_mutex<T>(&self, value: T) -> Self::Mutex<T>`.
Construction is infallible and does not borrow the platform. Both ARM and x86
implement it with the same core type; the mutex itself needs no boot/MMIO
authority. Do not return a fake unsupported mutex on x86.

There is no unbounded spinning `lock()` operation. Under the current cooperative
scheduler, spinning would prevent the owner from running. A waiting callback
instead yields with no guard held by this acquisition operation. Keep the
context borrow independent of the returned guard lifetime, so a caller may
explicitly yield while holding a guard. Such yielding remains memory-safe but
can cause contention or deadlock through application lock ordering.

No safe API exposes the value's raw pointer, unlocks someone else's guard,
resets a live lock, fabricates guard ownership, or shares a guard across threads.
No guard cloning, mapped guards, poisoning state or owner-ID authentication is
introduced in this increment.

## Exclusive access and ordering proof

An unlocked mutex has flag false. `try_lock` performs a strong
`compare_exchange(false, true, Acquire, Relaxed)`. Success is the only path
that constructs a guard and grants value access. Failure returns None without
reading or forming a reference to `T`. Using a strong exchange makes a failed
attempt actual contention rather than a permitted spurious failure.

The guard contains a private borrowed mutex reference and a
`PhantomData<*mut ()>` marker. It implements neither Send nor Sync, even when
`T` is Sync. `Deref` constructs a shared value reference tied to `&guard`;
`DerefMut` constructs the exclusive reference tied to `&mut guard`. Rust borrows
prevent conflicting accesses through the same guard and prevent guard Drop
while one of its references remains usable. No other guard can exist during
that ownership interval.

Guard Drop performs a Release store of false after all usable value borrows
end. The next successful Acquire exchange observes the publication of the
previous owner's accesses. No protected-data access occurs after unlock.
The mutex is `Sync` only for `T: Send`: different execution contexts can access
or replace the value sequentially, but never concurrently. Requiring T:Sync
would unnecessarily forbid useful interior-mutable values such as Cell; omitting
T:Send would incorrectly allow thread-bound values to cross contexts. Mutex
Send follows T:Send through its fields; do not add a weaker blanket impl.

UnsafeCell permits interior mutation but supplies neither exclusion nor race
prevention itself. All shared-owner access therefore passes through acquisition.
`get_mut` uses the exclusive mutex borrow and `UnsafeCell::get_mut`; `into_inner`
consumes its storage using `UnsafeCell::into_inner`. These paths need no flag
operation because a usable guard or value reference would prevent the required
exclusive borrow/move. They do not change a forgotten guard's flag.

The only unsafe sites should be the reviewed Sync implementation and guard
reference construction. Each explains the ownership and ordering invariant.
The cooperative acquisition loop is safe Rust: no guard exists when it calls
`yield_now`, and each retry must acquire afresh. It must remain sound even if a
different safe ThreadContext implementation returns immediately, panics or
reenters unrelated code. Its soundness must not depend on trusting that context;
an arbitrary context provides no progress or termination guarantee.

## Forgetting, failure and execution scope

Forgetting a guard leaves the lock occupied. Other shared borrowers can only
fail `try_lock` or wait indefinitely; there is no forced recovery that would
invalidate a surviving reference. Once safe code can obtain exclusive ownership
of the mutex, `get_mut`/`into_inner` remain sound even if its flag is still true.
Moving/consuming a mutex with a usable guard borrow is rejected by Rust. A local
forgotten-guard test must use bounded try-lock attempts, never hang a test by
calling cooperative lock on an intentionally unreleased mutex.

The kernel uses panic=abort. Hosted tests may also exercise guard cleanup under
ordinary unwinding, but no poison or application-invariant recovery is promised.
Dropping a guard does not drop T; consuming/dropping the mutex owns that duty.
Test non-Copy values and destructor counts to avoid assuming byte-only storage.

The current kernel executes Rust with IRQ masked. This increment does not
alter DAIF or imply preemption protection. A future preemptive task may wait
while its owner runs elsewhere, but an interrupt handler must never wait on a
mutex held by interrupted code. Existing IRQ handlers remain mutex-free. Future
IRQ-safe locks, preemption-disable ownership and wait/wake protocols require
their own reviewed contracts; this mutex cannot be substituted for them silently.

## Verification

Safe policy tests use the generic platform mutex: successful acquisition,
exclusive mutable access, failed try-lock while held, reacquisition observing
prior writes, get_mut/into_inner, non-Copy values and exact destructor behavior.
Test a deliberately forgotten local guard with bounded operations and show
exclusive ownership still permits safe value extraction without forced unlock.
Neither generic sequential tests nor byte counters alone establish concurrency.

Actual cooperative tests use a static instance of the real core mutex and safe
callbacks on the existing Runner. A fixture module may declare
`#![forbid(unsafe_code)]` inside core so the callbacks demonstrably use only the
safe API. This avoids forging a mutex pointer through a job's integer input or
adding a cyclic policy-to-core dependency. Protect a multi-field value containing
Cell (Send but not Sync). Hold a guard across a yield to force another worker's
try-lock failure, then use cooperative acquisition to make progress after the
owner releases. Check final exact counts, cross-field consistency, completion,
observed contention and repeat-run reset. No IRQ handler touches the fixture.

A hosted harness compiles and runs this exact core module, not a replica, on
four OS worker threads with bounded iteration counts and an outer timeout.
Exercise repeated contention, non-atomic multi-field updates/publication,
non-Copy ownership and exact final totals. Record host toolchain, target,
commands and raw results. This tests actual cross-thread atomic behavior on the
host; it is explicitly not QEMU guest SMP or an exhaustive memory-model proof.

Compiler clients must accept Mutex<Cell<T>> sharing when T is Send, and reject
Mutex containing a non-Send value being shared or transferred. Reject Guard
Send/Sync, Clone/forging, escape past the mutex, moving/exclusively borrowing a
mutex with a live guard, simultaneous mutable value borrows and value-reference
use after guard Drop. Include positive cases and exact diagnostic checks using
the actual module. Run the existing dual-architecture suites and preserve raw
failures. x86 supports the generic mutex tests; ARM additionally runs real
cooperative worker contention, while hosted stress has its own explicit scope.

## Primary sources

Consulted 2026-10-26; no Linux implementation source used or copied.

- [Rust UnsafeCell](https://doc.rust-lang.org/core/cell/struct.UnsafeCell.html):
  interior mutability, exclusive-reference obligations and data-race limits.
- [Rust atomic ordering](https://doc.rust-lang.org/core/sync/atomic/enum.Ordering.html):
  Acquire/Release publication and compare-exchange ordering constraints.
- [Rust mem::forget](https://doc.rust-lang.org/core/mem/fn.forget.html):
  safe callers may omit destructors, so soundness cannot require guard Drop.

These references describe the language contract; the proposed implementation
and test design are original. Their moving documentation does not update the
repository's pinned compiler or introduce dependencies.
