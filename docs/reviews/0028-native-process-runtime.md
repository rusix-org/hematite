# Native process runtime: independent review

2026-10-26. Independent AI review, not human review. Current verdict: Stage A
verified; Stage B design, source, linked stacks, ordinary groups and the fresh
single-CPU runtime matrix accepted in the scopes below. Actual runtime trace
gate approved; final local ARM/x86 regression suite and artifact identity accepted.
Matching remote CI remains unverified.
Stage C userspace spawn/wait/cancel is not approved or implemented here.

The following chronological review preserves rejected candidates and the original
constraints; later explicit verdicts supersede earlier pending statuses.

## Constraints from the existing implementation

The existing private assembly slice saves848 bytes of user context (all GPRs,
SIMD, PC/SP, FP control/status, TLS and trap metadata), restores the boot root
with full invalidation, copies context back only under boot mappings and restores
kernel/GIC state before returning Rust. It can support resumption only through
sealed checked core authority. Every reentry sanitizes user PSTATE to preserved
NZCV plus0x00400340; timer resumption must specify this existing behavior too.

Current Executor::execute consumes one process with one absolute100ms deadline;
a valid timer IRQ is terminal. A resumable scheduler needs a written accounting
contract: original wall deadline, charged execution interval, or another explicit
bounded policy. Resetting the deadline each round silently permits unlimited
execution. TRAP_COUNT and TIMER_STATUS are private assembly frame fields outside
the copied user context. Exact EL0 CPU time cannot be inferred from the current
interface. Charging a counter interval around the complete slice would include
entry/teardown overhead and must be named accordingly, with checked monotonic
arithmetic and bounded wrap admission.

Session must remain attached to its process across slices: cumulative calls,
console bytes, handle grant and terminal state cannot reset. A64th reply may be
recorded but cannot reenter. Scheduler slot identity and generation are distinct
from the existing persistent console issuer namespace. Stale tickets, completion
retention/reaping, wrap retirement and reuse need exact contracts and tests.

Every active slice retains exclusive BUSY and the actual CPU0/peer-OFF proof;
all frames/tables/context remain owned until full root/control restoration.
Inactive owners may be returned or dropped only after that boundary. Safe public
callbacks or caller-forgeable register state cannot be introduced. Cancellation,
execution error and runtime Drop must each reclaim or return ownership exactly
once and define what happens to completed-but-unreaped records.

Storage is a material constraint: normal launch already reaches a conservative
64,128 of65,536 coordinator-stack bytes. Process is a large owning value. Arrays
of resident Processes and extra by-value return layers cannot be assumed to fit.
Existing Pool::box_value can provide bounded owned storage but itself accepts T
by value and returns T on failure; initialization/failure/transitions still need
actual emitted stack inspection. Budget accounting must include resident owner
storage and completion bookkeeping as well as user/table pages. No stack-size
increase or weakened verification gate is approved.

## StageA design verdict

Read the exact Budget API and transitions in design0028. **Approved for the
safe accounting component only.** Private non-Copy/non-Clone Budget, checked
whole-budget and quantum ends, monotonic begin/finish times and permanent
poisoning prevent reset/revival through this API. Rejected operations must leave
all snapshot counters unchanged; zero-elapsed successful finish still increments
turns. Remaining plus charged stays equal to initial total, and final overrun
saturates at the budget without a claim of precise EL0 CPU time. StageB must
still enforce its architectural56-bit counter bound independently of this
platform-neutral u64 policy.

Accepted the private cfg(test) self-check seam for turn-count overflow, called
by a dedicated integration target importing the actual module source. It does
not expose field forgery or a constructor in the production rlib; actual-library
compiler clients must prove the seam absent. Fourteen public-API integration
cases plus that overflow case establish the intended15-case inventory without
changing the existing strict native cargo--lib inventory. This source-import
case is scoped as actual-source testing, distinct from production-library
compiler boundary evidence.

StageB/C remain unapproved pending the concrete hardware/API/record freeze.
In particular: retired entry must still permit pending cancellation to progress
without ERET; dispatch completion versus simultaneous budget exhaustion needs
explicit precedence; counter-wrap admission must use MAX_COUNT; persistent
turn/spurious bounds and per-slice restoration observations need exact rules.
The safe component alone cannot establish resumable execution or complete the
multiprocess milestone.

Approved final error refinement: exhausted turn counter is State and begin
rejects it before Running; the defensive finish increment also reports State.
Clock is reserved for counter regression and timestamp/deadline arithmetic.
Transactional snapshots and permanent poisoning are unchanged.

## StageA source verdict

Reviewed process_budget.rs, its15 integration cases and all three dedicated
verification tools. **Source approved; no findings.** Candidate arithmetic
precedes every counter update, poisoning changes no snapshot field, and no
terminal state revives. Whole-budget overflow is checked even when a quantum
fits. Turn capacity is rejected before entry; the defensive finish guard is
also transactional. Private fields and absence of reset/Clone/Copy preserve
one accounting owner. Snapshot is deliberately detached and freely copyable.

