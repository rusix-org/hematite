# Review: masked-WFI idle transaction

Status: **Fixed transaction and ownership design approved for implementation;
concrete hardware test profile pending.** The final decision below governs;
earlier findings remain as review history. Reviewed as design only.
The existing RW-copy and safe READ-policy approvals confer no UART/GIC authority.

Suppressing the source, disabling its forwarding, observing pending=0 while
active=1, then issuing the exact acknowledged IAR to EOIR and observing
pending=0/active=0 avoids assuming instantaneous timer-to-GIC propagation.
The inference requires that the admitted GIC's pending read exposes the asserted
level even while disabled and active. This precise architecture/emulator
premise remains to be independently confirmed; a pending read that hides the
active level would not prove the proposed pre-EOI condition. Do not replace
that check with a fixed delay. Later arrivals remain possible; all restored
owned sources and forwarding are disabled, with exclusive ownership retained.

The proposal correctly preserves independent raw cause observations, prevents
nested same-priority acknowledgements before EOI, never EOIs a spurious or
foreign value, preserves unrelated pending state and never drains PL011 DR or
clears RX/RT/error evidence during cleanup. Fatal paths retain ownership and
cannot return a usable lease. The phase table and private shortened-limit
provenance are concrete improvements over the original draft.

## Remaining implementation blockers

1. Confirm the exact disabled/active level pending-read premise against the
   cited GICv2 rules and pinned QEMU 7.2.22 read/update path. This is a bounded
   source question, not a request for physical-board proof. The independent raw
   URL fetch during this review failed; the harness author is investigating it.
2. Freeze one complete field/phase table: exact admitted security/EOImode,
   GIC control/priority/group/config/target/enable masks and writable timer/UART
   masks; saved values; setup order and readbacks; restoration action following
   failure at each first mutation. Restoring a partial configuration must not
   assume later fields were initialized. Before IAR, a verified restore may
   return Configuration/Clock; foreign IAR or unverifiable restoration is fatal.
3. Separate hardware-quiescent completion from clock-valid completion in the
   bounded poll. A regression/out-of-range sample permanently poisons the
   session, but hardware cleanup must still recognize pending/active/enable
   conditions using its iteration bound. Verified restoration may then return
   Clock without accepting a recovered counter; otherwise terminate. The draft
   phrase requiring a valid counter before accepting poll success conflicts
   with that intended behavior and needs this explicit distinction.

The final private interface must bind its execution lease to the inactive owner
and retain RX ownership until restored or fatal. Specify the copied observation
and errors, with no public constructor for limits, leases or a successful
restoration token. No arbitrary MMIO-address, interrupt-ID or mutable-hook
parameter is acceptable. The proposed fixed two-ID transaction and coordinator
spurious history are sufficient; a general interrupt framework is unnecessary.

Once these three concrete items are resolved, a separately reviewed fixed WFI
profile can proceed to source, linked ordering/stack and raw execution review.
Its evidence must establish completed execution of the named WFI path without
single-stepping, exact restoration and the actual production-duration deadline.
No new hardware code, candidate build or VM execution is approved by this record.

## Refined register, rollback and ownership review

The complete S0–S7 table now resolves the partial-configuration gap: snapshots
are fully initialized before mutation, dirty flags precede writes, shared-word
updates preserve unrelated bits, and every partially armed path uses common
source suppression plus verified restoration before returning. Outstanding
owned acknowledgements retain the exact IAR and use pre/post-EOI checks;
foreign acknowledgements remain terminal. Clock poison is now explicitly
separate from hardware poll completion and cannot be erased by a later sample.
These changes resolve blockers 2 and 3 above.

The ownership supplement closes the actual competing monitor read path with a
BootPlatform reservation state. Drop/forget cannot re-enable Monitor, and only
verified completion restores it. The production witness binds the retained
inactive Process and existing BUSY lease to the same coordinator; the private
limit is tied to that coordinator and immutable wall_end. Fixed linker-symbol
device paths are validated before MMIO. A dedicated test-only no-Process witness
is a distinct, stronger inactive condition, not a general public constructor.
This is acceptable as the proposed narrow ownership boundary.

One additional exact admission condition was requested: require
GICD_TYPER.ITLinesNumber >= 1 before accessing SPI33's bank/configuration/target.
A GICv2 identity and no-Security view alone do not establish that ID33 exists.
The pinned pending-read derivation is now documented in the proof addendum;
independent access to those source bytes is being arranged because browser
fetches fail and direct network DNS is unavailable in this review environment.
No new source implementation approval is recorded yet.

