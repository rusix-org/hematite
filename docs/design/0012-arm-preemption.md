# ARM timer-driven kernel preemption

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0012-arm-preemption.md). Date: 2026-10-26. Extends designs 0009–0011. No preemption
implementation or execution evidence is claimed by this document.

## Scope and observable contract

Add `Runner::run_preemptive` and a corresponding Platform operation, retaining
`run` and every existing cooperative test unchanged. Accept the same bounded
zero-to-four `ThreadJob` values and borrowed `ThreadContext`; use the same global
execution lease and guarded stacks. Return a distinct report containing the
existing result/completion data, explicit yield counts, per-worker timer
preemption counts and a bounded dispatch/event trace with truncation indication.
Do not infer preemption from a dispatch count or count timer interrupts as yields.
Reject oversized and nested runs before touching stack state. x86 returns
Unsupported. Reject unavailable timer configuration before entering callbacks; setup failure
must roll back source/controller changes, clear private publication and release
the execution lease without invoking any job.

Timer expiry must suspend ordinary safe Rust callback code without a call to
`yield_now` and allow another runnable callback to execute. Start with one CPU,
GICv2 physical timer PPI30 and the existing pinned ARM target. A timer quantum
is a scheduling opportunity, not a latency guarantee. No callback cancellation,
stack abandonment, unwinding, SMP, priority scheduling or userspace is added.
A run returns only when all callbacks return normally. A nonterminating callback
may let peers progress but keeps its own stack and the run lease live forever.
Host test deadlines terminate the VM; they do not discard live Rust frames.

## Owned asynchronous context

Retain the AAPCS64 cooperative context for coordinator dispatch and explicit
yield. Add one private, aligned asynchronous context per admitted task, with
compile-time size/alignment/offset assertions shared with assembly. It owns
x0–x30, full Q0–Q31, SP_EL1, ELR_EL1, SPSR_EL1, FPCR and FPSR. Preserve the full
saved PSTATE, including NZCV, PAN, DAIF and mode; accept only the expected EL1h
worker state. The existing CPU profile does not enable SVE/SME or per-thread
translation/TLS state. Adding those requires extending this contract.

IRQ entry first switches to the existing guarded SP_EL0 emergency stack and
saves all GPRs before using scratch registers. Save all SIMD/FP and exception
state before calling or resuming Rust. Validate active run authority, task ID,
RUNNING state, timer ID and interrupted EL1h state. The interrupted SP may be
at a stack-probe boundary: saving cannot depend on writable worker-stack space.
A fault during entry uses the existing fatal SP0 route. IRQ remains masked;
nested IRQ handling is not supported.

Copy the complete interrupted image to that task's owned asynchronous slot,
mark its saved-continuation kind asynchronous, and mark it runnable. No pending
exception return may depend on ELR/SPSR remaining in CPU registers. Deassert the
timer source and acknowledge/deactivate the exact GIC token once, using the
reviewed ordering. Clear the local exclusive reservation before switching tasks;
interruption of an atomic exclusive sequence must cause retry rather than let
a different task inherit its reservation.

Before dispatching anything, fully release emergency scratch storage and restore
SP_EL0 to its canonical top. Switch SPSel back to SP_EL1 and restore the already
saved coordinator AAPCS context through a private assembly restore path. The
handler does not suspend a Rust frame or retain a continuation on SP_EL0. The
coordinator resumes with DAIF masked, regardless of the interrupted worker state.
Do not call the existing switch with an outgoing context that will later resume
an abandoned emergency-stack frame.

When the coordinator selects an asynchronously suspended task, an assembly-only
resume path loads its SP_EL1 and complete register image, writes its saved
ELR/SPSR, restores all SIMD/FP state, and uses ERET. All scratch registers,
including the pointer used to read the record, must be restored last in a
reviewable sequence. ERET restores the worker's interrupt state atomically with
its interrupted PC. Ordinary calls/returns cannot substitute for this operation.
The restored PC is captured by hardware; safe callers cannot select it.

## State transitions and Rust borrows

