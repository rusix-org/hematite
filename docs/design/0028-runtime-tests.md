# Resumable process runtime: hardware verification proposal

Status: Stage B revision1 below approved by independent review, 2026-10-26.
Earlier exploratory notes are superseded by that concrete freeze and the main
0028 mechanism contract. Source, compiled-stack and actual execution gates
remain required before a verification claim. These tests must exercise actual
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
test-only inactive operation writes the fixed eight-byte value 1 to that exact
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

## Stage B implementation freeze, revision 1

This section supersedes the open-ended scenario list above and is approved
for implementation by independent [review0028](../reviews/0028-native-process-runtime.md). The main
0028 API/terminal-precedence specification owns runtime semantics. This section
owns fixture expectations, test order, serial grammar and host acceptance.

### Profiles and fixed inventory

Feature `runtime-tests` runs CPU0/one CPU at 64 and 128 MiB. Run two passes of
these twelve cases, in this order: `svc_context`, `irq_context`, `busy_peer`,
`byte_budget`, `call_limit`, `fault_peer`, `cancel_new`, `cancel_reply`,
`cancel_quantum`, `completion_tokens`, `admission_rollback`, `drop_live`.
Exactly 24 case results are required. A case begins/ends with the same common
Pool baseline and no live Group. One successful eight-IRQ timer preflight occurs
before the cases. The existing 41-case baseline and its exact sixteen EL1 timer
IRQs follow all cases, then the sole privileged semihost exit. No PSCI events
or unexpected spurious IRQs are accepted in these pinned profiles.

Every process has the main design's 256 accounted-turn bound. Bound each case
at 1024 selected turns and each pass at 4096 selected turns; reaching a harness
bound is failure, not a substitute completion. Bound each guest at 60 seconds
and each raw artifact at 32 MiB. Ordinary success/call-limit/fault fixtures must
produce their prescribed terminal result; a Budget/TurnLimit result cannot stand
in for it. Only `busy_peer` deliberately requires Budget. Dynamic timer events
are accepted only as fully correlated steps, with persistent charges and the
specified lower bounds; no wildcard exception inventory is introduced.

| Case | Exact admissions and required actions |
| --- | --- |
| `svc_context` | Canonical context A/B in slots0/1; each performs one zero WRITE with sentinel registers, then captures/validates the resumed state in EL0, emits its complete capture, and exits0. Alternate actual turns; require both real processes complete and private same-VA data checks pass. |
| `irq_context` | Canonical polling A/B in slots0/1. For each require two timer IRQs whose interrupted PC is in its poll interval, and at least one peer turn between its first poll IRQ and release. Release each once when eligible; both capture/validate full state, emit it and exit0. |
| `busy_peer` | Slot0 infinite loop, slot1 hello. Loop has at least two Quantum returns followed by actual saved-PC resumes; hello writes/exits0 while loop remains live; loop eventually completes Budget. |
| `byte_budget` | Slot0 byte-budget probe; slot1 grant probe supplied slot0's actual console grant. Slot0 executes exactly19 calls and writes4096 bytes; slot1's foreign-token WRITE returns Handle with no bytes, then its own marker WRITE and exit0 succeed. |
| `call_limit` | Slot0 performs64 zero WRITE attempts; slot1 hello exits0. Slot0 completes CallLimit on attempt64, with no next ERET or65th call; counters persist across peer turns. |
| `fault_peer` | Slot0 writes its own RX entry and faults; slot1 hello still writes/exits0. Exact fault PC/FAR/ESR bind to ELF. |
| `cancel_new` | Admit loop with console=false (console_generation=0) and hello with console=true. Mark loop cancelled twice before any turn; both requests succeed without freeing frames. Its selected turn completes Cancelled without entry; hello exits0. |
| `cancel_reply` | Admit SVC-yield probe and hello. After the probe's first actual successful zero WRITE, request cancellation twice. Pending frames remain charged until its next selected turn; no second entry for that probe. Hello exits0. |
| `cancel_quantum` | Admit loop and hello. After loop's first actual timer Quantum, request cancellation twice. Next selected turn completes Cancelled without re-entry; hello exits0. |
| `completion_tokens` | Fill four slots with hello, keep all completions unreaped, and verify Full for another admit despite full frame recovery. Query each completion twice, then reap slot0 and admit a grant probe there with the retired slot0 console token. It must reject that grant then use its own. Reject old slot0 TaskId for cancel/completion/reap, and reject reap of the live replacement as Pending. Reap all final completions. Save a TaskId across group drop; a new group's valid child remains runnable after foreign-token cancel/completion/reap/release requests are rejected. |
| `admission_rollback` | Frame limit one below the independently calculated hello charge rejects before issuance. With cap512 and n=hello ELFpages+28, retain actual Pool owners leaving respectively0, n, n+1 free; ordinary admit must fail in load, startup allocation, and box allocation respectively. Drop each pressure set and verify all temporary/group owners reclaimed. Do not add a Pool.free whole-charge precheck: only the declared group cap is checked before allocation. The latter two failures burn one console generation each; load/cap failures burn none. Finally admit/run hello and require its actual grant generation reflects exactly those two burned prepares. No simulated allocator failure seam. |
| `drop_live` | Admit polling A/B, run until each has at least one actual poll IRQ, then drop the inactive group without release. Reclaim every process/box frame, preserve boot controls, and observe no later ERET for either child. A fresh hello group completes normally. |

