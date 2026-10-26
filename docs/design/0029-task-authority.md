# 0029 supplement: Group-owned task authority

Status: **authority component reviewed; complete core implementation remains
draft and unapproved**. See [review 0029](../reviews/0029-native-task-creation.md). Real Group catalog/capability lifecycle, depending
on the reviewed core-dispatch boundary and implemented task_wire codec.

## Principal and persistent issuance

Authority lookup always starts with the currently selected full `TaskId`
(group namespace, slot index, slot generation). Never resolve a user token in a
global table. One task-enabled Group has exactly one supervisor; children receive
no launch entry. Console tokens retain their independent existing slot1 issuer.
Task token low32 is slot2 Launch or slot3/4 Completion; high32 is a nonzero
checked generation. A new private `Executor.next_task_cap: u32` issues generations
across all task-enabled Groups for that executor. Zero means permanently exhausted.
MAX succeeds once then stores zero; no reuse. Group/TaskId/console IDs stay distinct.

```rust
struct CompletionCap { generation: u32, child: TaskId, rights: task_wire::Rights }
struct LaunchCap { generation: u32, console_allowed: bool }
struct TaskAuthority<'p> {
    root: TaskId,
    launch: Option<LaunchCap>,
    children: [Option<CompletionCap>; 2],
    catalog: [KernelBox<'p, Image<'static>>; 2],
    closing: Option<End>,
}
// Group owns Option<KernelBox<'pool, TaskAuthority<'pool>>>.
```
Require selected TaskId==authority.root, then low slot/generation/kind/rights
before child state. Missing,
foreign, stale, zero and wrong-kind tokens yield Handle; insufficient valid
rights yield Denied. Public codec values encode data, never authority. Child
copies of parent tokens remain unusable even when their bytes match exactly.
Fixed policy permits two outstanding children, max128 charged frames per child,
512 frames for the whole Group, WAIT or WAIT|CANCEL attenuation and optional
fresh console delegation. LaunchCap.console_allowed is set only by the private
constructor's policy argument and encoded into Grant.flags; a request for console
when false returns Denied before allocation. Never derive permission from child
request bits or userspace's copy of the envelope. No recursive spawn or transfer.

## Public-mode gates and private setup

```rust
enum EntryMode { Ordinary, TaskActive { root: TaskId }, TaskClosed }
// Additional Group Error::Mode; recoverable, no state/issuer/frame effects.
pub(super) fn new_task<'executor, 'pool, 'image>(
    executor: &'executor mut Executor, factory: Factory<'pool>,
    root_image: &Image<'image>, archive: &'static [u8], console_allowed: bool)
    -> Result<Group<'executor, 'pool>, Error>;
```
Group exclusively borrows Executor and retains Factory's shared Pool lifetime;
it retains no borrow of root_image.
It constructs a new empty Group, fixes limit512/catalog paths, and activates task
mode atomically with root publication. No conversion of a populated Ordinary
Group is allowed. Root console grant is enabled; console_allowed controls children.

Public gates in TaskActive: admit always returns Mode; info, completion,
request_cancel and reap accept only the exact root TaskId (others return Mode,
even valid children). Root info reports Closing explicitly; completion returns
None and reap returns Pending during Closing. Root cancellation is idempotent
while live/Closing and cannot replace a saved terminal reason. Root reap succeeds
only after structured child cleanup, then drops authority/catalog boxes, subtracts
metadata charge and switches permanently to TaskClosed. TaskClosed admits no new
tasks: admit/info/completion/cancel/reap return Mode; turn returns Idle. No silent
fallback to Ordinary or namespace/capability reset occurs. Ordinary mode keeps
all existing behavior. Private lease-bound child admission/cancel/reap bypass
public gates only while reconciling the matching cap and PendingWait relationship.
Group Drop never invokes these public gates: under the existing inactive-owner
contract it destroys all bounded owners/metadata without executing or replying to
any task. Drop need not synthesize externally observable completions. Forget
retains bounded claimed memory. No public child reap can strand a parked parent.

## Catalog and setup placement

The immutable embedded archive supplies program1 worker and program2 loop.
Complete archive/ELF admission precedes root publication. Each Image is boxed in
the same Pool and borrows static archive bytes, never user memory or a temporary
Archive. SPAWN performs no archive parse; artifact paths/hashes must be pinned.

Box each Image in its own noninline helper; retain only two Option<KernelBox>
handles before boxing TaskAuthority, never [Image;2] or Process/Group arrays.
Charge `2 * ceil(size_of::<Image>()/PAGE_SIZE)` plus
`ceil(size_of::<TaskAuthority>()/PAGE_SIZE)` metadata pages. Check arithmetic and
allocator alignment/actual pages; reject mismatched layout rather than guessing.
Group.charged includes these retained catalog/authority pages as well as all
Process frames and Process boxes. Held completions retain no Process frames.
Authority remains until root completion is consumed or Group drops; then its
boxes are dropped once. No metadata allocation is repeated per child spawn.

