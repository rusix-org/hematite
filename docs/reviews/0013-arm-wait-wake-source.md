# ARM wait/wake source review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit. No builds or guest runs are performed by this reviewer.
Implementation execution evidence belongs to the integration record.

Status: implementation, instrumentation and compiler clients inspected;
**no blocking source findings remain**. Design-level review is recorded in
[review 0013](0013-arm-wait-wake.md). Runtime results are recorded separately.

The final design chooses bounded coordinator scanning when all workers are
blocked. It retains the execution lease and live frames without claiming CPU
sleep or an external wake source. Forced timer expiry tests exclusion during
masked registration; no policy notifier executes inside that masked region.

## Event and core registration inspection

`wait_api::Event` keeps AtomicU64 storage private. Its Acquire snapshot and
checked AcqRel fetch-update implement monotonic notification without wrap or
reset. The successful return addition is bounded by the checked update that
produced its previous value. Exhaustion leaves the value unchanged and returns
an explicit error. Boundary fixtures can initialize private storage only from
inside the safe module; they do not expose a reset operation to clients.

The preemptive token masks IRQ with compiler memory side effects before its
last snapshot. Changed and exhausted paths do not register or increment parks.
The park path checks RUNNING and empty registration, then stores the pointer
derived from the live shared Event reference, its observation and a distinct
BLOCKED status/kind. All projections are raw; no whole-task reference spans
the context switch. The saved mask is an internal stack local, not a guard
policy can forget. All normal returns restore that exact prior mask.

`arm_preempt_park` preserves BLOCKED, disables the source and clears the local
exclusive reservation before the ordinary switch. It does not route through
the yield state transition or increment explicit yields. The dispatch branch
recognizes kind 3 as an ordinary continuation; it does not restore the task's
older asynchronous image. The usual coordinator return path quiesces pending
timer state before scanning registrations.

The coordinator examines only BLOCKED records. Its temporary shared Event
access is valid under the suspended wait call's borrow and touches atomic
storage only. It clears pointer and observation before RUNNABLE publication.
The resumed wait checks empty registration and performs its own Acquire load
through the original reference before returning. Sequence monotonicity makes
unchanged resumption an internal invariant failure. Completion checks that no
event pointer remains. There is no Event-held run pointer or reusable waiter
handle through which a later notification can reach retired stacks.

When all unfinished tasks remain blocked, the scan preserves the local State,
stack lease and suspended borrows indefinitely. The test-only serial marker is
emitted after that state is established. Production does not turn the deadlock
into a return that would destroy live callback frames.

## Safe workloads and accounting

The safe Event tests use the actual private representation to seed MAX-1 and
check transition to MAX, failed notifications and terminal waits. An ordinary
local Event is notified before waiting, then moved after completed calls; its
report must contain zero parks. Default unsupported contexts do not mutate it.

Three broadcast workers snapshot before waiting and preserve stack arrays and
destructor-bearing locals. Their notifier first changes a different Event and
yields repeatedly while the expected predicate remains false. An early resumed
waiter therefore returns failure. The eventual matching notification occurs
immediately before notifier return. Reports require exactly one actual park and
Wake dispatch for each waiter, with zero explicit yields by those waiters.

The repeated producer/consumer workload snapshots before checking its atomic
predicate, uses notification to publish preceding relaxed atomic payload writes,
and requires real parks by both workers. Selected notifier rounds spin without
yielding and therefore require timer scheduling for the peer to acknowledge.
These tests exercise publication behavior but are not an exhaustive weak-memory
proof. They preserve local data, exact destructor counts and repeated-run state.

Report validation includes parks in the dispatch total, distinguishes Wake from
Yield and Timer, checks per-worker trace counts against complete counters, and
handles bounded-prefix truncation explicitly. Zero park counts are required for
the immediate-return fixture, so yielding or unconditional rescheduling cannot
substitute for that behavior.

## Transition instrumentation and negative profile

The ordinary ABI mask helper is a compiler memory barrier and returns the
actual prior DAIF value after masking IRQ. In the pressure image its existing
register-preserving boundary macro first forces delivery while the task remains
RUNNING. The host now includes the park window in its ELF-resolved interrupted-
PC requirements. The macro does not expose an interrupt-mask owner to policy.

After BLOCKED publication, the separate park instrumentation verifies IRQ is
masked, forces timer expiry and requires ISTATUS pending. It increments a
private diagnostic counter only after those checks, then disables the source
without unmasking. Its x2–x4 scratch registers are caller-clobbered; task/host
arguments x0/x1 and saved LR remain intact. `wait_boundary_check` requires this
counter to be nonzero for the pressure image. The IRQ handler's RUNNING-state
check would reject accidental delivery against a partially parked task.

The all-blocked image creates a callback-local Event and a destructor witness
whose destruction takes a rejected fatal path. The coordinator emits the single
marker only after every unfinished task is actually BLOCKED. The host requires
that complete marker exactly once, no normal/fatal report, a launched guest,
bounded logs and the expected killed timeout. This is evidence of retained
blocked execution, not successful wakeup or CPU idle. It does not weaken the
positive harness's ordinary timeout rejection.

## Compiler coverage

The new actual-type clients accept a local Event wait and Event Send/Sync, and
reject Event cloning. Existing runner/context authority clients remain. The
requested additions now explicitly attempt private sequence reset and escape
of a local Event reference, requiring E0616 and E0515 respectively. Valid local
borrowing is the positive control. These compile the actual implementation
types, not a duplicate representation. Execution of the compiler clients and
all guest checks remains the integration record's responsibility.