Release refusal tests belong to `irq_context` and `completion_tokens`: an ordinary
admission of the very same context ELF has no private provenance and release must
fail; stale/foreign tokens, non-context tasks, completed tasks and a second release
also fail. Before/after scalar readback must show the word unchanged. The cfg-only private observer
`test_release_word(TaskId)->Result<u64,Error>` checks inactive boot context,
token and owned RW mapping, and reads only the fixed eight-byte word. It takes
no address/output buffer and does not require release provenance. It therefore
can observe an ordinary admission of the same canonical image without granting
write authority. Add `HMRT WORD group=G id_group=H slot=S generation=V value=W`
before/after each applicable refused release; for stale/foreign tokens observe
the legitimate target owner, never reinterpret an invalid token as an owner. These
negative calls cause no user transition. Valid owners subsequently execute and
produce the required capture, so a rejected operation cannot silently destroy them.

### Canonical fixture and context protocol

Archive `runtime-initramfs.cpio` contains `bin` followed by these regular files:
`context_svc_a`, `context_svc_b`, `context_irq_a`, `context_irq_b`, `loop`, `hello`,
`byte_budget`, `call_limit`, `fault`, `yield`, `grant` (each prefixed `bin/`).
The checked-in path is `crates/hm-core/fixtures/runtime-initramfs.cpio`.
Private ContextProbe::A/B select exactly `bin/context_irq_a` and
`bin/context_irq_b` from that archive. Both have fixed `poll_start=0x100000100`
and `poll_end=0x100000108`: exactly the LDR/CBZ pair. The linker places this
small poll section separately from the larger setup/capture code; ELF inspection
must reject any differing address/instruction before embedding.
The manifest records exact file/path/SHA, entry, pages, segments and symbols.
No kernel absolute symbol is present. Required symbols as applicable are
`native_start`, `write_svc`, `exit_svc`, `fault`, `loop_start`, `loop_end`,
`poll_start`, `poll_end`, `release`, `private_data`, `capture`, and `message`.
Every used SVC is independently checked as SVC0; the loop is B-to-self; the
fault is the independently decoded own-entry write. Record every distinct
capture-output SVC site if the compiler emits more than one; a shared shim is
preferred so each chunk uses the same `write_svc`.

Context release is the main draft's **eight-byte** little-endian word at
`0x100002000`, initially zero, in a RW/NX LOAD. This supersedes the earlier
four-byte suggestion. `private_data` follows at `0x100002008`. A dedicated linker
layout may split executable storage around this fixed data page; no overlapping
LOADs or implicit huge file padding is accepted. The compiled ELF, not assumed
section ordering, establishes permissions and addresses.