## Final fixed-transaction design decision

**Approved the fixed transaction and sealed ownership design for implementation;
the hardware test profile still requires its concrete freeze.** The three
blocking design findings above are resolved, including the added requirement
that at least 64 interrupt IDs exist before any SPI33 register access.

Independently inspected complete pinned QEMU v7.2.22 source bytes supplied for
read-only review, and recomputed their SHA-256 values against the addendum:
`gic_internal.h` 2fe9dbbe…; `arm_gic.c` 42216ce0…; `virt.c` 5b856d28….
The revision-2 pending helper ORs the software latch with the level input for a
level-configured interrupt, without enable, forwarding or active-state terms.
Distributor pending reads use that helper for both PPI and SPI masks, including
32-bit reads assembled from bytes. Level updates occur independently of the
forwarding decision. The security filter is conditional on Security Extensions,
and virt supplies its disabled secure setting to the emulated GIC. Thus the
admitted no-Security GICv2 view does support the exact pre-EOI pending=0/active=1
observation. The recorded source URLs/fingerprints in the proof addendum are
the provenance; no implementation code was copied.

This establishes the pending observation premise for the pinned profile, not
an instantaneous interrupt-deassertion guarantee. Retained suppression,
exclusive ownership, level configuration, bounded polls and post-EOI checks
remain necessary. The complete S0–S7 dirty-before-write rollback table and
sticky clock/hardware-completion split are part of this approval, superseding
the earlier less-specific transaction text.

The next test-profile freeze must name the exact private test witness/entry,
initial timer-quiescence establishment, fixture/case inventory, executable WFI
and resume labels, host injection protocol and strict execution-log grammar.
Fresh boot must not be presumed to have CNTP_CTL=2 merely because later process
admission requires it. Retain actual 300-second expiry and restored timer/EL0
regressions; a shortened deadline or watchdog cannot replace that evidence.
Source, linked stack/order and independent first raw-guest review remain gates.
This approval does not establish an interactive coordinator or a passing WFI
implementation, and it authorizes no general IRQ API or mutable testing hook.

## Initial fixed hardware profile review

Reviewed the current proposed three-boot inventory: four short cases at each
RAM size and one genuine 300-second quiet deadline, followed by existing hello
and bootstrap regressions. Its pending-before-call, already-expired, UART/WFI
boundary and timer-only cases are concrete. The UART case correctly avoids
claiming a proven interval in QEMU's halted state. Deferred simultaneous/race/
error/spurious cases are explicitly excluded rather than fabricated by input
sleeps or guest-set interrupt state.

Source approval awaits the promised exact private witness/limit/result API and
serial fields, plus the fixed second eight-IRQ lazy-preflight path. Replace
'every checked counter sample' with bounded critical/final samples, poll counts
and sticky validity; per-iteration checking is enforced in source, without a
large observation log on the kernel stack. This is a profile freeze issue,
not a reopened objection to the approved fixed transaction.

## Fixed hardware profile source-implementation approval

Approved the completed `0031-idle-test-profile.md` freeze for source
implementation. BootTest is feature-only and minted solely from the actual
bootstrap proof before any Process entry. It holds the real Terminal reservation
and BUSY lease, rejects wrong/repeated cases, and accepts no arbitrary timeout,
interrupt ID, byte or MMIO pointer. The separate fixed `pump_one` is permitted
once only after verified restoration; it returns copied full checked DR/count/
empty evidence and blocks the next wait or finish until successful. Explicit
finish is the only restoration of monitor RX ownership.

The copied observation bound is 1,536 bytes, with exact 40-field snapshots,
bounded ACK/cleanup records and critical counter samples/counts rather than
per-poll arrays. Actual spurious-IAR evidence is explicit. The pending-byte
readiness poll has both a 16,777,216-iteration ceiling and a checked five-second
limit bounded by the immutable original wall deadline, with sticky per-read
clock checks. Its later one-second transaction limit does not rebase wall_end.

The frozen three-boot/nine-case inventory includes real 300-second expiry.
Exact trace order is initial eight-IRQ preflight, exception-free idle cases,
verified release, retained launcher's fixed lazy eight-IRQ preflight, hello's
two SVCs, baseline 16 IRQs and 24 fault/return pairs, then privileged final exit.
Named WFI-first and adjacent ISB/MRS/RET continuation labels support independent
execution-attempt/resume/completed-observation correlation; READY alone cannot
substitute for the UART injection trigger. The profile does not claim unforced
simultaneous causes, instruction-race coverage or halted residence time.