Mask/unmask assembly must act as a compiler memory barrier: do not mark it
nomem, readonly or pure, and do not let accesses to protected state move across
the boundary. Architectural interrupt masking and compiler ordering are separate
requirements; preserve the required instruction synchronization as reviewed.
All scheduler mutation runs masked. Every dispatch saves a fresh coordinator
AAPCS context, including dispatches that resume through ERET; an ERET-only helper
must not accidentally reuse a previous dispatch continuation. Dispatch saves
this context before publishing the incoming worker as active; the final first-entry/resume
assembly operation is the only transition to IRQ-enabled execution. First-entry
unmasks IRQ immediately before the callback adapter, after all state needed by
an immediate timer interrupt is valid, including the worker SP and saved coordinator
continuation. Adapter metadata reads are copied before
calling policy; no reference into mutable scheduler storage spans policy code.

Explicit yield masks IRQ before reading or writing run state, then records
RUNNING -> RUNNABLE and switches. Its eventual continuation restores precisely
its prior interrupt mask after the scheduler transaction has ended. In cooperative
mode that prior state stays masked; this change must not enable its IRQs. Completion
masks IRQ as the first instruction after the Rust adapter returns, writes result
and DONE, and switches permanently to the coordinator. An interrupt just before
that mask is permitted: it resumes the same assembly continuation before DONE
is published. DONE must never describe a still-executing callback stack.

The run state remains in an UnsafeCell at a stable coordinator-stack address.
IRQ assembly and core Rust use bounded raw field projections, with no live
whole-state/task reference across unmasking or switching. The IRQ handler does
not touch a callback's token or stack-local Rust values. Suspending their live
borrows is safe only because no other task is given access to those locations.
The coordinator holds no mutable reference to an outgoing context while the
handler writes it. Private atomic/assembly publication state identifies the
active mode and task; it never grants safe code raw-pointer access.

## Existing services and interrupt masking

Current function-pointer-plus-u64 jobs cannot capture a Pool, Mapper or
BootPlatform reference, and those capabilities are non-Sync/private. The
coordinator remains masked; the IRQ handler never allocates or maps memory.
Thus enabling these callbacks does not by itself establish a reachable shared
Cell allocator race. Preserve that capability boundary in this milestone.
Explicitly inventory callable core entry points and update their constructor
proofs: the old blanket statement that all Rust executes masked becomes false.
Do not claim allocator/mapper access from preemptive workers until separately
implemented and verified.

If implementation exposes such access, every Cell read-modify-write and mapping
transaction must instead be serialized before that exposure: e.g. a private
masked critical section around bounded bitmap updates, completed map/unmap/TLB
transactions and associated bookkeeping. Payload destructors must run outside
allocator critical sections. Diagnostics that retain allocator snapshots need
an equally explicit transaction or exclusion proof. Merely keeping Pool !Sync
is not a substitute if multiple task capabilities can reach one instance.

Do not expose a safe mask guard whose restoration is required for scheduler
correctness. Safe code can forget it, nest/drop guards out of order, or call
`yield_now` while holding it; blindly restoring a saved DAIF can enable IRQ in
an outer critical section or migrate a CPU-local mask obligation. This milestone
uses private assembly transitions and bounded internal masking with no arbitrary
policy callback inside the masked operation. Internal restoration records never
escape to safe clients. Any later public critical-section API needs its own
forget/reentrancy/yield proof and tests.

The existing mutex remains preemptible: a task may be suspended with its guard
held; another task's cooperative acquisition yields and eventually lets the
owner run. IRQ handlers must never acquire it or wait for an interrupted owner.
No scheduler or interrupt path calls user callbacks, destructors or allocating
logging. Scheduler internal state uses masking, not this blocking mutex.

The existing scoped timer probes and preemptive mode are mutually exclusive.
A shared private mode/ownership check must route the vector without trusting an
arbitrary PC range as scheduling authority. Choose coordinator-driven rearming:
on each return to the coordinator (IRQ, yield or completion), the source is
disabled/masked; complete source deassertion and clear any unacknowledged pending
PPI before another dispatch. The IRQ route deasserts the source before EOI. Each
dispatch computes a fresh future compare value and enables the timer only after
its continuation/publication is valid, just before entering the worker. Thus an
IRQ cannot leave the run permanently running on an expired one-shot timer.
Validate frequency/range arithmetic and use checked or explicitly wrapping
counter calculations. A spurious GIC ID receives no EOI and resumes the same
worker without recording a scheduling event; an unexpected real ID takes the
fatal path, never an arbitrary worker dispatch. Quiescent teardown masks IRQ,
disables the timer/PPI, drains or rejects remaining active state, and only after source/controller quiescence clears run
publication, before stack cleanup or lease release. Retain existing diagnostic
probe semantics and exact trace checks.

