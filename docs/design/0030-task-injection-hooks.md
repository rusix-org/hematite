# 0030: fixed task-failure injection profiles

Status: **draft, pending independent review; no implementation authorization**.
The ordinary task matrix has passed both declared guest profiles; these ten
negative profiles remain separate, unimplemented coverage. They test deliberate
failure-boundary behavior, not spontaneous hardware errors. This extends the
[0029 protocol](0029-task-protocol.md), preserving its wire, budgets, capacity,
ordinary behavior and existing stack allowance. No assembly transition change,
mutable frame/context export or arbitrary callback is proposed.

## Fixed profile and authority

Compile a dedicated `task-negative-tests` feature. `option_env!("HM_TASK_NEGATIVE")`
selects exactly one of the ten literal names below; missing/unknown selection
fails bootstrap as Unsupported, never selects a default or records PASS. The
host records the environment and hashes the resulting kernel. All-features lint
may compile without a selection, but that image cannot run a negative profile.
Every new hook field, branch, constructor and observer is cfg-gated out of
ordinary and task-tests binaries. Existing ordinary records remain unchanged.

Only a fixed private constructor mints hook provenance:

```rust
// Proposed private core APIs, only under task-negative-tests.
// Called inside the existing boot constructor after its proof/admission succeeds.
fn Executor::mark_negative_profile(&mut self) -> Result<(), Error>;
fn Group::new_negative_task<'e, 'p>(executor: &'e mut Executor,
    factory: Factory<'p>) -> Result<Group<'e, 'p>, GroupError>;
fn Group::negative_observation(&self) -> Result<NegativeObservation, GroupError>;
```

The existing unsafe boot constructor remains the only boot-authority boundary.
Its dedicated negative-profile branch calls private safe mark_negative_profile
before returning the Executor. That helper requires the private fresh-state
predicate (no groups, no issued console/task generations, no prior mark), rejects
otherwise, and accepts no seed/index/value. It is not exposed outside process.rs.
No additional unsafe constructor or reminting of boot proof is needed.

new_negative_task consumes that one-use authorization and internally selects
its canonical root and complete immutable negative archive. It admits actual
ELF bytes through the existing loader and uses the same lease-bound construction
as ordinary tasks; it must not call a lease-acquiring constructor recursively.
A failed construction consumes authorization and fails the profile, never retries
an injection silently. Ordinary new_task does not mint provenance, even for
byte-identical programs. No caller image, path, address, timestamp or mutable
owner is accepted by the negative constructor. Slot-exhaustion's fixed mutation
occurs after fresh empty Group construction and before any root publication.

A private cfg-only HookState inside the owned Authority stores the compiled case,
Armed/Fired/ConsumedFailed state, and the actual first-child TaskId where required. The latter
is captured from real SPAWN commit, never supplied by the driver. Canonical input
identity follows the constructor's immutable archive origin; generated loop/marker
constants are independently checked against those same ELF bytes. This is not a
runtime pathname-as-authority check. Production constructors leave no hook state.
NegativeObservation returns only copied case/fired-count/seed-kind facts; it grants
no mutation or rearming. Fired count must equal one (or exactly one seed for the
two exhaustion cases) before successful completion. A guard is consumed before
applying its effect so another restored turn cannot repeat it.

The canonical archive is `fixtures/task-negative-initramfs.cpio`, exactly 15
entries: directories `bin`, `demo`; `bin/<profile>` for each of the ten literal
names in the table; `bin/worker`, `demo/loop`, and `bin/legacy_hello`. Each compiled
profile selects only its matching `bin/<profile>` root. Catalog paths remain
worker/loop; legacy_hello is only for the exhaustion recovery check. Manifest and
independent archive/ELF validation bind all bytes before any guest acceptance.

The copied cfg-only observer is:
```rust
struct NegativeObservation {
    case: NegativeCase, // closed ten-variant enum; no public construction
    state: HookStatus,  // Armed, Fired, ConsumedFailed
    fired: u8,          // exactly 0 or 1
    seed: SeedKind,    // None, TaskCapabilityMax, ChildSlotsMax
}
```
Fields are exposed through read-only getters only. Group observation is allowed
while Authority exists, including Closing/held completion, and must be captured
before task_reap releases metadata; afterward it returns Mode. Seed profiles
set Fired/fired=1 only at the actual fixed mutation; no mutable observer survives.