This permits the fixed core/profile/host source work. No compiled candidate or
VM is approved yet: source review, actual MMIO/WFI ordering and complete stack
bounds precede first diagnostics. New raw trace grammar remains gated until
independent actual-log review. Production interactive delivery remains separate.

Approved a limit-origin clarification before BootTest implementation: its
constructor captures one immutable `start` and checked `wall_end = start +
frequency * 300`. All cases report that same Counts.start/wall_end; actual case
samples remain pre_arm/precheck. Short transaction/readiness limits are minted
from current validated counts and capped by the original wall_end. ProductionWall
uses limit==wall_end. Case transitions, input and retry never rebase the owner
wall deadline. This supersedes the ambiguous per-case-origin wording without
adding a caller-selected duration or another observation field.

## Initial implementation source gate

Reviewed `idle.rs`, `idle-aarch64.S`, `idle_checks.rs`, feature declarations,
BootPlatform terminal/read boundary and bootstrap dispatch. Approved the
corrected source for static checks and compilation; no guest is approved yet.

Hardware admission follows checked linker table paths before device MMIO,
requires SPI33 existence and the exact no-Security GICv2 view, and retains the
real BUSY/terminal owners. Register masks match the frozen table. Dirty bits
precede writes; configuration failures restore initialized snapshots before
returning. Service suppresses each acknowledged source, verifies pending=0/
active=1 before exact-IAR EOI, then pending=0/active=0. No duplicate or foreign
ACK can reach normal return. Suppression/restoration failures take the no-return
fatal path while owners remain live. Clock poison persists independently of
bounded hardware cleanup. Full saved-control equality precedes success.

The assembly leaf is exactly WFI followed by named ISB/MRS CNTPCT_EL0/RET
continuation. No UART read, formatter, callback or allocation occurs in the
armed transaction. The fixed pump runs only after restored return, checks the
full DR error bits and emptiness, and gates the next case/finish. Terminal Drop
never re-enables monitor reading; explicit verified finish does. The safe driver
formats copied bounded observations and subsequently uses the retained native
launcher for hello.

Two concrete initial findings were corrected before compilation: missing
HM_IDLE_PROFILE no longer silently defaults to short (the wrapper rejects it
before timer preflight), and skipped/expired observations now initialize wake
count to zero, including the unsampled post-cause record. Accepted the clarified
expired-limit construction: it uses a genuine current count sampled before
transaction admission, satisfying immutable start <= limit <= pre_arm. It need
not equal the constructor origin; original start/wall_end remain unchanged.

Linked review must count all live Snapshot/Transaction/Observation and by-value
BootTest storage, verify every MMIO/system-register/readback and WFI boundary,
and bind the two independently compiled fixed profiles. Exact raw record and
execution-log source review remains pending with the dedicated host tools.

## First linked candidate: guest gate rejected

Verified all 37 actual ELF and emitted assembly hashes in
`docs/evidence/idle/idle-candidates-first/inputs.json`. The 35 existing profiles
are byte-identical to the accepted RW-copy matrix. Both new idle profiles
compile, but their post-idle legacy hello chain exceeds the stack limit:
boot 7,600 → idle_tests 128 → launcher run_inner 1,472 → load_and_run 17,968 →
run_loaded 29,632 → Executor.run 9,872 → execute 960 → audit 176 = **67,808
bytes before the unchanged 640-byte allowance**. The 65,536-byte stack is
insufficient. No new guest is approved.

Short ELF `b97fc34500390bbf7bd5943b6dc35732045405628c6a0fb8bd393f00e96b38f1`
and wall ELF `1cd5084cf1fe1f371c8be568e8b48b57b8c781d89a392c8ef25c46d83448b5ec`
are the failed-gate candidates, not runtime evidence. The idle driver path
itself is smaller; restoration hello is a required reachable path and cannot
be excluded from the bound.

Recommended the narrow existing-pattern fix: add idle-tests to arch::init's
conditional inline(never), separating initialization spills from the long
post-idle call chain. This changes no owner, budget, stack size or test case;
both new profiles must be recompiled and remeasured before a guest decision.

## Second linked candidate: bounded diagnostics approved

