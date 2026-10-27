# 0030: task failure-profile verification

Status: **draft pending hook/API review**. This is a proposed acceptance contract,
not execution evidence or permission to implement core hooks. The ordinary
[0029 matrix](0029-task-protocol.md) remains unchanged and independently required.
Only new userspace fixture/builder source is authorized separately; generated
artifacts, core hooks and guests wait for their respective review/build gates.

## Fixed artifacts

One dedicated `task-negative-initramfs.cpio` has exactly these 15 entries, in
order. No ordinary archive changes, runtime-loaded executable or host-selected
program path is involved:

```text
bin
bin/final_budget_spawn
bin/final_budget_park
bin/platform_reply
bin/platform_park
bin/abandon_dispatch
bin/wake_install_failure
bin/capability_exhaustion
bin/slot_exhaustion
bin/child_retire_ready
bin/child_retire_waiting
bin/worker
bin/legacy_hello
demo
demo/loop
```

Program IDs remain 1=`bin/worker`, 2=`demo/loop`. The dedicated worker uses the
reviewed ordinary finite protocol: four checked zero WRITEs, opaque copied-job
WRITE, EXIT 55; its exact job is `worker complete\n`. The loop clears NZCV and
branches to itself. Legacy hello is the independently pinned existing ordinary
hello ELF, copied byte for byte, not rebuilt through an alternative API.

New source files are `task_negative_cases.rs`, `.S`, `.ld`, with
`tools/build_task_negative_cases.py`; a `task-negative-cases/manifest.json`
records all 13 executable files. Build metadata includes every actual source
input (including the safe startup/task codecs and the reused hello source and
artifact), pinned command, stdout/stderr, and compiler-derived layout witness.
Canonical updates require an explicit builder switch and fresh evidence folder.
No generic mutable seed, input-selected hook or arbitrary memory target appears
in these untrusted fixture sources.

Reuse the independent ELF/CPIO inspectors: bounded static ARM ELF, exact entry,
RX/R/RW permissions, no kernel-address symbols, actual SVC0 sites and register
wire setup, self-branch loop, exact literal bytes, canonical GNU list and
stdout-only extraction. Check the complete archive appears exactly once in a
read-only kernel LOAD, with no dedicated raw ELF outside it. If ordinary apps
also embed the identical legacy hello, authorize only that independently verified
ordinary archive's exact member span; do not weaken the scan to allow arbitrary
duplicates. Independently inspect compiled Rust call/reply-check ordering.

Mandatory symbols: native_start; spawn_svc, wait_svc, cancel_svc, write_svc,
exit_svc; worker_yield_1..4 and worker_copy_begin; loop_start/end. Parent fallback
EXIT 220 is a failure sentinel, not an accepted outcome. All reply-producing
operations check both x0 and x1 before progressing. EXHAUSTION success messages
are unnecessary: exact checked EXIT 0 plus literal request/result evidence is
sufficient.

## Ten fresh-boot scripts

One CPU, 128 MiB, one profile per boot. Each reports BEGIN 1, CASE index/pass 0
with its exact name, one expected root outcome, exact teardown, PASS/END 1,
then the existing full 41-case baseline and privileged exit 0. No six ordinary
setup-pressure failures run here. Enforce 60 seconds per boot, 32 MiB serial and
16 MiB QEMU trace, at most 1024 selected task steps and 256 accounted turns per
owner. Unexpected earlier Budget, failure, panic, missing hook, timeout or
semihost exit is failure, not an alternative negative-test success.

