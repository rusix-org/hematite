# Native task creation: independent review

2026-10-26. Independent AI review, not human review. Standalone safe wire codec design approved; Session and core design approval
remain pending. No implementation or hardware result is implied.

## Existing implementation constraints

The real native Session owns the 64-attempt and 4096-byte counters, version/error
precedence and sticky terminal state. New task operations must use that same
attempt authority. Existing 64th WRITE reply and valid EXIT behavior must remain
compatible; suppressing new task side effects on their 64th attempt needs explicit
status/termination ordering rather than an out-of-band dispatcher bypass.

Group::turn retains BUSY and a mutable selected Slot/Process through the sealed
transition, dispatch and final accounting. Public Group::admit acquires BUSY
itself. Syscall admission therefore needs a separately reviewed private
lease-bound phase; recursive public admission would fail Busy. Safe borrows must
end or be split before mutating another slot, without releasing the exclusion
lease or placing large Process/Space values into a scheduler array. Current
four-slot capacity includes held completions, not merely runnable processes.

Session console tokens, Group TaskId, launch authority and child-completion
handles are distinct principals/kinds. The selected task supplies caller identity;
user bytes cannot nominate a parent. Retirement and overflow must fail closed
without token reuse. Failed creation must account for actual load/startup/box
rollback, any irrevocable issuer consumption, and handle/slot commit ordering.
No completion or child may retain a borrow of the parent's input/memory.

The startup prefix remains 64 bytes. The 0027 decoder validates the entire 4032-byte
extension including zero padding; silently adding capability fields to that tail
breaks existing apps. A separately versioned payload within owned opaque input
or an explicitly revised startup profile is required. Program IDs local to an
immutable launch allowlist are a reasonable bounded alternative to path lookup,
provided their namespace, mapping and missing-ID error precedence are explicit.

Wait must distinguish parked owners from completed/empty slots. No runnable work
must not silently become successful Idle while a parent waits for a child.
Budget charging excludes parked time, but syscall admission/cleanup work and
terminal precedence need a defined accounting boundary. Parent termination must
close grants and reclaim/cancel descendants under a bounded policy before the
owner group can be considered complete. Child completion delivery, consuming
wait and cancellation must each have exact once-only behavior under faults,
budget exhaustion, 64th attempts and cancellation of an already-waiting parent.

Existing linked coordinator-stack headroom is narrow. Any new wire-copy buffers,
parent metadata or callback adapter must be measured in actual linked call chains.
The previous runtime's verified matrix is a regression requirement, not evidence
that new creation paths are sound or executable.

## Initial concrete wire review

The proposed grant-local program IDs and opt-in run-task path are acceptable
bounded design choices. They do not imply a VFS or ambient executable authority.
The separately versioned 64-byte HMTASK payload fits inside existing owned input
and preserves the original startup prefix and full-tail codec. Child input stays
opaque and cannot create a matching authority-table entry merely by copying bytes.
The 64-byte SPAWN descriptor and scalar WAIT/CANCEL/completion formats provide
bounded copy surfaces without output-buffer lifetimes.

Standalone codec implementation approval still requires exact Rust APIs, malformed
field precedence, encoder transactionality and test inventory. That approval can
be independent of the unresolved Session/core mechanism. No Session or Group
change is approved by a pure-codec decision.

A concrete core blocker remains: the draft attributes wake work to completing or
cancelling turns, but current cancellation has no Budget::begin and completion is
resolved after final accounting. Specify a feasible attribution before promising
charged wake work. Bounded post-finish management can be explicitly excluded and
reported separately, or a new timing transaction needs its own review. Parent
Closing must retain the original outcome while hiding final completion until all
children are reclaimed; wake must validate the stopped parent/cap/child identities
without a second syscall count or fabricated active turn.

## Standalone codec approval

The separately scoped `task_wire` proposal is approved for implementation. It
exports only safe, allocation-free byte codecs and copied metadata. Private
validated Spawn/Grant fields prevent invalid encoder state; exact destination
length is checked before mutation and successful encoding overwrites every
reserved byte. Decode order distinguishes size, header, flags, reserved fields,
rights, frame bounds, input canonicality and range overflow. Unknown program
numbers remain metadata until actual grant lookup. A canonical numeric launch
token is not evidence of issuance or authority.

Completion decoding rejects unknown kinds, high bits and non-exit status bytes.
WAIT/CANCEL scalar helpers confer no handle rights. The proposed literal-byte,
unaligned, exhaustive bounded-length, malformed-combination and ownership tests
are appropriate. Concrete exported names/derives and exact test inventory should
be recorded with the implementation before source acceptance. Approval excludes
native Status/Session changes, capability issuance, core dispatch and startup
format changes; none is necessary for this codec stage.

The revised management boundary resolves the earlier accounting objection:
wake/consume/Closing bookkeeping is explicitly uncharged quiescent management,
examining at most two child entries with scalar context/metadata updates and
existing owner release. It performs no allocation, user-memory or bulk payload
copy, UART output or callback. SPAWN validation, input/ELF copying and allocation
remain charged to the parent. This is consistent with existing post-finish
reclamation and does not claim total kernel CPU accounting. It does not by itself
approve the still-unfrozen Session guard or core ownership implementation.

## Safe codec source review

Reviewed `crates/hm-kernel/src/task_wire.rs` and its sole safe-crate module export.
No source blocker found. Every byte decoder establishes the exact 64-byte extent
before fixed-offset access. The private helper offsets are bounded and all public
encoder errors precede writes. Private Spawn/Grant fields and validated constructors
preserve encoder invariants; copied values retain no source-byte borrow. Range
addition uses checked arithmetic after the specified length/canonicality checks.
Grant validates only numeric token shape, not issuance. Completion decoding admits
all 256 exit statuses and only the six exact non-exit words, rejecting high bits
and non-exit status bytes. WAIT/CANCEL helpers enforce their complete scalar shape.

This is source acceptance of the standalone codec only. Independent tests, actual
compiler clients and regression/runtime-identity evidence remain pending. No native
Session, status, core dispatch or execution-authority change is covered.

## Independent tests and host gate

Reviewed all 18 integration cases and the host validator, five validator mutation
cases, six actual-library compiler clients, and Makefile integration. No blocking
coverage or validator issue found. The independent byte examples and explicit
error chains cover the promised wire semantics; exhaustive completion codes,
reserved regions, input lengths and sentinel output checks go beyond round trips.
The existing startup codec integration checks the unchanged prefix and full zero
tail without pretending that a numeric Grant issues authority. Compiler clients
exercise private struct construction/mutation and detached Copy metadata against
the actual library. Makefile requires both target compiler runs and the host suite.

