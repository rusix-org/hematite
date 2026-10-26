# 0028: bounded native process runtime

Status: **Stage A implemented and verified locally; Stage B design and hardware protocol approved; Stage C draft and unapproved**.
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
After a preliminary no-effect platform admission, a turn starts immediately
before the per-entry admission recheck and ends after restoration
and bounded syscall dispatch. It includes kernel work within that interval;
parked time between turns is not charged. No timestamp assembly amendment or
claim of precise user CPU time is required. Before dispatching a captured SVC,
read the counter against that whole-turn budget end: exhaustion wins over
executing the request. Check the same end at the final Rust admission boundary immediately before
the sealed transition. Assembly setup can cross that comparator before ERET;
the armed timer then regains control. This is a conservative accounting bound,
not a hard real-time promise that no user instruction runs after the counter
threshold. The buffered-diagnostic amendment below supersedes the initial
Trace serialization boundary: ENTER is emitted retrospectively only for actual
entry, and private diagnostic serialization is outside the charged interval. No new assembly timestamp/check path is introduced. Complete the
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId { /* private group:u64, slot:u8, generation:u32 */ }
pub struct Group<'executor, 'pool> { /* four small slots, no Process array */ }
pub enum Turn { Idle, Progress(StepReport), Completed(Completion) }
pub enum Event { None, Svc, Timer, Spurious, Fault }
pub enum End { Exited(u8), Fault, Budget, CallLimit, TurnLimit, Cancelled, Unavailable }
pub enum TaskState { Ready, CancelPending, Completed }
pub enum Error {
    Unsupported, Bounds, Full, Resources, Stale, Pending, Busy, Unavailable,
    Load(user_space::Error), Prepare(process::Error),
}
pub struct TaskInfo { /* private copied metadata, getters below */ }
pub struct StepReport { /* private copied metadata, getters below */ }
pub struct Completion { /* private copied final report, no pointers */ }
impl<'executor, 'pool> Group<'executor, 'pool> {
    pub fn new(executor: &'executor mut Executor,
               factory: Factory<'pool>, frame_limit: usize) -> Result<Self, Error>;
    pub fn admit(&mut self, image: &Image<'_>, console: bool,
                 input: Option<&Input>) -> Result<TaskId, Error>;
    pub fn turn(&mut self) -> Result<Turn, Error>;
    pub fn request_cancel(&mut self, id: TaskId) -> Result<(), Error>;
    pub fn info(&self, id: TaskId) -> Result<TaskInfo, Error>;
    pub fn completion(&self, id: TaskId) -> Result<Option<Completion>, Error>;
    pub fn reap(&mut self, id: TaskId) -> Result<Completion, Error>;
    pub fn charged_frames(&self) -> usize;
    pub fn last_prepared_generation(&self) -> Option<u32>;
}
```

All enums and copied metadata derive Copy, Clone and Debug; equality is provided
where all constituent fields support it. Private fields have getters, never
setters or public struct constructors. TaskId getters are `group() -> u64`,
`slot() -> usize`, `generation() -> u32`. TaskInfo getters are `id()`, `state()`,
`root() -> u64`, `process_frames() -> usize`, `box_frames() -> usize`,
`console_generation() -> Option<u32>`, and `budget() -> process_budget::Snapshot`.
A no-console process has None, never a fabricated console generation.

StepReport getters are `task() -> TaskInfo`, `event() -> Event`,
`ordinal() -> u64`, `entered() -> bool`, `entry_pc() -> u64`,
`elr()/esr()/far()/spsr() -> u64`, `calls()/written() -> usize`,
`irqs()/spurious() -> u64`, `boot_root()/vbar() -> u64`, `controls() -> bool`,
`charged_delta() -> u64`, `timing() -> Option<Timing>`, and
`live_frames() -> usize` (group total after turn). Timing is copied private data
with `start()/end()/comparator()/hard_end() -> u64` and `accounted() -> bool`
getters. None means begin never succeeded; Some contains actual bracketing
counter observations and actual comparator. Accounted=false records a rejected
finish (including a backwards end); snapshot counters then remain unchanged.
Event::Svc covers both replies and terminal Exit/CallLimit; Event::Timer covers
both resumed quantum and terminal budget IRQ. End determines terminal reason.
Completion getters are `end() -> End` and `report() -> StepReport`.
Ordinal is the committed selected-task turn count, starting at1, including
cancellation, begin/finish clock failure and other no-entry terminal selections.
BUSY/minimal-context or full platform admission failure before selection commits
no ordinal/cursor change and returns Error while retaining owners. Once admitted
selection commits, every outcome has a report and advances the cursor; policy turns separately count successful
begin/finish. Initial/cancel-before-entry trap fields are zero, entered=false,
entry_pc=ELF entry and Event::None. A resumed no-entry completion retains the
last captured trap metadata but does not claim a new trap. Neither completion
nor info exposes the console token or pointers. Diagnostics retain historical
root/frame counts after frames are freed; `live_frames` is the live balance.

Error precedence is bounds/unsupported/retired before capacity/resource work,
then Busy/context admission, then checked image geometry/budget, load, prepare,
box allocation. Full means four occupied/retired slots; Resources means the
aggregate cap or backing heap cannot satisfy admission. Load/Prepare preserve
actual typed errors; their Budget/Exhausted cases may be mapped to Resources,
but Corrupt/Context/Generation retire entry authority and return the typed
cause. Stale/Pending apply to token operations. No input or image borrow is
retained. Read-only info/completion and invalid token/cancel/reap operations
perform no allocation or hardware action.

Private `Factory::reborrow(&self) -> Factory<'pool>` copies only the shared
pool reference, admitted CPU count and non-Send marker for successive groups.
It duplicates no page, issuer or mutable authority; load already uses &self.
Independent review approved this proof reuse, with all admission checks retained.

Group::new validates frame_limit1..512, exact one-CPU matching factory/executor,
CPU0 boot context, BUSY availability and existing platform admission before
issuing a group namespace. All preceding failures leave the namespace unchanged.
On success it consumes the next nonzero namespace; MAX is issued once, then
creation is permanently retired. No further fallible work follows issuance.
The borrow of Executor lasts until group drop. Context that cannot establish
boot-root safety cannot be treated as an ordinary safe destruction environment;
the existing core authority contract still controls where these APIs execute.

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
before any effect. Slots begin with next generation1. Successful admission alone commits the
reserved generation. Failed admission changes neither slot generation nor
occupancy. Reap advances the next generation; reaping MAX retires that slot
instead of wrapping. A retained old token never matches a new occupant.
Group namespace exhaustion retires group creation. Forgetting a group never
permits namespace reuse. This is internal owner identification, not another
user capability model.

Four occupied slots maximum, counting unreaped completions. `frame_limit` is
1..512 and includes every Space frame, startup frame and actual rounded
KernelBox storage pages. Precompute checked image geometry and storage charge
before allocation/issuance. Actual frame count must agree after construction.
Current measured `size_of::<Process>()` is 9,832 bytes, requiring three
4 KiB box pages. Budget/slot metadata stays outside Process. A compile-time
bound requires `size_of::<Process>() <= 4 * PAGE_SIZE`; charge actual rounded
size, not that upper bound. The emitted-size oracle is regenerated for the
actual candidate, rather than trusting this historical measurement. The pool itself can still exhaust
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
Preliminary full admission precedes selection commit and Budget::begin; its
failure returns without accounting or ordinal changes. After begin, the sealed
entry performs the usual per-entry admission recheck. Failure there is a
committed no-entry Unavailable completion, finishes accounting and retires
execution. This preserves the no-effect preselection error contract.
Admission also holds BUSY through temporary-owner rollback. Core additionally
checks both budget and quantum ends against the admitted
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
end. A captured timer event with verified timer IRQ/ISTATUS returns Progress
with Event::Timer if budget remains; otherwise it completes Budget. A SVC performs at most one
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
retires further group entry and the borrowed Executor; cancellation/drop must remain available. A core
error after a verified restored slice records Unavailable and retires the group;
remaining owners can be cancelled/reaped or dropped. Retirement disables ERET,
not cancellation service: turn continues selecting pending cancellations, under
BUSY and minimal CPU0/DAIF/boot-root proof, even after entry has been retired.
Cancellation does not require timer/GIC admission or a valid clock because it
performs no EL0 transition. If minimal context admission fails it returns
Unavailable without selecting, charging or dropping a task; restoration to a
valid caller context is required before retry. Busy similarly leaves all state
unchanged. Retired non-cancelled tasks are skipped; if any remain and no pending
cancellation exists, turn returns Unavailable rather than Idle. Retirement is sticky in Executor, not only Group: dropping this group and
creating another cannot revive it. Executor::prepare, prepare_with_input, run
and Group::new reject a retired executor before issuing/entering anything.
Existing consuming run reports process::Error::Platform for retirement, after
minimal boot-context admission and BUSY acquisition, and drops its inactive
Process before releasing BUSY. Prepare/prepare_with_input return
`Err((process::Error::Platform, original_space))` unchanged, before any issuer
or startup mutation. Group::new returns group Error::Unavailable.
Busy, capacity and ordinary allocation failures never retire execution. The
existing healthy legacy path and trace remain unchanged. Namespace exhaustion
separately retires only group creation; it does not invent a console generation
wrap or revoke a healthy existing legacy process. Never turn an uncertain
restoration into an ordinary error. Freeze these branches in source review.

Additional B bounds: a per-process maximum of 256 successful begin/
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
with a successful begin finishes accounting once before stop/publication, except
when a hardware observation is invalid against an intermediate counter read or
MAX_COUNT. Reject that observation before finish: the existing invalid stop on
a Running budget transactionally poisons it without changing counters. Report
the actual observed end with accounted=false, retire Executor and reclaim only
after quiescence; never fabricate a timestamp or commit an untrusted charge.

## Legacy adapter and ordinary use

`Executor::run(Process)` remains the existing public consuming API. It uses the
same private entry/dispatch primitive with its original absolute 100 ms wall
clock limit, continuing SVCs internally, treating its timer as terminal, and
preserving the existing literal HMPROC grammar, run numbering and two-CPU
PSCI admission counts. A group turn is a separate sealed scheduling mode;
legacy tests must not acquire extra quantum IRQs or new output records.

Add ordinary `run-group <path1> <path2>` after core review. Parse exactly two
paths with ASCII-whitespace splitting; repeated/leading/trailing ASCII spacing
is normalized. There are no opaque trailing arguments for this command. Both exact archive
paths are admitted before either executes, using the existing checked archive
and persistent terminal Executor. Failure of either admission drops the whole
unstarted group. No console callback is introduced. The driver loops over
bounded turns, reports each terminal result and reaps it, then returns to the
prompt. Existing `run` behavior remains unchanged. Two real worker instances
consume copied startup job bytes, yield through bounded console writes and
finish independently. The ordinary proof uses existing `bin/echo` twice; no
new executable or normal archive change is required. The core supplies copied worker job payloads
`b"worker-a"` and `b"worker-b"` in admission order, even for other chosen apps;
this is documented startup input, not a new argv convention. Interactive gates also pair a busy loop with hello
and require hello completion before the loop budget outcome. This is useful
kernel-scheduled multi-program execution, explicitly not userspace spawn.

The safe Platform method is `run_native_group(&mut self, paths: [&str; 2]) ->
Result<GroupLaunchReport, LaunchError>`, default Unsupported. Its copied report
is `{ outcomes: [GroupOutcome;2], completion_order: [usize;2], completed:u64,
generation:u32, before:usize, after:usize }`; GroupOutcome mirrors End. Outcomes
are indexed by admission order; completion_order is an exact permutation [0,1]
or [1,0]. The UI prints in actual completion order only after teardown. The
existing cumulative completed counter advances once per terminal task and last
prepared generation tracks actual successful Process preparation, including a
later box/admission failure. Core records that scalar after successful prepare;
no-console preparations do not replace the last nonzero console generation.
An admission failure before any execution increments no completion counter.
Both images are parsed in separate noninline helpers; do not retain two large
Image/Space return slots in the monitor. Every return after allocation checks
exact original free-frame balance after group destruction. No callbacks or
Group owner escapes through Platform.

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
obligations and records the independently approved exact inventory/grammar. Required real
hardware cases: two different address spaces using identical VAs
with distinct data; alternating SVC progress and persistent console counters;
actual timer resume at saved ELR rather than restarting entry; busy loop resumed
at least twice while hello advances/completes; full context sentinels across
both SVC and timer handoff; budget exhaustion across many turns; 64th-call and
byte-budget behavior across turns; cancellation retaining frames until service;
held completion preventing slot reuse; reap/reuse with stale and foreign token
rejection; partial load/prepare/box failure rollback through real pool pressure (leave
0, n, and n+1 free frames respectively for an n-frame loaded Space; group cap
stays sufficient, and admission must not short-circuit on total Pool.free); group drop with live
inactive tasks; baseline recovery after faults and cancellation.

Context-test hook contract (`#[cfg(feature = "runtime-tests")]` only): private
`pub(super) enum ContextProbe { A, B }` and Group methods
`admit_context_probe(probe: ContextProbe) -> Result<TaskId, Error>` and
`release_context_probe(id: TaskId) -> Result<(), Error>`, plus
`test_release_word(id: TaskId) -> Result<u64, Error>`. The last method reads only
the same fixed eight-byte owned RW word under minimal inactive boot context;
it requires a valid task but no probe provenance, so tests can observe that a
normal admission of identical fixture bytes remains ineligible for release.
It accepts no address/output buffer and returns a copied scalar. Admission selects the
fixed canonical archive entry internally, runs the ordinary complete archive/
ELF admission and group load, and attaches private enum provenance only after
success. Caller-supplied Image/path/name cannot confer provenance. The ordinary
admit API always stores None. The actual canonical ELF/segments/instructions
and archive bytes are independently bound by the host artifact gate.

Release validates the full token, provenance, Ready state, at least two captured
timer IRQs, and last captured PC inside that probe's independently fixed polling
window. Under BUSY and minimal boot-root proof, it checks the fixed aligned
u64 word at `0x1_0000_2000` is owned RW/NX and currently zero, then writes exactly
little-endian1 through private inactive backing ownership. No caller VA/value,
PC/SP/context mutation, mutable frame or arbitrary write helper is exposed.
Wrong token is Stale; wrong provenance/state/already-released is Bounds. All
checks precede the store; rejected release leaves bytes, budget, counters and
owners unchanged and emits no GIC/EL0 transition. Hook targets the selected
owner even though both fixtures use the same VA. Test wrong identity, stale
prior-group token, peer substitution and repeated release. The hook is absent
from ordinary profiles and production compiler clients.

This avoids fixed loop-count timing assumptions: release follows at least two
actual IRQs and independently verified peer progress. Complementary A/B polling
loops use different scratch registers so the pair covers all GPRs, with explicit
scratch exclusions per probe; FP/SIMD/TLS and NZCV have independent sentinels.

The harness must bind each actual slice to stable group/task identity, ordinal,
entry PC, captured resume ELR, trap kind, cumulative calls/bytes/IRQs, budget
before/after, exact frame balance and restoration evidence. Invalid token paths
must generate no EL0/GIC/HVC activity. Exact profile inventory, wire record fields and fixture symbols are frozen in
that independently reviewed hardware contract; changes require explicit review
rather than accepting whatever a new runtime prints.

Work split: safe accounting/tests in hm-kernel; independently reviewed core
Group/Process refactor; native fixtures and archive builder; independent host
trace/type validators; ordinary monitor adapter; parent-serialized builds and
stack audits. Keep changes reviewable and preserve rejected candidates.
No new ARM architectural instruction semantics are assumed: reuse the primary
sources and restoration contract linked in 0023; any assembly/layout change
requires an explicit amendment with primary architectural provenance.

## Amendment: charge execution work, defer diagnostic serialization

Proposed after the preserved first detailed runtime guests: ordinary group
execution passed its scoped interactive/trace gates, but the Trace-mode
call-limit fixture exhausted its unchanged 100 ms active budget after 41/52
calls. Per-turn diagnostic formatting and UART serialization consumed the
budget before the independent 64-call limit could be exercised. This is a
measurement failure, not permission to accept Budget instead of CallLimit,
increase the budget, or relax the fixture/trace contract.

In group Trace mode only, stage a single turn's native-call observations in a
private fixed local buffer, and serialize diagnostic records after the final
counter read and accounting decision. The buffer is not in Process, Group,
KernelBox, static storage or admission return values. Process layout and its
three-page box charge remain unchanged. Proposed private process-module API:

```rust
pub(super) struct GroupCapture {
    // Private optional copied Request/Response, ordinal, captured SVC PC,
    // [u8; native::MAX_WRITE], and initialized output length.
}
impl GroupCapture {
    pub(super) fn new() -> Self;
    pub(super) fn emit(&self, group: u64, turn: u64);
}
impl Executor {
    pub(super) fn dispatch_group(
        &mut self, process: &mut Process<'_>, capture: &mut GroupCapture,
    ) -> Option<(Reason, u8)>;
}
```

The owned buffer is at most512 bytes (compile-time size assertion), no Copy or
Clone required. One private Group host implementation calls the same existing
Session::dispatch policy: request validation, user copy, call/byte accounting,
reply register updates and terminal decisions retain their existing semantics.
It stages only the current copied request, optional response, actual successful
nonempty output (at most256 bytes), ordinal and SVC successor PC. No user pointer
is retained as a borrow: request addresses are diagnostic u64 values; emitted
output comes from owned copied bytes. Bounds/duplicate-output violations in the
private host fail closed rather than indexing past the buffer. Emit accepts
only scalar identity and uses the existing fixed private UART; no callback,
arbitrary writer or public buffer access is introduced.

`run_turn` initializes the empty local capture before Budget::begin as bounded
diagnostic setup (excluded with STEP formatting), then records CLOCK,
actual entry/trap observations and dispatch results as scalars, and reads the
final counter after all admission, cache work, assembly, restoration, validation,
user-memory copying, capture copying and bounded policy dispatch. Those costs
remain charged. Counter validation, poisoning, terminal precedence and the
100 ms/1 ms/256-turn limits are unchanged. Only diagnostic formatting and UART
serialization occur after that final read and accounting. The next turn treats
that interval as excluded kernel diagnostic time, like other parked time;
this still is not a claim of precise EL0 CPU time. No extra clock source or
assembly timestamp/layout change is needed. Group-only
`group_counter(process: &Process<'_>, capture: &GroupCapture) -> u64` reuses the
existing ISB/MRS sequence but omits `nomem`, supplying opaque input pointers to
both owners to the inline assembly. Both start and final reads use this helper;
all intervening group observations use it too. It does not dereference or mutate
those pointers. This compiler memory boundary keeps actual capture copying and
Process/Session updates before the final observation, and after the initial
observation; inspect emitted assembly for that ordering and for formatting after
the final read. Legacy counter code stays unchanged. The compiler-memory rationale follows
the [Rust Reference inline-assembly rules](https://doc.rust-lang.org/reference/inline-assembly.html),
including the memory-effect promises made by nomem; actual linked inspection
remains mandatory. ISB is not a DSB: this is
not a claim of global store completion or precise EL0 CPU time.

Emit the frozen logical sequence CLOCK, ENTER if entry actually occurred, TRAP
if a trap actually returned, CALL/OUTPUT/RESULT if captured, and CHARGE, then the
existing RETURN after terminal reclamation. STEP stays before begin. ENTER is
now explicitly a retrospective record of an actual completed transition, not a
promise written before ERET. Zero-entry completion never emits ENTER or TRAP.
The host already correlates ordered actual exception events with these records;
it must not infer wall-clock simultaneity between separate serial and exception
streams. Existing field names, actual values, output bytes and ordering remain
unchanged.

BUSY stays held throughout staging, final accounting, serialization and terminal
owner reclamation. Flush completes before any next user entry or return to a
caller that could dispatch another task. UART failure is fatal, so a partial
flush cannot be credited as successful execution or resume. The capture never
outlives the turn and never aliases user backing. A completed process cannot
release an output source before the actual user copy; buffered bytes remain
owned until emission even when the result is terminal.

Ordinary Terminal mode is different: its actual console payload continues to
be written inside dispatch and charged to the process. It emits no diagnostic
records. Legacy consuming Executor::run and HMPROC formatting/timing remain
unchanged; the new group-only dispatch route still uses the same native Session
policy and does not replace the existing legacy dispatcher. The raw failed
runtime candidates remain evidence. Approval requires independent source review,
new emitted execution-stack bounds including this local buffer, exact unchanged
capture/byte-output validation, successful 64-call termination, ordinary group
regression and all earlier profiles. No success is inferred before those gates.
