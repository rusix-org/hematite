# 0030 independent review: task finalization and injection tests

## Final-boundary production change: design approved

Reviewed 0030-task-final-boundary against the actual execute_task_turn and caller
at ordinary checkpoint 67b86e1b. The current final accounting branch does not
inspect Executor retirement; the caller can install a provisional Reply/Park
before its later administrative retirement handling. This conflicts with the
specified selected-step failure boundary.

Approve the exact change after genuine finish/Timing recording: accounting error
or actual Executor retirement selects Unavailable; otherwise retain any existing
terminal outcome; otherwise Exhausted selects Budget. A valid clock/accounting
result remains accounted=true even when retirement selects Unavailable. Do not
invent a Clock error, alter counter placement, or expand the scope to other
terminal paths.

The existing caller then bypasses settlement installation, drops the detached
Session/provisional park once, clears Dispatching, and runs its terminal cleanup
under the retained Lease. Root retirement becomes Executed Unavailable; child
retirement retains its actual Step and administratively closes the root before
wake. Existing immutable root closure remains unchanged. No-entry cancellation
and preselection retirement retain their existing separate precedence. Exclusive
Executor ownership and the Lease prevent an interposed user re-entry.

This is implementation design approval only for that final branch. No hook,
seed, mutable observer, new unsafe operation or public API is approved here.
Source review, all affected linked stack/counter-order checks, actual ordinary
and separately reviewed negative traces, and full local regressions remain
required before verified acceptance. The broader task-injection proposal is
pending separate review.

The implemented production change matches the approved branch exactly: after
actual Timing/accounting, result.is_err() || self.executor.retired() selects
Unavailable. No other source path changed in that diff. Source acceptance does
not imply compiled or hardware verification.

## Broader fixed hooks: concrete pending freezes

Reviewed the constructor/provenance and ownership proposal. A one-use private
bootstrap authorization, constructor-selected immutable archive, actual committed
first-child identity, consumed original TaskAdmission and original Session returned
from a deliberately refused wake install are consistent with existing ownership.
No caller seed/address/callback is needed. Three details block implementation:

- Replace the draft's stale pre-counter guard wording with the approved actual
  post-finish/Timing precedence above.
- Freeze an exact archive path list and root mapping. Ten roots plus worker,
  demo/loop and legacy_hello, with explicit bin/demo directories, would be 15
  entries; no implicit extra fixture or pathname authority is acceptable.
- Define real budget-poll failure behavior for regression, range violation or
  polling-bound exhaustion. Consume its authorization without recording a
  successful INJECT/Fired result, preserve cleanup and fail the profile through
  Unavailable/retirement (or explicit fatal failure), never a fabricated Budget.
  A failed hook must not rearm. Freeze copied NegativeObservation fields and its
  access lifetime relative to Authority reap.

The ten hooks remain unapproved pending these narrow corrections. Existing
ordinary source and final-boundary approval are unaffected.

Re-reviewed the revised fixed-hook proposal. The original three freezes are now
resolved: exact 15-entry negative archive, typed read-only observation with Mode
after metadata reap, and ConsumedFailed/retirement cleanup without successful
INJECT or rearming. Existing boot construction now uses a private safe fresh-state
marker instead of adding another unsafe constructor. Shared guard wording links
the approved post-accounting boundary. The original finite-worker wake-install
scenario is retained: actual installed Waiting must be observed; an early worker
completion/missed hook fails rather than substituting a different workload.

One accounting clarification remains: an invalid or regressing budget-poll read
must set the existing sticky clock_failed condition, preserving the last valid
snapshot with accounted=false even if the final read recovers. Exhausting only
the observation-count bound with valid readings can retain genuine accounting
while selecting Unavailable through retirement. Both paths are ConsumedFailed
and cannot pass the intended Budget profile. Other constructor/ownership/trigger
mechanisms are ready for implementation once this precise distinction is frozen.

Verified the final polling distinction on disk. Approve exact fixed ten-hook
implementation design, retaining original finite-worker wake scenario, canonical
one-use constructor, safe fresh-state marker, typed observer and the stated
ownership/trigger restrictions. Root assignment and separate source/layout/
stack/ordering/raw-trace gates remain required; no generic seam or scenario
substitution is authorized.

## Guard-only compiled gate accepted

Independently verified all 24 actual ELF/assembly hashes against guard-candidates-
first. Twenty-one ELFs are byte-identical to the ordinary matrix baseline. The
three changed profiles retain their reviewed bounds including 640-byte allowance:
monitor 64,432, ordinary runtime 65,376, task matrix 61,552. Runtime's task branch
is unreachable through its ordinary constructors; the actual task profile includes
that branch in full. No allowance, stack or path justification changed.