The idle-only init boundary fixes the failed stack gate without changing the
stack size or allowance. Independently remeasured both profiles in
`docs/evidence/idle/idle-candidates-second/inputs.json`: boot is now 2,592 bytes.
The largest reachable chain remains the required post-idle native hello:
2,592 + 128 + 1,472 + 17,968 + 29,632 + 9,872 + 960 + 176 = **62,800 bytes**;
with the unchanged 640-byte allowance this is **63,440**, leaving 2,096 bytes
inside the 65,536-byte stack. Separate initialization (15,072 before allowance)
and idle transaction (8,272 before allowance) fit. The 35 existing candidate
ELFs were independently byte-identical to the accepted RW-copy matrix; the
conditional init change applies only to the two newly rebuilt profiles.

Verified the actual second files against their recorded hashes:

- Short ELF `389b2e2d262345020a0fd8862b24225dbb0f53401af0cac114d8c43c2f92ba63`;
  assembly `d109f6a89378b3ff1472168823fd6b0ef81b8c4956454b22afe0f452ec3574e1`.
- Wall ELF `af7544acee51c82f8e9a8f9eb8aee5eba2950de64e065648290d231db986ee13`;
  assembly `5e09a5128aaf4c5000eb23cca79d197a37b646bdd388f54390f412678ec26e06`.

Normalized emitted instructions for idle_checks::run, Transaction::configure
and Transaction::restore are identical between the first and second candidates.
The linked audit confirms dirty-before-write ordering, CTL/CVAL ISB readbacks,
pre-cause capture before WFI, post-cause capture before IAR, source suppression
and pending=0/active=1 polling before exact-IAR EOI, and post-EOI quiescence
before another acknowledgement. Restoration and complete saved-state comparison
precede formatting or DR access. Bounded cleanup remains independent of sticky
clock poison. The isolated leaf is WFI/ISB/MRS/RET, with no intervening callback.
Both artifact bindings (`artifact-short-first`, `artifact-wall-first`) pass and
bind these exact hashes, leaf opcodes/callers and the retained hello archive.
All 136 recorded source hashes in each artifact binding still match.

Reviewed the host builder/parser and final runner corrections: exact frequency
1000..1,000,000,000, admitted PMR/BPR limits, complete execution-block parsing,
real WFI-triggered UART injection, and live/final combined trace-plus-stderr
16 MiB limit. Ten synthetic/mutation tests pass in `host-second`; all 93 recorded
tool source hashes still match. These tests establish host rejection behavior,
not hardware execution. The parser requires exact case order, register/counter/
ACK/cleanup records, privileged preflight and baseline events, and subsequent
actual hello execution. Spurious IAR 1023 has its own acknowledgement trace and
no EOI, consistent with the pinned implementation.

Approved only the fixed first diagnostics: short 64/128 MiB and real 300-second
wall 128 MiB, under their frozen deadlines and log bounds. No guest result is
claimed yet. Keep the raw-trace review flag false until independent inspection
of actual serial and execution/interrupt logs; full regression and interactive
EL0 input delivery remain separate acceptance gates.

## First diagnostic failure: uniprocessor target-register assumption

`docs/evidence/idle/guest-first-short64/short64` is a genuine failed diagnostic:
the guest returns Configuration after receiving P, before WFI or the idle PMR
write. The eight preflight IRQs complete; subsequent CPU-interface writes belong
to verified rollback. There is no idle execution or successful-case claim.

Independently inspected official QEMU v7.2.22 `hw/intc/arm_gic.c` lines
1078–1095 and 1400–1415: ITARGETSR reads zero and ignores writes for a
uniprocessor non-11MPCore GIC. `hw/intc/arm_gic_common.c` lines 302–308 initializes
all such interrupt targets internally to CPU0; `gic_get_best_irq` uses that
internal target mask for SPI delivery. The source hashes match the existing
pinned arm_gic/internal evidence; common source SHA256 is
`d2bd4a8a109bdd45a00c7aa13a4346b87c13f1b23eb9757e17bdbdf1b3e57865`.
Thus the requested target8 value 0x100 must fail exact readback at field 6,
before PMR. This is a concrete invalid configuration assumption, not evidence
of missing UART delivery or WFI behavior.

Approved the narrow correction before implementation: additionally require
GICD_TYPER.CPUNumber bits 7:5 zero and the entire saved target8 word zero.
Represent target8 as a zero-mask observation-only entry; skip it before marking
dirty or writing, and exclude bit 32 from the changed mask. Entry/setup/restored
target words remain exactly zero. Keep exact readbacks/restoration for all eight
other fields. The fixed one-CPU profile relies explicitly on the pinned implicit
CPU0 SPI routing, not programmable ITARGETSR. Host mask expectations and design
must match this exact amendment. Both affected idle candidates require renewed
compiled ordering/stack/artifact gates before diagnostics; no general readback
relaxation or additional hardware support is approved.

