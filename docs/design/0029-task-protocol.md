# 0029: proposed task hardware protocol

Status: **concrete review draft; no implementation approval**. This freezes a
proposal for review, not evidence that a case ran. The wire, owned Session,
[dispatch](0029-core-dispatch.md), [authority](0029-task-authority.md) and
[lifecycle](0029-task-lifecycle.md) supplements own mechanism semantics.
This document supersedes the exploratory inventory in
[the acceptance notes](0029-task-creation-tests.md) once independently approved.
The fixed failure/exhaustion hooks below remain explicitly unapproved.

## Profiles and inventory

`task-tests`: one ARM CPU, 64 and 128 MiB, two passes of these 25 cases in order,
exactly 50 PASS records:

| Index | Name | Bounded actual parent script |
| --- | --- | --- |
| 0 | supervisor | SPAWN worker; SPAWN loop; third SPAWN→Resources; WAIT worker→Exited 55 (immediate or finalized park/wake); CANCEL loop→0; CANCEL loop→1 after cancellation service; WAIT loop→Cancelled; WRITE success marker; EXIT 0. Nine parent calls. |
| 1 | copy_inputs | Spawn/wait three workers sequentially with absent, present-empty and full 256-byte binary input. After each SPAWN return overwrite all source descriptor/input storage, then issue the named zero-WRITE overwrite marker. Child performs four zero WRITEs before examining/copied-output use, then returns 10/11/55 for absent/empty/present. Six task calls, three zero-WRITE overwrite markers, one success WRITE and EXIT: exactly 11 parent calls. |
| 2 | wait_poll_ready | SPAWN loop; poll WAIT→Pending; CANCEL→0; CANCEL→1 after child cancellation service; completed WAIT→Cancelled; repeated WAIT and CANCEL→Handle; EXIT. Eight calls, no completion double-consume. |
| 3 | wait_budget_child | SPAWN loop; blocking WAIT must actually finalize Waiting; loop exhausts its real active budget after repeated timer resumes; matched wake returns Budget; EXIT0. Exactly 3 parent calls. No parent entry while parked; wake consumes no additional call or budget turn. |
| 4 | rights_console | Two fresh groups: child-console policy false rejects a console request→Denied, then allows console=false; WAIT-only completion rejects CANCEL→Denied and succeeds WAIT. Worker without console performs no WRITE. Root exits. Each parent exactly 4 calls. |
| 5 | foreign_authority | Parent spawns loop A, then worker B with actual parent launch and A completion tokens in copied input. B attempts SPAWN/WAIT/CANCEL with those tokens, console/task kind confusion and self/parent guesses; all fail Handle with no effects. Parent waits B, cancels/waits A, exits. Parent 6 calls; B exactly 9 calls. |
| 6 | stale_reuse | SPAWN finite worker, WAIT it; SPAWN replacement into freed slot; WAIT/CANCEL using old cap→Handle; WAIT replacement; EXIT. Require new capability generation and internal slot generation, seven calls. |
| 7 | malformed_outer | Exact ordered requests in the malformed-request table below, followed by a valid SPAWN/WAIT/EXIT. Exactly 12 parent calls. |
| 8 | malformed_descriptor | Exact ordered descriptor mutations below, followed by valid SPAWN/WAIT/EXIT. Exactly 20 calls. |
| 9 | copy_faults | Exact ordered whole-descriptor and payload range failures below, then valid spawn with unaligned descriptor/input, WAIT/EXIT. Exactly 9 calls. |
| 10 | child_fault | SPAWN worker in own-RX-fault mode and normal worker; WAIT fault→Fault; WAIT peer→Exited 55; EXIT. Five calls. |
| 11 | held_capacity | SPAWN two loops; CANCEL each. Round robin services each cancellation before the next root entry, leaving two held completions and no child Process frames. Third SPAWN worker→Resources; WAIT first→Cancelled; SPAWN replacement worker; WAIT old first cap→Handle; WAIT second→Cancelled; WAIT replacement→Exited 55; EXIT. Exactly 11 calls. |
| 12 | child_frame_cap | SPAWN with independently calculated child charge minus1→Resources; exact charge succeeds; WAIT/EXIT. Four calls. No physical pressure or generation burn for cap rejection. |
| 13 | group_frame_cap | Fixed large-BSS root and catalog worker satisfy first child admission but metadata+root+two child charges exceed 512; second SPAWN→Resources while first remains owned. WAIT first, then another SPAWN succeeds and WAIT/EXIT. Six calls. Artifact oracle must establish both inequalities before boot. |
| 14 | allocation_rollback | Actual parent issues three SPAWN attempts under pressure leaving 0,n,n+1 free, failing load/startup/box. Each rejection leaves parent and prior owners unchanged; release pressure at fixed inactive boundaries. Fourth SPAWN succeeds; WAIT/EXIT. Six calls. Task-cap/console burns follow actual reservation points, not rollback fiction. |
| 15 | parent_exit | Parent spawns finite worker and loop, exits 17 with both still owned. Closing saves 17, denies further root entry, cancels/reaps children and publishes root Exited 17 only after cleanup. Three calls. |
| 16 | parent_fault | Same two admissions, then pinned own-RX write. Closing preserves Fault; actual abort PC/FAR/ESR required. Two calls. |
| 17 | parent_budget | Same admissions then actual non-cooperative parent loop. Repeated real timer resumes exhaust its active budget; Closing preserves Budget. Two calls. |
| 18 | parent_call_limit | Two admissions followed by 61 zero WRITEs, then a valid 64th SPAWN. No task effect on attempt 64, Closing preserves CallLimit. Exactly 64 calls; no 65th entry. |
| 19 | parent_cancel_ready | Root creates two children; test driver requests public root cancellation at the fixed second-SPAWN returned boundary, while Ready. Root's selected no-entry cancellation starts Closing; children quiesce before root Cancelled publication. |
| 20 | parent_cancel_waiting | Root creates loop then blocking WAIT. After actual finalized Waiting record, driver requests public root cancellation. No wake/reply or parent ERET follows; Closing preserves Cancelled and removes child. Two calls. |
| 21 | limit_spawn | Separate root: 63 zero WRITEs then valid SPAWN at 64. No descriptor copy, reservation, child or task reply. Root CallLimit. |
| 22 | limit_wait | SPAWN loop, 62 zero WRITEs, WAIT at 64. No provisional Park or completion consumption; Closing cancels child. |
| 23 | limit_cancel | SPAWN loop, 62 zero WRITEs, CANCEL at 64. No user cancellation effect; root terminal Closing independently cancels its owned child. Distinguish those events. |
| 24 | drop_waiting | Root SPAWN loop and finalized blocking WAIT; driver drops inactive Group directly. No synthetic completion/wake, no subsequent old-owner ERET, exact frame recovery. A new task group runs the ordinary supervisor script successfully. |