Fourteen integration cases exercise the production module, including actual
mutation of detached snapshots without changing owner state. The fifteenth
imports the actual source only to call its cfg(test) self-check: MAX-1 reaches
MAX, another begin poisons unchanged, and the defensive finish overflow likewise
cannot partially charge. The production compiler client must fail to find that
seam. Ten compiler clients cover three valid ownership/snapshot uses and seven
specific rejected authority/duplication/mutation attempts. These compile checks
do not claim their positive assertions executed; corresponding runtime behavior
is covered by the integration cases.

The host harness requires all15 named tests exactly once in order, a complete
zero-skip summary, successful exit, no timeout/launch error and bounded artifacts.
Mutation cases cover missing/duplicate/reordered/failed/skipped tests, malformed
summaries, truncation and perfect output paired with failed execution. Only an
unused safe module export is added outside these files. Actual verification and
runtime identity remain pending; this source verdict grants no StageB/C approval.

## StageA actual evidence in progress

Independently revalidated first-host and the ARM full-suite batch's raw15-case
output with execution/artifact conditions; recorded source hashes match the
reviewed files. Inspected both ARM and x86 actual-library compiler batches:
all ten clients have the intended outcomes and rejected clients contain their
expected diagnostic classes. The batches are
`20261026T205938-739216496-aarch64` and
`20261026T210205-802680084-x86_64` under target/evidence.

Independently hashed all22 current runtime ELFs against both the new candidate
inventory and startup-input-stage-b/third-candidates: every byte is identical.
The previous linked stack/runtime identities therefore remain applicable;
this unused module does not establish any new scheduling behavior. Stable final
archive and complete regression acceptance remain pending.

## StageA final local acceptance

Independently inspected the stable
[final archive](../evidence/process-budget-stage-a/final/verification-summary.json):
all69 ARM and15 x86 result objects report success. Fresh validation of the final
raw host output confirms exactly15 cases with successful execution; both final
actual-library batches contain ten intended outcomes with expected rejection
classes in raw diagnostics. The full summary records `make test-all` exit0 and
169 Python cases per target. Together with independently verified byte identity
of all22 runtime ELFs, **StageA final local acceptance passes**.

Audited the milestone evidence and requested only precise wording: fourteen
cases exercise the public production library, while the fifteenth imports the
actual source for its private counter-capacity seam. This is a verified unused
safe accounting component, not resumable execution, group ownership or spawn.
StageB/C remain unimplemented and require their separate design/source/compiled
stack/runtime approvals; no phase or broader objective completion is claimed.

## StageB freeze review in progress

Re-read the current group/runtime and hardware-test proposals against actual
Process, KernelBox, Session and assembly restoration code. No new mechanism
blocker is apparent in bounded separately boxed owners with the sealed saved
context. The proposed active-turn policy, independent256-turn cap, cancellation
service after entry retirement, hardware56-bit bounds and explicit terminal
precedence address earlier conceptual concerns. They do not yet approve code.

The following must become concrete before implementation: exact Error/report
APIs and observational getters; group namespace and slot-generation commit/error
ordering; cancellation behavior if basic boot-root proof fails; all fixed test
cases, dynamic IRQ/turn bounds and wire fields; ordinary monitor jobs/output;
and compiler-bound actual box charge. A completed record reserves a slot but
must not keep user/box pages; admission failure cannot silently mutate occupied
slot state. Platform retirement prohibits ERET while permitting cancellation
service only under established inactive boot-state ownership.

The proposed release hook is additional private test authority. Its canonical
fixture binding must be established from actual immutable admitted ELF bytes,
not a caller-controlled path or label. Freeze its cfg/signature, exact RW
location/value, provenance marker and refusal cases. Rejected foreign/stale,
wrong-image/state and repeated-release attempts must leave bytes and ownership
unchanged, followed by actual owner resumption. Complementary polling probes
need exact register/SIMD values and independent symbol/instruction binding;
printed metadata or a context checksum alone is not full preservation proof.
StageB remains pending until these items are frozen and independently reviewed.

Reviewed the initial safe monitor interface separately: default Unsupported
adds no execution authority, and exact two-path parsing plus validated completion
order prevents out-of-bounds report indexing. Its whitespace-normalizing grammar
must be frozen distinctly from run's opaque first-delimiter input grammar.
Real group wiring, help/banner, status accounting and end-to-end evidence are
not established by this safe interface.

Confirmed an architectural timing clarification against existing assembly:
there is no counter check atomically before ERET. The last Rust admission check
can expire during sealed setup. The already armed comparator regains control;
accounting includes bounded setup/teardown overrun. The revised proposal's
conservative accounting claim is appropriate; no hard real-time no-entry-after-
threshold guarantee or unreviewed assembly amendment is approved.

## StageB mechanism design approval

Read the completed main API, Timing/Event reports, namespace/slot commit rules,
retirement/cancellation behavior, terminal precedence, fixed test authority and
ordinary adapter contract. **The main StageB mechanism design is approved.**
The separately required exact hardware inventory/record grammar is still a
preimplementation gate; this approval does not waive it or authorize StageC.

