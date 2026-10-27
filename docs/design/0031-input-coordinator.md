# Production owned interactive-input coordinator

Status: **Draft for independent core review; no implementation approval.**
Only this new document changes. Existing safe policy, RW-copy and fixed idle
proofs remain scoped; none establishes an interactive process. This proposal
uses the [input contract](0031-interactive-input-contract.md),
[owned idle boundary](0031-idle-ownership.md) and actual current sources.

## Owner and entry surface

Propose private `interactive.rs`, one synchronous entry from the real retained
`launcher::State`, not a Group, second Executor or BootTest:

```rust
// New proposals, not existing APIs. All fields and constructors private.
struct Interactive<'exec, 'pool> {
    executor: &'exec mut Executor,
    process: Option<KernelBox<'pool, Process<'pool>>>,
    state: KernelBox<'pool, InputState>,
    // actual TaskId, execution Lease and borrowing RX reservation retained
}
fn run_owned(owner: Interactive<'_, '_>) -> Result<Report, Error>;
```

`InputState` owns Budget, lifecycle, input capability record, FIFO[512], indices,
ParkedRead when present, monotonic clock state, immutable wall bound and bounded
counters. No borrowed user pointer/input payload/host callback is retained.
Lifecycle is Ready, Dispatching, Waiting, DeliveryReady, Closing, Closed; only
Ready with an installed resumable Session permits an EL0 slice. Waiting means
Process.session=None and exactly one owned ParkedRead. Dispatching is private
and must settle before returning to the event loop. No public mutable Process,
FIFO, input handle constructor or one-turn callback interface is proposed.

BootPlatform owns the real Terminal monitor/reserved/unavailable state. Its
read_byte must enforce that state in ordinary ARM builds, not only idle-tests.
The synchronous entry holds its exclusive terminal reservation plus one real
process::Lease until Process/input reclamation and verified hardware cleanup.
The lifetime prevents monitor reentry; forgetting owners may leak reservation,
never grant another reader. No arbitrary callback, including fmt::Write, runs
while hardware is armed. Output uses existing sealed fixed UART policy.

Existing State::initialize runs timer::checks then mints the sole retained
Executor::from_boot_terminal; do this before acquiring BUSY. Repeated sessions
reuse that Executor and its checked issuer state. Existing legacy run/group/task
paths and output traces retain their current profiles. x86 remains Unsupported.

## Construction, identity and resource ordering

1. Admit the entire canonical archive and requested ELF using boot_image::admit
   before initialization/issuance. First useful command is a dedicated
   `interact bin/shell`; shell initially implements help/echo/count/exit in EL0,
   not a kernel line parser or a launch-capability bypass.
2. Require actual one-CPU BootPlatform proof, Terminal Monitor, capacity for one
   completion counter, and usable retained Executor. Initialize if necessary,
   reserve RX, acquire Lease, then read-only process/idle hardware admission.
3. Allocate the small InputState box and load Space using the same actual Pool.
   Preflight limits before issuance. Call existing Executor::issue_group once;
   its persistent namespace authenticates a new real TaskId(slot0,generation1).
   A new narrow crate-private `TaskId::single_root(namespace)` constructor is
   needed because fields are currently private to process_group.rs. It only
   constructs copied identity from the just-issued namespace, not page authority.
4. Consume one generation from existing Executor::issue_task_cap; encode
   `(generation<<32)|interactive_wire::INPUT_SLOT` (slot5). This uses the same
   persistent task-cap issuer as task creation, never console generations.
   The actual private capability stores principal TaskId, kind Input, READ
   permission, generation and live/revoked state. Matching serialized Grant
   bytes alone confer no authority. No launch grant or child slot exists.
5. New sealed `Executor::prepare_interactive` factors prepare_inner's existing
   loading/startup/context construction, using actual Issuer::issue_interactive
   rather than legacy issue. Encode canonical interactive_wire::Grant into a
   bounded64-byte local, copy into owned startup_input::Input, install through
   Space::install_startup. Prefix remains Session.startup's actual interactive
   flags3/4096calls/65536bytes; startup tail obeys the existing canonical codec.
   Never forge a Session by editing startup bytes or resetting its counters.
6. Box the real Process; no duplicate Process table or large owner array.
   Capture the immutable wall origin at publication and check frequency*300,
   start+duration and all56-bit bounds. Publish Ready only after every fallible
   step succeeds. Before publication errors return no grant/process; all owners
   reclaim once under the lease. Burned namespace/cap/console generations are
   never rolled back. Last-prepared console generation follows actual successful
   prepare, even if a subsequent box fails; no completed count increments.