Every case returns to its common pre-group free-frame baseline, including all
catalog/authority boxes. `rights_console` and `drop_waiting` have exactly two
groups; others one. Do not silently add roots or reuse a stopped Session.
Each group has four slots including root and held completions, at most two child
caps, and logical 512-frame cap. Per-owner 256 accounted turns, per-case 1024
selected steps and per-pass 8192 selected steps are hard bounds; reaching a larger
count is failure. Guest deadline 60 seconds; each raw artifact 32 MiB maximum.
Existing Session 64-call/4096-byte limits are unchanged. Every purported fixture
success has exact expected calls/output/outcomes; Budget is not an acceptable
substitute except `parent_budget` and the dedicated negatives.

Before these cases, six root-construction failures run under real owned Pool
pressure. Independently derive Image box pages I, authority box pages A, root
Space pages N and Process box pages K. Leave respectively 0, I, 2I, 2I+N,
2I+N+1 and 2I+N+1+K free to fail first Image, second Image, root load, startup,
Process box and authority box. Require no root publication/ERET, exact baseline
restoration after pressure release, and reservation burns matching authority
setup order. No total-Pool-free shortcut may stand in for the claimed stage.

Separate `task-negative-tests` boots run one fixed scenario each at 128 MiB:
`final_budget_spawn`, `final_budget_park`, `platform_reply`, `platform_park`,
`abandon_dispatch`, `wake_install_failure`, `capability_exhaustion`,
`slot_exhaustion`, `child_retire_ready`, `child_retire_waiting`. Each requires exactly one expected
root outcome and full reclamation, then the existing EL1 bootstrap baseline.
These ten profiles are successful negative tests only when their exact injected boundary and
fail-closed outcome are independently observed. No generic kernel crash/timeout
counts. Executor retirement forbids any later EL0 entry in that boot.

