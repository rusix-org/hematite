# Native process runtime: independent review

2026-10-26. Independent AI review, not human review. Proposal and implementation
approval are pending. This milestone must execute multiple resident protected
processes with concrete completion/cancellation and resource evidence; a new
metadata model alone cannot satisfy it.

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
