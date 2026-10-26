# ARM wait/wake design review

Date: 2026-10-26. Independent AI review of the current preemptive runner at
`dbf0474` and the proposed wait/wake contract. This is not human approval or a
complete soundness certification. No runtime implementation, build or guest
execution is part of this review.

## Existing scheduler constraints

The current runner has RUNNABLE, RUNNING and DONE states and dispatches every
runnable worker in a bounded round-robin scan. Adding blocking requires a
distinct non-dispatchable state: calling the existing yielding path repeatedly
would not establish a wait queue. Registration, final generation observation,
status change and context saving must be one IRQ-masked transaction. The
coordinator must restore a complete continuation before a blocked callback can
resume, and timer entry must never see a partially published blocked state.

The current synchronous run owns worker stacks and coordinator metadata until
every callback returns normally. Therefore an all-blocked run cannot return an
error, drop its state or release its execution lease while those Rust frames
remain suspended. A masked coordinator scan can retain that ownership, but it
does not make the CPU idle and supplies no external wake source by itself.

## Required ownership and notification proof

A concrete Event may expose safe atomic generation operations while retaining
private storage. A blocked callback's shared Event borrow must remain live
through the entire wait call, including suspension. The scheduler may retain a
raw pointer only under that borrow and only while the corresponding task is
blocked. Clear the registration before publishing RUNNABLE, resuming the call
or allowing normal callback return. Event notification must not retain a run
pointer or grant authority to unrelated task records.

Safe clients may supply arbitrary observed generations, implement their own
ThreadContext, forget values or wait while holding an application mutex.
Those actions may cause poor progress or deadlock; they must not invalidate
the pointer/lifetime proof. The core cannot require policy to call notify in a
particular way for memory safety. No safe mask guard may escape to policy.

Successful notification must advance a generation without wrap. A terminal
generation must never admit another indefinite wait, and failure to advance
must be explicit. Failed notification at exhaustion cannot promise publication
of new payload writes. Acquire observations of a successful release operation
can provide publication; they do not make unrelated mutable payload accesses
safe without their own synchronization.

The application pattern must snapshot before checking its synchronized
predicate, then wait only if that predicate is still false. The notifier changes
the predicate before notifying. Final observation under the registration mask
must cover notification before registration; later coordinator scans must cover
notification after blocking. Every return requires rechecking the predicate.

## Design verdict

Reviewed [design 0013](../design/0013-arm-wait-wake.md), including requested
clarifications. **Ready for implementation at the design level.** No blocking
design finding remains. This does not approve an uninspected implementation or
claim runtime verification.

The design preserves the shared Event borrow across a real parked continuation,
permits coordinator reads only while BLOCKED, and clears registration before
RUNNABLE publication. Its resumed wait performs its own Acquire observation.
Notification advances only atomic state and cannot retain scheduler authority.
Arbitrary observations and safe trait implementations therefore affect progress,
not pointer validity. Monotonic exhaustion wakes older observations but never
allows a wait on the terminal value; failed notification makes no new publication
promise. The predicate-loop ordering and before/after registration argument are
explicit and correctly scoped to the current single-CPU execution model.

Two requested transition clarifications are present: every return path, including
Changed/Exhausted without parking, restores the prior IRQ state; and Wake selects
the ordinary saved park continuation rather than an asynchronous image. Existing
assembly currently uses only kind 1 for ordinary restoration, so source review
must verify the new branch and matching offsets rather than infer it from a new
report enum. BLOCKED tasks must remain undispatched when their sequence is
unchanged. An all-blocked run retains all frames and the lease indefinitely;
the chosen bounded scan deliberately spins the CPU and provides no external
wakeup or low-power-idle claim. The final refinement also tests masked-transition
exclusion through forced timer expiry, rather than injecting a worker notification
into a masked interval where that interleaving cannot occur.

The proposed tests distinguish actual parks from immediate returns and explicit
yields, require notification ordering and unrelated-event exclusion, preserve
borrowed locals/destructors, and include retained all-blocked lifetime evidence.
Implementation review must verify raw projection/lifetime boundaries, masked
registration and early returns, ordinary-context restoration, per-task metadata
clearing, report accounting and the absence of any callback inside masked
transitions. No tests were executed for this proposed behavior.

## Primary reference

Consulted 2026-10-26: [Rust atomic Ordering documentation](https://doc.rust-lang.org/core/sync/atomic/enum.Ordering.html).
Acquire/Release synchronization orders accesses that actually observe the
published atomic value; it does not replace payload ownership. No Linux
implementation source was consulted or copied.