Exact negative root scripts and one-shot hook trigger:

| Profile | Root calls before terminal | Expected first root closure |
| --- | --- | --- |
| final_budget_spawn | First SPAWN worker commits; spin before final read of call 1. | Executed Budget, child receives no ERET, no reply installation. |
| final_budget_park | SPAWN loop then blocking WAIT; spin after provisional Park on call 2. | Executed Budget; no Waiting install, wake or consumed completion. |
| platform_reply | First SPAWN worker commits; retire before final read/check of call 1. | Executed Unavailable; no child entry or root reply. |
| platform_park | SPAWN loop then WAIT; retire after provisional Park on call 2. | Executed Unavailable; no Waiting/wake. |
| abandon_dispatch | SPAWN loop; on second SPAWN drop the actual admitted guard before command effects. | Executed Unavailable with calls 2 preserved; only first child was committed. |
| wake_install_failure | SPAWN worker then blocking WAIT; when that actual worker completes, consume the matched PendingWait and reject its one checked install. | Administrative Unavailable triggered by that child's actual terminal Step; no fabricated root turn or parent ERET. |
| capability_exhaustion | Fresh fixed-seed constructor sets task-cap issuer to MAX before any preparation. Root grant consumes MAX; SPAWN→Resources then EXIT 0, two calls. After reap, another task setup→Resources; ordinary legacy hello still executes. | Executed Exited 0; task issuer stays exhausted without retiring Executor. |
| slot_exhaustion | Fresh fixed constructor sets only child slots 1..3 next-generation to MAX. Parent SPAWN/WAIT three workers sequentially, then fourth SPAWN→Resources and EXIT 0, eight calls. | Executed Exited 0; all three child slots retired, root slot and task-cap issuer remain distinct. |
| child_retire_ready | Root SPAWN loop, then at most 32 zero WRITEs keeping itself Ready. First eligible actual child timer trap inside its loop interval while root is Ready triggers fixed retirement before child finalization. No trigger by the bound is failure. | Child Step terminal Unavailable; Administrative root Unavailable triggered by that exact child, no further root entry or invented root turn. |
| child_retire_waiting | Root SPAWN loop then blocking WAIT. First eligible actual child timer trap inside its loop interval after root is finalized Waiting triggers fixed retirement before child finalization. | Child Step terminal Unavailable; Administrative root Unavailable before wake, dropped parked Session, no fabricated root clocks/registers/reply. |

For child-retirement profiles, the hook additionally requires the exact child
TaskId committed by that root's first SPAWN, program2, a real lower-EL timer trap
inside the independently pinned loop interval, and root Ready/Waiting state as
listed. Trigger the first eligible actual boundary, not an arbitrary later
ordinal; prior ineligible setup/out-of-loop or Ready events do not trigger the
Waiting profile. Record every prior step so the host independently establishes
first eligibility. These profiles do not rely on a fixed child SVC ordinal.
Ordinary Fault does not retire Executor and cannot stand in for these tests.

All use exact canonical negative fixture identities; hook state is not user
input. Each failure profile has a single required INJECT; exhaustion profiles instead
have one fixed SEED record before root/group preparation. A missing/unreached hook/seed or
an earlier ordinary Budget/CallLimit does not pass the intended negative.

## Request subinventories

`malformed_outer`, in order: wrong version+invalid launch→Version; unknown
opcode→Operation; zero launch+bad size/address→Handle; console token as launch→
Handle; unknown task generation→Handle; valid launch with descriptor size 63→
Length; size 65→Length; WAIT console token+bad scalars→Handle; CANCEL launch
instead of completion→Handle. Each x1 error result is 0. Then valid recovery.