Private hook shape is `admit_context_probe(ContextProbe::{A,B})` and
`release_context_probe(TaskId)`, under the runtime-test cfg only. The former
internally selects the canonical polling archive member and sets private
provenance; ordinary `admit(Image,...)` can never acquire provenance. The latter
accepts no address/bytes, validates token/state/provenance and writes LE u64 1
only once to the exact release word. It neither edits saved context nor performs
an EL0 transition. SVC context probes require no release authority.

A uses polling scratch x17, B x18; each uses the other as its release-address
register. Poll assembly is exactly a 64-bit LDR from the fixed address followed
by CBZ back to the load, with no flag-setting instruction. `poll_end` is the
successor instruction after that branch. Setup/capture/output IRQs are allowed
only at an independently admitted executable PC and count toward the same budget;
they do not count toward the two poll IRQs. Bound setup IRQs to eight per task;
exceeding that bound fails cleanly. Every resume still targets its own previous
ELR exactly. Capture preserves registers before calling Rust validation.

Export and emit an explicit 832-byte capture wire, little-endian words:
31 GPR u64s, SP u64, NZCV u64, 32 pairs of SIMD u64 lanes, FPCR u64, FPSR u64,
TPIDR_EL0 u64, TPIDRRO_EL0 u64, DIT u64, SSBS u64, one reserved-zero u64. This is 104 words, independent
of the core trap-frame layout. It takes four WRITE calls (256/256/256/64 bytes)
followed by exit0. SVC context adds its initial zero WRITE, for six calls total;
IRQ context has five. Kernel OUTPUT must originate from copied user bytes.
Host comparison is word-for-word, not a checksum or success flag alone.

Use seed A=1/B=2. Except the designated scratch/address/SP and native SVC argument
registers, GPR i is `0x1111000000000000 | (seed<<8) | i`. SIMD lane0/1 of register i
are `0x2222000000000000 | (seed<<8) | i` and
`0x3333000000000000 | (seed<<8) | i`. FPCR is seed shifted22; FPSR is
`0x08000000 | seed`; TLS is `0x4444000000000000 | seed`; NZCV is A=`0xa0000000`,
B=`0x50000000`. TPIDRRO/DIT/SSBS are zero after resume. SP must equal `0x100200000 - 1024`: entry assembly allocates that fixed
capture area before sentinel setup. The captured value is compared to this
independent constant, never accepted as its own expected value. For SVC capture, x0/x1 are zero replies; x2 is u64::MAX,
x3=0, x8=1; these override the sentinel formula. For IRQ capture scratch is1
and its address register is the fixed release VA; x0/x1 retain their sentinels.
Private data contains the seed before yielding and must still contain that seed
after peer progress; Rust validates it before capture output. Initial fresh-state
checks retain the existing zero-register rules and fail with a distinct nonzero
exit before any success capture.

### Exact serial grammar proposal

All integers below are canonical unsigned decimal; enum words are lowercase.
Reject duplicate/unknown/reordered records and unexpected fields. A single
private fixed UART formatter emits group-mode records; no caller writer or
callback is accepted while BUSY. Legacy HMPROC records stay byte-for-byte
unchanged and are forbidden inside this profile. Fixed terminal mode emits only
normal native output. All named identities come from actual owners, not UI counters.

