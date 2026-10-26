# ARM event waits and worker wakeup

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0013-arm-wait-wake.md). Date: 2026-10-26. Extends
[preemption](0012-arm-preemption.md). No execution evidence is claimed here.

## Contract and safe interface

Add a bounded event-wait primitive to the existing preemptive runner. A worker
whose observed event has not changed becomes BLOCKED and is excluded from
dispatch until notification changes that observation. This is actual worker
descheduling, not a loop calling yield. Initially support wake-all notification,
one outstanding wait per worker and the existing four-task synchronous run.
There is no cancellation, timeout, wake-one fairness, external device producer,
SMP scheduler, mutex integration or wait handle that escapes the callback.

The safe policy crate defines an allocation-free `Event` with one private
`AtomicU64`, initially zero, and no scheduler pointer or registration storage:

- `const Event::new() -> Event`.
- `Event::snapshot(&self) -> u64`: Acquire load of the sequence.
- `Event::notify(&self) -> Result<u64, NotifyError>`: checked monotonic increment;
  successful AcqRel compare-exchange publishes earlier writes and returns the
  new sequence. Failure ordering is Acquire. At `u64::MAX`, return Exhausted
  without wrapping, changing the sequence or promising publication of new writes.
- `ThreadContext::wait(&mut self, event: &Event, observed: u64)
  -> Result<(), WaitError>`, with a default Unsupported implementation. Existing
  cooperative contexts and fake safe contexts remain compatible. The ARM
  preemptive token implements the blocking contract below.

The sequence is an observation, not an ownership token: callers may supply any
u64, including a value from a different event. This can change when their wait
returns, but must never compromise memory safety. Event uses ordinary atomic
Send/Sync guarantees and does not implement Clone by duplicating identity.
Moving an unborrowed event is allowed. No unsafe policy code is required.

For the real preemptive context, the final Acquire check returns immediately if
the current sequence differs from `observed`. If they are equal to MAX, return
WaitError::Exhausted; never park on the terminal sequence. Otherwise park until
an Acquire observation differs. The successful increment MAX-1 -> MAX wakes
older observations; a later wait with observation MAX cannot block. A private
safe test constructor may seed boundary values inside the Event module; do not
add a production reset that can restore an earlier sequence while borrowed.

A notification means “recheck the predicate,” not one consumable resource or
one queued message. Repeated notifications can coalesce. Callers must snapshot
the event **before** checking their independently synchronized predicate, then
wait only if that predicate remains false. The producer changes the predicate
and releases its lock before notifying. If a wake returns while the predicate
is still false, repeat that sequence. Snapshotting after a false predicate check
can miss the intervening producer action and is explicitly incorrect usage.
Event does not make unsynchronized non-atomic payload access safe, release a
mutex on behalf of a waiter, or prevent application deadlock.

## Borrow and registration lifetime

Each `wait` invocation retains its shared Event borrow for its entire suspended
call. The callback stack, local values and the run's coordinator frame remain
owned until normal callback completion, as in design 0012. Add private per-task
wait metadata: an event pointer, observed sequence and park counter. Preserve
existing assembly offsets or update every assertion and assembly use together.

The core forms a raw pointer only from the live shared reference. It may read
the referenced Event through its safe Acquire method only while the task is
BLOCKED. No address is accepted from a policy integer, and no pointer is retained
in the Event or returned to policy. Coordinator reads are shared accesses to an
atomic object; they do not touch callback-local tokens or mutable values.

When an observation changes, clear the event pointer/registration before marking
the task RUNNABLE. A resumed wait returns with no registration remaining. Clear
all inactive metadata before completion/stack reuse as a checked invariant.
Before returning, the resumed wait performs its own Acquire snapshot through
the original borrowed Event reference. Its value must differ from the supplied
observation because the sequence never resets or wraps. This both provides the
caller's publication observation and makes the shared borrow span suspension;
do not rely solely on the coordinator's earlier Acquire load for that contract.
Consequently the event may be moved or dropped immediately after `wait` returns;
the scheduler never reads a stale pointer. Existing Rust borrowing prevents the
event from being invalidated while its wait call remains suspended. Forgetting
an Event after a completed wait cannot resurrect registration. A callback that
never resumes retains its live stack and borrow; there is no reclamation on a
host timeout or an all-blocked condition.

Arbitrary safe implementations of ThreadContext may return Unsupported or
otherwise fail to provide useful wake behavior. They receive only a shared
Event reference and cannot corrupt scheduler storage or bypass atomic privacy.
Core soundness does not rely on policy implementing the trait honestly.

## Scheduler transitions and lost-wakeup argument

Add a BLOCKED state distinct from RUNNABLE/RUNNING/DONE. Only the preemptive
token can park its currently RUNNING task. Mask IRQ before the final sequence
check and every wait-metadata/state transition. This is a private bounded core
operation; no arbitrary policy closure executes masked and no mask guard escapes.
Assembly/inline-assembly barriers must prevent compiler movement across masking.