`malformed_descriptor`, each independently restored literal 64-byte request:
version 2, size 63, unknown flags bit 2, reserved byte 44, reserved byte 48, reserved
byte 56, rights 0, rights 2, frames 0, frames 129, absent/nonzero address,
present-empty/nonzero address, unknown program 0, unknown program 3. These return
Argument 7; input length 257 returns Length 5; nonempty range addition overflow
returns Address 4. The same group includes a combined-invalid descriptor to pin
header-error Argument before an otherwise oversized-input Length result.
Equal-status internal decoder precedence is established by the separate actual
codec tests; a hardware Argument reply alone cannot distinguish those checks. Then valid recovery.
No invalid descriptor reserves a task generation or creates a child.

Foreign-worker script is exactly SPAWN(parent launch), WAIT(sibling completion),
CANCEL(sibling completion), WRITE(parent console), WAIT(own console), CANCEL(own
console), SPAWN(0), WAIT(0), EXIT 0. First eight return Handle 3/result 0; EXIT is
successful only after verifying them. Its copied job carries the parent's actual
launch, sibling completion and console tokens. It receives no launch entry.

`copy_faults`: descriptor NULL, arithmetic-overflow range, last mapped 31 bytes
plus 33 guard bytes; valid descriptor whose nonempty input is NULL, arithmetic
overflow, last mapped byte plus guard byte. Length-valid faults return Address 4.
For each, prove no partial input accepted and no allocation/issuance. Final
unaligned valid descriptor and full 256-byte input succeeds. Zero-length input
uses canonical zero address; do not introduce a hardware dereference for it.

## Canonical fixtures and deterministic scheduling

Dedicated archive `task-initramfs.cpio` contains explicit `bin`, the named root
fixtures, `bin/worker` (program 1), explicit `demo`, and `demo/loop` (program 2). Exact root basename is the case name, except `rights_console` uses
`rights_console_policy` then `rights_console_wait_only`. The supervisor reused
by `drop_waiting` is the same `bin/supervisor` already in the inventory. The ten negative roots have their scenario names; include the existing pinned
hello ELF as `bin/legacy_hello`. The normal test archive therefore has exactly
39 entries: bin plus 38 regular files (25 main roots after the documented
variant/large-root adjustments, ten negative roots, worker, loop and legacy hello). Catalog metadata is admitted once per task group from
this complete archive; no public path/program bytes can select another image.
The logical-cap profile uses a separate pinned archive with large-BSS root and
worker; its immutable catalog still contains exactly program 1/2.

Export `native_start`, each actual `write_svc`, `spawn_svc`, `wait_svc`,
`cancel_svc`, `exit_svc`, `fault`, `loop_start`, `loop_end`, plus fixed descriptor,
input and marker symbols where applicable. Wrappers share one site per operation
unless a named special boundary requires a separate site. Inspector binds every
used instruction as SVC0, loops as B-to-self and fault target as own RX entry.
Returned SVC PC means the successor instruction. Capture raw x0/x1 replies in
EL0 and compare them before the next success marker/EXIT; host metadata alone
cannot establish the application received the result.

Finite worker issues four bounded zero WRITEs before reading its job, then one
WRITE of the exact copied job and EXIT55 (absent 10, empty 11 variants). A hardware turn may end in a parent IRQ rather than a SVC, so four yields do
not universally force the supervisor's WAIT to park: its correct immediate or
parked result is accepted. The separate wait_budget_child case requires actual
parking and wake while the loop child exhausts its budget. No fixed iteration
count substitutes for that trace evidence.

For copy_inputs, the actual named overwrite-marker SVC must precede the
worker's fourth zero-WRITE SVC, whose successor leads to the pinned job-read
site. Linked source/instruction ordering binds the parent's overwrite before
its marker and the child's input use after that fourth yield. Reject a run
where interrupts cause the child to pass that boundary too early; do not
claim the source mutation happened before child use merely from final bytes.
This is a required observed order, not a claim of timing-independent scheduling. The no-console variant performs no WRITE
and exits with its independently expected code. Worker job dispatch is fixed: exactly eight bytes `HMFAULT\0` select its pinned
own-RX write; exactly 32 bytes consisting of `HMAUTH\0\0` followed by three
little-endian u64 tokens (parent launch, sibling completion, parent console)
select the nine-call foreign-authority script above. Every other present job is
opaque output, with normal 55 exit; absent/empty yield 10/11. No-console worker
skips the four zero WRITEs and payload WRITE entirely, then returns its matching
presence outcome. Fault/authority modes do not execute the normal four yields.
The full 256-byte copy case is exactly byte values 0..255, avoiding mode prefixes.
The normal worker job is exactly `worker complete\n`. Parent descriptor/input
storage mutation sites and worker read-after-yield and parent overwrite_svc symbols are mandatory artifact
symbols; their compiled order must be independently inspected.