```text
HMRT PREFLIGHT checks=15 irqs=8
HMRT BEGIN 24
HMRT INFO wire=1 cpus=1 frequency=F total_ticks=T quantum_ticks=Q box_bytes=Z box_frames=K
HMRT CASE index=I pass=P name=N before=B
HMRT GROUP group=G limit=L before=B
HMRT ADMIT group=G slot=S generation=V console_generation=C fixture=N entry=E root=R process_frames=F box_frames=K before=B after=A
HMRT ACTION group=G id_group=H op=cancel|query|reap|release|admit_reject slot=S generation=V result=ok|pending|stale|full|resources|release_denied before=B after=A
HMRT STEP group=G turn=U slot=S generation=V state=live|cancel_pending calls=C written=W irqs=J remaining=M charged=D turns=T before=B
HMRT CLOCK group=G turn=U begin=X remaining=M comparator=Q whole_end=E
HMRT ENTER group=G turn=U pc=E root=R
HMRT TRAP group=G turn=U kind=svc|irq|fault esr=E far=F elr=P spsr=S
HMRT CALL group=G turn=U ordinal=N pc=P version=V operation=O arg1=A arg2=B arg3=C
HMRT OUTPUT group=G turn=U ordinal=N length=L hex=H
HMRT RESULT group=G turn=U ordinal=N status=S bytes=B
HMRT CHARGE group=G turn=U end=Y accounted=1 remaining=M charged=D turns=T
HMRT RETURN group=G turn=U event=none|svc|timer|spurious|fault terminal=none|exit|fault|budget|call_limit|turn_limit|cancelled|unavailable entered=E code=C calls=N written=W irqs=J elr=P remaining=M charged=D turns=T root=R vbar=V controls=1 before=B after=A
HMRT IDLE group=G before=B after=B
HMRT DROP group=G before=B after=A
HMRT PASS index=I pass=P name=N before=B after=B
HMRT END 24
```

U is the per-task committed selection ordinal `StepReport.ordinal`, starting1
and including cancellation/no-entry selections. It may repeat across tasks; every
CLOCK/ENTER/TRAP/CALL/OUTPUT/RESULT/CHARGE/RETURN is scoped to its immediately
preceding STEP (group,slot,generation,U), with no interleaved step record. No new
group-global serial counter is required. Budget turns remains the independent
number of successful begin/finish pairs. After all live tasks terminate, one
IDLE call must observe no runnable task while completions are still held.

This block lists record forms, not an assertion that every form occurs in every
turn. Each CASE contains its scripted GROUP/ADMIT/ACTION/STEP sequences, followed
by DROP and PASS. Group namespaces are nonzero strictly increasing across group
creation; slot generations begin1 and advance only on reuse. Console generations
come from the resident Issuer and may skip only at a documented failed prepare.
The independent native CALL token is `(console_generation<<32)|1` when granted.

ACTION records both target Group and the supplied TaskId namespace; tokenless
admit_reject uses id_group=slot=generation=0. This makes foreign-owner rejection
distinct from stale slot reuse.

RETURN separates the main API Event from optional terminal End: a successful
reply is svc/none, a quantum is timer/none, and cancellation is none/cancelled.
CLOCK/CHARGE serialize the actual optional Timing data. This successful pinned
suite requires accounted=1; a rejected finish must not fabricate that flag or
new counters, and defensive error tests use the last valid snapshot.

A normal turn is STEP, CLOCK, ENTER, TRAP, optional CALL/OUTPUT/RESULT, CHARGE,
RETURN. A successful begin always has exactly one CHARGE before RETURN. A
captured SVC dispatch suppressed by whole-budget expiry has no CALL; permitted
WRITE has OUTPUT only when nonempty and successful. Exit has CALL but no RESULT.
No other native request occurs except those prescribed by its fixture.
A pending-cancel/turn-limit completion is STEP, RETURN with entered=0 and
unchanged budget counters. A pre-entry Budget completion has CLOCK/CHARGE but
no ENTER/TRAP and follows the main terminal precedence. No-entry RETURN has
elr equal its last saved PC (entry before first run); it must not fabricate a
hardware exception. This pinned suite accepts no Unavailable or Spurious result;
those defensive error paths remain separately reviewed/host-tested.

Host reconstructs four-slot round robin, live versus held-complete states,
per-owner counters and frame charge from the script and records. Reap/query/cancel
idempotence follow the main API; query has no side effect. Completed slots are
not runnable and still prevent admission until reaped. Cancellation request
leaves pages charged; selected cancellation RETURN frees them. Terminal RETURN
adds exactly process_frames+box_frames to free count. DROP frees only remaining
live owners. Every CASE restores the common baseline. Invalid operations have
no CLOCK/ENTER/TRAP or other hardware event.

