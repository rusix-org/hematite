# 0029 supplement: typed lifecycle and reports

Status: **ordinary mechanism and demo verified locally; detailed negative matrix pending**.
Test injection remains unapproved; see [review 0029](../reviews/0029-native-task-creation.md).
Ordinary Group/legacy wire behavior remains unchanged.

## Error domains and admission

Add `Error::Mode` and `Error::Catalog(boot_image::AdmissionError)` to existing
process_group::Error; keep its existing variants. Private internal error type:
`InternalError::{State, Identity, Accounting, Clock, Platform}`. All five mean
Unavailable, sticky Executor retirement, no resumable syscall error. Lost boot
restoration remains fatal rather than returning/reclaiming uncertain owners.

new_task precedence: context/profile, lease, platform, namespace reservation,
catalog admission/boxing in the [authority setup order](0029-task-authority.md),
checked layout/charge, issuance, root/authority allocations, atomic publication.
Wrong archive/missing/directory/invalid ELF maps Catalog(cause), nonretiring.
Unsupported profile maps Unsupported; malformed configured bounds/layout overflow
maps Bounds, nonretiring before allocation. Group namespace or task-cap issuer
exhaustion maps Resources and permanently disables that exhausted issuer only.
Console issuer exhaustion maps Prepare(Generation), retires Executor as existing
prepare policy requires. Busy is retryable with no namespace/cap generation burn.
Later failures may burn namespace/task/console generations, never roll them back.

Preserve actual load_boxed mapping, in both modes: load/prepare Budget or
Exhausted returns Resources without retirement; every other load error returns
Load(cause) and every other prepare error returns Prepare(cause), retiring the
Executor. This includes Busy returned unexpectedly inside the retained lease.
External Lease::acquire Busy remains recoverable Busy, with no issuance. Box
allocation failure is Resources. Frame/accounting mismatch returns Unavailable
with retirement. Do not replace diagnostic Load/Prepare causes in setup results.
Task dispatch instead converts their resource class to Status::Resources and
retiring class to terminal Unavailable; no Load/Prepare value crosses the wire.

new_task never calls public Group::new while holding a Lease. Share a private
construct_empty_under_lease(executor, factory, &Lease) helper: after preliminary
context/profile checks, acquire lease once, admit platform, reserve namespace,
construct empty private Group, then catalog/charge/cap/root setup and commit.
Namespace reservation precedes catalog admission; catalog failure may burn it.
For simultaneous faults, first failure in this order wins; rollback does not
replace that typed error unless restoration itself becomes unknowable/fatal.

For task syscalls, no Group Error value crosses the wire. Frozen dispatch order
is Session version/opcode/64th limit, selected principal/token, required rights,
scalar/request length, complete request copy and codec validation, catalog and
console delegation, complete input copy, slot/quota/frame checks, issuer capacity,
then actual child allocation/commit. Mappings to native::Status:

| Cause | Status | Effect |
| --- | --- | --- |
| Invalid principal/token/kind/generation | Handle | no lookup in another principal |
| Missing WAIT/CANCEL rights or denied console delegation | Denied | no allocation |
| Request x3 !=64, codec Length, payload >256 | Length | no payload copy |
| Complete user copy fails or codec Address | Address | no child publication |
| Codec Header/Flags/Reserved/Rights/Frames/Input/Arguments | Argument | no allocation |
| Unknown program number | Argument | no allocation |
| Full completion/Group slots, quota, frame ceiling or cap issuer | Resources | no publication |
| Actual load/startup/box exhaustion | Resources | rollback; reserved generations may burn |
| Live child polled | Pending | no consumption |

Codec Handle/Limits/Completion cannot originate from the admitted SPAWN/WAIT/
CANCEL decoders; encountering them internally means State, not a user status.
Task console issuance exhaustion becomes terminal Unavailable with retirement,
not Resources. Wait-only requests are valid attenuation, not missing rights.
Error replies have result0, count once and remain subject to terminal finalization.

## Observable task states and turns

Extend TaskState with `Waiting` and `Closing`. Dispatching stays private and may
never be returned. Closing preserves the original terminal reason and report,
while holding no parent Process; Waiting retains that Process and parked Session.
Add one Turn variant for task-enabled mode; existing variants remain Ordinary:

```rust
pub enum TaskTurn {
    Step { report: StepReport, terminal: Option<End>, management: Management },
    RootCompleted { completion: TaskCompletion, management: Management },
    Administrative { management: Management }, // no selected process turn
}
pub struct Management { // private fields, copied getters for each
    root: TaskId, woken: Option<TaskId>, reaped: u8,
    children_remaining: u8, closing: bool, live_frames: usize,
    closed_root: Option<RootClosure>,
}
pub enum RootEvidence { Executed(StepReport), Administrative { trigger: Option<TaskId> } }
pub struct RootClosure { root: TaskInfo, end: End, evidence: RootEvidence }
pub struct TaskCompletion { closure: RootClosure, live_frames: usize }
// All above have private fields/copied getters; enums/scalars are Copy, not authority.
```