Large-BSS artifact inequalities are mandatory, not guessed from current type
sizes: metadata+root<=512; each child<=requested 128; metadata+root+child<=512;
metadata+root+2children>512. Current prospective page counts are root 228 and
worker 96, giving charges 257+K and 125+K; these are only valid when the actual
compiler-derived K and metadata sizes satisfy all inequalities. A changed size
fails artifact admission rather than changing the runtime limit.

## Bounded driver mode checks and capture storage

No extra fixture is needed for mode gates. In supervisor, after root publication
and before first entry, check public ordinary admit/completion/reap return Mode.
After the first actual child Step supplies its copied TaskId, check child info,
request_cancel, task_completion and task_reap all return Mode. At root Closing,
check task_completion=None and task_reap=Pending. After actual task_reap, check
admit, info, completion, reap, request_cancel, task_completion, task_reap and
task_root all return Mode, then turn returns Idle. Execute these exact groups
once per supervisor case/pass. The root's real current identity is obtained by
readonly task_root, never synthesized from numeric logs.

Waiting is mandatory in wait_budget_child, where task_completion=None and
task_reap=Pending are checked exactly once after finalized Waiting. Supervisor
may legally take an immediate WAIT, so no unconditional Waiting gate is assigned
to it. All checks happen while inactive and cause no CLOCK/ERET. GATE records
bind unchanged actual Pool free count, Group charge, persistent task-issuer next
value and last prepared console generation. A tiny cfg-only readonly issuance
snapshot is proposed if the core getters do not expose both scalar issuers;
it has no mutation, pointer, callback or authority-granting constructor. The
exact getter must be reviewed before driver implementation.

Freeze one bounded private capture per selected turn: descriptor 64 bytes,
input 256 bytes, console output 256 bytes, at most two copied child-management
identities, and fixed scalar records. Total actual TaskCapture size must be
<=1536 bytes, witnessed by the compiler and checked with linked stack chains.
No Vec, formatted text, Process/Image owner, page-sized duplicate scratch,
unbounded event array or retained caller buffer belongs in it. Capture overflow
is a failing profile, never truncation or silently dropped evidence. Serialization
occurs after accounting/restoration. No public callback/sink is added.

## Fixed test-driver authority and negative hooks

Existing public root cancellation and inactive Group drop suffice for their
positive cases. Pressure uses existing owned Pool blocks, never replacement
allocation results, artificial status replies or user context mutation. A cfg-only
scenario gate may pause scheduling after a named root SVC has been restored and
its turn fully finalized, before the next selected turn; retain/release pressure
there. It cannot edit saved PC/registers, grant authority, child state or clocks.
Emit exact free counts before/after pressure and bind the next actual SPAWN to
that scenario ordinal. The test driver cannot admit children directly.

**Pending independent hook approval:** private enum-selected hooks, enabled only
for dedicated negative profiles and canonical root hashes, with no caller VA,
callback, arbitrary timestamp, response or mutable context:

- Final budget hooks spin on the actual counter until the already computed
  whole-turn deadline, after a real Spawn commit or provisional Wait Park and
  before the genuine final counter read. Require actual elapsed exhaustion;
  never forge Budget::finish or enlarge the normal budget. The provisional
  reply/park is discarded, new child cannot enter, Closing ends Budget.
- Platform hooks invoke sticky Executor retirement at the same fixed Reply/Park
  boundary; the actual final admission/retirement check selects Unavailable.
  They do not corrupt mappings or hardware restoration and do not pretend to
  test an unverified hardware fault. No future entry in that boot is allowed.
- Abandon hook drops one actual task guard after admission/counter snapshot but
  before settlement. The sole finalizer must use the preserved actual counters,
  retire entry and close Unavailable; no Session replacement or residual
  Dispatching state is accepted.
- Wake-install hook forces the private checked-install failure branch after the
  actual matched completion/parked value is consumed. No context store or Ready
  publication occurs; root closes Unavailable and resources are reclaimed once.
  This is a fixed error-path test, not evidence of a spontaneous hardware error.