Process remains its current9832-byte layout/three box pages. Require InputState
fits one4096-byte box at compile time; at most256 image pages+12tables+16stack+
1startup+3Process-box+1state-box =289 Frames. Derive sizes from actual compilation,
not serialized claims. Stack must include preparation return slots and bounded
256-byte receive/work buffers; split load/prepare/run phases as existing launcher
helpers do, preserve borrowed Result owners, and measure every affected profile.

## Existing mechanisms and minimal new seams

Reuse `Executor::group_slice` (actual saved register state, private assembly,
full restoration), `process::admission/observe`, executable_pc/cache preparation,
Lease, process_budget::Budget, Session and GroupCapture's fixed console host.
Do not call Group::turn/admit under the retained Lease or import Group's four
slots/task-authority model. Its run_turn cannot be reused wholesale: it owns a
Slot and ordinary limits, and dispatch_group currently calls legacy dispatch.

Factor a sealed input-dispatch entry alongside existing admit_task_call: take the
actual Session from Process, copy the actual SVC registers into Request, invoke
Session::admit_input with the same fixed private console host, and return owned
InputAdmission. Process::put_session/task_reply already provide the restoration
operations. Add only private Process forwarding to Space::validate_rw and
copy_into_rw; neither backing references nor generic external mutators escape.