Timing reports now expose actual bracket observations, comparator/whole-budget
ends and whether charging committed; no-begin cases have no invented timing.
Events describe captured SVC/timer/fault independently of terminal outcome.
Committed selection increments ordinal/cursor; rejected preliminary admission
leaves them unchanged. Implement preliminary admission and post-begin entry
checks consistently with this no-effect error promise.

The cfg-only context admission chooses canonical archive entries internally and
attaches unforgeable private provenance. Ordinary supplied Image admission cannot
obtain that provenance. Fixed release validates full token, inactive Ready state,
IRQ history, last poll PC, owned RW/NX word and zero value before writing only
little-endian u64 one at0x100002000. It exposes no arbitrary target/value/context
mutation. Host evidence still must establish two IRQs in the completed sentinel
poll interval and peer progress, not merely any earlier IRQ count.

Reviewed the four safe monitor policy additions and exact12-case harness;
fresh validation of recorded host output passes. They establish arity/no-effect
errors, whitespace normalization, completion order and malformed-order refusal.
Actual two-resident echo instances and loop/hello progress remain required;
none of this policy evidence substitutes for core integration or hardware runs.

## StageB final preimplementation approval

Read revision1's final fixed hardware contract. **StageB design and verification
protocol are approved for implementation**, including the explicitly scoped
runtime-test release/read hooks. StageC remains unapproved. This approves the
proposal, not unreviewed source or guests; source review, actual compiler clients,
all affected linked-stack gates and independent first-trace review remain required.

The fixed matrix is twelve cases twice at64/128MiB, with bounded selected turns,
setup IRQs, host time and artifact size. Each dynamic hardware event must match
one owned task/ordinal/root/resume PC and exact GIC restore transaction. The
832-byte capture now has an explicit reserved-zero104th word (the original
listed fields were only824 bytes), fixed expected SP=0x100200000-1024, full
register/SIMD/control expectations and actual copied output. Early setup IRQs
do not count toward the two sentinel-poll IRQs. ACTION records preserve the
supplied namespace, distinguishing foreign-group and stale-generation errors.
The fixed copied-word observer provides unchanged-value proof without granting
write or arbitrary-address authority.

Real retained Pool pressure leaving0, n, and n+1 frames, with n=ELFpages+28,
reaches actual load/startup/box failures. No synthetic allocator seam or whole-
charge free-count shortcut substitutes for these paths. Expected native issuance
burns are explicit; dropping pressure and retrying must run a real child. Physical
frame reclamation remains distinct from held completion-slot occupancy.

Ordinary echo payload and newline are separate syscall yields. Corrected the
proposal's intact-line assumption: only four atomic-write interleavings preserving
each task's payload-before-newline order are accepted, correlated with slices.
Loop/hello must prove actual preemption and peer completion before loop Budget;
sequential legacy runs cannot satisfy that gate. Existing echo bytes remain
unchanged. Suggested final literal alignments use the already defined per-task
StepReport ordinal for U and give cancel_new's loop no console grant, exercising
TaskId independence without adding a case or mechanism.

Approved sticky Executor retirement across Group destruction: platform/counter/
corruption failure cannot be cleared by constructing a new group. Retired prepare
returns Platform with original Space before issuance; retired consuming run drops
its inactive Process under the retained valid-context lease. Allocation/Busy are
recoverable and namespace exhaustion disables only group creation. Bounded private
ENTER serialization is part of sealed setup after the final Rust timing check;
no recoverable post-ENTER branch may silently skip the promised transition.

## Initial ordinary integration source review

Reviewed root-owned launcher run_group/run_pair/admit_group, shared initialize,
private BootPlatform adapter and help/banner validator changes. Source approval
is provisional against the frozen Group API; the complete core implementation
and actual linked stack remain separate gates. Both admissions precede execution;
partial failure unwinds the local Group before outer free-count verification.
Last prepared generation is copied after each admission attempt, including a
late failure, while completion increments exactly once after a real terminal
result. Duplicate/foreign completion IDs fail closed.514 selected turns cover
two tasks'256 accounted turns plus a possible no-entry terminal selection each.

On an Unavailable completion the driver requests cancellation of the remaining
child, and Group destruction reclaims any remaining inactive owners on errors.
State retires after unavailable outcomes or cleanup mismatch. No owner, callback
or borrowed job input escapes Platform. Separate noinline image/admission helpers
avoid deliberately retaining both large image/owner temporaries, but only emitted
assembly can prove these new paths fit. Shared initialization preserves the
legacy timer preflight and sealed output-mode construction. Exact monitor banner
and help expectations agree with the safe UI's added command.

Reviewed the initial twelve added group compiler probes and actual-module wrapper:
they cover exclusive Executor borrowing, Pool lifetime, typed/unforgeable tokens,
non-Clone/non-Copy/non-Send/non-Sync Group, mutable-only turns and no callback.
Requested the positive input-detachment client mutate underlying ELF bytes after
dropping Image and before another group operation; merely dropping Image while
its immutable source remains live does not independently test source detachment.
Production cfg intentionally omits runtime-tests; compiler rejection of all three
private context test hooks must also be included once final names are available.
No compiler result is inferred from these source strings.