Independently revalidated `target/evidence/task-wire/host-first`: the exact 18-case
raw stdout passes, process exit is zero, both raw log hashes match, and all recorded
source hashes match current inputs. This verifies host codec behavior only; final
compiler-client and full regression evidence remain pending. Suggested adding
`native.rs` to this host gate's input hashes because the startup integration case
uses its Issuer; the enclosing full-suite input inventory also binds source files.

## Owned Session admission proposal

Reviewed the separate `0029-session-admission.md` against current native dispatch.
The owned guard is an acceptable safe-policy design: it carries the actual
non-Clone Session, preserving console identity and both counters without a token
that could be applied to a different Session. Consuming reply/park/wake operations
avoid self-reference and count a parked WAIT only once. Task guards cannot exist
at attempt 64. Correctly versioned task operations then stop before side effects;
wrong-version/unknown-opcode requests retain legacy error Reply plus terminal
CallLimit, and legacy WRITE/EXIT retain existing behavior.

This approves the policy direction, not core integration. Before core approval,
freeze the abandonment cleanup mechanism without borrowing the entire selected
slot/Group across child allocation; a proposed Drop guard must not recreate that
borrow conflict. Explicitly handle final budget/platform termination after a WAIT
has produced a ParkedSession: discard that state, preserve terminal precedence,
and never install a reply or resume a terminal parent. Every core exit must either
restore the original Session once or enter bounded teardown. Source, compiler
ownership tests and actual parking/wake evidence remain required.

## Final standalone codec acceptance

Accepted the local codec milestone using durable evidence in
`docs/evidence/task-wire/full-first`. The complete local `make test-all` exited
zero in 259.4 seconds. Independently inspected the archived inventories: all 80
ARM and 16 x86 result files pass. The log contains 205 Python validator cases per
target. Revalidated the exact 18-case host stdout and execution envelope; both
architectures' six actual-library clients pass, with raw negative diagnostics
limited to the expected private-construction E0451 or private-field E0616 errors.
The two positive clients compile owned detached Copy metadata.

All recorded full-suite source hashes still match. Both durable archive hashes
match their manifest. Independently hashed all 23 current linked kernels against
the recorded identities: they remain byte-identical to the previously reviewed
runtime candidates, including the seventh runtime-test binary. The codec stage
therefore does not alter their linked execution or stack proofs; repeating the
previous detailed raw runtime audit is unnecessary for this unchanged mechanism.

This acceptance establishes safe wire behavior, ownership/API boundaries and
local regression results only. No EL0 task creation, launch-capability issuance,
Session extension, parking/wake integration or new kernel execution path is
implemented by this stage. Remote CI and the full 0029 core/hardware design remain
unverified or pending, respectively. The standalone codec review is now frozen.

## Safe Session implementation scope

The exact owned-admission API in the Session supplement is approved for safe-policy
implementation independently of core integration. This includes statuses Denied 8,
Resources 9 and Pending 10; copied TaskOperation; owned non-Clone Admission,
TaskAdmission and ParkedSession; and shared private counting/finalization helpers.
Session layout, Stop and existing dispatch semantics must remain unchanged.
Only WAIT may park, with rejection returning the same guard. Consuming reply/wake
returns the original Session without another count or console effect. Dropping
an unfinished guard merely loses that Session; it performs no hardware action.

This approval does not claim that losing a guard already tears down a real Process.
The core abandonment mechanism, Process state transitions and terminal-after-park
handling remain unimplemented and unapproved. Independent source and policy tests
must check both legacy regressions and the newly specified admission precedence
before accepting this safe-policy stage.

## Safe Session source review

Reviewed the `native.rs` implementation diff. No source blocker found. Session
fields/order and existing evaluate/write behavior remain unchanged. The extracted
begin/finish/stop helpers preserve the original single attempt increment,
console revocation and sticky terminal ordering. Opt-in admission recognizes
correctly versioned task operations only, suppresses all guards/Host effects on
attempt 64, and routes other requests through existing evaluation/finalization.

TaskAdmission owns the original Session and copied Request. Neither it nor
ParkedSession has a public constructor, Clone/Copy implementation or borrowed
user state. Non-WAIT parking returns the original guard; reply and wake consume
ownership without counters or effects. Dropping these safe metadata owners does
not perform hardware cleanup. This accepts source within the approved policy
scope only. Independent tests, actual-library compiler probes and linked runtime
identity/stack checks remain necessary; legacy dispatch refactoring may change
code generation even when its source semantics are preserved.

## Safe Session tests and initial evidence

Reviewed all 16 public-API integration cases, the strict host runner and its five
mutation cases, 13 actual-library ownership clients, and Makefile wiring. No
blocking issue found. Literal expected legacy outcomes supplement direct/owned
parity checks; tests cover task attempt 63/64 precedence, no Host effects for task
admission, the shared attempt and 4096-byte counters, original console continuity,
owned Host-independent guards, non-WAIT rejection and owner-local abandonment.
The compiler clients test private construction/extraction, no Clone, moved Session
ownership, reuse after reply/park and double wake; positive clients check actual
owned results rather than a substitute type.

Independently revalidated the exact 16-case raw host report and all recorded
inputs in `target/evidence/task-admission/host-first`. The new policy is locally
host-tested, while compiler execution and full-suite acceptance remain pending.
Initial linked candidates, including normal ARM, match their reviewed baseline
ELF hashes; final all-profile identity review waits for the complete inventory.

The completed Session-policy candidate inventory contains exactly the prior 23
profiles, each with build exit zero. Independently hashed every actual linked ELF
from `target/evidence/task-admission/first-candidates/inputs.json` and compared it
with the accepted task-wire/runtime baseline: all 23 are byte-identical. This
includes the previously narrow runtime and normal launcher stack paths. Existing
compiled stack and transition proofs therefore remain applicable without a new
stack interpretation. Approved these unchanged binaries for full-suite guests;
this does not turn the unused safe admission API into executed core integration.

## Lease-bound core dispatch draft review

The sequential owned-command proposal addresses the original wide-borrow and
provisional-WAIT problems in principle. Short selected-Space borrows end before
sequential Group mutation; an outer finalizer replaces a Group-borrowing Drop guard.
A provisional Park is discarded before Waiting publication if final accounting
selects a terminal outcome. This is not yet core implementation approval.

Concrete remaining refinements sent to the authors:

- Carry copied calls/written metrics through every Settlement and DispatchFailure.
  Once an error drops the owned Session and Process.session is None, the outer
  finalizer cannot recover its actual admitted counter from the Process.
- Carry the trusted capability slot/generation in the WAIT command alongside its
  child TaskId; Park must retain the exact lookup identity.
- Define a checked Waiting-to-Dispatching installation transition or separate wake
  installation helper. The proposed put_session accepts only Dispatching, while
  the wake description currently starts in Waiting.
- Preserve Fault when final accounting exhausts budget. Actual run_turn substitutes
  Budget only when no terminal reason exists; the draft's broad “otherwise” rule
  must not override that existing behavior.
- Extend the compiler-ordering proof to detached Session/Command/Settlement storage.
  Existing group_counter exposes opaque Process/Capture pointers, which must not
  silently be assumed to cover newly detached stack owners. Validate the chosen
  ordering in actual linked code before claiming admission/commit work is charged.

Catalog/capability commit rollback, structured Closing and exact hardware protocol
also remain separately pending; no core source edits are approved by this review.

The revised core-dispatch supplement resolves those five specific findings at the
design level. DispatchResult snapshots actual post-admission counters before any
fallible task helper or guard drop. WAIT owns the trusted PendingWait identity.
Wake validates first, takes Waiting into Dispatching, consumes once and publishes
Ready last. Fault precedence now exactly follows existing run_turn. A private
TurnWork stores the detached result before the final counter; explicit opaque
Group/Process/TurnWork/Capture addresses establish a new compiler-ordering review
obligation instead of assuming the old proof covers detached storage.

Approved this ownership/finalization boundary as one design component. This is
not complete core implementation approval: catalog/capability commit rollback,
typed internal errors, complete lifecycle and hardware protocol remain pending.
Actual source must still establish every exit reaches finalization, error snapshots
remain accurate, wake install failures close safely, and compiler ordering plus
Process/Slot size and 64 KiB stack bounds hold in linked artifacts.

## Preserved Session-stage full-suite failure

Independently inspected the first Session-stage runtime failure at
`target/evidence/20261026T225647-084005624-aarch64/runtime/runtime-64m`.
This is a genuine failed guest, not a timeout or merely a parser rejection:
QEMU exited 1 after about 4 seconds. The raw stream contains 21 completed HMRT
cases, then case index 21 `completion_tokens` fails its expected outcome.

Group 27 slot 2 turn 2 starts at counter 227603833 and finishes at 245182292:
17,578,459 ticks, or 281.255344 ms at 62.5 MHz, against 5,668,590 ticks
(90.69744 ms) remaining. The actual raw trace shows EL0 IRQ at 0x10001099c,
PPI 30 acknowledgement/EOI and restored controller sequencing. Budget accounting
terminates that owner with zero calls/written bytes and reclaims its 36 frames.
Its peers subsequently exit; free frames reach 3241 before the held-completion
assertion expects all four owners to have exited normally. The tail is an EL1
failure semihost exit. Recorded raw artifact hashes match.

The kernel binary is identical to the previously reviewed runtime. These records
establish an unexpectedly long measured interval and correct fail-closed budget
outcome, but cannot establish a historical host pause or locate the delay within
that interval. Current host pressure readings are not proof of its cause. The
128 MiB sibling passing does not erase this failure. Full-suite acceptance for
the Session stage is therefore pending; preserve this result and do not weaken
budgets or expected outcomes to manufacture a pass.

## Final safe Session acceptance

Accepted the safe Session policy stage with the first full-suite failure explicitly
retained. The unchanged isolated runtime rerun passes both profiles; the subsequent
complete `make test-all` exits zero in 180.4 seconds. Durable evidence is in
`docs/evidence/task-admission/full-second`. Independently checked all 82 ARM and
17 x86 archived result files, the exact 16-case host stdout/execution envelope,
and all 13 actual-library compiler clients per architecture. Negative raw
compiler diagnostics contain the expected private-field/construction, no-Clone
or moved-owner errors; three ownership-positive clients compile. The full log
contains 210 Python validator cases per target.

All recorded source hashes and both archive hashes match. Independently hashed
all 23 linked kernels again: each remains identical to the prior accepted runtime
baseline. The new Session admission remains unused by core execution. This
supports local policy/API and regression acceptance, not EL0 task creation or
core waiting/teardown acceptance. The repeat pass does not explain or erase the
preserved first runtime timing failure, and no historical host-pause cause is
claimed. Remote CI remains unverified.

## Authority/catalog draft findings

The proposed boxed catalog/authority lifetime and local scalar reservation before
atomic child/cap publication are reasonable design components. Full authority
approval remains pending two concrete policy details: store the console-delegation
permission in the actual authority (or explicitly freeze it as always enabled),
and define existing public Group mutation behavior while task authority exists.
Unrestricted external child reap could invalidate a parent's capability or
PendingWait; ordinary admission could consume reserved capacity. Freeze a private
setup/empty-group precondition plus restricted root-only administrative operations
or exact relationship reconciliation. Typed setup errors and complete hardware
protocol remain pending. No core implementation approval follows from the safe
Session stage's passing evidence.

The revised authority supplement resolves those two component findings. Persisted
LaunchCap.console_allowed governs both the actual envelope and console-delegation
checks. A fresh task-mode constructor cannot convert populated ordinary groups;
TaskActive blocks public admission and restricts administration to the exact root.
TaskClosed permanently rejects mutation after root reap, preventing accidental
ordinary-mode fallback. Private child operations remain relationship-aware and
lease-bound. Root completion stays hidden during Closing; metadata charge persists
until root reap or Group Drop. These are acceptable component design choices.

Before concrete API freeze, name the constructor's executor borrow lifetime in
its returned Group instead of ambiguous `'_` across multiple reference inputs;
root_image must remain an independent temporary borrow. Typed errors, exact root
Closing reports, actual charge/layout proofs and the complete hardware protocol
remain pending before core implementation approval. The logical 512-frame test
must use measured legal image charges and is distinct from physical Pool pressure.

## Typed lifecycle draft review

The separate task Step and one-time RootCompleted publication proposal is coherent:
root teardown precedes Closing, child cleanup precedes external completion, and
metadata charge persists until root reap. Timing remains the actual selected turn;
bounded management must not fabricate an ordinal, syscall or counter interval.
The typed syscall mappings preserve ordered validation and separate resource
failure from corrupted admitted state. Existing ordinary behavior must stay scoped
outside these task-mode changes.

