# Resumable process runtime: hardware verification proposal

Status: draft, 2026-10-26. No runtime API, quantum policy or wire grammar is
approved by this document. Freeze those decisions against the main 0028 design
and independent review before implementation. These tests must exercise actual
EL0 execution; safe-policy unit tests and printed scheduling counters cannot
substitute for the hardware observations below.

Initial proposed profiles are CPU0 only, one CPU, 64/128 MiB on pinned QEMU.
Existing 66-run process, 24-run startup-input, ordinary launcher and x86 gates
remain separate requirements. No SMP scheduling, Linux compatibility, service
workload or performance improvement follows from this bounded test profile.

## Alignment with the current main proposal

The [main 0028 draft](0028-native-process-runtime.md) now proposes four occupied
slots, a 1..512-frame limit including actual rounded KernelBox storage, immediate
frame reclamation at terminal completion, and completion-slot retention until
reap. It proposes `Group::admit/turn/request_cancel/completion/reap`, token identity
from executor-issued group namespace plus slot generation, and a 100 ms active-turn
budget with a 1 ms quantum. These are proposed values to bind during review,
not assumptions authorizing implementation. The matrix below should use those
semantics if approved, without inventing different test-only ownership rules.

Under that proposal, a cancellation request retains pages and marks pending;
the next selected turn must publish Cancelled and reclaim without ERET. A
completed slot immediately contributes zero process/box frame charge but remains
occupied until reap. Reap changes slot availability, not the frame count. Since
a Group exclusively borrows Executor, obtain a foreign token from an earlier
group, drop that group, then use the saved Copy token against a new group; do not
invent concurrently borrowed executors to test the namespace boundary.

Budget records should expose actual bracket-counter reads and before/after
snapshots. Independently recompute each saturating charge and conservation across
turns; require nondecreasing timestamps and correct whole-budget/quantum comparator
bounds. Exact safe-policy arithmetic remains separately tested. QEMU exception
traces prove interruption/resumption, not the authenticity of every counter
read or precise EL0 CPU time; retain that distinction in evidence claims.
A busy process must reach the documented cumulative Budget terminal outcome,
without another ERET, even when its peer makes progress between quanta.

The box charge must come from actual compiled Process size/allocator rounding,
not a mirrored Python struct layout or only a guest-printed value. Freeze how
compiler evidence supplies that scalar before final resource expectations.

## Invariants and evidence shared by every case

Use independently built native fixtures in a deterministic archive, with source
hashes, actual ELF symbols/segments/instructions, GNU listing/stdout extraction,
unique complete read-only kernel embedding and no standalone ELF fallback.
Prefer kernel-independent fixtures; no kernel-BSS target is needed here.

Bind every process to an immutable test identity, actual issued generation,
private root, fixture and owned-frame count. This identity must survive multiple
steps. Current run diagnostics increment an index per execution; a resumable
implementation must explicitly define stable process identity and step ordinal
instead of silently interpreting the old index as both. Group control tokens
and native console grants are different authorities and need separate tests.

For each admitted step, correlate its serial envelope with exactly one defined
hardware slice or another explicitly frozen bounded operation. Require the
actual source-EL0 exception, expected vector/PC/syndrome and exact GICC
setup/acknowledgment/EOI/restoration sequence. Before the next process runs, the
boot root, vector base, control state and timer/GIC ownership must be restored.
Do not accept arbitrary repetitions of otherwise allowed register writes.

The first ERET for a process targets its ELF entry. Every subsequent ERET must
target that process's previous saved ELR: the instruction after a completed SVC,
or the interrupted PC after a timer IRQ. A resumed process must not restart at
entry, inherit the peer's PC, or resume after terminal completion/cancellation.
Preserve reply semantics for x0/x1 after SVC; an IRQ must preserve these registers
too, since it is not a native call return. Check the terminal PC and exception
against independent ELF symbols, never only a printed reason.

Record cumulative native calls, bytes and timer interrupts per process, with
monotonic step ordinals. A yielded process must retain its Session, context and
owned pages. Reports must distinguish per-step deltas from cumulative totals.
An error path must not silently erase earlier budget consumption or retire a
process without an observable ownership outcome.