Inspected the changed execute_task_turn branch flow in all three emitted images:
real dispatch returns before final ISB/MRS; budget and Timing stores precede the
Executor retirement byte read; retirement returns Unavailable (6), preventing
caller settlement installation. Invalid accounting follows its existing rejection
path. This approves bounded ordinary regressions for the guard-only candidates,
not hook-feature binaries or dynamic failure-path evidence.

Changed ELF SHA256 values: monitor
fb64ee1247de7df42e8785f8a905ebdae0b45677058c4e1e8e02968d1a6425a3;
runtime 5319dfeae33e6b7abcd180228151fb4c26461bfea5a3666189b5773808be682b;
task matrix 0ba7f356b84bd34ed64b5fff3899e6ea6fa047c2ad641c6aed272dca75225650.

## Negative bootstrap, fixtures and legacy recovery

Approved the negative bootstrap/safe-driver design: same existing boot proof and
preflight, fixed compiled profile, borrowed large results and lexical cleanup,
actual published/held/reaped equality, full frame recovery and no replacement
Executor. Negative compiler layout mode must explicitly identify its feature and
hash transitive inputs; ordinary output remains unchanged. A provisional ordinary
layout seed can build fixtures but cannot satisfy final negative guest admission.

Reviewed task_negative_cases.rs/S/ld and its builder for fixture-only compilation.
Ten literal scripts, original finite-worker wake, MAX grant/token checks, three
real sequential slot retirements and explicit EXIT220 fallbacks match scope.
Local buffers and scalar assembly ABI follow the reviewed bounded ownership.
Builder binds exact 15-entry inventory, literal case-selection entry, canonical
legacy hello, loop symbols and independently checked archive bytes. Final kernel
verification must require the actual negative-feature layout marker.

Approved an explicit protocol amendment for capability exhaustion's same-Executor
legacy recovery: preserve existing Trace-mode HMPROC records only inside HMTASK
LEGACY_BEGIN/END, parsed with the exact existing hello report grammar and actual
two WRITE/EXIT hardware transitions. Require original payload, control restoration
and frame recovery; reject HMPROC elsewhere or arbitrary extra diagnostics. This
supersedes the earlier no-HMPROC sentence without introducing a mutable output-mode
setter. Update 0029/0030 consistently. These are implementation approvals only;
negative sources, artifacts, stacks and actual raw results remain pending.

## Hook source and layout extractor review

Reviewed the fixed hook implementation in process.rs, process_task.rs,
process_task_turn.rs and process_task_negative.rs. Approved for compilation,
not guest execution. The standalone negative-feature observer cfg mismatch was
corrected to include both test features before this verdict.

The fresh private Executor marker selects only a compiled literal profile; the
negative constructor consumes it before fallible archive admission. Its private
shared constructor installs provenance only for that canonical construction.
Ordinary construction carries no hook, including when given identical bytes.
Capability and slot seeds occur before publication, and successful observation
cannot be reminted by the safe driver. The six additional production-absence
compiler clients retain strict E0599 expectations; their execution remains a
separate evidence gate.

Actual committed child identity is recorded by the commit path. Abandonment
consumes the original TaskAdmission after copying its real counters. Budget
polling uses ordered real counter reads, consumes the hook before polling, and
propagates invalid readings through sticky clock_failed. Poll-limit exhaustion
retires without reporting a successful injection. Deferred facts cannot replace
an existing fact, and no diagnostic UART occurs inside the poll.

The Ready hook predicate uses has_session, supported by the existing settlement
invariant: terminal policy outcomes set End and close/release the root before a
subsequent child selection; a parked root has no installed Session. Together
with the explicit no-Closing, no-cancellation, no-Waiting and no-Dispatching
checks, this establishes the required resumable root without a new public API.
The wake hook receives the original Session only after actual matching completion
consumption. Returning that Session as Err drops it before administrative failure
cleanup; it installs neither a reply nor Ready state. Child completion evidence
remains distinct from the administrative root closure.

Reviewed check_runtime_layout.py's negative option: the reversible original
Group copy is unchanged, actual feature cfg is supplied to both compiler stages,
and the hook module, canonical archive and generated layout are hashed. The
negative result has an explicit feature marker; ordinary output retains its old
shape. This is layout evidence only. Require a fresh witness after final source
and fixture generation, actual negative-feature artifact binding, and compiled
stack/counter ordering review before any negative guest.

Reviewed the new safe task_negative_checks driver and ARM bootstrap wiring. The
adapter preserves the existing CPU0 topology, timer preflight and private boot
proof, with explicit feature priority and isolated initialization. The driver
bounds both selected Steps and total turns, checks actual report controls and
frame accounting, compares published/held/reaped completion, and destroys Group
before checking full recovery. Capability recovery uses the same Executor for
both the Resources retry and actual legacy execution.