Two concrete blockers remain before mechanism approval. First, retirement caused
by a child can leave a Ready/Waiting root unable to reenter while the draft only
services structured cleanup after that root is already Closing. Define a minimal-
context retirement cascade that closes a nonterminal root as Unavailable, discards
parked state and cancels children, preserving an already finalized root reason.
Report this honestly as management rather than inventing a root execution turn.
Second, new_task must use private lease-bound construction or another explicitly
reviewed sequence; calling public Group::new while already holding Lease repeats
the nested-Busy problem. Constructor precedence, no-effect early rejection and
namespace burn must match that actual sequence. Sent both issues to the authors.

The revised lifecycle resolves both blockers at design level. Private construction
under one Lease fixes nested acquisition. A child-triggered or preselection
retirement closes a nonterminal root under minimal context, suppresses wake and
starts bounded child cleanup. Administrative evidence names the actual trigger
without inventing parent Timing/traps. Separate TaskCompletion/task_reap preserves
the existing Completion report API and the immutable original root closure.

One final wording issue was returned to the author: explicitly order preselection
retirement versus an already requested but not finalized root cancellation. The
cascade currently selects Administrative Unavailable while the general statement
says cancellation wins before entry. Require one precise precedence; the proposed
choice is retirement first, preserving any already finalized root reason, with
ordinary selected cancellation winning only when the executor is not retired.
Subject to that alignment, the lifecycle component is acceptable. Complete hardware
protocol and implementation review remain outstanding; no core edits approved.

## Ordinary mechanism implementation decision

Approved implementation of the ordinary task-creation mechanism within the
reviewed dispatch, authority and lifecycle contracts. The final lifecycle wording
now handles sticky preselection retirement before unfinalized cancellation while
preserving already finalized root reasons. No unresolved ordinary ownership
blocker remains. Owned Session movement, lease-bound construction/admission,
private principal-bound capability commit, provisional WAIT finalization,
relationship-aware wake/reap and structured root Closing must follow those
contracts without public owner escapes or recursive Lease acquisition.

This permits real spawn/wait/cancel source work, not negative test hooks or guest
acceptance. Before implementing expanded diagnostic serialization, freeze its
fixed capture capacity and overflow-fatal behavior: complete descriptor/input/
output bounds are 64/256/256 bytes and management examines at most two child
records. Management must allocate nothing and trace serialization stays outside
charged work. Exact fixed failure/exhaustion hooks remain separately unapproved.

The positive hardware scripts are sufficiently concrete to guide ordinary
implementation. Their full acceptance still requires exact copied management
facts, runtime mode-gate observations, artifacts and parser review. Recommended
mode checks at existing inactive supervisor boundaries rather than extra fixture
programs: task-active rejection of ordinary/child administrative APIs, Waiting
and Closing Pending behavior, and permanent closed-mode rejection/Idle after
root reap. Source ownership, actual type/layout charges, compiler ordering and
all affected linked stack gates remain mandatory before any guest executes the
new mechanism. A full passing test matrix remains mandatory for final acceptance.

Approved the two implementation freezes: `task_root()->Result<TaskId,Error>`
returns only copied identity in TaskActive through held completion, and Mode in
Ordinary/TaskClosed. TaskCapture has fixed descriptor/input/output storage of
64/256/256 bytes, at most one commit/settlement and two child-management facts,
a compile-time size bound of 1536 bytes, and fatal overflow. No management
allocation or charged trace UART is introduced. Actual fields must fit that bound;
raising it requires an explicit amendment and renewed stack review. These freezes
do not replace source, layout, ordering or linked stack gates before guests.

## Ordinary fixture and monitor boundary

Approved narrow ordinary fixture/builder source work: the supervisor checks its
nine real syscall results, the finite worker performs four zero-length WRITEs,
then writes the exact copied job and exits 55, and the loop executes a self-branch.
Both archives use fixed catalog paths `bin/worker` and `demo/loop`. Timing may
produce immediate completed WAIT or a genuine parked wake; both must deliver the
same checked result. No negative hooks, foreign/fault fixture modes or guest
approval is included. Artifact/source review must bind actual SVC instructions,
exact outputs, payload copy/read ordering and executable identities.

The proposed safe `Platform::run_native_task(path)` interface and copied
TaskLaunchReport are acceptable. `run-task` accepts exactly one path and no
payload. The success report follows root task_reap, Group destruction and exact
Pool recovery. Its completed count must reflect actual terminal owners once,
including children cancelled by structured teardown; its generation retains the
existing last-successfully-prepared console-generation meaning, never a task
capability or internal TaskId generation. Failed setup cannot publish success.
Actual source, ownership clients and linked verification remain outstanding.

## Ordinary ARM adapter source review

Reviewed launcher run_task/create_tasks/run_tasks and the BootPlatform adapter.
The 33-owner bound is conservative and valid: at most 32 successful SPAWNs fit
within 63 task-effecting calls because each child beyond the first two requires
a consuming WAIT. Child terminal Steps count once; the root counts only after
matching completion lookup/reap and zero remaining Group charge. Group ownership
ends before the outer Pool equality check. Last-prepared console generation is
preserved across late failure, and actual Executor retirement disables authority.

The author applied the suggested retained Result<Image>/Result<Group> borrowing,
avoiding extra moved owner slots before compiled stack measurement. One mapping
correction remains: Error::Catalog currently falls through to LaunchError::Unavailable,
which permanently disables launcher authority despite Catalog being explicitly
nonretiring. Map its actual AdmissionError through the existing admission_error
function (or another nonretiring typed launch error). No compile, stack or guest
acceptance is implied by this adapter source review.

Verified the author added `Error::Catalog(error) => admission_error(error)`.
This resolves the nonretiring setup-error mapping issue. Ordinary adapter source
is accepted within its reviewed scope, pending complete core integration source
and actual compiled layout/ordering/stack gates before guests.

## Initial ordinary task-core source audit

Read process_task.rs, process_task_turn.rs and the Process/Group integration diff.
The initial audit found no unsafe aliasing blocker in ordinary paths: catalog/root
and child tentative owners drop under Lease on failure; committed capabilities
name exact owned slots; detached Session counters are captured before fallible
work; provisional terminal parks are dropped; root retirement suppresses wake;
and metadata remains charged through root completion until task_reap. The
counter exposes Group/Process/Work/Capture addresses, requiring actual compiled
ordering verification. Initial compilation is permitted, not guest execution.

Three concrete fixes were sent to the author before source acceptance:

- HMTASK RETURN must print the restored boot root, not the selected private root.
- Reject residual authority.dispatching before selection/ERET, rather than only
  detecting it at the next SVC. The explicit reviewed entry invariant must hold.
- Root finalization currently calls Budget::stop twice (outer terminal handling
  and close_root); the second call poisons an already Stopped budget. Assign root
  stop/release solely to close_root while child finalization keeps its own path.

These findings precede full source acceptance and compiled size/ordering/stack
review. No test hook or new guest is approved by this initial audit.

Verified all three task-core corrections on disk. Ordinary task-core source is
accepted for compilation and linked review, not guest execution. Also reviewed
ordinary tasks.rs/S/ld and build_tasks.py: small Reply follows AAPCS64 x0/x1,
startup reads are bounded, assembly yields/copy/overwrite are real operations,
and builder checks the canonical wires, loops, literals and archive. Linked Rust
call ordering must still establish dynamic copy sequencing; symbol order alone
is insufficient evidence.

Initial monitor stack screening of `task-first-candidates` (ELF 7a92b0d5e95bd705,
assembly 7c18691b7f26da4a) found no overflow: conservative calls including tail
edges reach 63,792 bytes on the legacy path; adding the existing 640-byte helper
allowance gives 64,432/65,536. The new run-task spawn and setup paths reach 61,808
and 61,744 bytes respectively before that allowance. These are initial screening
numbers, not final guest approval: the corrected source and fixture archive must
be rebuilt, and every affected profile needs its own linked stack/ordering gate.

The sequential catalog ordering amendment is accepted and verified in the design:
validate the full archive, admit/box worker, then admit/box loop; first-image
allocation failure may precede a second ELF error. No root publication precedes
both successes. This accurately states bounded one-image-at-a-time construction.

Fresh task-second monitor screening retains the same frame bounds. Inspected its
emitted branch flow: dispatch_task completes reads/copies, load_boxed, populate,
cap/charged stores and Work settlement stores before returning to the final
CNTPCT read; budget state commits afterward, diagnostic formatting later in the
caller. Counter asm exposes the four intended addresses. No ordering blocker was
found in this ordinary monitor candidate; final linked identities remain required.

Independently inspected candidate-2 fixture disassembly and manifest hashes.
Worker calls worker_yield_four at 0x1000001e0, checks its return, calls copy_job at
0x100000200, then compares/output-copies those bytes. Supervisor calls first SPAWN
at 0x1000004b0, checks its handle, then performs actual overwrite_job calls at
0x1000004dc/0x1000004e8 before later calls. The five-entry archive hash is
350cc3833bf04cf0b87cb191458fbb1c63e8ebcf61cb79eed65e1ae553ad3851.
These establish compiled fixture behavior and artifacts, not guest execution.

## Completed second-candidate matrix: runtime stack gate pending

All 23 candidate builds completed successfully and actual ELF hashes match the
inventory. Twenty-two profiles fit the established conservative helper allowance:
normal monitor 64,432 bytes, SMP 63,376, secondary 62,640, space/process/input
62,480; x86 monitor 49,568 and boot 49,232. The runtime test needs a safe refactor:
its feasible ordinary cancel_case admission chain totals 65,168 bytes before the
640-byte allowance (65,808 with it), exceeding the frozen 65,536 bound. Chain:
boot 7600, run 16, checks 1904, cancel_case 6176, Checks::admit 1616, Group::admit
208, load_boxed 17936, box_loaded 9888, box_process 19776, allocate_heap 48.
The larger generic graph path through task dispatch is infeasible for this ordinary
runtime driver and is not used as the finding. No allowance or stack-size relaxation
is approved. Suggested isolating cancel_case's large Turn loop in a noinline helper;
actual remeasurement is required. The full affected-profile gate stays pending.

Reviewed ordinary test_task_launcher and its eight mutation cases. Exact output,
resource/generation accounting, dynamic round-robin reconstruction, immediate or
parked WAIT, mandatory SVC/IRQ/resume/GICC transactions and no-entry cancellation
are consistent with its narrow ordinary scope. The actual trace grammar gate
remains false until independent raw inspection. This harness does not establish
the separate detailed negative capability or mode-gate matrix.

## Third runtime candidate: complete preguest gate accepted

Verified the driver-only noinline wait_cancel_boundary split preserves the prior
step/match semantics and borrowed owner. Its emitted frame is 784 bytes, while
cancel_case decreases from 6,176 to 5,152. The fresh feasible ordinary runtime
maximum is now 64,736 bytes plus the unchanged 640-byte allowance: 65,376/65,536,
leaving 160 bytes. The controlling chain is boot 7600, run 16, checks 1904,
irq_context 4752, Checks::probe 1136, admit_context_probe 1472, Group::admit 208,
load_boxed 17936, box_loaded 9888, box_process 19776, allocate_heap 48. As before,
the task-dispatch branch is unreachable for this ordinary runtime driver.

Actual third runtime ELF SHA256 is
bc977b002db590eeb5160a42a7c36892f377a2ecd107687626dd1551557b18b8;
assembly SHA256 is
94603199936b0a0867f4cf7038b136449c11e1a9f78be2815b671b9e432502f9.
Both match task-third-candidates/inputs.json. Independently rehashed the other
22 actual ELFs against task-second-candidates; all remain identical. No stack
size, helper allowance or behavior gate was relaxed.

Reviewed check_runtime_layout.py and independently verified every recorded input
hash, the exact single module-path replacement plus appended constants, and the
88-byte target witness in task-expanded-layout. Unchanged original task/turn
modules compile through that generated copy. Actual Image is 472 bytes/one page,
Authority 928 bytes/one page, Group 1960 bytes, and metadata charge three pages.
Existing Process 9832 bytes/three pages and other original witness fields remain
unchanged. This is compiler layout evidence, not executed allocation evidence.

The complete affected-profile source, compiler ordering, layout and stack gate is
accepted for bounded ordinary task-launcher diagnostic guests. Its raw trace
review flag must remain false until independent inspection of actual traces.
The detailed task-negative hooks and full capability/failure matrix remain outside
this approval; no ordinary guest execution is claimed here.

## First ordinary task execution: raw trace grammar accepted

Independently inspected ordinary-first raw serial, exception/GIC traces, commands
and execution records for task-64m, task-128m and task-128m-fresh. Recomputed all
raw artifact hashes and recorded harness source hashes, verified the actual kernel
against reviewed SHA256 23b91da7052ad8043395364acfc913b5a00de6a93e5fe483b7df4bdafb7eea5f,
and verified the embedded archive interval against SHA256
de72c7e7e66e8a418b5d5657837e8d7026b29acf74ceda1a846ff5e3562cc472.
Commands use single-CPU max/TCG virt and explicit semihosting userspace=off.
Fresh invocations of the strict validators accept each unchanged raw pair.