Approved a minimal private Factory::reborrow(&self)->Factory<'pool> amendment
for repeated test groups. Inspected Factory's actual fields: shared Pool borrow,
immutable CPU-count proof and local-thread marker only. It contains no pages,
namespace, mutable execution state or retirement latch. Existing load(&self)
already permits multiple distinct owned address spaces. Copying that reusable
proof preserves the Pool lifetime and !Send/!Sync marker and cannot duplicate
owners or bypass Group/load admission checks. Keep the helper pub(super), with
no public Clone or new boot-authority constructor.

## Complete core source review and first compiled rejection

Reviewed process.rs shared lease-bound transition/dispatch, process_group.rs owner
state machine, Factory/Space helpers and fixed context-fixture source/builder.
Source is suitable for compilation and emitted-stack inspection, not guest
acceptance. Assembly and process-layout offsets remain unchanged. Group terminal
cleanup occurs under the retained lease after boot-root restoration; cancellation
and stale/completed tokens do not publish mutable owners. Real allocation failures
unwind Space/startup/box owners without publishing a slot. Factory reborrowing
copies only reusable immutable proof. Namespace diagnostics convey no authority.

Resolved source findings: Group construction/admission now retire on Context but
not Busy; boxed allocation errors borrow metadata rather than move another large
Process; invalid final hardware timestamps poison accounting before committing.
A clock_failed flag also preserves the original snapshot if an earlier invalid
observation is followed by an apparently recovered final counter. Such reports
retain actual end observations and accounted=false. Valid platform-failure turns
can still account elapsed time. The legacy adapter retains its separate absolute
wall deadline and output protocol. Runtime hardware-driver coverage is still
pending; an empty, unwired module is not verification.

Fixture review confirms all 104 capture words are initialized, GPRs and SIMD lanes
are captured before scratch clobbers, reserved word is zero, and fixed polling
instructions preserve NZCV. Separate process data/seed state and complementary
scratch registers are checked. The builder binds fixed poll and release geometry,
SVC opcodes, fault ADR target, ELF permissions, payloads and archive bytes. Actual
register preservation and trace claims still require execution. QEMU's previously
recorded DIT exception-entry limitation continues to apply.

The first normal candidate is **rejected for execution**. Independently checked
ELF SHA256 e00dc108e480ad228cbae86cfc83c395b6e01fe3886f27614aeda83838a86e72
and emitted assembly SHA256
96d2ae1b1ea05c026d965b8586f53c94a78acaa785ecf1668324b52819b61b36
against target/evidence/runtime-stage-b/first-candidates/inputs.json and the
archived aarch64-monitor.s.gz. Actual prologues/calls retain this chain:
boot_main 2528 + monitor 448 + group command 176 + State 128 + run_pair 4000 +
admit_group 1456 + Group::admit 208 + load_boxed 17936 + box_loaded 29600 +
prepare_with_input 9120 + install_startup 112 = **65,712 bytes**. This already
exceeds the 65,536-byte coordinator stack by 176 bytes before deeper callees.
No guest may run this candidate. Borrowing Group/Image/Input in their owned
Result storage is an approved safe refactor; its actual compiled reduction must
be measured before lifting this rejection.

## Second ordinary candidate stack gate

Root's safe Result-borrowing refactor preserves owner destruction and error
precedence. The second candidate matches ELF
 e3d70d96bab50c4d8f5b9763abc0b360c303602bf39b1c200e731eabffacf70c
and assembly
2e98b8bec6c58b157e8f3daa07454f586ea4c31ebf68fad5126e09a31a9d221c
in target/evidence/runtime-stage-b/second-candidates/inputs.json. admit_group
shrinks from 1456 to 816 bytes; run_pair remains 4000. Including Space::audit's
48-byte leaf, the deepest group admission path is **65,120 bytes**, leaving
**416 bytes**. All calls at this deepest tail were resolved; there is no
formatting or arbitrary indirect call there. Linked memcpy/memset are leaf
routines without stack adjustment. This exact path fits without inventing a
fixed reserve requirement; the narrow margin requires fresh inspection after
any linked change.

The legacy run path through the newly separated dispatch retains 63,648 bytes;
a conservative 640-byte allowance covers Console128, fmt::write112, scalar
formatting at most48, pad_integral96, prefix48 and UART16. Its bound is64,288.
Monitor diagnostic baseline direct calls are smaller (run_tests54,240 before
boot/monitor), and the group turn path is far smaller than admission. Reviewed
all relevant direct/tail edges and linked formatting prologues, rather than
summing mutually exclusive prepare and execute paths.

This approves the second ordinary candidate's stack only. Hardware driver,
strict parser, every affected profile's compiled stack and actual traces remain
separate gates; no guest acceptance follows from this record. The first failed
candidate must remain preserved. Actual compiler evidence at
 target/evidence/runtime-stage-b/first-types/result.json passes41 clients,
including source-buffer mutation after owned admission and rejection of all
three production-only-forbidden test hooks; this establishes compiler boundaries,
not hardware execution.