## Exact locations and effects

All selection happens after genuine hardware restoration under retained Lease,
while the parent and child owners remain inactive and claimed. Failed trigger
predicates leave Armed unchanged; reaching another terminal outcome before the
required trigger fails the test. The first matching boundary must fire.

| Profile | Exact first eligible boundary | Only permitted injected effect |
| --- | --- | --- |
| final_budget_spawn | Canonical root's call1 SPAWN committed child; actual provisional Reply Ok exists in Work | Wait for actual existing hard_end before final counter read |
| final_budget_park | Root call2 WAIT has actual provisional Park for first loop child | Same real-counter wait, without installing Waiting |
| platform_reply | Same committed call1 Reply boundary | Invoke existing sticky Executor retirement |
| platform_park | Same call2 provisional Park boundary | Invoke existing sticky retirement |
| abandon_dispatch | Root call2 SPAWN after owned admission and copied calls/written snapshot, before descriptor copy/dispatch_owned | Consume/drop the actual TaskAdmission and take existing failure-settlement path |
| wake_install_failure | Real first worker completion matches installed Waiting; cap/result consumed and ParkedSession::wake returned the original Session, before checked install | Return that Session as installation failure without modifying Process |
| capability_exhaustion | Dedicated fresh bootstrap before any Group or Session preparation | Set next_task_cap to u32::MAX exactly once |
| slot_exhaustion | Dedicated fresh Group before root publication | Set only child slots1,2,3 next to u32::MAX |
| child_retire_ready | First real timer trap of actual first program2 child, ELR within canonical loop interval, while root Ready | Invoke sticky retirement before child finalization |
| child_retire_waiting | Same child/timer/ELR predicate while root has finalized Waiting | Invoke sticky retirement before child finalization |

Ready here requires the exact root identity, no pending cancellation/Closing,
a present resumable Session and no parked value. Waiting requires the exact
owned ParkedSession/PendingWait targeting that child, not merely a requested WAIT.
Both child-retirement cases match actual lower-EL kind3 traps and copied ESR/ELR,
not a guest-reported counter or an arbitrary SVC ordinal. Earlier out-of-loop or
wrong-parent-state traps remain visible and do not consume the hook. The ready
fixture permits at most32 root zero WRITEs; no match by its bound is failure.

The budget hooks retain the existing computed hard_end and real counter frequency.
Use the task-only ordered counter helper with Group, Process, Work and Capture
opaque addresses after their real updates. Poll monotonically until now>=hard_end,
with a fixed upper bound of 2^24 observations. Regression, out-of-range counter,
or exhausted polling bound sets ConsumedFailed, retires Executor and follows the
normal Unavailable finalization/owned cleanup path. It emits no successful INJECT,
never sets Fired, and fails the profile even if cleanup succeeds; it cannot rearm.
A regression/out-of-range observation also sets sticky clock_failed, even if the
final read recovers: existing rejected-clock finalization preserves the previous
budget snapshot and reports accounted=false. Poll-count exhaustion with otherwise
valid observations may account the genuine final elapsed interval before retiring
as Unavailable. Both remain ConsumedFailed and can never pass. Do not forge elapsed
time or convert failure into the requested Budget result. Record the injection only when
the actual threshold was reached, then let the genuine final read and Budget::finish
establish exhaustion. No UART, allocation, user access or callback occurs in this
wait. The ordinary100ms budget is neither shortened nor extended. Guest timeout
remains an independent failure bound, not a substitute for this bounded loop.

Abandonment does not substitute a fabricated Session, Response or End. Preserve
the actual admission counters in Work before consuming the guard; return the same
private dispatch failure that already sets Work.failed and retires Executor.
The existing sole finalizer reclaims/Closes the detached process. No Dispatching
state may survive a normal turn return.

The wake fixture uses the specified finite worker. The hook requires an actually
installed Waiting parent at worker completion; a mere issued WAIT is insufficient.
If worker completion precedes parking, the profile fails rather than injecting at
another boundary or substituting a loop/Budget outcome. Scheduling refinements
would require a separate reviewed protocol amendment. Child completion retains
its genuine outcome; failure closes the root administratively as Unavailable with
that exact worker TaskId as trigger.