Every profile executes two supervisor runs. Each has nine parent SVCs, six worker
SVCs, no loop SVC, IRQ counts [1,1,6], and 23 actual EL0 slices. The observed first
WAIT precedes the worker's last two yield calls, output and exit: parent resumption
occurs after worker completion. The exact resumed PCs, lower-EL SVC/vector/ESR,
timer ACK/EOI and GICC restoration sequence match the reviewed grammar. Cancelled
loop teardown introduces no subsequent loop ERET, and root exit follows child
completion. These observations establish actual parked WAIT and cancellation in
this ordinary fixture, not every authority failure or administrative-close case.

Serial checks show completed/prepared generations 3, then 6, then 7 after the
subsequent legacy hello's two SVCs. Free frames remain 3103 after warm-up or 3106
in the fresh profile. Missing-path and arity failures preserve counters/resources;
all runs retain the final prompt and are then terminated by the bounded host
harness, without timeout or forced kill. The first aggregate remains false solely
because its intentionally disabled trace-review flag was recorded false.

Approve enabling that flag for this ordinary harness and rerunning its strict
aggregate gate. This acceptance does not cover the separate detailed capability,
exhaustion, retirement or negative-hook matrix, nor replace the full regression
suite required for these changed core binaries.

The first full-suite attempt stopped before guests on the stale forge_process
negative client: its Session field expression no longer matched Option<Session>,
producing E0308 instead of the required privacy error. Reviewed the correction to
session: Some(session); the expected E0451 remains strict, so the client again
isolates private construction without relaxing the gate. Added task/turn module
source hashes correctly cover the expanded compiler witness dependencies. The
original failure remains evidence; an actual successful retry is still required.

The corrected process compiler gate subsequently passes all 41 clients, including
strict E0451 for forge_process. The second full-suite attempt then exposed a
separate stale host witness reader before runtime guests: it still required five
u64 values after the approved layout oracle grew to eleven. Reviewed the exact
11-word comparison, mandatory six task fields, positive integer checks and page
arithmetic, with matching mutation fixtures. These preserve strict byte/hash
binding rather than discarding the new fields. Requested additionally requiring
Group/task/turn sources in the witness dependency set, since task fields now
claim those types. Production binaries are unchanged by these host corrections.

## Ordinary 25-case acceptance expansion: implementation review

Reviewed the current task protocol against the implemented authority and emitter.
The non-injection fixture/driver work can use existing public cancellation,
inactive Group drop and real retained Pool blocks. Six setup-pressure prefixes
match sequential catalog/root/startup/box/authority ownership; child pressure
must likewise preserve real reservation burns and fail at actual allocation.
The large-BSS inequalities, 64th-call no-effect checks, parked child-budget WAIT,
source overwrite ordering and bounded mode-gate checks remain required.

Approved the ordinary-only artifact inventory proposed by the harness author:
29 entries comprising explicit bin/demo directories, 25 root files (rights split
adds one; separate large-BSS root removes one), and worker/loop catalog files.
A separate five-entry archive contains the large-BSS root and the same fixed
catalog paths. Require an explicit path list; the stale negative-root/legacy
inventory does not authorize those excluded profiles.

Before final driver/protocol approval, freeze the read-only cfg-only issuer
snapshot signature, correct the Budget exception to include wait_budget_child's
child, and align exact record ordering with the actual emitter (CONSUME before
SETTLE/CHARGE; PUBLISH charged field versus CLOSE without it). Add an actual
compiler witness value for the combined bounded task capture if claiming that
measurement; the existing compile-time size assertion remains mandatory. None
of these grants authority for the ten failure/exhaustion injection profiles.

Verified the harness author's saved explicit 29/5-entry lists, corrected Budget
scope and exact conditional emitter order. Approve implementation of these
ordinary fixtures/builders and strict host parser, plus the six real Pool
pressure setup stages and driver actions using existing public APIs. Preserve
the already verified ordinary demo artifacts. The only remaining small driver
API freeze is the read-only issuer snapshot; the ten negative hooks remain
excluded rather than blocking this implementable non-injection scope. Linked
fixture behavior, every affected stack/layout and first actual trace grammar
still need their separate evidence gates.

The remaining diagnostic API is now frozen and approved under task-tests only:
Executor::task_issuance_snapshot(&self) -> (u32, Option<u32>), with a Group
forwarder of the same shape. The first value is actual next_task_cap (zero is
permanent exhaustion); the second is existing last_prepared_generation, not the
console issuer's next value. Console-disabled reservations can burn without
changing that second snapshot. Use Executor after failed construction/outside a
Group borrow, and Group at live/closed boundaries. Both only copy scalar state.

Also approved cfg-only Group::task_launch_generation(&self) -> Result<u32, Error>:
read actual Authority.launch, return its generation while present, and Mode when
absent (including Ordinary, TaskClosed or revoked Closing). ROOT diagnostics read
it after publication before first entry. No lease, MMIO, mutation, reference or
owner escapes through these observers. Production-absence compiler probes are
required. Actual layout witness values supply pressure sizing; no mutable size
or issuance seam is authorized. Ordinary driver implementation approval is now
complete within the non-injection scope above.

Implemented observers match that freeze: actual cfg task-tests scalars only,
stored grant lookup, and unchanged production visibility. Independently reviewed
the twelve added compiler clients and compile-only wrapper bridge calling the
original crate-private constructor. The 53-client plus library report in
docs/evidence/task-negative-types-first passes; current source hashes and wrapper
hash match, and all ten new negative stderr diagnostics contain their precise
expected codes. Positive clients establish root Image/source lifetime release
and copied completion ownership; negatives cover constructor privacy, exclusive
Executor, static catalog, Pool lifetime, mutable reap, typed identity, sealed
completion and all three production-absent observers. These are type/privacy
results, not executed task behavior or a claim of unchanged linked binaries.

Reviewed dedicated task_cases.rs/S/ld and build_task_cases.py for fixture-only
compilation. Scripts use actual scalar syscall replies, bounded local storage,
forced assembly overwrites, unaligned valid buffers, own-entry faults and explicit
64th-call failure sentinels. Builder verifies actual ELF geometry, canonical
29/5-entry archives, layout-derived charges and both embedded archive intervals;
no new negative hook is present. Actual compiled sequencing remains to inspect.
Clarified that fixed worker mode-prefix recognition reads input before yielding:
the source-mutation claim concerns the complete opaque payload copy/output after
the fourth yield, not absence of all earlier input reads.

