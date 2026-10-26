# Kernel mutex review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness certification. Reviewer inspected the existing safe thread interface
and cooperative execution boundary. No build or guest execution performed by
this reviewer.

## Design review

Ready for implementation. No blocking design findings remain. The proposed
contract establishes the necessary ownership and publication rules. The
requested wording correction is present: arbitrary safe ThreadContext behavior
may prevent progress, while memory safety must not depend on that behavior.
No fairness or termination guarantee follows from a yielding loop.

Only a successful strong compare-exchange from false to true may construct a
guard. Acquire ordering pairs with the previous owner's Release unlock;
failed acquisition must not touch or form a reference to the payload. The
AtomicBool's modification order supplies single ownership, while UnsafeCell
only permits the interior mutation that ownership controls.

Mutex Sync must require T:Send because different threads may replace or take
ownership of T sequentially. T:Sync is unnecessary for exclusive access and
would reject Cell-based payloads. Mutex Send follows its fields. The guard's
explicit non-Send/non-Sync marker is essential: a borrowed Mutex<Cell<_>> may
be Sync, but sharing a guard would otherwise expose unprotected shared Cell
access. Guard dereference lifetimes must be tied to the guard borrow, rather
than merely the longer mutex lifetime.

No reference may remain usable after Drop releases the flag. Rust borrowing
prevents moving/dropping the guard while its value references remain usable,
and prevents moving or exclusively borrowing the mutex while a usable guard
exists. Safe forgetting does not violate those rules: it loses the unlock
operation and leaves shared acquisition unavailable. get_mut/into_inner can
still operate when safe code later obtains exclusive ownership; neither needs
to force-unlock a shared object or resurrect a guard.

The cooperative loop calls yield only after a failed acquisition, with no
newly acquired guard or payload reference held by the operation. Its return
lifetime must depend only on the mutex, allowing callers to yield explicitly
while holding a guard. That permits contention tests and remains memory-safe;
application lock order, self-deadlock, starvation and arbitrary callback
termination are separate concerns. The module does not change DAIF and cannot
be treated as an IRQ-safe or preemption-disabling primitive. Existing interrupt
handlers remain outside this API.

The proposed hosted stress must compile the actual core module and protect
non-atomic multi-field data, while guest tests must force actual worker
contention and completion. These provide distinct evidence: OS-thread atomic
behavior on the host and cooperative integration in the guest. Neither is
guest SMP validation or an exhaustive memory-model proof. Compiler tests need
positive controls and rejection of private/escaped/transferable guards, non-Send
payload sharing, and references that outlive release.

## Source review

No blocking findings in the implementation inspected on 2026-10-26:
`hm-core/src/mutex.rs`, `mutex_checks.rs`, the main/module and architecture
hooks, `hm-kernel/src/mutex_api.rs`, `mutex_tests.rs` and test dispatch, plus
`tools/check_mutex_types.py` and `tools/test_mutex_host.rs`.

The core has exactly one guard-construction path following successful strong
Acquire CAS. Private fields prevent guard forgery and lock-state changes;
Release Drop is the only shared unlock. Both dereference implementations tie
their references to a guard borrow and have local safety explanations.
`PhantomData<*mut ()>` blocks guard Send and Sync, including the otherwise
dangerous Cell payload case. The sole unsafe trait implementation requires
T:Send, and Mutex Send follows its fields. Exclusive extraction uses the safe
UnsafeCell APIs, including after forgetting a guard. The cooperative operation
retains no payload reference across a failed attempt or arbitrary yield call.

The ARM fixture itself forbids unsafe code. Its four real callbacks yield while
holding the guard with deliberately inconsistent protected Cell fields, then
check consistency and publish the completed update before release. A separate
atomic lease serializes fixture resets; it does not grant payload access.
Completion, contention and final state are checked twice by the safe dispatcher.
The x86 hook explicitly reports the worker fixture unavailable, while running
the actual same mutex for sequential contract and lifetime checks. The x86
worker test therefore establishes only the unsupported boundary, not execution
of workers.

Compiler probes compile the actual core module with valid controls and enforce
specific error codes for trait, privacy and borrow failures. Hosted stress
also uses that module, four OS threads and a non-Sync Cell payload, checks
exclusion against an initially held guard, performs 80,000 updates with an
independently computed checksum, and observes exactly one payload destructor.
The shared subprocess runner bounds execution to 20 seconds. These tests are
appropriate evidence for the stated scope; they are not exhaustive race or
memory-model verification. This reviewer inspected their source only; passing
execution claims and raw logs belong in the integration evidence.

## Primary references checked

Consulted 2026-10-26; no Linux implementation source consulted or copied.

- [Rust UnsafeCell documentation](https://doc.rust-lang.org/core/cell/struct.UnsafeCell.html):
  interior mutability does not remove exclusive-reference or data-race rules.
- [Rust atomic Ordering documentation](https://doc.rust-lang.org/core/sync/atomic/enum.Ordering.html):
  successful acquire operations observe writes published by release operations;
  failed acquisition must not access protected data.
- [Rust mem::forget documentation](https://doc.rust-lang.org/core/mem/fn.forget.html):
  safe clients may omit a guard destructor, so unlocking cannot be a safety
  prerequisite for resource reclamation through exclusive ownership.
- [Rust Mutex API documentation](https://doc.rust-lang.org/std/sync/struct.Mutex.html):
  payload transfer bounds and exclusive access are distinct from poisoning or
  deadlock/fairness behavior. Only API documentation was consulted, not copied
  implementation code.