Approved feature-gated crate-private re-exports of copied HookStatus and SeedKind
for exact observation checks; no fields, setters or arming inputs are exposed.
Driver compilation approval is conditional on installing those exact state/seed
comparisons in addition to case/fired checks. The legacy prepare/run chain moves
large owners between helpers and needs its own emitted stack bound, alongside
negative construction and task turns. This source review does not establish fit
or runtime success.

The formatted driver now includes the exact Fired/seed comparisons, satisfying
that compilation condition. Reviewed the serial candidate builder: it removes
inherited HM_TASK_NEGATIVE for ordinary candidates, sets each literal only for
its isolated negative output directory, and records the selection.

One subsequent provenance refinement remains before final source freeze: consume
must reject authorization retained across intervening ordinary preparation or
namespace use. The canonical driver constructs immediately, so this is not an
observed safety failure. Last-prepared-console alone cannot detect console=false
issuer burns. Proposed cfg-only correction clears pending authorization at shared
prepare_inner entry and checks fresh namespace/capability state when consuming;
negative construction already consumes before its own preparation. No native
Issuer getter or public mutable hook API is needed. Await the exact source fix.

Freshness refinement is now source-approved: take_negative_profile consumes
first and validates retirement, execution index, group namespace, exact expected
capability seed and last-console state. Shared prepare_inner clears authorization
before any failure or issuer use. The immediate canonical construction remains
unchanged, including console-less internal preparation after consumption.

Independently decoded the reproduced newc archive: exactly 15 entries and 13
ELFs, each member byte-identical to its separately retained ELF, with zero tail
and no writable executable LOAD. The reproduced archive equals the canonical
269,312-byte archive, SHA256
cdd793fa869f53eb8b61303a5aabcb6a8f9f1420f4c96721b2883c851ec1613f.
Inspected the worker disassembly: four checked zero-length WRITE SVCs precede
the actual 16-byte copy, literal comparison, output and Exit55. The loop is an
actual self-branch after clearing NZCV. Slot-exhaustion code contains three
sequential SPAWN/WAIT pairs. This supports the fixed fixture contract; it does
not prove that the finite-worker wake hook reaches parked state in a guest.
The final source-bound negative witness still must be refreshed after source
formatting/freshness changes and bound to the candidate binaries.

## First complete compiled candidate gate

Independently verified all 34 actual ELF hashes and archived assembly hashes in
`docs/evidence/task-negative/candidates-first/inputs.json`. Twenty-two ordinary
ELFs are byte-identical to guard-candidates-first. The changed monitor and task
matrix retain their previously reviewed maximum chains: 63,792 and 60,912 bytes,
respectively, before the unchanged 640-byte helper allowance. The identical
ordinary runtime retains its 65,376-byte reviewed bound, including allowance.

All ten negative profiles have a conservative maximum of 60,496 + 640 = 61,136
bytes, leaving 4,400 bytes in the existing 64 KiB coordinator stack. The chain is
boot 2704, run 16, checks 192, execute 2064, step 768, Group::turn 5568,
execute_task_turn 384, dispatch_task 1152, load_boxed 17936, box_loaded 9888,
box_process 19776, allocate_heap 48. The constructor and initialization paths
are included in the call graph. The separate same-Executor legacy path totals
52,448 + 640 = 53,088 bytes, including legacy's 18,416-byte frame, preparation's
9,888-byte frame and legacy_run's 10,128-byte frame. No stack size or allowance
was changed.

The emitted execute_task_turn and dispatch_task bodies are byte-identical across
all ten profile assemblies. Inspected their real ordering: detached Work/capture
materialization and dispatch return precede the hook branch; the bounded poll
uses ISB/CNTPCT with all four opaque addresses, consumes state before looping,
checks 56-bit range and monotonicity, and writes Fired/injection only after
reaching the existing hard end. The emitted bound is exactly 2^24 reads.
Success then reaches the separate final ISB/CNTPCT. Invalid-clock state reaches
the non-accounted path; valid finish commits actual budget/Timing before the
Executor retirement byte is checked. Retirement produces Unavailable before
caller settlement installation. Hook fact creation performs no UART operation;
formatting remains in deferred report emission after execution returns.

This clears the linked stack and counter-order gate for these exact candidates.
It does not approve guests independently of the final source-bound negative
layout/artifact witness, privacy probes and strict host validator source review.