Factor idle Hardware/Clock/Transaction out of BootTest without duplicating MMIO
or WFI assembly. Production's short-lived `Inactive<'a>` witness borrows the
actual stopped Process, retained Lease and coordinator RX reservation. Creation
is possible only in Waiting/DeliveryReady or Closing while no slice is active; it cannot
be copied, publicly forged or kept across resumption. Its IdleLimit is bound to
this TaskId and immutable wall_end. BootTest and its fixed Case/short constructors
remain feature-only; production never calls them.

`process::task_counter<G,W,C>` already accepts explicit owner/work/capture
addresses and a compiler memory clobber. Reuse that ordering shape for the actual
Interactive state, detached Session/ReadAdmission/ParkedRead work and FIFO/copy
storage; no claim that Process's address alone orders detached updates. Inspect
linked copy/policy/counter/commit ordering before any guest. Normal console
output and real byte copies stay charged; diagnostics flush only afterward.

## Fixed limits and event loop

Use actual admitted1000..1e9 frequency. Budget::new(frequency*2,
max(1,frequency/1000)): two seconds total active work and nominal1ms slices.
Session independently owns4096 attempts and65536 successful output bytes.
Coordinator caps total charged turns8192 (User/Receive/Delivery combined),
device boundaries16384, and consecutive empty idle returns8; none resets on
input, yield, READ completion or wait. Wall_end=start+frequency*300 never rebases.
No per-read active credit or hidden legacy100ms run wrapper is used.

Before each charged turn, validate context, remaining+now<=MAX_COUNT and
monotonic now, then Budget.begin. For user entry use comparator
min(Budget comparator, wall_end); final Rust preentry check excludes an already
observed expiry. Assembly setup can cross a bound before ERET; its timer then
regains control. This is the reviewed bounded-overrun model, not hard realtime.
Reentry still sanitizes PSTATE and preserves only the existing supported state.
One SVC or timer/spurious/fault ends the user turn. Keep cumulative spurious
IRQ count bounded8 as group runtime does; separate it from empty idle returns.

Receive and Delivery are real kernel-only charged turns, never fabricated SVC,
IRQ or PC reports. They perform bounded work with no WFI. Idle while Waiting
and FIFO empty is uncharged, uses real masked-WFI transaction and the immutable
wall limit; after restored wake it schedules Receive, not an immediate reply.
Each charged turn finishes exactly once on valid clocks. Any detected regression
or >MAX_COUNT poisons accounting transactionally (existing Running stop rejection
keeps snapshot unchanged), retires Executor and forces Unavailable; a recovered
final sample cannot commit trusted charge. Final Executor retirement is checked
after Budget.finish, as in the reviewed task dispatch guard.

## Finite arbitration and cancellation visibility

Freeze the following single-owner loop; there is no receive-only work queue.
Each selection checks sticky retirement, monotonic clock and wall expiry first.
A fixed private `rx_status()` reads FR/RIS/RSR without DR and returns copied
readiness/error facts under the actual Hardware/RX/Lease proof. It is bounded
constant work and counts against the16384 device-boundary cap. Like selection
and closure bookkeeping, this preliminary status read is explicitly outside
Budget's active-turn bracket; the bounded DR pump and transformations remain
charged. It invokes no callback, formatter or user entry.

1. If Closing, perform only closure/handoff, never select Receive or EL0.
2. At each new service cycle, sample rx_status. A visible device error closes
   immediately; data readiness selects at most one Receive turn for this cycle.
   Otherwise proceed directly to the eligible consumer action.
3. After that one Receive, if Ready select one User turn; if an actual ParkedRead
   exists and FIFO nonempty select one Delivery turn. Do not select another
   Receive ahead of this eligible consumer merely because FR still has data.
4. Immediately before that User/Delivery action, sample status again to catch
   newly visible errors and recheck sticky cancellation/deadlines. Do not perform
   an extra DR scan here. A new byte's identity, including Ctrl-], is unknown
   until the next Receive; no claim that a status register reveals that byte.
5. Waiting with FIFO empty and no remaining device-ready indication enters
   one masked-WFI idle transaction. Its restored return starts a new service
   cycle. Waiting with device bytes but no staged payload starts another bounded
   Receive cycle without WFI; this occurs, for example, after discarding the
   initial suppressed LF. No eligible Delivery/User was skipped in this branch.
6. Finishing a User or Delivery starts the next cycle. A timer IRQ in a busy
   Ready process therefore returns to status/Receive arbitration before reentry.

Continuous RX cannot indefinitely starve a Ready process or an eligible
Delivery: at most one256-byte Receive precedes each consumer action. Conversely
no Ready busy loop bypasses receive checks at its next bounded user turn.
Cancellation visibility is bounded in service steps for bytes actually present
in the hardware RX queue: inspect the next at most256 DR values per Receive,
then at most one consumer turn before the next batch. There is no bound from a
host's write timestamp for a byte still queued outside the guest, nor an atomic
promise against an escape arriving after the final DR sample. Existing1ms timer
regain, bounded SVC/copy work and global budgets bound the subsequent work;
this is not a hard realtime microsecond latency claim.

A full256-word staging batch means only that this turn's work limit was reached.
Sample FR afterward as `more_rx`; do not consume a257th word to infer overflow.
The next service cycle handles remaining hardware input. Actual FIFO overflow
requires an observed ordinary payload byte whose append would exceed512 after
CRLF/control filtering. Example: FIFO256 +256 payload fits exactly; FIFO512 +one
ordinary payload byte fails. Full staging with FIFO0 is not overflow, nor is a
fullFIFO with no new ordinary byte. Retain the oldest bytes; never discard them
to make space. Inspect all staged words for errors/escape before choosing the
batch's terminal reason, so a later error/escape wins over an earlier provisional
overflow in that same finite batch. Commit no batch payload when closing.
This policy can terminate a slow consumer rather than backpressure indefinitely;
it does not guarantee sustained arbitrary input rates.

After successful Delivery, the Session is Ready but no user execution happened
in that turn. The next cycle may perform one Receive before the next User turn;
these are separately charged and count toward8192. Receive/Delivery can exhaust
that turn limit before4096 syscalls, intentionally. No fairness path resets
allowances or treats a full staging buffer as an automatic kernel failure.
Attempting a charged turn beyond8192 or a device boundary beyond16384 closes
with TurnLimit before that operation; completing the final allowed turn does
not authorize another entry. A ninth empty idle return or ninth spurious user
IRQ closes Unavailable and retires the executor under the existing unexpected
hardware-event policy. Those two counters are separate and never disguised as
user progress.
The initial public cancellation mechanism is the actual Ctrl-] byte only;
a future administrative cancel API requires a separately scoped authority.

## READ admission, parking and delivery commit

On SVC, call the actual Session::admit_input so all valid/invalid operations
share one attempt counter. Complete WRITE/EXIT/unknown/version outcomes retain
legacy policy semantics. Recognized READ4096 is Stop before a guard, lookup or
copy; invalid version/opcode follows existing final reply-then-stop policy.
For Read guard, validate actual input capability (principal/kind/generation/live),
then length1..256, then whole destination via validate_rw; reject through typed
ReadError::Handle/Length/Address, restoring that same Session once. Corrupt/context
errors retire rather than pretending an ordinary user Address error.

Valid READ calls guard.park and keeps provisional ParkedRead in bounded owned
turn work. Do not put a Session back or publish Waiting until final accounting
and terminal checks succeed. If they terminate, drop that owned parked policy,
revoke input and never reply. Otherwise store the actual ParkedRead plus copied
address/capacity in InputState; FIFO-ready means DeliveryReady, else Waiting.
No self-referential borrow, repeated Session issuance or second call admission.

Factor a fixed private `RxReservation::pump_batch` over the existing Hardware
read methods, returning at most256 copied full DR words plus status; it accepts
no arbitrary register address, callback or interrupt ID. BootTest.pump_one stays
a separate fixed test adapter. Receive stages at most256 actual PL011 bytes into bounded kernel storage while
RX output is masked and boot context retained; error bits are checked before
truncation. Apply the one-shot CRLF suppression carried from monitor handoff,
reserve Ctrl-]0x1d for cancellation, and check FIFO capacity512 before append.
Overflow/error do not evict old bytes to make a test pass. Source status errors
win over escape, which wins over overflow. On failure close; staged bytes never
become user-visible. Normal append is a bounded kernel commit, with final
clock/wall/cancellation checks; no arbitrary callback or caller buffer.

Delivery begins only with nonempty FIFO and actual ParkedRead. Stage
n=min(capacity,FIFO length,256) bytes without removing them. Revalidate destination
and copy_into_rw through the inactive exclusive Process. Actual staging/copy and
Session policy work are charged. Call ParkedRead.finish(n) provisionally, retaining
its returned Session/reply as owned work; invalid finish is a core invariant
failure, never a newly minted Session. Before commit, sample final counter,
finish Budget and select terminal outcome. Only on success remove exactly n FIFO
bytes, install the same Session and x0/x1 reply, then mark Ready last.

Final priority is Unavailable(clock/platform/retirement), input device error,
Cancelled(escape/request), InputOverflow, WallLimit, then an already captured
Fault/Exit/CallLimit, then Budget/TurnLimit. For Delivery/Receive there is no
captured user terminal; all higher-priority stops discard provisional work.
A successful copy followed by termination may have changed the dying backing,
but no EL0 resume/reply/FIFO removal occurs. This is not copy rollback. No new
hardware sampling after the final boundary can make commitment globally atomic
against future arrivals: newly arriving bytes belong to the next Receive turn.

## Closure, quarantine and reports

Closing revokes the actual input capability first, discards provisional/parked
policy exactly once and prevents entry. Stop Budget only if it is Ready or
Exhausted; do not call stop twice or revive Poisoned state. Keep Process and input-state owners through quarantine, with entry permanently
disabled, so the production inactive-Process witness remains real even for
closure waits. Retain scalar reason/accounting and generation. Reclaim both
owners once quarantine succeeds or is abandoned, under restored boot/hardware
context, before releasing terminal reservation/Lease and publishing the report. A hardware
restore/foreign-ACK failure is the existing no-return fatal path with owners
retained; no ordinary Err may release armed hardware.

Quarantine follows the existing finite contract: discard at most2048 actual
bytes for at most250ms, require10ms empty quiet, restart quiet on each arrival
without restarting the absolute bound. Idle during quarantine uses a shortened
limit <= original wall_end. If wall_end has already expired, a new10ms quiet
interval cannot be waited for under that contract: mark Terminal Unavailable
and return no prompt. This deliberate fail-closed limitation avoids silently
extending wall authority; permitting a separate post-session handoff timer would
require an explicit amendment. Continuous RX, device errors or failed quiet
also retire terminal access. Successful quiet permits explicit Monitor transition
and Lease release; Drop/forget alone never grants monitor RX.

Copied Report includes terminal reason, actual TaskId, last prepared console
generation, calls/output bytes, charged/remaining/turns, receive/delivery counts,
actual WFI count, before/after Frames and terminal availability. Count completion
once for a published session, never for failed construction. Launcher State
retains actual last-prepared generation and sticky Executor retirement. Input
or budget termination alone need not retire a healthy Executor; hardware/clock
failure does. Terminal Unavailable blocks new interactive/monitor reads even if
legacy output-only execution would otherwise be possible.

## Typed publication and monitor CRLF handoff

The existing `InputLine` owns private `skip_lf`; feed(CR) sets it, feed(LF)
clears it, and clear() intentionally retains it. BootPlatform currently sees
only read_byte(), not this line-parser state. Therefore the adapter must move
this metadata explicitly, rather than infer it from UART or fabricate a fresh
line boundary. Proposed safe Platform addition (default Unsupported, no core
owner types crossing this boundary):

```rust
fn run_native_interactive(&mut self, path: &str, skip_one_lf: bool)
    -> Result<InteractiveLaunchReport, InteractiveStartError>;