Two additional fresh-boot constructor profiles seed only reviewed state:
capability_exhaustion fixes the persistent task-cap issuer to u32::MAX before
any Group or console preparation; slot_exhaustion fixes only child slot next
values 1..3 to u32::MAX before root entry. Their constructors internally select
canonical fixture/archive identities and expose no numeric seed, slot index,
mutable owner, setter or active-state mutation. A generic constructor or caller
image does not confer this provenance. The former must prove later task setup
remains exhausted while actual ordinary legacy hello still runs; the latter
retires each child slot through real completed WAIT, rather than a forged reap.
Exact signatures are being reconciled with the designer's Executor borrowing
API and are not approved by this prose.

The exact private signatures, locations and one-shot provenance are a required
review decision before coding these ten profiles. If a hook cannot be made sound,
revise the proposed profile explicitly; do not replace it with a success flag.

## Ordered evidence grammar

Integers are canonical decimal u64 unless a bounded lowercase enum or lowercase
hex byte string is specified. No unknown fields/records, skipped cases or noise
inside the report. All records are private fixed capture emitted after charged
work; the ordinary terminal path emits only its product output.

```text
HMTASK PREFLIGHT checks=15 irqs=8
HMTASK SETUP stage=image1|image2|root_load|root_startup|root_box|authority free=F before=B after=B published=0
HMTASK SEED kind=capability_max|child_slots_max value=4294967295
HMTASK BEGIN 50
HMTASK INFO wire=1 cpus=1 frequency=F total_ticks=T quantum_ticks=Q process_bytes=P process_pages=K image_bytes=I image_pages=J authority_bytes=A authority_pages=L
HMTASK CASE index=N pass=P name=C before=B
HMTASK GROUP group=G limit=512 metadata=M before=B after=A
HMTASK ROOT group=G slot=S generation=V launch_generation=L console_generation=C fixture=N entry=E root=R process_frames=F box_frames=K before=B after=A
HMTASK PRESSURE group=G phase=hold|release stage=load|startup|box before=B after=A held=H
HMTASK STEP group=G turn=U slot=S generation=V state=ready|cancel_pending calls=C written=W irqs=J remaining=M charged=D turns=T before=B
HMTASK CLOCK group=G turn=U begin=X remaining=M comparator=Q whole_end=E
HMTASK ENTER group=G turn=U pc=E root=R
HMTASK TRAP group=G turn=U kind=svc|irq|fault esr=E far=F elr=P spsr=S
HMTASK CALL group=G turn=U ordinal=N pc=P version=V operation=O arg1=A arg2=B arg3=C
HMTASK COPY group=G turn=U part=descriptor|input length=L hex=H
HMTASK COMMIT group=G turn=U cap_slot=C cap_generation=V child_slot=S child_generation=T program=P console_generation=H entry=E root=R process_frames=F box_frames=K before=B after=A
HMTASK OUTPUT group=G turn=U ordinal=N length=L hex=H
HMTASK SETTLE group=G turn=U kind=reply|park|terminate|failure status=S result=R child_slot=C child_generation=V cap_slot=H cap_generation=J
HMTASK INJECT group=G turn=U kind=final_budget|retire|abandon|wake_install|child_retire
HMTASK CHARGE group=G turn=U end=Y accounted=A remaining=M charged=D turns=T
HMTASK RETURN group=G turn=U event=none|svc|timer|fault terminal=none|exit|fault|budget|call_limit|turn_limit|cancelled|unavailable state=ready|waiting|closing|completed entered=E code=C calls=N written=W irqs=J elr=P remaining=M charged=D turns=T root=R vbar=V controls=1 before=B after=A
HMTASK CONSUME group=G turn=U child_slot=C child_generation=J cap_slot=H cap_generation=K outcome=O
HMTASK CLOSE group=G root_slot=S root_generation=V end=E code=C evidence=executed|administrative turn=U trigger_group=H trigger_slot=I trigger_generation=J
HMTASK ADMIN group=G reason=retired
HMTASK MANAGE group=G sequence=N woken=W reaped=R remaining_children=C closing=L charged=F
HMTASK PUBLISH group=G root_slot=S root_generation=V end=E code=C evidence=executed|administrative turn=U trigger_group=H trigger_slot=I trigger_generation=J charged=F
HMTASK GATE group=G phase=active|child|waiting|closing|closed op=O slot=S generation=V result=mode|none|pending|idle before=B after=A charged_before=C charged_after=D task_next_before=N task_next_after=M console_before=P console_after=Q
HMTASK CONTROL group=G operation=cancel_root|drop state=ready|waiting|closing before=B after=A
HMTASK REAP_ROOT group=G outcome=O before=B after=A
HMTASK RETRY_SETUP result=resources before=B after=B published=0
HMTASK LEGACY_BEGIN entry=E root=R before=B during=D frames=F
HMTASK LEGACY_END reason=exit code=0 calls=2 written=15 root=R vbar=V controls=1 before=B after=B
HMTASK DROP group=G before=B after=A
HMTASK PASS index=N pass=P name=C before=B after=B
HMTASK END 50
```