CLOCK comparator is `begin + min(remaining, quantum_ticks)`; whole_end is
`begin + remaining`, both within the admitted counter range. CHARGE recomputes
`delta=min(end-begin, prior_remaining)`, remaining/charged conservation and
one increment of turns. Parked gaps are not charged. Last valid snapshots are
never replaced by invented zero charges on errors. INFO requires total=freq/10,
quantum=freq/1000 and an actual compiler-derived box-size witness. Current measured
Process size9832/three pages is context, not a permanent hardcoded oracle.

Each ENTER corresponds to exactly the reviewed three-write GICC setup and ERET;
TRAP corresponds to the actual exception, optional timer ACK30/EOI30, then the
reviewed four-write restore. Correlate stream order even when serial formatting
happens after the restored slice. A bounded no-entry operation has no matching
hardware event. Terminal transitions forbid later entry of that generation.
The exact preflight and existing baseline trace grammar remain unchanged.

### Ordinary group acceptance

Reuse the existing `bin/echo` ELF twice; do not add a new normal application.
The ordinary adapter supplies separately owned present startup bytes `worker-a`
to slot0 and `worker-b` to slot1. Each real echo performs payload WRITE, newline
WRITE, exit0: six source-EL0 SVCs across the pair. Exact visible payloads are
`worker-a`, newline, `worker-b`, newline as four atomic native WRITE outputs.
Accept only interleavings preserving each task's payload-before-newline order;
the usual order is `worker-aworker-b\n\n`, not two intact lines. There are
exactly four distinct resulting byte strings: `worker-a\nworker-b\n`,
`worker-aworker-b\n\n`, `worker-b\nworker-a\n`, and
`worker-bworker-a\n\n`. Correlate each native write with slice order;
transport remains LF to CRLF.
The bytes are copied jobs, not a new grant or userspace spawn operation.

At64/128MiB, warm up the existing bootstrap tests, record free baseline, run
`run-group bin/echo bin/echo`, status, `run-group demo/loop bin/hello`, status,
`test`, then legacy `run bin/hello`, status. Add fresh128MiB echo-group first
without warm-up. Existing single-program/input profiles remain independent.

Monitor syntax/response formatting is the actual safe adapter in native_launch.rs:
exactly two split_ascii_whitespace path tokens, no shell expansion; paths remain
exact archive names. Completion lines are emitted only after the whole driver
returns quiescent and in actual completion order. For the echo pair require two
`bin/echo: exited (status 0).\n` lines, no missing/duplicate job output and no
other completion. The hardware trace requires six SVCs at independently checked
write/newline/exit symbols, exact per-slice control restoration and only bounded
correlated timer interruptions. Do not claim identical-PC ordinary traces alone
expose TTBR identity; source review and the special Trace profile supply the
stronger per-task identity/context proof.

For loop+hello, require actual timer IRQ/resumption of loop and real hello WRITE/
EXIT, with hello completing before loop Budget. Human completion lines must be
`bin/hello: exited (status 0).\n` then
`demo/loop: exhausted its active-time budget.\n`. Normal hello bytes are unchanged.
The driver must not run two legacy consuming calls sequentially and call that a
group: independent source inspection and the busy-peer trace are required.
Every group restores the free baseline, including after the interleaved test.
Completed count advances per successfully completed child; last prepared native
generation comes from the last actual prepare, not completion order.

For failed second admission, the adapter may already have prepared the first
child: it must drop the whole unstarted group, restore every frame and produce
zero EL0 transitions. Its generation observation must match actual attempted
prepares; never require a rolled-back Issuer. Normal syntax rejection occurs
before Group creation and changes neither frames nor generations. These paths
need exact safe-adapter host tests and bounded ordinary guest recovery, not a
new process mechanism.