A Step corresponds to one selected real process turn, including zero-entry
cancellation, and uses existing ordinal/Timing/counters/trace meanings. Waiting
parents are skipped by selection. A child terminal Step has terminal=Some and
report.task.state=Completed; it is not externally reapable. The parent terminal
Step has report.task.state=Closing and terminal=Some(original reason), even if
there are no children. Its actual root teardown and charge reduction precede the
report. Preserve its complete captured StepReport privately for final publication.
Executed evidence is the original root terminal report, never a child report.
Administrative evidence has no parent ordinal, Timing, trap or ERET fields.

After each Step, bounded management may reap up to two held child results for
Closing, or consume the one matching completed WAIT and wake its live parent.
Management records the actual counts/identity and final charged frame total.
It never overwrites the Step's captured hardware/counter evidence. report.live_frames
and management.live_frames both describe the post-management total; Step Timing
excludes management. Parent cancellation is tested before wake; it suppresses
wake and marks that parent for cancellation selection.

If root is Closing, subsequent turn calls select only cancel-pending children,
never enter EL0, and report their cancellation Steps normally. Before selection,
reap at most two already-held child results for Closing. When all children are
quiescent/reaped, the next call publishes RootCompleted (no synthetic Step), using
a TaskCompletion containing the immutable saved RootClosure and current metadata
charge; its closure root snapshot remains Closing. Management reports reaps in that
call. Returning RootCompleted occurs once; later turn calls return Idle while
root completion is held. A terminal root with no children still publishes on a
separate next call. At most two child cancellation Steps and one publication call
follow a root's Closing Step. No Idle is allowed while cleanup remains serviceable.

A minimal-context failure prevents cleanup and returns Unavailable while keeping
owners/Closing metadata. It must not publish completion or falsely claim charge0.
On child-triggered retirement, before any wake, close any Ready/Waiting root as
Unavailable under validated minimal context: drop its Session/Process, invalidate
launch, mark children cancelled, store Administrative{trigger:Some(child_id)}.
The child's Step.management.closed_root reports this exact transition; never
invent a root Step or overwrite an existing RootClosure. A preselection retirement
with no Step closes it with trigger=None and returns TaskTurn::Administrative.
If minimal context fails, retain owners and return Unavailable; next valid call
performs this cascade before selection, even though Executor remains retired.
Completed/Closing roots preserve their saved End. Cleanup continues quiescently.
No UART/user copy/allocation in management; diagnostic emission stays separate.

## Terminal precedence and provisional effects

Preselection sticky retirement first closes an unfinalized root as Administrative
Unavailable; otherwise selected cancellation wins without ERET. During a started
turn, broken clock/platform/control invariants win as Unavailable.
Otherwise preserve any already selected End (Fault, Exit, CallLimit, etc.);
Finish::Exhausted selects Budget only when end=None. TurnLimit applies before
admitting a further execution turn, as in0028; it never overrides a prior End.
Retain the root's first finalized End throughout Closing; child failures or later
cancel requests do not replace it. New corruption during cleanup blocks progress
with Unavailable but does not rewrite the saved root result.

A provisional Park/Reply is not installed if finalization selects a terminal End.
Drop its owned policy state, close the root, cancel any committed children before
their next entry. If no terminal End, install the reviewed Ready/Waiting state
once. Child completion wake is a different subsequent management action requiring
still-valid Waiting identities; it cannot resurrect a terminated parent. Immediate
completed WAIT may already have consumed a result during charged dispatch; a
later terminal parent never consumes it again or reverses that successful reap.

## Public gates, charge and teardown

To preserve existing Completion::report()->StepReport, add task_completion(root)
->Result<Option<TaskCompletion>,Error> and task_reap(root)->Result<TaskCompletion,Error>.
In TaskActive, old admit/completion/reap always return Mode. New task accessors,
info and request_cancel accept only exact root ID; others return Mode unchanged.
Waiting/Closing task_completion=None, task_reap=Pending; info returns actual state.
Repeated root cancel while Closing/Completed is Ok without replacing End.
Successful task_reap removes its held slot, advances slot generation normally,
drops authority/catalog, subtracts metadata charge, and enters permanent
TaskClosed. TaskClosed mutations/lookups return Mode and turn returns Idle.
Task-only accessors return Mode in Ordinary/TaskClosed. Mode checks precede
lease/MMIO/issuance; private lease-bound child operations
alone reconcile capability records and pending waits. Read-only namespace and
charged_frames remain available in every mode; charge becomes0 after root reap.

Invariant: charged_frames = retained authority/catalog box pages + live Process
frames/boxes. Held child/root Completion and capability metadata allocate no
additional pages; metadata charge remains until root reap or Group Drop. Live
child teardown subtracts its actual charge exactly once, independent of WAIT.
Logical512 and physical gates differ. Drop releases inactive owners without
execution/replies/completions. Forget retains only bounded claimed resources.

## Required implementation and protocol gates

Hardware protocol must encode Step versus publication and management explicitly;
record charges and distinguish Executed from Administrative root evidence. Exercise
mapped failure causes, Waiting-parent cancellation, held completion, no-child
Closing, two-live-child Closing, retirement cleanup and permanent TaskClosed.
No injection API is approved; source/linked ordering/layout/stack review precedes guests.