This is the proposed exact field set. Zero represents an absent child/cap/status
field only where the case/action specifies absence; it is never a wildcard.
Record forms are conditional: SEED occurs only in its fixed exhaustion
profile, once after PREFLIGHT and before any Group preparation. RETRY_SETUP and
LEGACY_BEGIN/END occur only in capability_exhaustion after root reap/drop and
before PASS. Between LEGACY_BEGIN/END permit exactly the real payload bytes
`hello from EL0\n`, from the existing pinned native hello ELF; no other raw
text or HMPROC diagnostic is accepted. Require its two real EL0 SVCs at the
independently bound WRITE/EXIT sites, correct restoration and original free
frames. This is an actual legacy run using the same nonretired Executor, not a
new instance or another boot. Include its exact ELF in the artifact inventory.

Negative profiles replace BEGIN/END 50 with 1 and have CASE pass 0/index 0 plus their
fixed name. They omit the six setup failures. COPY input is emitted only for a successful nonempty payload copy; a canonical
empty/absent input needs no hardware dereference or synthetic COPY. Child ordinary WRITE/EXIT uses the
same call/result data in SETTLE; task-attempt 64 has CALL then terminate, never
COPY/COMMIT/Park. OUTPUT appears only for nonempty successful console writes.
COPY is a successful complete copy only; partial-fault diagnostics must not
claim copied data or retain user pointers.

U is the selected task's committed ordinal, not a group-global number. All turn
records belong to the immediately preceding STEP identity. The wake-install INJECT is a post-CHARGE management fact scoped to the actual
child terminal Step, before CLOSE/MANAGE; it cannot masquerade as parent
execution. Other INJECT records occur at their declared charged dispatch/final
check boundary, before CHARGE. SETTLE is explicitly
provisional: root Budget/Unavailable after Reply/Park cannot imply successful
reply installation or Waiting. RETURN is the finalized selected-step result.
For ordinary terminal exits the exact saved terminal wins over ordinary budget
exhaustion; platform/counter failure uses the reviewed Unavailable precedence.
Use accounted 0 only for a dedicated rejected-finish scenario, with unchanged
last valid snapshot; none of the currently proposed spin-budget negatives needs
that relaxation.

A selected step serializes STEP through CHARGE/RETURN, optional CLOSE, then
exactly one MANAGE summary from its actual copied Management report. MANAGE's
strictly increasing group sequence is not a selected-task ordinal. `woken` is
0/1 for the sole root; `reaped` is 0..2, counting actual child slot reaps, not
frame releases. Host already knows exact owned child identities from COMMIT and
pending WAIT; summary counts must match that independently reconstructed state.
Immediate completed WAIT emits CONSUME during charged dispatch before SETTLE
and CHARGE; deferred wake consumes its pending identity in post-finish MANAGE.
No synthetic child/parent turn, CLOCK or ERET is introduced by either operation.

CLOSE records the immutable first RootClosure. Executed closure references the
actual matching root terminal RETURN by U; its trigger fields are 0. Administrative
closure has U=0 and contains no invented root PC/register/counter evidence. A
child-triggered retirement uses that actual child TaskId as trigger, after its
terminal step and before wake; preselection retirement emits ADMIN then CLOSE
with all trigger fields 0, then MANAGE. Root TaskInfo identity/charge remains
bound to ROOT and its prior state. Retired cleanup cannot enter any user task.
A root already Closing retains its original End even if a child later fails.