Setup ordering: acquire lease/admit platform, choose root slot and candidate
TaskId without advancing its generation, validate complete catalog, build boxed
Images, check metadata+root charge<=512, reserve root launch generation, encode
the actual launch grant into owned Input, prepare/box root, build authority box,
then publish root+authority+charge together. All tentative owners remain local
and drop under Lease on failure. No runnable root or authority is published on
partial failure. Launch/console generations may burn, but slot generation and
Group charge remain unchanged. Setup may fail at any real allocation; free-page
counts must return to their entry baseline. No Pool.free early shortcut replaces
actual load/startup/box rollback tests.

## SPAWN reservation and commit

SPAWN runs inside the selected parent's already charged turn and retained Lease.
The private dispatch function uses short parent borrows then safe disjoint Group
field borrows; it never calls public Group::admit (which acquires another lease).
The owned command carries copied descriptor/Input and original TaskAdmission.

1. Validate selected root, launch token, descriptor, requested rights/console,
   catalog membership and complete child input copy in the frozen wire order.
2. Find free completion entry and free Group slot with next!=0. Held completions
   count as outstanding. Candidate child TaskId uses the slot's existing next.
   Reject >2 children, requested frame cap or checked Group total excess with
   Resources before reserving a task-cap generation.
3. Required child charge is Image.page_count()+29+actual Process box pages,
   matching current load/startup ownership; require <=requested cap<=128. Build
   fresh child Budget using the existing100ms/1ms policy. No deadline/counter reset
   is applied to the parent. Check issuance capacity then reserve task-cap
   generation; from here it may burn on failure.
4. Load actual Space, prepare with copied Input and requested console, then box
   actual Process. Preserve existing console-generation burn and last-prepared
   semantics. Validate actual frame count/charge. No child is runnable yet.
5. Revalidate root identity/Dispatching state and reserved empty slot/cap entry
   under the same lease. All remaining operations are infallible field moves:
   publish child owner/budget/info, completion cap targeting exact TaskId, and
   new charged total. Return token only in the parent's provisional Reply.

Reservations are invisible scalar candidates. Load/startup/box/validation failure
drops tentative owners without publishing cap/child. Slot.next advances only on
reap; failures preserve it. Task/console generation burns never roll back.
Invariant/platform failure retires the Executor, not merely a Resources reply.
A successful commit followed by parent final Budget/Unavailable causes Closing;
that child is cancelled before any entry, and the unobserved token grants nothing.

## WAIT, cancellation and completion ownership

Validated WAIT retains the parent cap slot/generation plus child TaskId in
PendingWait. Completion lookup requires exact Group identity and child slot
identity as well as cap identity. Poll of live child yields Pending. Blocking
WAIT parks only after the parent turn's final checks; terminal finalization drops
the provisional park instead. Child terminal teardown releases its quiescent
Process/box and charge, retaining copied Completion and occupied slot.

For completed WAIT, compute the completion word, remove the one matching cap and
reap its exact child once, then return the word. Reap advances the child slot
next with checked add, retiring that slot at zero. A stale token cannot consume
a replacement child's result. Blocking wake performs the same consume/reap only
after validating Waiting and all identities, then uses the reviewed checked
Waiting→Dispatching→Ready install. Parent cancellation wins over wake selection;
no wake writes a reply into Closing or freed state.

CANCEL requires CANCEL rights, sets the validated live child's pending flag and
returns0; repeats pending return0, already-completed held child returns1. It
never consumes the cap or frees frames synchronously. Scheduler cancellation
uses existing quiescent terminal processing; WAIT-only rights do not restrict
kernel-owned structured teardown. No child entry follows selection of its cancel.

## Closing and bounded service

Root termination immediately removes Launch and makes its child caps unusable
from userspace. Save the root's original End in Closing; do not publish externally
observable root Completion yet. Drop the quiescent root Process and parked
Session, retaining only authority/relationship metadata. Mark both live children
cancel pending; reap already completed children through their exact identities.

Service at most two child records per step, using existing lease-bound cancellation.
Wake/reap/Closing bookkeeping is bounded uncharged management after finish, with
no allocation, user/bulk copy or UART. Parent End is not replaced by child End.
Publish root completion only after children are quiescent, their caps cleared,
and their held slots reaped. Authority/catalog charge persists until root reap.
Retirement is sticky; quiescent cancellation requires valid minimal boot context.
Unknown restoration remains fatal, retaining potentially active owners.

## Remaining gates

Review typed errors/commit borrows; measure Image/authority/Process/Group layouts
and all64KiB stack chains. Test allocation-prefix rollback, real Pool pressure,
rights/issuer/slot exhaustion and Closing races. Separately exercise logical512
cap with pinned large-BSS root/child images: legal <=128 child charges plus a
large admitted root and metadata must exceed512 on the second child. Prove exact
charges from actual sizes; do not alter Grant512 or substitute physical pressure.
Actual EL0 creation and ordinary supervisor remain required. Full mechanism and
hardware protocol approval are pending; no Phase3 completion claim.
