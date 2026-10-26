# 0029 supplement: lease-bound task dispatch
Status: **ownership/finalization boundary reviewed; complete core mechanism
remains draft and unapproved**. See [review 0029](../reviews/0029-native-task-creation.md).
Design only; no complete core implementation is authorized here.
The implemented safe owned Session admission API is the policy dependency.
This document resolves its core borrow/abandonment boundary, not the complete
creation-capability or hardware-test specification.
## Private state and entry invariant
Keep each Process in its existing KernelBox. Do not move a Process into a
command, grow a local owner array, or introduce raw pointers. Proposed changes:
```rust
// process.rs; private fields, no public replacement API
struct Process<'pool> {
    space: Space<'pool>,
    session: Option<Session>, // Some only when runnable; initially Some
    // existing private context and handle fields
}
// process_group.rs; additional private Slot state
// PendingWait contains copied identities only, never user addresses/borrows.
enum DispatchState {
    Ready,
    Dispatching,
    Waiting { session: ParkedSession, pending: PendingWait },
    Closing { reason: End },
}
struct PendingWait { child: TaskId, capability_slot: u8, capability_generation: u32 }
```
A runnable slot requires Ready AND Process.session=Some AND no cancellation,
completion or retirement. Both the selector and final sealed-entry check enforce
this. Waiting and Dispatching never enter EL0. A parked Session lives in exactly
one place, the slot state; the Process remains owned with session=None. Closing
may release the quiescent Process while retaining scalar relationship metadata.
Legacy helpers retain Some Session and reject absence before entry. Never issue
a replacement Session or unwrap an absent one to obtain report counters.

## Scoped interfaces and borrow sequence

Private proposed interfaces:
```rust
// Checked extraction: failure returns before changing owner state.
fn take_session(process: &mut Process<'_>) -> Result<Session, InternalError>;
// Only used by the single finalizer after state and identity validation.
fn put_session(process: &mut Process<'_>, session: Session) -> Result<(), Session>;

// Local owned values; no pool/parent-memory references.
enum TaskCommand {
    Spawn { guard: TaskAdmission, request: task_wire::Spawn, input: Input },
    Wait { guard: TaskAdmission, pending: PendingWait, mode: task_wire::WaitMode },
    Cancel { guard: TaskAdmission, child: TaskId },
}
enum Settlement {
    Reply { session: Session, outcome: Outcome },
    Park { session: ParkedSession, pending: PendingWait },
    Terminate { session: Option<Session>, reason: End },
}
```

WAIT lookup constructs PendingWait from the validated current parent capability
slot/generation and child TaskId. Preserve it through Command into Park; do not
repeat an ambiguous handle lookup to reconstruct the identity.

1. Acquire and retain the existing Lease. Check platform, selected generation,
   Ready state and budget as before. Never release BUSY around task effects.
2. Following a restored SVC, set selected state Dispatching and extract its
   Session. Use a lexical parent borrow to build the fixed private Host and call
   owned `admit_task`. Complete admission becomes a local Settlement. A task
   admission retains its actual Session in its guard.
3. With short separate borrows, validate current parent's capability records,
   copy the entire descriptor and bounded payload from stopped Space, and produce
   TaskCommand. Validation failure consumes the guard into an error Reply.
   All source copies finish before allocation; no pointer/borrow enters Command.
4. End the selected-slot/Space/Host borrow. A private dispatcher can now borrow
   `&mut Group` sequentially because the command contains only owned data. It
   receives the existing `&Lease`; it never calls public `Group::admit`.
5. Child admission splits Group fields into executor, immutable factory/catalog,
   charged-frame accounting and slots. Select a free slot distinct from parent;
   use safe `split_at_mut` if simultaneous parent/free-slot metadata access is
   needed. Build using existing load/prepare/box helpers, then commit child and
   parent's capability together. Failure drops tentative owners under Lease,
   preserves slot/handle publication invariants and returns an error Reply.
6. WAIT yields Reply or Park; CANCEL changes only the validated child's pending
   cancellation flag and yields Reply. No wake runs recursively during dispatch.
7. End all temporary borrows; complete accounting, then pass the owned Settlement
   to the sole finalizer. No allocation or command effect occurs after accounting.

Change run_turn's current long-lived `&mut Slot`/owner/budget borrows into lexical
accesses before introducing this path. A Group method may orchestrate sequential
borrows; it must not keep `&mut selected_slot` while borrowing `&mut Group`.
Parent Space stays owned through every child allocation.

## Abandonment and single finalization