Root terminal always reaches Closing first. At most two zero-entry child
cancellation Steps follow, with any already-held children reaped by bounded
management. A distinct later Group::turn emits MANAGE then PUBLISH, without
STEP, using the immutable saved RootClosure. Even a no-child root must publish
separately. Repeated publication is forbidden; subsequent turn is Idle while
completion is held. Existing actual reports may describe post-management total
charge; the host combines child terminal releases plus any administrative root
release before comparing RETURN.after, rather than inventing an intermediate
free count that the API does not expose. Management never changes saved Step
Timing/counters, nor charges parked intervals.

Authority/catalog metadata persists at its exact charge through publication.
REAP_ROOT must release exactly that charge and enter permanent TaskClosed. DROP
reclaims inactive owners directly and cannot manufacture completion or wake.
The final protocol review must align copied Management accessors and the private
CONSUME/CLOSE facts with the final lifecycle signatures; no public owner escape
or arbitrary observer callback is implied.

Every ENTER binds to actual QEMU GICC setup 3, EL1→EL0 ERET exact saved PC,
lower-EL trap at process vector+0x400 or IRQ+0x480, optional ACK 30/EOI 30, then
restore 4. IRQ ESR is retained state, not a fabricated timer syndrome. SVC and
fault syndromes, FAR and relevant fixture sites are exact. Globally monotonic
real timestamps and per-owner budget conservation remain mandatory. Sleeping
parent and management produce no user trace events or fresh calls. No child
entry is allowed before its actual parent SPAWN commit, after serviced cancel,
after parent final terminal selected in the same committing turn, or after
Executor retirement. Counterpart metadata cannot replace missing hardware.

After END, require the full current 41-case baseline and exact existing EL1
exception/fixup/timer inventory, then sole EL1 semihost exit. Explicit
userspace=off; no PSCI, unexpected CPU, spurious event, unknown vector or extra
entry accepted. First full pinned traces require independent review before the
final parser gate can be enabled. Preserve failures and bind kernel, archive,
all ELF/source hashes, actual type-layout witness and linked-stack review.

## Ordinary supervisor acceptance

`run-task bin/supervisor` enables the actual opt-in authority path. Freeze the
normal archive additions with its owner: supervisor `bin/supervisor`, worker `bin/worker`, and existing `demo/loop`
catalog member. Program 1 maps to bin/worker and program 2 to demo/loop in both normal and
dedicated test archives. There is no catalog-path constructor variant; no
user-provided path or program descriptor changes that binding. The dedicated
loop ELF may be independently pinned, but the immutable path is identical. The supervisor performs row 0's exact nine-call script with state-correct immediate-or-park WAIT and
checks actual results before printing `supervisor complete\n`. Worker emits
its copied job `worker complete\n` and exits 55; loop produces no bytes. The
monitor prints the root outcome only after Closing/root reap/catalog teardown.
The root completion is Exited 0; no child outcome is relabelled as the root's.

Run at 64/128 MiB after one diagnostic warmup and include a fresh 128 MiB
supervisor-first profile without warmup. Check free frames equal the appropriate
initial baseline, actual monotonic console/task generations, return to prompt,
then ordinary legacy hello. Bad path/missing/malformed authority setup must
return the documented error with no child or root ERET and exact recovery.
The product remains a native fixed-catalog experiment, not a shell, fork/exec,
general capability transfer or IPC service. Exact status/help/product lines
remain owned by the monitor adapter and must be frozen before its harness.

## Decisions still blocking implementation

1. Approve fixed negative hook signatures/provenance and failure classification.
2. Confirm lifecycle report exposes enough copied facts for ordered MANAGE,
   provisional settlement and immediate charged WAIT reap without retaining owners.
3. Freeze worker job mode bytes, large-BSS archive geometry and normal monitor
   strings with the fixture/adapter authors. No decoder may infer them from logs.
4. Confirm all ordinary monitor strings with the adapter owner. Held-capacity
   no longer depends on a finite-worker yield bound: two loop cancellations
   establish exact held ownership before the rejected third SPAWN.