Account for all simultaneous owners: while A/B remain live, free frames equal
the common baseline minus their independently derived frame counts and any
explicit group-owned allocations. Record the exact documented reclamation
transition. Slot occupancy and physical ownership are different quantities:
a completed report may still occupy a slot even if process pages were already
reclaimed. The design must freeze whether pages die at completion or reap;
tests cannot assume either model. Repeated cancellation/reap must never free
pages twice. Repeat scenarios with fresh processes and distinct generations.

Reuse the reviewed TraceReader timer/GIC/fixup-table decoder. It validates known
EL1 preflight/baseline activity against actual kernel fixup bytes and exact fault
classes. The existing fixed-run validators cannot be reused unchanged: they
assume a terminal timer IRQ and one initial entry followed only by SVC resumes.
The new parser must reject unknown transitions, extra steps, missing cases,
unexpected CPU events and a final successful serial report over failed execution.
Freeze preflight placement and exact baseline counts with the runtime API.

## Proposed case inventory

Names below identify obligations, not a frozen wire inventory. They may share a
bounded scenario when every independent obligation remains observable.

| Scenario | Required hardware/ownership observation |
| --- | --- |
| Two live contexts | Alternate actual SVC yields between distinct seeded A/B processes; both later validate their own state and emit distinct exact bytes. |
| Busy loop and peer progress | A takes at least two real lower-EL timer IRQs and is resumed; B completes actual WRITE/EXIT calls while A remains live. No cooperative yield from A substitutes for preemption. |
| Context across IRQs | Deterministic release-word fixture below proves actual IRQs occurred after sentinel installation and before EL0 validation, with peer execution between captures. |
| Persistent byte budget | One process writes sixteen 256-byte blocks across interleaved steps, then attempts one extra byte, a zero write, and exit: exact 4096-byte total, Budget response for the extra byte, and no reset at a step boundary. |
| Persistent call budget | A process reaches exactly 64 attempted calls across interleaving; errors still count. Require the frozen terminal CallLimit behavior and no 65th call or ERET after retirement. |
| Exit/fault isolation | One process exits or faults while its peer remains owned and later completes; no peer root, context, Session or resource balance changes. |
| Cancel before entry | Cancel an admitted inactive process; reclaim exactly its documented owners, no EL0 entry or native output, peer still progresses. |
| Cancel after SVC | Cancel while quiescent after a real SVC yield; no subsequent ERET for that process, no duplicate reply/output, exact reclamation. |
| Cancel after IRQ | Cancel after a real timer yield; timer is quiescent, no deferred interrupt or resumed PC leaks into the next process, exact reclamation. |
| Drop live group | Drop an inactive group containing live/suspended children; all owned resources obey the documented drop policy, no later user transition, subsequent fresh group works. |
| Held completion capacity | Hold completed results, attempt capacity overflow, then reap one and reuse only that slot with a new generation. Distinguish frame reclamation from slot availability. |
| Stale/wrong-owner token | A token from another group or an earlier occupant fails before hardware entry/allocation; the intended owner remains runnable and its release word is unchanged. |
| Foreign console grant | Real EL0 A submits B's native console token (and, separately, a retired grant); require Handle rejection, zero bytes and no peer authority. This is separate from group-token validation. |

Use the existing native limits, not larger budgets introduced to simplify tests.
For example, the byte-budget probe has 19 calls: sixteen full writes, rejected
one-byte write, valid zero write, exit. A peer's independent Session must retain
its own untouched budget. Expected output is full byte comparison, not checksum
or kernel-reported totals alone. Fixtures must check native replies in EL0 and
exit with a distinct failure code on any mismatch.

Negative control operations must have no associated EL0/GIC/PSCI event, subject
to any explicitly reviewed admission check the final API requires. Invalid input
must preserve the owned process or return it according to the frozen API; a host
error enum alone does not prove that ownership survived. After invalid operations,
resume the legitimate process and verify its real output and context.

## Deterministic context-preemption fixture

A fixed instruction count is not an acceptable way to guarantee an IRQ before
validation: host speed and QEMU translation can move the boundary. EL0 architectural
counter access remains disabled. Do not add a counter-access exception merely to
make this test deterministic.