Existing fixed-site synchronous fault recovery requires a masked saved PSTATE.
Keep that whitelist unchanged initially: diagnostic calls execute masked;
an unexpected worker fault with IRQ enabled takes the fatal route. The emergency
stack is available because no prior timer handler remains suspended there.
Worker overflow while preemptible must still terminate through this route.
Do not relax fault recovery to arbitrary PC/FAR or silently mask return PSTATE.

## Required implementation evidence

- Two or more safe Rust callbacks that never invoke yield share atomic progress
  flags. Each requires another callback's progress before normal completion;
  without timer scheduling the VM hits the host deadline. Require nonzero
  per-worker preemption counts, correct results and eventual completion. Include
  four workers and a callback that finishes while others remain runnable.
- Check stack-local arrays and destructor-bearing values across repeated timer
  suspension; exact destructor counts after normal completion, repeated runs,
  zero/five-job cases, nested Busy, and trace truncation.
- An independent assembly sentinel workload checks all x0–x30, full Q0–Q31,
  NZCV, FPCR/FPSR and SP across actual timer scheduling while peers install
  distinct values. Account explicitly for registers used by the polling/check
  mechanism; use complementary passes where needed. Inspect interrupted PC and
  trace evidence rather than relying on a helper call's callee-save behavior.
- Exercise mixed explicit yields and timer preemption, immediate expiry around
  start/yield/completion boundaries, and mutex contention with a preempted owner.
  Require continued peer progress and consistent protected data.
- Preserve 64/128 MiB profiles, worker guard-overflow rejection, all prior
  cooperative/compiler/exception/timer cases, and post-run DAIF/SP_EL0,
  protections, allocator accounting and timer/GIC quiescence checks.
- Test missing/disabled timer delivery with a bounded host failure; never label
  an unexecuted scheduling case passing. Separate expected negative images from
  the required positive run. Archive exact source/ELF hashes and raw traces.

Independent design review precedes implementation; source review must audit
assembly save/restore offsets, IRQ routing/ownership, exclusive-monitor clearing,
all transition boundaries and the callable-core capability inventory. This
single-CPU increment cannot satisfy Phase 1's multi-CPU exit gate.

## Sources and provenance

Inspected existing project threads, timer, exceptions and frame allocation on
2026-10-26; no Linux implementation source consulted or copied. Architectural
reasoning uses Arm's [AArch64 Exception Model](https://documentation-service.arm.com/static/63a065c41d698c4dc521cb1c):
exception return restores saved processor state and PC through SPSR/ELR. The
full task-image and emergency-stack ownership scheme above is our design.
Rust [destructor rules](https://dev-doc.rust-lang.org/stable/reference/destructors.html)
permit safe code to omit destruction; therefore mask restoration cannot depend
on a guard destructor that arbitrary safe code owns. Revalidate exact timer,
GIC and exclusive-monitor instruction ordering against their primary manuals
during implementation review; those details are not established by this
high-level design alone.

## Boundary-expiry diagnostic refinement

A separate `preemption-boundary-test` image injects one expired timer per run at
first entry, explicit yield and completion before their scheduler mutations.
A private assembly macro saves its scratch GPRs/NZCV on the current guarded
worker stack, masks IRQ before changing assembly-only one-shot flags, and sets
CNTP_CVAL to zero with synchronization. These three sites require the existing
IRQ-enabled RUNNING-worker state; an already-masked entry is an invariant failure.
After restoring registers, flags and SP it restores that IRQ-enabled state in
a labelled instruction window. No policy callback runs while the macro is
masked, and no Rust borrow or emergency-stack frame survives its transition.
One-shot flags prevent a resumed worker from repeatedly forcing expiry.

The host must observe actual IRQ interrupted PCs within each exported
start/yield/completion window, using ranges from that image's ELF. Missing
observations fail. The normal image has no injection. This instrumentation
was independently reviewed before implementation; register/flag preservation
and exact source/trace correspondence remain source/runtime verification duties.