## Hardware driver and entry source gate

Read the complete safe runtime_checks driver and private aarch64/runtime-tests
entry. Approved for compilation and emitted-stack review. Actual 12×2 cases use
owned groups, persistent sessions, explicit no-grant cancellation, held completion
slots, stale/foreign tokens and real Pool pressure leaving0/n/n+1 pages. Failed
startup issuance leaves last_prepared unchanged; successful preparation followed
by box failure advances it by two across the pressure sequence, and retry uses
the next console generation. Group drop and every case verify actual Pool counts.
Context release refusal observes unchanged fixed-word storage, including ordinary
admission of identical bytes; completed-probe refusal chooses whichever owner
finishes first rather than assuming completion order. All owner-moving helpers,
including Group destruction, remain subject to compiled aggregate stack checks.

The private entry requires actual topology[0], successful timer preflight15,
BootPlatform-owned Pool/mapping proofs and a single persistent Executor. Main cfg
priority preserves earlier dedicated profiles and excludes runtime-tests from the
normal monitor and competing baseline/preemption branches. This is source evidence,
not a hardware test result. Initial host-parser review requested exact action-script
inventory,56-bit timestamp bounds and inclusive turn limits. Captured context-probe
SPSR must distinguish deliberate DIT/SSBS writes from sanitized reentry; full capture
bytes continue to require the specified zero values after resumption. Actual trace
review remains mandatory before enabling its acceptance gate.

## First full linked inventory: hardware profile rejected before guests

Independently hash-checked all23 linked ELFs and archived assemblies in
 target/evidence/runtime-stage-b/third-candidates/inputs.json. Normal monitor is
byte-identical to the second approved candidate. Other existing profile direct
chain maxima remain bounded: SMP62,704; secondary61,968; space/process/input61,808;
ordinary baseline56,768; x86 monitor48,928 and boot48,592. Existing switched-stack
mechanisms require their separate established stack bounds, not summing both
stacks into a fictitious coordinator chain.

The new runtime profile is rejected: ELF
 e6cdc40761ba48c7534d2fa245e8f665bd27537d8efce7d50db69b055b04d7c3,
assembly0dc962349664beabc30f9bdb12beff69a4329105554c27fff1fddec0ac944046.
Actual retained chain: boot7568 + run16 + checks1904 + drop_live4448 +
drop_suspended4528 + Checks::probe1136 + Group::admit_context_probe1472 +
Group::admit208 + load_boxed17936 + box_loaded29600 + prepare9136 +
install_startup112 + audit48 = **78,112 bytes**,12,576 beyond64KiB.
Safe helper/ownership temporary reductions are required. No guest has been
approved for this candidate. Preserve this failed emitted evidence.

Ordinary group harness source reviewed: exact prompts, completion/resource and
last-prepared generation counts; four permitted echo byte interleavings are
cross-checked against actual SVC order. Busy peer traces require repeated poll
IRQs and hello completion; failed second admission may emit no user transition.
The shared run_profile scenario argument preserves the legacy default. Requested
and reviewed a full main-path synthetic positive plus missing peer/IRQ/ACK/restore
and ghost failed-admission entry mutations, in addition to fresh echo cases.
Source approval does not replace first actual trace inspection.

Fourth candidate source refactor preserves behavior: boxed allocation Result
storage moves into noninline box_process; driver groups remain in owned Results,
DROP logging takes scalars after destruction, and outer fresh/foreign helpers no
longer retain another Group. Hash-checked fourth-candidates normal/runtime ELF
and assembly. Normal's deepest group path shrinks below its legacy64,288 bound.
Runtime remains rejected: ELF
 a0f6cc7f80315135d9c361e3733d56f3af39707bf18cef2322f060cceea3e46d,
assembly fef58c26e86b6ad85119240c023f366a0be14197445c7ec3e2a36fe1b02a15ec.
Its rollback chain is boot7568 + run16 + checks1904 + admission_rollback10144 +
Checks::admit2080 + Group::admit208 + load_boxed17936 + box_loaded9888 +
box_process19776 + allocate_heap48 = **69,568 bytes**, still4032 over64KiB.
Other hardware cases now reach at most64,576 before any applicable indirect
formatting allowance; the deep owner path itself has no formatter callback.
Splitting rollback phases into noninline scalar-driven helpers is required;
no guest approval is implied by the reduced first failure.

## Fifth runtime candidate: source and compiled stack accepted for first guests

Reviewed the driver-only rollback split: scalar outer state, separate cap and
pressure helpers, borrowed Image Results, unchanged allocation/issuance order.
Actual fifth ELF SHA256
b8855e7decc7bbe16eab6662899b58a3ea1aec2c2247b16187cc9b2fba44ed75
and assembly SHA256
6a868280dcbc0152dd2586af4855a493f0df7aba6d8d9cd14c62649ff195e050
match target/evidence/runtime-stage-b/fifth-candidates/inputs.json.
Maximum direct retained chain is now boot7568 + run16 + checks1904 +
irq_context4624 + Checks::probe1136 + Group::admit_context_probe1472 +
Group::admit208 + load_boxed17936 + box_loaded9888 + box_process19776 +
allocate_heap48 = **64,576 bytes**. Even adding the conservative640-byte helper
allowance gives65,216, leaving320 bytes. The deepest allocation tail itself has
no formatter or arbitrary callback. This exact runtime candidate passes the
stack gate; any change requires fresh linkage inspection.