| Profile | Exact root requests | Hook and required observed outcome |
| --- | --- | --- |
| final_budget_spawn | SPAWN worker on call 1; no successful reply or subsequent root entry. | After actual COMMIT, spin to the real whole-turn deadline. Executed root Budget; child receives no ERET. |
| final_budget_park | SPAWN loop, blocking WAIT on call 2. | After provisional park, spin to real whole-turn deadline. Executed Budget; no finalized Waiting, consumption or wake. |
| platform_reply | SPAWN worker on call 1. | Retire at the fixed provisional reply boundary after COMMIT. Executed Unavailable; no reply installation or child entry. |
| platform_park | SPAWN loop, blocking WAIT on call 2. | Retire at provisional park. Executed Unavailable; no finalized Waiting/wake. |
| abandon_dispatch | SPAWN loop, SPAWN worker on call 2. | Drop that actual admitted guard before copy/reservation/effect. Root calls=2 remains; only first child exists; executed Unavailable. |
| wake_install_failure | SPAWN worker, blocking WAIT on call 2; no root resume. | Require actual finalized Waiting before that child's terminal step. At checked installation of the matched wake, fixed rejection produces administrative root Unavailable triggered by the real child; no invented root timing/return. See timing decision below. |
| capability_exhaustion | SPAWN worker→Resources 9/result 0, EXIT 0: 2 calls. | Fixed MAX seed before any task preparation. Grant consumes MAX. No child commit or console preparation. A later task setup remains Resources, while the same Executor runs legacy hello successfully. |
| slot_exhaustion | SPAWN/WAIT worker three times; fourth SPAWN→Resources 9/result 0; EXIT 0: 8 calls. | Child slots 1..3 start at MAX only. Each consuming WAIT retires its slot; all three concrete slot identities must appear. Final rejection creates no owner/cap. |
| child_retire_ready | SPAWN loop, then at most 32 checked zero WRITEs. | First eligible real child timer in its named loop interval while root is Ready retires entry. Child Unavailable; administrative root Unavailable with that child as trigger. Exact root call count derives from the observed first eligible boundary, not a guessed constant. |
| child_retire_waiting | SPAWN loop, blocking WAIT on call 2. | First eligible child timer in the named loop interval after finalized root Waiting retires entry. Administrative root Unavailable precedes wake; no parked-session resume. |

The Ready fixture exits 220 if its bounded write sequence completes without the
required hook. Every non-exhaustion root exits 220 if an operation that must not
resume unexpectedly returns. Slot-exhaustion waits check packed Exited 55 and
capabilities differ; successful child cap generations must increase even though
all three slot generations are MAX. The parser checks issuer/slot independence.

**Frozen scheduling condition:** retain the specified finite worker. Four worker
yields cannot universally force WAIT to park because parent timer IRQs permit
extra child progress. The profile requires actual finalized Waiting before the
worker completes; an unreached hook is failure. No Budget-child substitution or
retry-until-pass is permitted. Later scheduling evidence may motivate a separately
reviewed design refinement.

## Independent negative automata

Do not add permissive cases to ordinary `Model`, relax its ban on administrative
closure, or turn off ordinary counter/authority checks. Reuse only immutable
field decoding, artifact validation, bounded process launch and exact QEMU
transaction readers. A separate negative parser has a fixed scenario table and
an explicit allowed transition automaton for each profile.

Keep the ordinary STEP identity, globally monotonic hardware counters, per-owner
active-budget conservation, exact COPY/COMMIT/OUTPUT/CONSUME ordering, frame
charges, cap ownership and round-robin rules. It must reconstruct every child
from actual SPAWN, not accept metadata announcing a preexisting child. Setup
failures or extra groups are forbidden except the named capability retry and
its subsequent legacy process. Exact actual descriptor/input bytes remain bound
to the fixture; the final admitted task call still increments the single Session
call counter. Hooks cannot create synthetic calls or replace the Session.

For each non-exhaustion profile require exactly one provenance-bound INJECT. Its
owner is the selected STEP identity, even when the root closes administratively
from a child event. Trigger fields must bind the first eligible boundary:

- Reply: root's first SPAWN has committed its worker; provisional SETTLE reply
  exists, but no CHARGE/finalized root return yet.
- Park: root call 2 targets its committed loop; provisional SETTLE park exists,
  but no Waiting installation yet.
- Abandon: actual root call 2 was admitted; no descriptor COPY, task-cap burn,
  COMMIT or provisional Park/Reply may follow it.