enum InputHandoff { Unchanged, Monitor, Unavailable }
struct InteractiveStartError { cause: LaunchError, handoff: InputHandoff }
enum InteractiveEnd {
    Exited(u8), Fault, Budget, CallLimit, TurnLimit,
    Cancelled, InputOverflow, InputError, WallLimit, Unavailable,
}
struct InteractiveLaunchReport {
    end: InteractiveEnd, handoff: InputHandoff,
    namespace: u64, slot: u8, slot_generation: u32, input_generation: u32,
    completed: u64, generation: u32, before: usize, after: usize,
    calls: usize, written: usize, charged: u64, remaining: u64, turns: u64,
    received: u64, delivered: u64, receives: u64, deliveries: u64, waits: u64,
    accounting_valid: bool,
}
```

These are proposed copied diagnostic structs, no capability authority. Capture
actual Session calls/written from the live Session, ReadAdmission or ParkedRead
before any guard-drop/terminal branch; keep that bounded scalar snapshot in turn
work so reports do not invent counters after the unique policy owner is dropped. The core
keeps real TaskId; the safe report carries its scalar components only. `received`
counts successfully committed FIFO payload bytes after filtering, `delivered`
counts committed READ reply bytes; receives/deliveries count successfully
accounted corresponding turns, including zero-payload Receive. waits counts
completed real WFI paths, excluding precheck skips. charged/remaining are final
Budget snapshots and never fabricated after a poisoned clock; unavailable
reports must additionally carry `accounting_valid: bool` so diagnostics cannot
claim a trustworthy charge for the failed turn. Frame equality is checked only
after actual owner reclamation; an unverified fatal teardown returns no report.

On entering an `interact <exact archive path>` command, the safe monitor takes
and clears its skip_lf once, passes that bool as non-authoritative metadata,
and retains the original bool locally until the synchronous result returns.
No extra opaque arguments are accepted by this first command. The core initially
copies the suppression bit into InputState; only its first error-free ordinary
received byte can discharge it: LF is discarded once, any other byte clears the
bit and is processed normally. Error checks and reserved escape handling take
precedence over line-ending filtering. Later CR/LF bytes are unmodified raw
input for the actual userspace shell to interpret.

Before any process is published, no DR consumption occurs. A normal start
failure releases reservation after verified cleanup and returns Unchanged;
the monitor restores exactly its original skip_lf and existing line clear rules.
Unavailable hardware/terminal state returns Unavailable (or fatal never returns),
not Unchanged. After publication, termination always produces a Report, including
Unavailable, rather than an ambiguous start error. Successful quarantine returns
Monitor: the old suppression bit is consumed/discarded and monitor starts a new
line epoch with skip_lf=false, then prints hm>. No second suppression of the
user's first new monitor LF is allowed. Unavailable causes no prompt and no
read_byte busy loop; the monitor returns to an explicit terminal stop path after
a bounded diagnostic. It must not use the existing unconditional prompt path.

`skip_one_lf` is not an RX or inactive-process proof; a safe Platform caller can
pass either value without acquiring hardware. All actual reservation/concurrency
checks remain in core. The initial extension only changes the new interact
branch; legacy run/group/task and ordinary monitor parsing keep their behavior.
A failed prepublication grant/console allocation may burn generation but consumes
no terminator byte. Persistent State still records the actual last-prepared
console generation; a start error is not completion. Terminal Unavailable and
Executor retired remain distinct, both reflected in the safe status behavior.

## Review and actual workload gate

First source slice should factor only the shared idle owner and sealed profile
preparation, with legacy artifact/type regressions. Then integrate owned event
loop, charged copy and FIFO; no metadata-only substitute completes this task.
Compiler clients reject public witness/limit/cap construction, cross-owner use,
Send/Sync, policy duplication and borrowed byte escape. Test allocation rollback
at state/load/startup/Process-box stages with actual Pool pressure, monotonic
namespace/input/console burns and exact reclamation; no new failure setter.

Real EL0 shell must prompt, block on READ, consume host bytes, echo/edit bounded
lines and run built-ins, then exit and return to monitor. Required actual gates:
byte bursts and cross-page destinations, invalid/stale/foreign input handles,
RX/RO/guard/partial-invalid writes unchanged, READ4095/4096, FIFO overflow,
escape during wait and before delivery, genuine idle wake/300s wall expiry,
active busy loop termination, repeated sessions with fresh input generations,
no extra pending reply after cancel, continuous-input handoff failure and later
ordinary timer/hello regression where terminal remains available. No host-memory
injection, kernel command emulation or busy-loop polling shell counts as success.

Exact fixture/protocol inventory and independent review of the proposed typed
report/error API, monitor CRLF transfer and quarantine-on-wall experience remain
before code. This document freezes a proposed ownership/accounting path, not
implementation or full Phase3 completion; task launch/IPC rights remain separate.