The accepted normal candidate is fourth-candidates ELF
c682cc32966dfcd6aefc5164ab33a23ac8bdf0aec9e0898024248c4ceab515d9,
assembly9792d71fb40ffa20797450cd8377b2bf36c5573ddb2002f859aa3cd590463eb1;
its bound is the legacy64,288 path. Remaining21 existing candidates use the
hash-checked third inventory and prior bounded mechanisms. These are concrete
linked approvals, not an assertion that future all-profile rebuilds match them.

Independently revalidated layout-second/result.json against retained object bytes,
assembly and current source hashes: Process9832, Space8936, KernelBox40,
Option<KernelBox>40, backing pages3. This is compiler layout evidence only.
Runtime parser source fixes now bind56-bit timestamps, cross-owner monotonic
counter ordering, exact action scripts and immediate ownership/release eligibility,
including readback of the actual target or required live canonical observer.
Full synthetic positive/mutation coverage does not enable the actual trace gate.
Source and compiled checks now permit first diagnostic guests; independent raw
trace review is still required for hardware acceptance.

## First ordinary group execution accepted

Independently revalidated all three raw transcripts and complete exception/GIC
traces in target/evidence/runtime-stage-b/group-first against the reviewed fourth
normal ELF c682cc32966dfcd6aefc5164ab33a23ac8bdf0aec9e0898024248c4ceab515d9.
Checked recorded raw artifact hashes and actual QEMU command: one CPU, pinned max
TCG profile,64/128 MiB, semihost userspace=off. The743,936-byte canonical archive
SHA ea708fb72cd2e9cd277de319bd6e9697c54872472eaa1e3cc4a8f13ca4a9169d
occurs exactly once at file offset393608, wholly in a read-only PT_LOAD.

Both warm profiles produce eight echo slices (six SVCs, two initial IRQs), then
83 busy-peer slices (two hello SVCs,80 loop IRQs and one hello IRQ), followed by
the ordinary legacy hello. Full traces enforce saved-PC continuation, alternating
live-owner selection, private vector entry, timer acknowledgement and exact GIC
restoration, both baseline test groups and no unknown transitions. Initial IRQs
can occur at the entry PC before user instructions execute; this is consistent
with the documented setup-overrun model. IRQ ESR is stale architectural state,
not interpreted as a synchronous fault.

Actual echo bytes are worker-aworker-b followed by two newlines, matching the
four separate write calls. Failed second admission emits no user transition,
reclaims its prepared first child and advances last prepared generation to3
without a completion. Each warm profile ends with free3120, completed5 and last
generation6. Fresh128 MiB ends with free3123, completed2 and generation2. Final
prompts remain alive until bounded host shutdown. This accepts the ordinary group
trace grammar and this execution scope; the dedicated runtime matrix and full
regression suite are still separate pending gates.

## Dedicated first run: preserved active-budget failure

Both profiles in target/evidence/runtime-stage-b/runtime-first exit1 before the
baseline. Raw records complete the first four cases, then the call_limit fixture
legitimately exhausts its100ms active-turn budget before reaching64 dispatches.
The64MiB trace ends at calls52/turn54:66,405 ticks remain at begin, the next SVC
is suppressed before dispatch and final accounting reaches zero. This is not a
parser-only mismatch or a timeout, and cannot be relabeled CallLimit. Full runtime
acceptance remains withheld. The first runs must stay preserved.

A proposed written amendment would defer only diagnostic formatting/UART until
after quiescent finish, using fixed local copied records whose capture cost is
still charged. Ordinary terminal output and all real execution/dispatch costs
remain charged;100ms/1ms and the64-call test remain unchanged. Review requires
retrospective ENTER semantics, BUSY held through flush, flush before next ERET,
no borrowed user buffer/callback, and terminal failure if credited output cannot
be flushed. This paragraph records constraints, not implementation approval;
concrete design/source and new linked stack gates remain required.

## Deferred diagnostic amendment: preimplementation approval

Approved the concrete final amendment in design0028, limited to process.rs and
process_group.rs. A private local GroupCapture of at most512 bytes stages one
request/response and at most256 copied output bytes; no Process/Group layout,
public callback, borrowed user storage or persistent output buffer is added.
The same Session policy remains authoritative. Empty capture initialization is
excluded bounded diagnostic setup before begin; actual admission, transition,
restoration, user/output copying and policy costs stay charged. Ordinary Terminal
payload remains synchronous and charged; legacy HMPROC timing remains unchanged.
Budgets, quantum,64-call limit and required tests are not relaxed.