Inspected the formatted correction in idle.rs and the corresponding host
parser/tests. The zero-mask branch precedes dirty marking and MMIO; admission
checks both CPUNumber and the saved whole target word. Dirty-gated restoration
therefore omits the target too. Host setup expectations omit only bit 32 and
continue requiring target zero and every other programmed field. Approved this
actual source for static checks and two-profile recompilation; linked and guest
evidence for the corrected candidate remains pending.

## UP-corrected linked gate

Independently verified both third actual ELF/assembly hashes. Short ELF is
`a8a067f23812684e2ba73934eab12589f69f06f6b48c7bf3e2ed7d0aadf37db2`, assembly
`1c68d716a80fc222df6b1a8ba792f9443bc9cb975d72ccee9af0ad62b76da4ad`;
wall ELF is `86e5e6e710b969d628ca051b039c041fbe674e4202dc554053270d55928033a5`,
assembly `93c80671ec38c87dc0a12dcc4ca7944fca495f0ea137cd5a9b91d6dc61f975f7`.
The previously bounded hello chain remains 63,440 including allowance. Comparing
named function allocations shows only idle_checks::run grows, by 32 bytes to
4,496; this separate idle path remains well below that maximum.

The emitted configuration loop checks the zero mask before shifting/setting the
dirty bit and before Hardware::checked. Thus target index 6 neither programs
MMIO nor marks bit 32. Other eight fields still use exact write/DSB/readback;
timer CTL/CVAL and enabling retain their reviewed ordering. Normalized restore
instructions are unchanged. Approved the renewed bounded diagnostic after
binding these current artifacts. Actual execution-log block segmentation
(including ISB boundaries) must still be established from raw evidence, with
the trace-review flag false until independent review.

## Corrected short64 raw diagnostic and exact TB amendment

Independently inspected `guest-second-short64/short64` against the third short
ELF. Guest execution exits zero in 3.189 seconds, all four cases complete,
retained hello exits zero with generation 1, and all 41 baseline cases pass.
The original host result remains failed because its assumed two-TB translation
shape was wrong; that failure is preserved.

Actual raw translation is three blocks: WFI at 0x402015ac, ISB alone at
0x402015b0, then MRS/RET at 0x402015b4/0x402015b8. Both entered waits execute
exactly those three ordered Trace PCs; neither skipped case does. No Stopped
attempt occurs. Approved replacing the old shape with only these exact three
shapes and ordered execution PCs, with missing/duplicate/reordered counter-block
and merged-block rejection tests. This is an observed logger grammar correction,
not a change to the leaf or an acceptance of an alternative execution path.

Replayed the entire serial and trace with only that amendment applied in memory
(no source or saved-log changes): 2 WFI sequences, 2 later EL0 SVCs, 32 preflight/
baseline timer IRQs and 24 baseline faults match the strict grammar. Pending P
skips WFI and is pumped as 0x50; expired case has no dirty writes or ACK; actual
UART wait has precheck MIS0 then post MIS0x10 and exact ACK/EOI33; timer wait has
post CTL5, count beyond its genuine comparator and exact ACK/EOI30. Each serviced
cause observes pending0/active1 before EOI and pending0/active0 after it. Each
transaction then observes spurious 1023 without EOI, exact restored snapshots,
and the expected pump bytes/emptiness. Immutable wall origin is shared across
all cases. This supports short64 only; short128 and the actual 300-second wall
profile remain pending and the global trace-review flag stays false.

The actual three-TB host implementation matches the approved amendment; its
new mutations target missing/duplicated/swapped counter execution and merged
ISB/counter translation. Independently replayed
`guest-third-short128/short128` using that frozen implementation and actual
linked fixup table/hello symbols. It binds the same third short ELF and returns
zero in 3.475 seconds. All four cases, complete serial/counter/ACK/restoration
checks, 2 real WFI sequences, 2 hello SVCs, 32 timer IRQs and 24 baseline faults
pass; the sole remaining validator error is the intentional review flag.
Readiness polling took 78 iterations (87 total samples), still within the fixed
bounds; no input/timing substitution is needed. The actual 300-second wall
case remains the final diagnostic prerequisite for global trace approval.