Verified the final task-negative-layout-fresh witness's current source hashes,
feature cfg in both compiler commands, exact 88-byte blob and digest. Actual
Authority is 968 bytes/one page; Image 472/one page, Process 9832/three pages,
Group 1960 bytes and metadata charge three pages. Independently checked all 59
recorded compiler clients pass and the six new production-absence stderr logs
contain their required E0599. The nine host tests are explicitly synthetic
validator evidence, not guest execution. Strict validator source approval and
artifact/kernel binding remain separate from these completed gates.

## Initial actual ten-profile trace review

Independently replayed unmodified serial and complete QEMU exception/GIC logs
from guest-first-final_budget_spawn and all nine guest-first-rest directories.
Verified each actual kernel hash against candidates-first and each recorded
embedded archive span against canonical archive bytes. All ten guests exited
zero with the full 41-check baseline; the only aggregate rejection was the
intentional unreviewed-grammar flag. No trace tolerance was added for this review.

The ten runs contain 71 actual EL0 slices: 50 SVCs, 21 timer IRQs and zero faults,
plus eight preflight and sixteen baseline IRQs per boot. Both final-budget cases
reach the actual whole-budget end and consume the original 6,250,000-tick budget;
SPAWN commits its child before the denied settlement, while WAIT remains a
provisional park. Closure cancels/releases children without a later user resume.
The first SPAWN budget case never enters its child. Platform reply/park and
owned-admission abandonment retain actual accounting and retire before further
entry; abandonment keeps the real second call count without a second commit.

The finite-worker wake case actually parks the root, runs the worker through
four zero writes, copied output and Exit55, then rejects installation. The child
retains executed Exit55 evidence; root closure is administrative with the real
child identity, no fabricated parent trap and no wake. Both child-retirement
cases fire at the canonical loop's real timer trap with their distinct actual
Ready/Waiting parent state, followed by administrative root closure and cleanup.

Capability exhaustion uses the literal MAX grant, rejects SPAWN and subsequent
same-Executor task setup, then runs actual legacy hello inside the strict HMPROC
interval and recovers all frames. Slot exhaustion commits child slots 1, 2 and 3
with generation MAX, waits/reaps each real Exit55 worker, and rejects the fourth
SPAWN. Every profile returns to its exact 3,236-frame baseline after metadata
reap/drop. Seeded profiles have no synthetic INJECT record; the other eight have
exactly their intended single consumed injection.

Approved enabling the strict trace-review flag for these ten fixed profiles.
This is evidence of specified test injections and seeds, not actual hardware
malfunction, arbitrary fault recovery, or full regression completion. Preserve
these initial flag-rejected results. A fresh strict rerun and full ARM/x86 suite
remain required for final milestone acceptance.

Reviewed Makefile integration: fail-fast shell clears inherited profile selection,
requires the canonical profile list command to succeed and return nonempty,
records that list, and builds every listed case with a command-local environment
value in an isolated target directory. Actual negative-feature layout and fixture
reproduction precede the ten strict guest gates. Existing ordinary ARM/x86 gates
remain present; the new mutation suite runs on both architectures.

Independently replayed all ten fresh guest-second serial/full traces through the
now-enabled strict validator, with actual candidate hash binding. All pass with
71 slices, 50 SVCs and 21 IRQs, plus every 41-check baseline. Initial flag-rejected
logs remain preserved. Full-suite completion is still pending at this checkpoint.

## Preserved first full-suite failure

The first full suite failed the ordinary 128 MiB copy_inputs case, before the
negative profiles or x86 phase. Its kernel hash equals the reviewed candidate.
This is genuine budget exhaustion, not a parser mismatch: group 8/root turn 8
started at 43,965,309 with 2,274,684 ticks remaining, and finished at 46,415,489.
The charged interval was 2,450,180 ticks (39.203 ms), exceeding remaining budget
by 175,496 ticks (2.808 ms) at 62.5 MHz. Earlier allocation turns cost 24.858 and
26.200 ms, versus 19.980 and 19.382 ms in the passing 64 MiB run; first-entry
cost was 11.365 versus 9.349 ms. The failure reflects cumulative slower charged
work, not evidence of one particular host pause.

The actual descriptor/input copy and third child COMMIT precede final Budget;
the provisional reply is discarded, the child is cancelled without entry, and
publication reports zero children with three metadata pages remaining. No full
baseline/reap success is claimed after that point: the driver rejects unexpected
Budget and exits. Source and reviewed emitted order keep diagnostic serialization
after the final counter; no observed evidence indicates UART diagnostics were
charged. The interval legitimately includes user execution, typed copies, child
allocation/loading/preparation and commit. Existing logs cannot separate host
scheduling/TCG effects from time spent in those operations. Do not claim a
confirmed host stall or weaken the 100 ms policy/expected outcome. Preserve the
failure and distinguish any subsequent diagnostic rerun from full-suite success.