Approved the separate task-bootstrap wiring proposal: reuse the existing CPU0
Pool/topology and preflight proof for unchanged Factory/Executor constructors,
keep driver lifetimes within the wrapper, preserve existing-profile dispatch
precedence and run the current baseline/EL1 exit afterward. Emit actual preflight
bits (success 15), and extend the vector declaration cfg for standalone task-tests.
No timer, exception or user-entry mechanism change is authorized by that wiring.

The implemented bootstrap matches that approved proof and feature precedence.
Candidate-2 dedicated artifact hashes and current source hashes independently
match: ordinary archive e0998dea00de2f42424d50f4777a5e087ce7d1526942c2017693ffdac307d1c9
and large archive de3dc49aca624bdd67c3cdcc86a09fbf6271ccbff32f84410846fac1e430cce1.
Worker calls yields at 0x100000504 then full copy at 0x10000052c; copy_inputs
executes overwrites at 0x100000430/43c before marker at 0x100000444. Actual worker
has four ELF pages, making child charge 36. Large root 228 pages gives charge
260, child 96 pages gives 128; metadata 3 yields totals 263/391/519 for zero/one/
two children, establishing the 512-frame inequality without altering the cap.

Initial safe task_checks/task_inputs source review is accepted for compilation.
The real setup and child pressure prefixes, issuer snapshots, exact finalized
root SVC boundaries, public mode/cancel/drop checks and borrowed owner cleanup
match the approved ordinary scope. Both files forbid unsafe code; no failure hook
or synthetic child admission appears. Group/TaskTurn return temporaries and
constructor chains still need actual emitted-stack review; noinline annotations
alone do not establish a fit. Formatting and compilation remain parent-owned.

## First dedicated task matrix candidate: stack gate fails

The standalone profile compiles, but its actual reachable task-SPAWN stack does
not fit. In task-matrix-first, the controlling chain is boot 7600, run 16, checks
1920, one 5408, drive 1264, Checks::step 2112, Group::turn 5584,
execute_task_turn 400, dispatch_task 1136, load_boxed 17936, box_loaded 9888,
box_process 19776, allocate_heap 48: total 73,088 plus the unchanged 640 allowance
is 73,728/65,536. Unlike the old ordinary runtime driver, this task-dispatch path
is genuinely reachable and cannot be excluded from the bound.

Other feasible bounds including allowance also fail: setup 67,200,
allocation_rollback 73,472, drop_waiting 72,896. Proposed safe driver reductions:
borrow owned Result<Turn> and return compact step facts rather than copying large
TaskTurn/TaskCompletion across helper frames; handle completion/reap inside that
owner scope; isolate constructor and formatting phases. Setup should avoid
returning Group through nested construct/reject_setup ownership slots. Exact
implementation and fresh emitted measurements are required; no stack enlargement
or allowance reduction is approved. No new task-matrix guest is authorized.

Candidate ELF 686f7fce5fbcfbb0bb72758f5be144df4008bec62fc15f9c477dac8d9d4113bb;
assembly 89b2e05240ea933327182bdfb3d92ae7ceff2193e56435d07f59db13f183329c.
Other 23 profiles have not yet been rebuilt for this matrix integration.

Second driver refactor source preserves checks through borrowed Turn reports,
compact StepFacts, held/publication/reaped completion equality and direct failed
construction. It is sound, but the second linked candidate still fails: maximum
71,840 plus 640 = 72,480. The controlling path now runs through checks 2768,
allocation_rollback 4016, child_pressure_phase 1280, failed_spawn_boundary 96 and
Checks::step 1296, with unchanged boot/core ownership frames. Candidate ELF is
5fc9a2c1ac65d100b831b2ed2b21c95b4d518f14d09bdbabddec808fc272f1d3;
assembly 399e1141dcd00cb37ef94f7d5f0e5faabb5c2e8b03ce15cec2880c4bf347168f.
No guest approval. Suggested measuring a narrow private Result<Space> ownership
handoff to eliminate duplicated large load return slots, without changing APIs,
allocation order or unsafe contracts.

Initial dedicated parser review found four corrected issues: console-disabled
preparation burns its issuer without changing last prepared console generation;
own-RX EL0 write fault uses lower-EL ESR 0x9200004f; timer CHARGE end must reach
its programmed comparator; each profile needs a kernel/artifact input hash record.
Verified those corrections on disk, including the explicitly tighter 16-MiB
shared trace limit versus 32-MiB serial limit. Exact fifty-case orchestration,
injection exclusion and strict record/trace correlation are present. Requested
a complete synthetic fifty-case positive and missing/duplicate ownership/action
mutations before source freeze; the current single lifecycle positive does not
exercise the entire multi-case orchestration. Trace approval remains false.

Approved the exact private experiment in 0029-boxed-preparation-stack.md:
load_boxed passes its owned Result<Space> into the existing noinline box_loaded;
the identical load-error classification/retirement moves into the callee before
preparation. Allocation, issuance, failure drops under Lease, box_process and
counter boundaries remain unchanged. No new unsafe/public API/layout change.
Source review and actual full-chain measurement must establish any reduction;
all affected profiles require rebuild before guest approval.

The exact private core handoff and noinline free-count wrapper are source-reviewed;
the 25 original observations remain at their corresponding checks and records.
Third candidate still fails: 67,712 plus 640 = 68,352. Snapshot isolation helps,
but the core experiment shifts storage rather than removing it: load_boxed falls
17,936→9,008 while box_loaded rises 9,888→18,832 (net +16). Controlling driver
frames are boot 7600, run 16, checks 176, allocation_rollback 4016, pressure 240,
failed boundary 96, step 784, followed by unchanged turn/dispatch/boxing chain.
Another 2,816-byte reduction is required; no allowance change is approved.
ELF d087adc7b710ca5ea2842d6e97c4b5203de3ff256ae4781fb682741d688121c8;
assembly bbb82ed7c43e186abfc9e92bb0c3d1fb8cdee25273ad85f2e0bcba89d4c3c3f2.

The parser now has a complete synthetic fifty-case/54-group positive with six
setup failures, both rights variants, large geometry, pressure, mode gates and
drop recovery, plus missing/duplicate/permuted action mutations. Reviewed its
separate state-transition emitter and explicit synthetic-only scope. Parser source
is accepted for diagnostics once compiled gates pass; actual trace flag remains
false. Synthetic acceptance is not an executed hardware claim.