Propose a dedicated release word in a canonical fixture's private RW page. EL0
initializes its data/sentinels and loops using LDR plus CBZ/CBNZ on that word.
The word starts at zero. Require at least two actual timer IRQs inside the named
poll interval, with the peer making real progress. Only then a narrowly scoped,
test-only inactive operation writes the fixed four-byte value 1 to that exact
word. Resume at the preceding saved ELR. EL0 then captures and validates its
state before emitting success output and exiting. Bound the number of permitted
steps and overall host time independently; no probabilistic loop duration is
part of the proof.

This release operation is additional test authority and requires explicit design
and source review. It is not assumed to exist. Required constraints:

- Compile only in the dedicated test profile, private to core test code.
- Accept no arbitrary address, length, data, context pointer or raw frame owner.
- Bind to the canonical admitted fixture and independently checked release-word
  symbol/VA and RW permissions; validate boot context and inactive ownership.
- Write only the fixed value to owned payload storage. Do not modify saved PC,
  SP, registers, Session, mappings, startup bytes or code to manufacture success.
- If the group owns processes, require the genuine owning token; stale or foreign
  tokens cannot release a peer. Verify zero before release and no unintended
  resource change. A token rejection must leave the word zero.
- Prefer an existing sound owned-byte copy primitive. Any added unsafe boundary
  needs its own documented invariant and independent review.

The polling scratch register is overwritten by each load and therefore is not
proved preserved merely by this loop. Use complementary fixtures (for example,
A polls with x17 and B with x18), excluding only each fixture's scratch from its
static sentinel set and checking the final scratch value is 1. Across the pair,
every GPR receives a real preservation check. The release-address register has
a known fixed expected value. Require sentinel setup to finish before the named
poll interval; source/disassembly and observed interrupt PCs establish this.

Set different per-process values for GPRs, all 32 128-bit SIMD registers, FPCR,
FPSR, TPIDR_EL0 and NZCV. Use non-flag-changing poll instructions so the expected
NZCV survives until capture. Validate SP and resumed control flow too. Preserve
the existing sanitized behavior for TPIDRRO_EL0, DIT and SSBS: their expected
resume value is zero, not their hostile user-written value. Fresh processes must
also check the established zero-state rules before installing sentinels.

Both processes use the same virtual addresses for private mutable data, with
different seed/counter values. After B overwrites its own data, A must still read
A's exact bytes and vice versa. Real validated output from both processes is
required; printed distinct root addresses alone do not establish isolation.

## Decisions required before wire/API freeze

The main design must resolve these rather than leave the host harness to infer
answers from a successful candidate:

1. Step ownership/error signatures and the exact maximum work/number of traps
   per call; whether every recoverable SVC yields immediately.
2. Timer quantum definition, frequency conversion, first-arm point and cumulative
   termination budget. State whether inactive peer time counts; repeated steps
   must not accidentally reset a hard lifetime bound.
3. Stable process identity and generation allocation, group owner identity,
   token reuse/exhaustion, and completion-slot semantics.
4. The point of frame reclamation, cancellation outcomes, drop behavior and
   handling of an error while a process remains owned.
5. Deterministic release-hook authority, fixture binding and permitted copies.
6. Exact bounded scenario inventory, step scheduling order, serial field sets,
   call/output correlation and permitted dynamic IRQ counts. Dynamic counts must
   have explicit bounds and exact event correlation, not an open-ended allowlist.
7. Existing synchronous run compatibility: old launchers and test profiles retain
   their timeout, console, teardown and private-output contracts.

Once frozen, add host parser mutations for every missing/duplicate/reordered
step, stale identity, wrong resume PC, wrong output/context proof, budget reset,
IRQ replaced by SVC, omitted GIC restoration, early/double reclamation and lost
completion. Compile actual APIs to establish affine ownership and lifetime
boundaries. Reinspect full compiled stacks, including retained group/process
storage and every affected old profile, before any guest run. First pinned traces
need independent review; preserve failed candidates and qualify all evidence as
local until the corresponding CI gate actually runs.
