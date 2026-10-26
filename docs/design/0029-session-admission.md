# 0029 supplement: owned Session admission

Status: **safe policy API approved for implementation; core integration remains
draft and unapproved**. The scoped decision is recorded in
[review 0029](../reviews/0029-native-task-creation.md). It does not amend the
frozen codec or authorize Process/Group changes.
It complements [task creation](0029-native-task-creation.md). Existing WRITE,
EXIT, startup bytes, console issuer and legacy dispatch remain compatible.

## Ownership choice

Use an **owned guard**, not a guard borrowing a Session across child allocation.
A task-enabled process stores its session in a private state enum:
`Ready(Session)`, `Dispatching`, or `Waiting(ParkedSession)`. The executor moves
Ready into admission while the process is stopped under its retained lease.
Dispatching cannot enter EL0. The selected slot and user-space copy borrow end
before allocation; the local guard owns the actual Session, including its
console grant, counters and termination state. No fresh Session is issued.

`Session`, `TaskAdmission` and `ParkedSession` are neither Clone nor Copy and
have private fields. No public constructor creates either guard. Dropping an
unfinished guard drops its owned Session; the process remains Dispatching and
cannot resume. A private core scope guard marks that process for teardown on an
abandoned operation. Memory forgetting can retain only bounded inaccessible
metadata; it cannot restore Ready. No Drop implementation performs hardware
access, callbacks or automatic process reentry.

This changes the task-enabled process's session storage and therefore requires
source, type, size and compiled-stack review. It deliberately avoids detached
nonce tokens that could be applied to another Session or another Issuer.

## Proposed safe API

Names below are proposed exact interfaces in `hm_kernel::native`; the standalone
`task_wire` codec remains unchanged. `TaskOperation` is a copied enum containing
Spawn, Wait and Cancel, deriving Clone, Copy, Debug, Eq and PartialEq. Existing `Status` gains Denied = 8, Resources = 9 and Pending = 10; `Stop` and legacy meanings are unchanged.

```rust
pub enum Admission {
    Complete { session: Session, outcome: Outcome },
    Task(TaskAdmission),
}
pub struct TaskAdmission { /* owns Session and copied Request privately */ }
pub struct ParkedSession { /* owns Session privately */ }
impl Session {
    // Existing dispatch remains available and does not recognize task opcodes.
    pub fn dispatch(&mut self, request: Request, host: &mut impl Host) -> Outcome;
    // Only the sealed task-enabled core path selects this new interface.
    pub fn admit_task(self, request: Request, host: &mut impl Host) -> Admission;
}
impl TaskAdmission {
    pub fn operation(&self) -> TaskOperation;
    pub fn request(&self) -> Request;
    pub fn calls(&self) -> usize;
    pub fn written(&self) -> usize;
    pub fn reply(self, response: Response) -> (Session, Outcome);
    pub fn park_wait(self) -> Result<ParkedSession, Self>;
}
impl ParkedSession {
    pub fn calls(&self) -> usize;
    pub fn written(&self) -> usize;
    pub fn wake(self, response: Response) -> (Session, Outcome);
}
```

`park_wait` accepts only an admitted WAIT; rejection returns the unchanged guard.
`reply` and `wake` return `Outcome::Reply(response)` and the original Session
with no counter increment, output effect, console issuance or capability lookup.
These are safe policy results, not permission to touch another process. Only
private core code can install them into its selected owned process. `wake` is
called only after the internal pending-wait identity has been validated. Terminal
teardown drops the guard/parked session rather than manufacturing a reply.

The existing Host remains a safe policy abstraction. The core provides only its
sealed adapter; no public runtime accepts callbacks. For task operations,
admission calls neither Host method. After admission returns, no Host, user
buffer or parent-memory borrow is retained by a guard.

## One attempt counter and exact limit precedence

Both entry points use the same private admission/counting helper. A stopped
Session returns its existing Stop without counting. Otherwise increment calls
once before checking version or operation. Every malformed request counts.
A valid EXIT still wins as Exit on attempt 64. Legacy WRITE on attempt 64 still
performs its current validation/output and returns its Reply; termination is
then CallLimit, so the executor records that reply but never resumes EL0.

The new `admit_task` ordering is:

1. Existing stopped/count-overflow behavior, then the single attempt increment.
2. Wrong x0 version returns Version. Unknown operation returns Operation.
3. WRITE/EXIT use exactly existing evaluation and attempt-64 finalization.
4. For correctly versioned SPAWN/WAIT/CANCEL on attempt 64, return Complete with
   `Outcome::Stop(CallLimit)` and a stopped Session. Do not inspect the handle,
   scalars, descriptor or user bytes; no task guard, allocation or side effect.
5. On attempts 1..63, return Task even for malformed task arguments. The sealed
   dispatcher validates rights/scalars/copies using the reviewed wire precedence.

Thus attempt-64 wrong version/unknown opcode still returns the legacy error
Reply and sets termination CallLimit. Attempt-64 recognized task opcode with
an invalid handle instead returns Stop(CallLimit), without a handle oracle.
No task guard exists at calls=64; `reply`, `park_wait` and `wake` cannot enable
attempt 65. A parked WAIT already consumed exactly one attempt, and wake is not
another syscall. Console byte accounting remains exclusively Session-owned.

## Core sequencing and accounting

Under the existing lease, move the selected Session into `admit_task`. For a
Task guard, copy/validate the request and bounded child Input using the stopped
parent's Space, then end that Space borrow. Keep the guard local while performing
lease-bound child admission on disjoint group fields. Never call public
`Group::admit` recursively. Install the returned Session and response only after
commit or rollback; every exit branch either installs it once or tears down the
Dispatching process. No pointer to the temporary guard is stored in a slot.

For WAIT, install ParkedSession plus copied internal child TaskId/cap identity
in the parent's private Waiting state. Wake consumes both only after matching
completion; no self-referential borrow, user pointer or second dispatch survives.
Parent cancellation while waiting drops this owned state during Closing.

SPAWN copying, validation and allocation stay inside the parent's charged turn.
Bounded quiescent wake/consume/Closing management after finish is **uncharged**,
like existing post-finish reclamation: at most two child records per service
step, no allocation, user-memory/bulk copy, UART or callback. Scalar stopped
context reply stores are allowed. This replaces the main draft's infeasible
promise to charge wake to a cancellation turn that has no Budget::begin.

## Review and tests still required

Approve owned-session movement versus enlarging the existing Process state, and
freeze the private core abandonment guard before implementation. Test all legacy
attempt-64 cases unchanged; task attempts 63/64 with valid/invalid version,
opcode and handle; dropping/forgetting guards cannot resume; park on non-WAIT
returns its guard; wake preserves calls/bytes/console and occurs once; guard
private construction/Clone/Copy fails to compile. Core tests must prove original
Session continuity through child allocation failure and parent teardown, and
actual EL0 WAIT parking/wake with no second attempt. This supplement alone is
not approval of the Group transition machine or hardware protocol.

The safe implementation preserves the existing Session fields and their order.
The owned guard types have no Clone/Copy or public construction. Legacy dispatch
and the new entry share private attempt/finalization helpers; only the new entry
recognizes task operations. Core Process storage is unchanged by this stage.