Rejected the measured no-gain core handoff; verified process_group.rs is restored
exactly (empty diff). Further emitted inspection shows baseline run_tests already
has an out-of-line call. The 7600-byte boot frame includes inlined initialization
metadata/Platform result storage at offsets 2528 and 5056..7488, retained during
the driver. Approved task-tests-only noinline isolation of existing arch::init as
an exact-behavior experiment; its initialization chain also needs measurement.
A preflight-only split is safe but not established to remove the dominant slots.
Designer additionally identified a second 1960-byte Group slot created by explicit
drop(owned); approved lexical owner scopes returning only namespace/free-count
scalars, preserving drop/control/check order. Parent authorizes concrete edits
and serialized builds; no stack reduction is presumed until measured.

## Fourth dedicated candidate: new-profile stack gate passes

Verified lexical Group cleanup occurs before existing DROP/control records, with
identical failure cleanup. Task-only noinline init leaves initialization behavior
unchanged. Actual emitted boot frame falls 7600→2704 and allocation_rollback
4016→2128; the failed core experiment remains reverted. The controlling live
SPAWN chain is now 60,912 plus unchanged 640 = 61,552/65,536 (3,984 spare).
It includes run 16, checks 176, rollback 2128, pressure 240, boundary 96, step 784,
turn 5584, execution 400, dispatch 1136, load 17936, prepare 9888, box 19776,
allocator 48 and boot 2704. Separate init subtree 12,480 plus boot/allowance is
15,824, so isolation has not hidden an initialization overflow.

Actual fourth ELF and assembly match their inventory:
01251bcee0e50b7103da61ba959dc72120781d5e33de3494804671cb9cc65389 and
85ac468c7301bdb44877d9826b02fa1ab0f4fb80e4399dc8b888469b3465be35.
New-profile stack gate passes; other 23 affected profiles, current compiler layout
and embedded artifact binding still need final rebuild/identity verification
before new guest execution under the frozen all-profile gate.

## Final 24-profile matrix gate before diagnostics

Independently rehashed all actual fifth-candidate ELFs and emitted assembly against
its inventory. Every ELF is byte-identical to its reviewed predecessor (22 task
second profiles, runtime third, new task fourth), preserving their stack proofs.
All 224 captured build-source hashes are current. Current layout witness validates
against actual sources and bytes; all 30 canonical fixture ELF hashes match their
manifest. The exact large and ordinary archives are uniquely embedded inside
bounded read-only LOADs at offsets 413316 and 482948 respectively, with the
previously reviewed SHA256 values.

Approve bounded first 64/128-MiB ordinary matrix diagnostics after static checks
pass. Trace-review flag stays false until independent inspection of actual raw
records and hardware transitions. This is preguest approval, not acceptance of
unexecuted cases or the excluded injection profiles.

## First ordinary matrix hardware evidence accepted

Independently reviewed guest-first for both 64/128 MiB: actual kernel SHA
01251bcee0e50b7103da61ba959dc72120781d5e33de3494804671cb9cc65389,
current parser-source hashes, both embedded archive intervals and every recorded
raw-log digest match. Commands retain one CPU and userspace=off. Revalidated the
unchanged complete serial ownership model and all raw QEMU exception/ERET/GICC
transactions. Each profile reaches 50 exact cases/54 groups plus the 41-case
baseline, exits zero without timeout or stderr, and ends through the sole
privileged exit. The intentional trace flag is the only remaining validation error.

64-MiB evidence contains 16,599 records and 1,931 slices (1,050 SVC, 877 IRQ,
four faults), 14.079 seconds; 128-MiB has 16,578 records and 1,928 slices
(1,050 SVC, 874 IRQ, four faults), 14.546 seconds. Real setup leaves
0/1/2/34/35/38 frames and recovers baseline 3129 each time. Child pressure leaves
0/32/33, recovers 3090 while root/metadata remain, then 3129 on teardown.

Observed copy markers are parent calls 2/5/8, each before its child's fourth
yield; exact original absent/empty/256-byte payload results and output follow.
Budget-child cases actually park and wake once after 72–78 loop IRQs. Own-RX
faults match lower-EL ESR 0x9200004f, own entry FAR and pinned fault PC. Shared
64-attempt cases terminate without task copy/commit/reply on attempt 64; Closing
retains the expected reason and reclaims children before publication. Mode gates,
stale/foreign rights checks, held capacity and physical/logical exhaustion match
the exact scripts and reconstructed ownership. Dropping Waiting creates no
completion/wake, recovers all frames and is followed by a fresh supervisor.

Approve enabling the ordinary matrix trace-review flag and rerunning strict
validation. This establishes the declared non-injection matrix only. Full local
regressions still remain required; ten dedicated injected failure/exhaustion
profiles, remote CI and wider OS/compatibility goals are not established here.

## Ordinary matrix full-suite checkpoint accepted

Independently audited task-matrix-full-first: make test-all exits zero in
272.002 seconds with no changed inputs. Both raw archive SHA256 values match
archives.json; their inventories contain 91 ARM and 17 x86 result records with
no false result. Required fixture reproduction, compiler layout, 53 actual process
clients, ordinary task launcher, new matrix, old runtime and regression gates are
present. The log reports 233 Python validator cases per architecture, no skips.
Nonzero expected rejection/timeout/debug-exit records have passing classified
results; they are not unreported failed tests.

Replayed the final archived matrix serial and QEMU traces independently. Both
profiles pass their exact 50 cases/54 groups and 41-case baseline. Final 64-MiB
trace has 1,924 slices (1,050 SVC, 870 IRQ, four faults); 128-MiB has 1,954
(1,050 SVC, 900 IRQ, four faults). Actual kernel and raw-log hashes bind to the
reviewed candidate. Timing-dependent IRQ counts vary within the strict contract;
no case inventory, outcome, budget or ownership requirement was relaxed.

All 308 captured current source hashes match. Independently extracted all 308
blobs with git show at implementation checkpoint
67b86e1b1396521848eb2da37724a91fcadab31b and verified their SHA256 values against
the tested inputs. This accepts the local ordinary task mechanism and declared
non-injection acceptance matrix with existing regressions. Preserved earlier
stack/compiler/reader failures remain part of the evidence history. The ten
dedicated injected failure/exhaustion profiles, remote CI, native ARM/KVM,
Linux compatibility, IPC/services and global phase exit goals remain unverified
or outside this milestone; no global completion is claimed.