If the final check permits parking, publish event/observation, mark BLOCKED,
save the ordinary cooperative continuation and switch to the masked coordinator.
The timer source must be disabled/deasserted on this return just as for yield.
The park continuation resumes with its original IRQ state only after the
scheduler transaction is complete. Do not route park through code that marks
the task RUNNABLE or charges an explicit yield. An IRQ before masking is ordinary
preemption; after masking it cannot observe a partially saved parked context.
Every return path, including immediate changed/exhausted observations, restores
the exact prior IRQ state before returning to policy. A Wake dispatch resumes
the saved ordinary park continuation, not the task's asynchronous image. Add an
explicit park-continuation kind and extend dispatch's cooperative restore branch;
the existing assembly treats only kind 1 as cooperative and cannot remain
unchanged when adding a distinct Wake cause.

Before selecting runnable work, the coordinator scans the at-most-four BLOCKED
tasks. An Acquire observation unequal to the saved sequence clears registration
and makes that task runnable with a Wake dispatch cause. Unchanged events leave
workers BLOCKED: timer ticks do not dispatch them, run their callbacks or count
as wakeups. Scan after every return to the coordinator, including yield, timer
preemption, park and completion, so notification is recognized even when the
notifier immediately finishes. Notification itself only updates the atomic
sequence; it never mutates a run record or directly switches contexts.

Within the current single-CPU closed execution model, notification before the
masked final check is observed by that check; a different worker cannot notify
between the final check and completed park because IRQ is masked. Notification
after the park is visible when the coordinator next scans. Thus no notification
can disappear in the check-to-registration gap. These arguments do not establish
SMP idle/wakeup correctness; adding another CPU or external IRQ notifier requires
a new registration/idle-kick protocol and independent review.

If all unfinished tasks are BLOCKED, no current worker can execute notify.
Retain the lease, task records, stacks and borrows indefinitely. The masked
coordinator continues bounded scans with a spin-loop hint after timer quiescence.
This deliberately spins the CPU; it only deschedules workers and does not claim
CPU sleep or low-power idle. No WFE or external wake guarantee is introduced
without a reviewed idle-kick protocol. This loop must never disguise blocked
workers as yielded/runnable work or
return an error that abandons their Rust frames. A negative guest test verifies
this retained all-blocked condition through the host deadline.

## Reports and required evidence

Extend PreemptReport with per-worker park counts and DispatchCause::Wake.
Increment parks only for an actual BLOCKED transition, not an already-changed
or exhausted observation. Each resumed parked continuation contributes exactly
one Wake dispatch. On a completed run, dispatches equal admitted tasks plus
explicit yields, timer preemptions and parks; retain the existing bounded trace
and truncation contract. Counters saturate explicitly without affecting control.

Before claiming this primitive verified, require:

- Pure safe Event tests for monotonic increments, arbitrary/stale observations,
  terminal exhaustion and publication; use the actual Event implementation,
  including a seeded MAX-1/MAX fixture, rather than a duplicate counter model.
- Real workers proving notify-before-park returns without a park, notify-after-
  park resumes exactly once, multiple parked waiters wake from one notification,
  and a distinct event does not wake an unrelated blocked worker. Require report
  and dispatch evidence that blocked workers were excluded, not merely yielding.
- Repeated producer/consumer predicate loops with notification before the final
  check and after BLOCKED publication. A different worker cannot notify during
  the masked transition: exercise that exclusion with private forced timer
  expiry immediately before masking and while registration is masked, verifying
  no partial registration is dispatched. Do not inject a notifier or arbitrary
  safe closure into the masked region merely to manufacture an interleaving
  forbidden by this execution model. Also exercise a notifier
  that returns immediately, and interleaved timer preemption/explicit yields.
- Callback-local arrays and destructor-bearing locals retained over real waits;
  exact normal destruction and repeated stack reuse after all waits complete.
  Compile actual clients accepting a local Event borrow, rejecting invalid
  lifetimes/private metadata access, and preserving current runner authority.
  Current function-pointer jobs do not allow sharing a newly created local Event
  with another worker: test such a local blocked borrow in the all-blocked negative
  image rather than introducing unsafe fixture sharing just to claim coverage.
- A separate all-blocked image reaches an explicit post-registration diagnostic
  marker, reports no normal completion, and times out at a bounded host deadline.
  It must not release the execution lease or drop suspended locals. Positive runs
  need all callbacks to return; a timeout never counts as successful wakeup.
- Preserve full preemption/register/boundary, mutex, fault, timer, allocator and
  dual-architecture regression suites, including same-guest checks after wake
  workloads. Archive exact commands, source/ELF hashes and failed attempts.

## Provenance

Inspected current Event-free ThreadContext, preemptive task records and scheduler
on 2026-10-26. No Linux implementation source consulted or copied. The ownership,
registration and generation protocol above is a proposed project design.
Rust's [atomic ordering documentation](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html)
supports the Release/Acquire publication contract; the
[destructor reference](https://doc.rust-lang.org/reference/destructors.html)
motivates keeping suspended frames owned rather than depending on future Drop.
Exact park assembly requires architecture review before source
approval; this design does not claim an external-interrupt or SMP wake protocol.