Wake-install failure uses a small private helper returning Result<(),Session>.
Its ordinary path calls Process::put_session unchanged. Only the selected
provenanced failure returns Err(the_original_session) before calling it. The
caller takes its existing Unavailable management-failure path and consumes that
Session once. It must not populate Process.session to provoke failure, forge a
reply, rewind a consumed cap or set Ready. m.reaped reflects the actual successful
consume before failure. This tests the caller's checked-install failure branch,
not a spontaneously failing Option store or memory fault.

## Shared finalization guard

The [separately reviewed final-boundary correction](0030-task-final-boundary.md)
is implemented, pending compiled/runtime verification. After the real final
counter, Budget::finish and Timing recording, invalid accounting **or** sticky
Executor retirement selects Unavailable before provisional settlement install.
Hooks must exercise this shared boundary; they never assign a counterfeit End.
This shared production correction is the sole exception to hook cfg exclusion.
Its precise ordering and verification gates live in that document.

## Exhaustion and fresh-boot behavior

Capability MAX is installed only inside the fresh bootstrap constructor. Root
grant issuance consumes MAX and stores zero; actual SPAWN returns Resources, root
exits0, then normal new_task must still return Resources. After full reclamation,
run actual pinned legacy hello through the same nonretired Executor. Never create
a new Executor, reset a counter or infer success from a copied status constant.
The legacy payload and its real two SVCs remain independently trace-checked.

Child slots are seeded only inside the fresh negative Group, never after a task
is published. The root's three sequential SPAWN/WAIT pairs actually reap slots1,
2,3, advancing each MAX generation to zero; the fourth SPAWN returns Resources.
Root slot generation and task-cap issuer are not seeded. No public setter or
numeric constructor parameter is introduced. Both seed profiles use SEED evidence,
not an INJECT record claiming a hardware boundary was reached.

## Capture, proof and teardown

Keep fixed diagnostic storage within the existing <=1536-byte combined capture
bound. At most one copied Injection fact is added for a selected turn; a wake
failure fact belongs to its actual completing child's management. Both carry
only typed kind and actual scoped identity/ordinal. No event vector or retained
borrow is allowed. Serialization remains after accounting/restoration under the
lease; wake injection is recorded after CHARGE and before CLOSE/MANAGE. Other
injections are serialized at their specified logical pre-CHARGE point. No
record is emitted merely because a profile was configured.

The actual Work, hook mark and committed owners must be covered by the final
compiler memory boundary. Newly detached hook storage must be an opaque input
or be part of existing Work/Capture/Group storage; do not rely on source order.
Inspect linked stores/copies, counter reads and formatting. No global-store
completion or exact EL0 CPU-time claim is added.

Independent host checks bind root/catalog hashes, actual trigger call/ELR/state,
first eligibility, descriptor copies/commit or park, real budget arithmetic,
no prohibited child/root ERET after terminal selection, original root reason,
exactly-once cap consumption, Closing publication and full frame recovery.
Raw QEMU traces prove execution/interrupt ordering; they do not independently
prove an internal retirement flag or a deliberately refused Session install.
Report those as injected source-bound failures corroborated by resulting control
flow, never as observed hardware malfunction.

Each profile boots separately at128MiB with one CPU. It must reach its precise
injection/seed and expected cleanup, then the existing EL1 baseline. Missing,
earlier or repeated triggers, unexpected faults, timeout or boot crash fail.
Retired profiles permit no later EL0 entry. The two budget-only profiles do not
retire Executor; the exhaustion profile requires the specified legacy execution.

## Review checkpoints before any code

Approve constructor authority/freshness, exact profile selection and archive
inventory; approve the shared final-retirement guard separately. Review each
trigger and failure-path ownership, especially consumed WAIT before failed install.
Implement only after those approvals. Then require no-hook ordinary regression,
actual feature-specific layout/stack witnesses (including any changed Authority),
compiler privacy/lifetime clients, source audit, linked ordering and independently
reviewed ten-profile raw evidence. Hook absence in ordinary builds must be checked
from their linked symbols/code and preserved input hashes, not just cfg comments.
The ten profiles do not complete Phase3 services, IPC, SMP or Linux compatibility.
