# 0028: bounded native process runtime

Status: **Stage A implemented and verified locally; B/C draft and unapproved**.
See the [independent review](../reviews/0028-native-process-runtime.md) and
[accounting evidence](../evidence/native-process-budget.md). No
runtime implementation or execution verification is implied. This specifies real resumable
ARM EL0 execution, then capability-scoped userspace creation and completion.
A separately testable safe accounting component is not completion of this
milestone. Existing [startup input](0027-native-startup-input.md),
[process entry](0023-arm-native-process.md), and
[native wire](0023-native-wire.md) remain the baseline. This is not a Phase 3
exit claim or a replacement for the existing [capability model](0003-capability-model.md).

## Objective and stages

The experiment is a bounded group of independently owned native processes:
a busy child cannot prevent a ready peer from executing; one process cannot
name another's memory or completion without an explicit scoped capability;
cancellation cannot release active memory. Measure dispatch order, charged
counter ticks, frame retention, and completion/reap behavior rather than
claiming a general performance advantage.

1. **A: safe turn accounting**, a small allocation-free policy directly used by
   stage B. It owns no pages, handles or scheduler queues. Unit and compiler
   tests establish checked timestamps, persistent budget and terminal states.
   Stage A alone is explicitly an unintegrated component, not a runnable
   multiprocess milestone.
2. **B: actual resumable execution and kernel-owned groups.** Refactor the
   existing private process loop into a sealed single-event transition; own up
   to four actual processes in separate `KernelBox` allocations; round-robin
   one hardware trap per turn. Run actual EL0 interleaving, timer resumption,
   cancellation and exhaustion workloads. Preserve all legacy execution gates.
   An ordinary monitor group-launch path is part of this stage, not only a
   special test image.
3. **C: scoped native spawn/wait/cancel.** Connect the same group ownership to
   versioned requests from actual EL0 parents and explicit launch authority.
   Freeze its wire and independently review before implementation. Kernel
   admission of several programs in B is not userspace spawn or IPC.

No stage introduces SMP userspace execution, Linux process semantics, dynamic
linking, shared user pages or service messaging. Those remain separate work.

## A: exact accounting component proposal