Both group boundary reads and intermediate observations use the explicitly
specified group_counter with ISB/MRS, no nomem option, and opaque input pointers
to Process and Capture. This exposes the affected storage to the compiler memory
boundary; implementation and emitted assembly must demonstrate copy/policy stores
inside the bracket and diagnostic formatting after final read. The primary
[Rust inline assembly reference](https://doc.rust-lang.org/reference/inline-assembly.html)
and [Rust By Example](https://doc.rust-lang.org/stable/rust-by-example/unsafe/asm.html)
describe the default memory effects and nomem restriction. No pointer is actually
dereferenced by this clock assembly, and no DSB/global store visibility or precise
EL0 CPU-time claim is introduced.

ENTER becomes retrospective actual-entry evidence. Logical serial ordering and
separate exception correlation remain exact; no zero-entry path fabricates a trap.
BUSY remains held through final accounting, flush and terminal reclamation, and
flush must complete before any next ERET. Fatal UART failure prevents a credited
write from being followed by successful resumption. Source/ownership review,
actual compiler ordering and stack checks, unchanged capture-byte validation,
real64-call termination, ordinary regressions and full suite remain execution
approval gates. This approves implementation only and preserves the failed first
runtime evidence without converting it into a pass.

Deferred-diagnostic implementation source reviewed in process.rs/process_group.rs
and approved for static checks/compilation. GroupCapture has private initialized
fields and a compile-time512-byte bound; single request/output guards and checked
length precede copying, and emitted length cannot be user-forged. The borrowed
GroupConsole cannot escape dispatch; captured output owns its bytes. Session
validation, counters, reply registers and terminal decisions remain authoritative.
Terminal UART executes synchronously before final accounting. Trace serialization
is after final read/accounting, with the caller's lease retained through flush and
terminal reclamation. No failure path can resume after a partial fatal flush.

All four group timestamp observations use the new pointer-bearing memory boundary;
legacy counter and HMPROC behavior remain separate. Actual compiler ordering,
stack and hardware tests remain unverified for this implementation until new
artifacts are inspected. No Process/Group field or assembly layout change was
introduced by this amendment.

## Buffered implementation: sixth linked inventory approved

Independently hash-checked all23 ELF/assembly pairs in
 target/evidence/runtime-stage-b/sixth-candidates/inputs.json. Nineteen ELFs are
byte-identical to the reviewed third inventory. Changed normal, legacy process,
startup-input and runtime profiles were re-audited. Normal ELF
34d4382d87ba55fedd9239924d7ce06f07556326cb1ca974a1c9f2581c99f972
has a63,776-byte direct maximum; adding640 gives64,416. Runtime ELF
700e39e93415b9f9e9f3dfff7b1c9b0d3356cfe9137b4df1f5768a8f7bb13cc9,
assembly f0edfe8a8540d8a821904f23b7515acf1e079958a384e3acc7c3ee896bf28534,
retains the64,576-byte admission maximum; allowance gives65,216/65,536.
Legacy process/input profiles remain61,808 plus640. No stack size increased.

Inspected actual control-flow edges, not textual assembly order: runtime run_turn
starts with empty capture initialization, then pointer-bearing ISB/MRS. Its SVC
block calls dispatch_group and returns to the final-counter block before any
HMRT formatting. dispatch_group performs actual user read, GroupConsole owned
output memcpy and length store, Session accounting and reply stores before
returning. Terminal UART remains inside that dispatch call. Final MRS precedes
budget commit and all reachable diagnostic formatting. Multiple error-path
final-read blocks preserve the same ordering. Normal emitted code independently
has the same bracket. A later memcpy used while formatting captured metadata is
diagnostic work, not a delayed user/output capture.

This approves the exact sixth linked inventory for the next diagnostic guests,
subject to refreshed layout/type/artifact bindings. It does not enable the actual
trace gate or turn the earlier truncated runtime tests into passes.

## Buffered second runs: execution complete, strict report still rejected

Independently inspected runtime-second raw logs against sixth runtime ELF700e39e9….
Both guests exit0 and baseline41 passes, but each emits two IDLE records for held
completion groups10 and27, contrary to the frozen single-final-IDLE grammar.
The strict failure is correct. For diagnosis only, removing exactly the first
IDLE of each affected group in memory permits full serial and untouched raw
exception/GIC trace validation:24 cases,518 slices (286 SVC,230 IRQ,2 faults),
eight preflight and16 baseline IRQs per profile. Full832-byte captures, persistent
64-call termination, byte budgets, rollback, generations, cancellation and drop
checks agree. Neither saved logs nor failed result files were changed, and this
counterfactual diagnostic is not a passing runtime result or trace-gate approval.

Reviewed the minimal driver correction: initial four-owner drain becomes the
same bounded step-until-complete loop without an IDLE observation; final reused
slot drain retains the one required IDLE. Reclamation, held-slot Full, repeated
queries and reuse/stale-token checks remain unchanged. Source approved for a
new runtime-only linked candidate; fresh stack/hash and unmodified guest logs
are required.

Current ordinary group-second raw profiles independently pass against sixth
normal34d4382d…: warm64/128 free3119, completed5, generation6; fresh128 free3122,
completed2/generation2. All echo groups have eight slices. Busy groups have83
and77 slices respectively (80/74 loop IRQs), showing why the validator requires
bounded progress rather than a fixed timing count. Raw digests, archive embedding,
complete restored traces, baseline tests, legacy hello and final live prompts
all agree. This scoped ordinary regression is accepted separately.

Seventh runtime-only candidate linked approval: ELF
 e52ad25713b8cbc051915a22027c653323d0656f2d85f714b94b1ebde5358e00,
assembly a770257812664d87aad126b57276cccbe08f6eb85a81a1f7231a6cbd3c128ba6,
independently hash-checked against seventh-candidates/inputs.json. Maximum remains
64,576 plus640 =65,216. Changed held_completions retains4672 bytes and stays below
that bound. Normalized emitted run_turn, dispatch_group and GroupConsole bodies
are identical to sixth, preserving the reviewed compiler-ordering proof. Approved
fresh diagnostic guests; prior duplicate-IDLE logs remain failed evidence.

## Fresh unmodified runtime-third: actual trace gate approved

Independently validated both complete, unmodified serial logs and raw exception/
GIC traces against seventh ELF e52ad25713b8cbc051915a22027c653323d0656f2d85f714b94b1ebde5358e00.
Checked raw digests and the224,768-byte canonical runtime archive, SHA256
1b5a86130b0b7fdc6de91a4dc00881c07ca6234b704fb85b319528fff086d5dd,
occurring exactly once at offset430506 wholly inside a read-only PT_LOAD.
Evidence: target/evidence/runtime-stage-b/runtime-third.

Each profile completes all24 ordered cases, restores free3241 after each case,
passes baseline41 and exits0 without timeout.64MiB has4160 records and520 user
slices:286 SVC,232 IRQ,2 faults.128MiB has4124 records and514 slices:286 SVC,
226 IRQ,2 faults. Both include exactly eight preflight and16 baseline IRQs with
expected vectors, acknowledgements, restoration and final privileged exit.
Different live roots, exact saved-PC resumes and all832-byte GPR/SIMD/FP/TLS/
NZCV captures agree with the fixtures. Prior DIT emulator qualification remains.

The64th calls actually terminate CallLimit with budget remaining (rather than
hiding the original failure); byte budgets persist, cancelled owners do not
reenter, completed slots remain occupied until reap, stale/foreign/provenance
refusals observe the correct storage, and live destruction reclaims owners.
Real cap/pressure rejection levels are3241/0/32/33; generation burns and successful
retry match. Full counter conservation and global monotonic observations validate
without charging parked/diagnostic serialization time. Variable quantum counts
are accepted only within the explicit bounded contract.

The actual trace grammar gate is approved for enabling by root. No preserved
failure is rewritten. This establishes the declared single-CPU runtime matrix;
full regression execution and final source/artifact identity remain pending.
It does not establish SMP process scheduling, userspace spawn, service IPC,
Linux compatibility, KVM performance or completion of a global project phase.

## Final local regression acceptance

Accepted the complete local make test-all run, exit0 in263.7 seconds, recorded in
[full-first](../evidence/runtime-stage-b/full-first/result.json). Independently
checked all239 frozen build/test/source hashes against current files, all23
actual target ELFs against reviewed sixth candidates plus seventh runtime, and
archive SHA256/byte sizes and their contained result inventories. Raw source
runs are ARM20261026T222046-249415583 and x8620261026T222426-695851582; durable
copies are the hashed aarch64/x86_64 evidence archives in full-first.

All78 ARM and15 x86 result files have passed=true. Required Makefile gate
inventories are present, with no skip/failure markers;200 Python validator cases
run on each architecture. The three additional timer traces verify16/32/32
IRQ/ACK/EOI/return sequences. Host tests and ownership compiler gates complete,
including41 process clients; current compiled layout is9832 bytes/three pages.
Fixture reproduction, legacy198 process executions,48 startup-delivery executions,
four launcher profiles, three group profiles, both runtime profiles, allocator,
exception, preemption, secondary/SMP and x86 unsupported-profile regressions pass
in their stated scopes. These are local results, not remote CI evidence.

Independently revalidated the final full-suite runtime raw logs and full traces:
64MiB526 slices (286 SVC,238 IRQ,2 faults),128MiB525 (286,237,2), all24 cases per
profile and baseline41, with exact3241-frame recovery. Final ordinary group
profiles also revalidate unmodified: warm free3119/completed5/generation6 and
fresh free3122/completed2/generation2. Their echo bytes agree with actual call
interleaving; variable busy-loop quantum counts obey the bounded policy. No
failed earlier candidate or diagnostic result was overwritten or recast as pass.

The Stage B experiment is accepted locally: multiple resident protected owners,
actual saved-context continuation, scoped identities, persistent limits,
completion/cancellation and bounded reclamation are evidenced. This is not a
complete soundness audit or a global phase/goal completion. Matching remote CI,
KVM/native ARM measurements, multicore user scheduling, userspace spawn/wait/cancel,
service IPC, Linux compatibility and an end-to-end useful service workload remain
outside this acceptance. The current evidence summary states those limits accurately.
