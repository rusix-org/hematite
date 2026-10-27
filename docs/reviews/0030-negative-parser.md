# 0030 negative parser independent source review

Status: **source approved after the boot-root alias correction; actual trace
acceptance remains pending**. Read-only inspection of
`tools/test_task_negative.py`, its nine synthetic tests, the reused ordinary
ownership model/trace reader, artifact verifier, approved 0029/0030 contracts and
actual core/driver serialization. No tests, builds or guests were run for this
review. Parent-reported passing synthetic tests are not raw hardware evidence.

## Resolved finding: boot-root alias accepted as an owned user root

`NegativeModel.begin` (lines 100–106) delegates root admission to inherited
`ordinary.Model.new_task`. That method checks alignment, RAM range and uniqueness
among live user roots, but does not exclude `symbols['arm_root']`. Inherited child
COMMIT admission has the same gap. `legacy` (lines 275–277) likewise accepts the
boot root. Thus consistently changing a ROOT/COMMIT physical root and its ENTER
records to the known boot root satisfies these ownership checks (provided it does
not duplicate another live user root).

The raw trace correlator does not independently observe TTBR values: it matches
ERET PCs, EL0 exception sources/ELRs, vectors and exact GIC transactions. Its
presence does not close this serial-field gap. This is an acceptance weakness,
not evidence that the implementation installs the boot root for EL0.

Require negative-specific root validation excluding the known boot root for all
admissions and legacy recovery. Keep live-owner uniqueness and alignment/range
checks. Add coordinated-field mutations for supervisor, child and legacy roots;
change every affected ENTER consistently so failure demonstrates forbidden root
identity rather than merely inconsistent text. The harness author added the negative admission override and legacy rejection,
plus coordinated supervisor/child/legacy mutations. Parent also added the same
single exclusion to ordinary Model.new_task and a coordinated supervisor/child
mutation in its existing tests. These small shared changes exclude impossible
ownership without changing valid ordinary behavior. Source reinspection confirms
the finding is resolved; parent owns execution of the updated test inventory.

## Reviewed behavior that matches the contract

- The ten closed fixture scripts bind SVC site, version, arguments, copied
  descriptor/input bytes, actual capability/slot generations and output. Resource
  arithmetic includes boxed process and authority/catalog metadata from the
  separately verified compiler witness.
- First eligible hook selection is reconstructed from actual prior commits,
  lower-EL trap kind/loop interval and parent Ready/Waiting state. Missing or late
  INJECT cannot be replaced by an arbitrary terminal record. Poll threshold is
  checked against the actual recorded whole-turn deadline and charge arithmetic;
  rejected clock accounting cannot pass.
- Provisional Reply/Park never survives injected terminal selection. Retirement
  prevents later ERET while allowing no-entry cancellation/reclamation. Worker
  wake failure requires actual parked WAIT, actual worker Exit55 and exact
  child-triggered administrative root closure; no loop/Budget substitution occurs.
- Seed timing, MAX-to-zero task issuer behavior, three separate child-slot MAX
  retirements, held completion/reap and full physical recovery are reconstructed.
  Capability exhaustion requires the same console-generation continuation and
  exact nested HMPROC hello records; HMPROC elsewhere is rejected.
- Emission order matches source: non-wake INJECT follows SETTLE before CHARGE;
  wake INJECT follows RETURN before CLOSE. Separate publication preserves the
  immutable closure and requires no remaining child capabilities.
- Shared raw trace validation consumes every slice, preflight, baseline and final
  privileged exit, rejecting extra/missing transitions. Synthetic legacy slices
  are expectations derived from independently inspected ELF sites, not claimed
  observations. Actual traces must supply their two EL0 transitions.
- Artifact admission checks the negative-feature compiler commands, transitive
  hook source hashes, actual layout bytes, canonical archive/ELF identity and
  unique read-only embedding. The deliberate shared legacy member exception is
  limited to independently verified archive membership.

## Remaining evidence gates

Keep `TRACE_GRAMMAR_REVIEWED = False` until independent review of all ten fresh
raw profiles. Nine current unit tests exercise serial transitions and mutations;
they do not call the hardware correlator. Add focused trace mutations for a
missing/extra post-retirement EL0 transition and either missing/swapped legacy
SVC before claiming negative-specific correlation regression coverage. Existing
shared trace tests remain useful, but do not replace these scenario bindings.

A first-eligible child-hook mutation (delay the hook across an otherwise valid
eligible timer transaction) would strengthen the new serial automaton tests.
Source inspection finds the current immediate `eligible`/`injection` pairing
rejects it; this is a coverage recommendation, not a second source blocker.

Acceptance remains pending required compiled bounds,
artifact/source binding, actual raw review and strict complete regression gates.
No completed OS phase or spontaneous hardware-failure coverage is established.

The current physical range check remains the broad identity RAM window. Actual
ARM Plan construction also excludes the prefix through `__kernel_end`; a future
artifact-bound lower-bound check could reject other known kernel storage claims.
Neither serial fields nor the current QEMU exception log alone prove arbitrary
physical allocation ownership. Do not describe this review as independent TTBR
or complete allocator provenance tracing.