Proposed new safe `hm_kernel::process_budget` API (private fields, no `Copy` or
`Clone` for `Budget`; copied enums and snapshots are harmless):

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error { Configuration, State, Clock }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Finish { Ready, Exhausted }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot { pub remaining: u64, pub charged: u64, pub turns: u64 }
pub struct Budget { /* private state and checked counters */ }
impl Budget {
    pub fn new(total: u64, quantum: u64) -> Result<Self, Error>;
    pub fn begin(&mut self, now: u64) -> Result<u64, Error>;
    pub fn finish(&mut self, now: u64) -> Result<Finish, Error>;
    pub fn stop(&mut self) -> Result<(), Error>;
    pub fn snapshot(&self) -> Snapshot;
}
```

`total > 0`, `quantum > 0`, and `quantum <= total`. States are Ready, Running
with recorded start, Exhausted, Stopped, and Poisoned. `begin` is allowed only
in Ready with `turns < u64::MAX`, verifies nondecreasing time since the previous finish, and returns
`now.checked_add(min(remaining, quantum))`; it marks Running only after the
addition succeeds. It also checks `now + remaining` for overflow, even when
the shorter quantum comparator fits. Core copies remaining before begin to
calculate this same whole-turn budget end; it does not inspect private policy
state. `finish` is allowed only in Running, checks `now >= start`,
charges `min(now-start, remaining)`, increments turns with checked arithmetic,
and returns Ready or Exhausted. Equality consumes the budget. The final turn
can take longer than its comparator while bounded kernel work completes; the
ledger saturates at the budget, never wraps or claims exact EL0 CPU time.
Any invalid transition, counter regression or overflow poisons the object;
subsequent begin/finish/stop cannot revive it. Every rejected operation leaves
remaining/charged/turns unchanged: compute all candidate updates before commit,
including checked turn-count increment. A successful zero-elapsed finish still
increments turns and returns Ready. Always `remaining + charged == initial total`.
Configuration errors occur only in new; transition errors and exhausted turn
count capacity are State. Timestamp regression or deadline arithmetic overflow
is Clock. Begin rejects `turns == u64::MAX` before marking Running; finish still
uses checked addition defensively. `Budget` derives Debug only. `stop` accepts Ready or Exhausted,
marks Stopped, and rejects Running: hardware quiescence is not something the
safe policy can infer. Snapshot is always available. Dropping a Running policy
is memory-safe but cannot release the runtime's process owner or BUSY lease.

The actual group uses a 100 ms **active-turn** budget (`frequency / 10`) and a
1 ms quantum (`frequency / 1000`), under the existing admitted frequency bounds.
A turn starts immediately before platform admission and ends after restoration
and bounded syscall dispatch. It includes kernel work within that interval;
parked time between turns is not charged. No timestamp assembly amendment or
claim of precise user CPU time is required. Before dispatching a captured SVC,
read the counter against that whole-turn budget end: exhaustion wins over
executing the request. Check the same end immediately before ERET; a quantum
comparator that has passed during admission may cause an immediate IRQ, but an
already exhausted whole budget must never enter EL0. Complete the
turn charge after any permitted bounded dispatch; exhaustion then prevents
another entry. Counter failure makes the runtime fail closed after quiescence.

Frozen Stage A integration inventory (`--test process_budget`):
`configuration_bounds`, `zero_and_one_elapsed`, `quantum_clamping`,
`exact_exhaustion`, `overrun_saturation`, `repeated_turns`, `parked_time_uncharged`,
`equal_timestamps`, `backward_begin`, `backward_finish`, `deadline_overflow`,
`invalid_transitions`, `terminal_no_revival`, and `stop_ready_and_exhausted`.
Each rejected-operation case verifies unchanged snapshots and subsequent
non-revival. `deadline_overflow` includes whole-budget overflow when the quantum
would still fit. Constructors cover 0 and u64::MAX boundaries, not only normal
frequencies. A fifteenth case `turn_counter_overflow` in this same dedicated integration
target imports the actual source under cfg(test), with a private self-check that
finishes MAX-1 to MAX, rejects the next begin, and rejects
a deliberately forged test-only Running/MAX finish with unchanged counters;
production rlibs contain no seam. Compiler clients reject private fields and
Budget Copy/Clone; valid clients move the owner and copy snapshots. No fake
clock abstraction enters core: core supplies actual synchronized counter reads.
This component must be exercised by actual stage B turns.

## B: ownership and API

Proposed core module `process_group`, with `Group` and all process owners
`!Send`, `!Sync`, non-Clone. All methods return with boot root, original vectors,
GIC and timer state restored. No public step context or active guard exists.

```rust
pub struct TaskId { /* private group namespace, slot, slot generation */ }
pub struct Group<'executor, 'pool> { /* four small slots, no Process array */ }
pub enum Turn { Idle, Progress(TaskId, Event), Completed(TaskId, Completion) }
pub enum Event { Reply, Quantum, Spurious }
pub enum End { Exited(u8), Fault, Budget, CallLimit, TurnLimit, Cancelled, Unavailable }
pub struct Completion { /* copied final report and accounting, no pointers */ }
impl<'executor, 'pool> Group<'executor, 'pool> {
    pub fn new(executor: &'executor mut Executor,
               factory: Factory<'pool>, frame_limit: usize) -> Result<Self, Error>;
    pub fn admit(&mut self, image: &Image<'_>, console: bool,
                 input: Option<&Input>) -> Result<TaskId, Error>;
    pub fn turn(&mut self) -> Result<Turn, Error>;
    pub fn request_cancel(&mut self, id: TaskId) -> Result<(), Error>;
    pub fn completion(&self, id: TaskId) -> Result<Option<Completion>, Error>;
    pub fn reap(&mut self, id: TaskId) -> Result<Completion, Error>;
}
```

This is a proposal to freeze during independent review, not an already exposed
API. `Error` distinguishes Unsupported, Bounds, Full, Resources, Stale,
Pending, Busy, and Unavailable; loader/prepare rejection retains its typed
cause where useful. `TaskId`, `Completion`, and scalar report values may be
Copy; constructors and ownership-bearing fields remain private.

One CPU only, with exact topology `[0]`, is the first group profile. `Factory`
is the existing privately minted proof binding the actual pool/boot tables;
`Group::new` checks matching executor admission and takes it by value. No safe
function gains boot authority from arbitrary topology arguments. Existing
legacy two-CPU process execution remains unchanged and tested separately.

Each live slot contains an `Option<KernelBox<'pool, Process<'pool>>>`, persistent
budget, slot generation and small state. A terminal slot holds copied completion
instead of a Process. No `[Process; 4]`, array of `Space`, or global mutable
process table. The group borrows the existing persistent Executor exclusively;
its console Issuer and private output mode cannot be reset or interleaved with
another executor through this borrow.

The Executor issues a separate checked nonzero u64 group namespace, never
based on the console token (a no-console prepare has handle zero). A task token
contains namespace, index 0..3 and nonzero u32 slot generation. Check all fields
before any effect. Reuse advances slot generation; wrap retires that slot.
Group namespace exhaustion retires group creation. Forgetting a group never
permits namespace reuse. This is internal owner identification, not another
user capability model.

Four occupied slots maximum, counting unreaped completions. `frame_limit` is
1..512 and includes every Space frame, startup frame and actual rounded
KernelBox storage pages. Precompute checked image geometry and storage charge
before allocation/issuance. Actual frame count must agree after construction.
A compile-time bound requires `size_of::<Process>() <= 4 * PAGE_SIZE`; charge
actual rounded size, not that upper bound. The pool itself can still exhaust
or fragment. Failed load/prepare/box returns reclaim all temporary owners and
leave slot and group charge unchanged. Native console generations may have
been burned by a late prepare failure; they never roll back or repeat.
No saved borrow to the Image, Input, their sources or caller buffers survives
successful admission. Input is copied into the existing owned RO startup page.

Boxing is not a stack optimization by assertion: `Pool::box_value` takes T by
value and returns T on error. Construction helpers must borrow error metadata
and let owned errors drop rather than moving duplicate large owners through
several frames. Inspect actual linked call paths, including admission and
rollback, before any guest. A private in-place construction mechanism would
need a separate design amendment if ordinary boxing cannot fit; increasing the
64 KiB stack is not an acceptable fix.

## B: turns, restoration, completion and cancellation

Round robin starts at slot zero and advances past the selected slot after every
turn, including SVC, IRQ, cancellation and fault. A turn handles at most one
assembly return; a spurious IRQ yields and counts toward a persistent maximum
of eight per process, never an unbounded retry loop. Empty/completed slots are
skipped by a scan of at most four entries. `Idle` means no runnable or pending
cancel slot; it does not silently reap completions.

Each turn acquires existing BUSY before platform inspection and retains it
through assembly, restoration verification, dispatch and terminal owner drop.
Busy before selection has no effect on cursor, accounting or task state.
Core additionally checks both budget and quantum ends against the admitted
56-bit MAX_COUNT, not merely u64 arithmetic. This hardware-specific check stays
outside the reusable safe policy. CPU0,
masked DAIF, boot roots, timer/GIC admission and immutable platform invariants
are checked again for every entry. The one-CPU profile avoids a peer proof
spanning parked turns. There is no Rust execution under the process root.

Cache preparation occurs once per newly admitted image before its first entry.
Resume uses the owned context saved by `arm_process_slice`, with existing PC
alignment/RX validation and existing sanitized SPSR (saved NZCV only plus
`0x00400340`) on every entry. GPR/SIMD/FP/TLS state ownership and the documented
DIT/SSBS/TPIDRRO sanitization remain as in 0023. No syscall counters, written-byte
budget, spurious bound or resource charge reset when the process yields.

Arm the comparator at the minimum of quantum end and remaining active budget
end. A captured timer event with verified timer IRQ/ISTATUS returns Quantum
if budget remains; otherwise it completes Budget. A SVC performs at most one
existing bounded native dispatch, stores reply registers, and yields. The
64th reply ends CallLimit before any subsequent ERET. Exit/fault/budget produce
terminal completion. A private admitted counter read brackets each turn; no
reconstruction of a precise trap timestamp from private assembly scratch is
claimed. Post-entry restoration mismatch remains fatal, as today, rather than
returning an owner whose hardware state is uncertain.

`request_cancel` validates the token and marks pending cancellation; it does
not drop the Process, release frames, or synthesize a completed result. It is
idempotent for an already pending or completed matching token. The next selected
turn services pending cancellation before any ERET, under BUSY and known boot
state, then drops the Process and its box and publishes Cancelled. There can
be no concurrent cancellation call during an active turn in the safe API.
Completed matching tokens stay valid until reap; stale/reaped/foreign tokens
have no hardware or allocator effect. Reap of a live/pending task is Pending.

Terminal completion reclaims process and box frames immediately after verified
quiescence, subtracts exactly their charge, and retains the small completion
record/slot. Reap consumes that record and permits slot reuse. This separates
physical reclamation from owned completion delivery. Drop of a Group drops all
inactive owners and records exactly once. Every exposed state is inactive, so
Drop requires no switching or timer operations. Forget leaks inactive claimed
frames but neither a live root nor BUSY; allocator ownership prevents reuse.

An unavailable platform before entry preserves owned inactive processes but
retires further group entry; cancellation/drop must remain available. A core
error after a verified restored slice records Unavailable and retires the group;
remaining owners can be cancelled/reaped or dropped. Retirement disables ERET,
not cancellation service: turn continues selecting pending cancellations, under
BUSY and boot-root proof, even after entry has been retired. Never turn an uncertain
restoration into an ordinary error. Freeze these branches in source review.

Additional B freeze proposals: a per-process maximum of 256 successful begin/
finish turns provides an explicit cap even for zero-duration events. Before
attempting turn 257, complete TurnLimit without ERET. Ordinary 64-call/eight-
spurious/100-ms bounds remain independently enforced, not replaced by this cap.
A no-entry whole-budget expiry is an observable completed Budget turn, with no
fabricated IRQ. Each slice compares restoration against an immediately captured
admitted kernel snapshot, not a parked user snapshot or a stale constructor
observation; parked monitor diagnostics must not be mistaken for process damage.

Terminal precedence is explicit: cancellation selected before entry wins without
running. A pre-entry or pre-dispatch whole-budget expiry is Budget and suppresses
that dispatch. Once a permitted dispatch has produced Exit or CallLimit, retain
that outcome even if bounded dispatch overhead consumes the last budget ticks;
a normal Reply then becomes Budget when final charge exhausts. Captured Fault
remains Fault after final charging. Counter failure instead retires execution as
Unavailable after quiescence; restoration uncertainty remains fatal. Every path
with a successful begin finishes accounting once before stop/publication.

## Legacy adapter and ordinary use

`Executor::run(Process)` remains the existing public consuming API. It uses the
same private entry/dispatch primitive with its original absolute 100 ms wall
clock limit, continuing SVCs internally, treating its timer as terminal, and
preserving the existing literal HMPROC grammar, run numbering and two-CPU
PSCI admission counts. A group turn is a separate sealed scheduling mode;
legacy tests must not acquire extra quantum IRQs or new output records.

Add ordinary `run-group <path1> <path2>` after core review. Both exact archive
paths are admitted before either executes, using the existing checked archive
and persistent terminal Executor. Failure of either admission drops the whole
unstarted group. No console callback is introduced. The driver loops over
bounded turns, reports each terminal result and reaps it, then returns to the
prompt. Existing `run` behavior remains unchanged. Two real worker programs
consume copied startup job bytes, yield through bounded console writes and
finish independent results. Interactive gates also pair a busy loop with hello
and require hello completion before the loop budget outcome. This is useful
kernel-scheduled multi-program execution, explicitly not userspace spawn.

## C: actual scoped creation and owned completion requirements

The subsequent wire amendment must connect to the same Group, not replace it
with a second simulated scheduler. A parent receives an explicit launch grant
bound to an immutable checked archive/program allowlist, group slot/frame
limits and permitted child console authority. No ambient path lookup, global
process IDs or implicit inheritance. Core derives the caller from the selected
owned task; userspace cannot nominate the authority-bearing parent.

Spawn copies bounded path and input bytes into kernel-owned scratch while the
parent is stopped and the boot root is restored. Full validation precedes
allocation/issuance where possible. No pointer into parent pages is retained
by the child or completion. Existing copied startup input is the data handoff.
A child gets its own process-scoped console grant if authorized, never a copied
parent token accepted across principals. Grant delivery must preserve the exact
v1 prefix and 0027 full-tail codec: use a separately versioned parent job payload
inside the existing owned input (containing an actual core-issued grant token),
or explicitly review a later startup version. Do not append capability fields
to padding that existing parsers require to be zero.

Wait/cancel rights use a parent-owned typed handle table, with generation and
kind checks distinct from console slot 1 and internal TaskId. Successful spawn
returns a child completion capability. Wait may park the parent; completion
stores an owned scalar outcome and wakes it. Consuming wait/reap is exactly
once. Cancel requests termination without freeing active ownership. Parent
termination closes authority and requests cancellation of owned children;
retain the group until all children are quiescent, rather than creating
unbounded orphans. No delegation or handle transfer is implied initially.

Before C code, freeze request layouts/opcodes, error precedence, generation
retirement, failed-spawn rollback, waiting-parent budget semantics, structured
teardown and console output policy. Required end-to-end workload is an actual
EL0 parent spawning at least two EL0 workers, waiting for one result and
cancelling another under exhaustion. Service IPC remains further work; copied
startup jobs plus owned completion are its prerequisite, not a claim of IPC.

## Verification and implementation ownership

Stage A needs safe unit/compiler tests and direct use in B. Stage B needs
independent design/source review, compiler ownership clients, all affected
linked-stack audits, exact fixture/archive hashes and strict serial plus QEMU
exception trace matching at 64/128 MiB, one CPU. Preserve existing ARM/x86,
66-run legacy process, startup input and normal monitor gates.

The [hardware verification proposal](0028-runtime-tests.md) expands these
obligations and still requires exact inventory/grammar freeze. Required real
hardware cases: two different address spaces using identical VAs
with distinct data; alternating SVC progress and persistent console counters;
actual timer resume at saved ELR rather than restarting entry; busy loop resumed
at least twice while hello advances/completes; full context sentinels across
both SVC and timer handoff; budget exhaustion across many turns; 64th-call and
byte-budget behavior across turns; cancellation retaining frames until service;
held completion preventing slot reuse; reap/reuse with stale and foreign token
rejection; partial load/prepare/box failure rollback; group drop with live
inactive tasks; baseline recovery after faults and cancellation.

Avoid a fixed loop-count timing assumption for context tests. A test-only
private hook may release a canonical fixture's fixed RW polling word after at
least two actual timer IRQs and peer progress. It must validate the owning task,
fixture identity, exact fixed VA and RW mapping; no public arbitrary write,
context/PC/SP change or mutable frame exposure. Two probes use different scratch
registers so their complementary sentinel coverage includes all GPRs. The hook
is cfg-only, operates on stopped owned pages under boot root, and requires
independent review before implementation.

The harness must bind each actual slice to stable group/task identity, ordinal,
entry PC, captured resume ELR, trap kind, cumulative calls/bytes/IRQs, budget
before/after, exact frame balance and restoration evidence. Invalid token paths
must generate no EL0/GIC/HVC activity. Exact profile inventory, wire record
fields and fixture symbols are a remaining **pre-implementation freeze** for
stage B; stage A does not authorize hardware edits while they remain open.

Work split: safe accounting/tests in hm-kernel; independently reviewed core
Group/Process refactor; native fixtures and archive builder; independent host
trace/type validators; ordinary monitor adapter; parent-serialized builds and
stack audits. Keep changes reviewable and preserve rejected candidates.
No new ARM architectural instruction semantics are assumed: reuse the primary
sources and restoration contract linked in 0023; any assembly/layout change
requires an explicit amendment with primary architectural provenance.