Do **not** implement a Drop guard borrowing Group, selected Slot or Process.
Use this owned return envelope; constructors are private:
```rust
#[derive(Clone, Copy)]
struct Counters { calls: usize, written: usize }
struct DispatchFailure { cause: InternalError }
struct DispatchResult {
    counters: Counters,
    result: Result<Settlement, DispatchFailure>,
}
```
Snapshot from the actual Session immediately after admit_task returns (from
Complete.session or TaskAdmission), before any fallible task helper. Task reply,
park and allocation do not change these counters. Legacy WRITE counters are
captured after its dispatch. Every helper preserves this snapshot in its return
envelope, including before dropping a guard on failure; pre-admission failure
uses the initial Session snapshot. No reconstruction from an absent Session.
No return or `?` escapes the outer turn between Dispatching and finalization.

The outer turn uses DispatchResult.counters for reports, validates
clock/platform restoration, and calls finalization under the retained Lease.
A dispatch failure without a Settlement closes the selected process as
Unavailable, retires entry authority, and reclaims/cascades through the existing
quiescent terminal path. It never manufactures a replacement Session. Before
any ordinary turn return, a bounded four-slot check rejects a residual
Dispatching state; treat it as an internal fail-closed terminal condition.
An unexpected restoration failure retains existing fatal handling: do not free
owners if boot mappings/quiescence cannot be established.

`put_session` requires state Dispatching, original selected TaskId still valid,
and an empty session field. On rejection the returned Session is dropped and
that process closes; no previous Session is overwritten. Every success installs
once; every failure/terminal branch drops the detached Session once. No unwind
cleanup is relied on: panic/fatal ends the kernel profile, never resumes userspace.

## Final budget/platform precedence and parked waits

A Settlement is provisional until the final clock/control checks and Budget
finish. Preserve existing precedence: detected platform/counter corruption wins
as Unavailable. Precisely: `if finish == Exhausted && end.is_none() {
end = Some(End::Budget); }`. Existing Some(Fault), Some(Exited), Some(CallLimit)
and other already-selected terminal reasons survive ordinary exhaustion.

For a provisional Park, budget exhaustion, turn limit, cancellation or platform
termination **discards ParkedSession and PendingWait** and enters Closing with
the selected terminal reason. Do not install Waiting, consume a future child
completion, or write a wake reply. A child created earlier in the same turn is
included in parent teardown and cannot enter after that parent's terminal commit.
For an ordinary Reply followed by terminal selection, no reply can enable ERET.

Only a nonterminal finalized Park installs Waiting. Wake management requires
Waiting, the unchanged parent TaskId, no terminal/cancel flag, and matching child
and capability identities. Revalidate all before consuming the held completion.
After validation, use `mem::replace(&mut slot.state, Dispatching)` to own the
Waiting value; no fallible user operation follows. Consume the matched completion
and ParkedSession::wake, then put_session under its Dispatching invariant, store
scalar x0/x1, and set Ready last. Unexpected install failure closes that parent;
it never reinstates Waiting or reuses the consumed completion. Validation failure
leaves Waiting unchanged for teardown. No second attempt/budget turn is created.

Wake/consume/Closing bookkeeping is bounded uncharged quiescent management:
maximum two child records per service step, no allocation, user-memory/bulk
copy, UART or callbacks; scalar stopped-context reply stores are permitted.
SPAWN validation, input/ELF copies, allocation and commit remain charged to the
parent. Existing deferred diagnostic serialization remains outside that charge.

## Detached-state compiler ordering

The old Process+Capture pointer barrier is insufficient for detached state.
Introduce private `TurnWork { dispatch: Option<DispatchResult> }`, initialized
empty before begin. Command/admission helpers complete and move their owned
result plus counters into TurnWork before the final counter read. Any error drops
its guard only after recording counters. No policy mutation follows that read.
A task-only clock takes `(&Group, &Process, &TurnWork, &GroupCapture)` as opaque
address inputs to the existing ISB/MRS asm with default memory effects (no nomem).
Use it for begin and final reads; borrows exist only for each call, not across
mutation. This covers detached final Session/ParkedSession and snapshots, plus
Group-owned child/capability mutations; it adds no pointer dereference or escape.
Transient commands must feed this stored result or committed Group effects before
the final read. Inspect linked code for copies, Session updates, rollback and
commit preceding the counter; formatting follows. No global-store-completion or
precise CPU-time claim, and no automatic reuse of the old ordering proof.

## Required review before implementation

Complete catalog/cap commit rollback and typed-error review remains required.
Test all failure snapshots, final termination after provisional WAIT, and wake
installation failures: no double consumption, lost accounting or terminal wake.
Type/size/linked-ordering/64KiB-stack gates and all legacy/Stage B regressions
remain mandatory. No assembly context-transition change is proposed.