- Wake failure: actual matched child completion and retained PendingWait, after
  the child's CHARGE, before any root Ready publication. The child's own immutable
  completion is preserved; the administrative closure carries no root CLOCK,
  ENTER, CALL or fabricated selected ordinal.
- Child retirement: real lower-EL IRQ, exact committed program-2 child identity,
  named loop PC and required root state. Earlier setup IRQs or Ready IRQs in the
  Waiting profile are recorded but ineligible. A second eligible IRQ before the
  hook, or a later arbitrary injection ordinal, fails.

Final-budget spin must yield actual CHARGE.end >= CLOCK.whole_end, remaining 0
and conserved capped charge. Ordinary terminal precedence and provisional versus
final settlement remain distinct. Retirement is not a fabricated hardware fault:
it is an explicitly injected private state transition with Unavailable outcome.
After retirement, consume only bounded no-entry cancellation/management/publication
and teardown; **no further EL0 ERET anywhere in that boot**. The 41-case baseline
has only its separately validated existing EL1 exceptions. Fatal restoration loss
is not accepted as reclaimed ownership.

Root Closing always precedes publication. Preserve the first immutable root
End and evidence; release root Process frames at closure, child Process frames
at their actual terminal steps, and retain authority/catalog metadata until root
reap. No-child roots still require a separate publication call. Administrative
closure must name the actual triggering child and use no invented root Step.
Every profile must return to the same initial free-frame baseline.

Seed profiles have one exact SEED before preparation and no INJECT. MAX is the
literal 4294967295; a scalar in the log alone is insufficient. The private
constructor must select canonical identity internally, and actual root grant,
child slot generation, issued capability and subsequent rejection independently
establish that the seed reached the real checked state. No caller-supplied seed,
slot list, executable or active mutation is accepted.

Capability profile expectations on a fresh Executor: root task cap MAX/low 2,
root console generation 1; SPAWN fails without a child console burn. Retry after
root reap does not issue another task cap or console generation. Legacy hello
then uses console generation 2, has exactly WRITE 15 bytes and EXIT 0, and fully
reclaims its frames. It is the **same nonretired Executor**, not a reset issuer.
For slot profile: root slot 0 generation 1, children successively slots 1,2,3
generation MAX; capability generations 2,3,4, console generations 2,3,4, and
root grant/console generation 1. Fourth SPAWN fails before cap/console issuance.

## Trace and evidence boundary

Each ENTER must match the existing three GICC setup writes, exact saved-PC
EL1→EL0 return, one lower-EL trap at the process vector, timer ACK/EOI 30 when
applicable, and four restoration writes. Trap ESR/FAR match the real exception;
IRQ ESR is retained state. No extra CPU, PSCI, spurious interrupt, user semihost
or unknown vector is accepted. `userspace=off` is explicit. Every post-retirement
EL0 transition fails independently of serial claims. Eight EL1 timer-preflight
IRQs and the full baseline's registered fixup/timer inventory stay exact.

Archive/kernel/layout/source hashes, input command, RAM, deadlines, raw serial,
raw exceptions, stderr and exit status are retained per profile. Actual parser
gate remains false until independent inspection of all ten pinned traces. A
passing subset or a synthetic state model does not satisfy this milestone.
Synthetic positives must cover all ten automata plus missing/duplicate/reordered
hooks, forged seed/issuer states, premature wake/publication, copied-byte errors,
post-retirement ERET, and unchanged-looking metadata with missing hardware.

## Remaining integration decisions

1. Freeze private hook/constructor signatures and exact insertion points with
   the core designer/reviewer; this document grants no core editing authority.
2. Wake-install retains the finite worker and observed-park requirement above.
3. Existing Trace-mode Executor legacy `run` emits HMPROC, not raw terminal-only
   output. Use a strictly nested existing HMPROC hello transcript between
   HMTASK LEGACY_BEGIN/END, independently matched to the same two EL0 transitions.
   Do not silently change output mode or create a second Executor just to fit the
   old draft's raw-only text. Freeze exact nested fields with the implementation.
4. Freeze retired-state admission rejection evidence, if required beyond the
   already mandatory absence of post-retirement entry; no fake successful run.
