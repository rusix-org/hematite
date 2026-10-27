# First fixed idle hardware profile

Status: **Fixed boot-only profile accepted in full ARM/x86 regression.**
All nine cases, later hello/baseline execution and exact three-block raw grammar
pass [independent final review](../reviews/0031-idle-transaction.md). Failed
candidates and diagnostics are preserved separately.
Scope is the [idle transaction](0031-idle-transaction.md), with its
[cleanup refinement](0031-idle-cleanup-refinement.md) and
[ownership boundary](0031-idle-ownership.md). This tests a private transaction
before introducing interactive process delivery. It does not establish READ,
FIFO ownership across sessions, quarantine, or a userspace shell.

## Admission and profile inventory

Use pinned QEMU 7.2.22, TCG, one CPU, `virt,secure=off,gic-version=2`, existing
ARM CPU model and boot mappings, and semihosting `userspace=off`. No debugger,
architectural single-step, synthetic clock, injected pending bit, or fake UART
byte is permitted. The new `idle-tests = ["boot-tests"]` feature enters private
`aarch64::idle_tests(&mut BootPlatform)` before the baseline, following existing
diagnostic feature precedence (all earlier named profiles win in combined
builds). The boot-only test entry borrows the actual BootPlatform,
reserves its actual monitor RX boundary, and acquires the real BUSY lease.
Its private feature-only witness means **no Process has been entered**. It is
not a substitute constructor for the production inactive-Process witness.
No new Executor is minted to bypass the retained launcher.

`option_env!("HM_IDLE_PROFILE")` admits exactly `short` or `wall` in an
idle-tests build; missing/unknown values fail the test entry before reservation
or device mutation. There is no run-time numeric deadline or arbitrary profile
parameter. Short-limit constructors exist only under idle-tests, require the
no-Process-entered witness and use the four fixed cases below. Wall uses the
actual production300-second constructor; it cannot select the short constructor.
The witness is minted once by this bootstrap before any Process entry, while
holding the real execution Lease and actual RX reservation; it is neither a
boolean supplied by the driver nor a general safe authority constructor.

Three fresh boots form the initial acceptance inventory:

| Profile | RAM | Cases in exact order | Host deadline |
|---|---:|---|---:|
| short64 | 64 MiB | pending_byte, expired_limit, uart_wait, timer_wait | 60 s |
| short128 | 128 MiB | pending_byte, expired_limit, uart_wait, timer_wait | 60 s |
| wall128 | 128 MiB | production_wall | 360 s |

Thus nine case completions are mandatory. Every boot also performs the
post-idle restoration sequence below. No missing case, retry-until-pass,
watchdog expiry, or truncated log is a pass. Initial timer admission must be
established by the existing exact eight-IRQ timer preflight once before BEGIN;
its trace is a separate required prefix, not an unrestricted allowance for
EL1 interrupts. Run `timer::checks` exactly once before BEGIN, require all four
checks (aggregate15), and emit `HMIDLE PREFLIGHT checks=15 irqs=8` only after its
complete restoration. This preflight precedes the real Lease/RX reservation;
no destructive timer probe runs while idle owns hardware. Record this explicit test preparation rather than assuming
firmware leaves CNTP_CTL=2.

## Concrete feature-only bootstrap API

Freeze implementation shape in private `idle.rs`, compiled only for idle-tests:

```rust
pub(super) enum Case { PendingByte, ExpiredLimit, UartWait, TimerWait, ProductionWall }
pub(super) struct BootTest<'a> { /* private Terminal borrow, real owned Lease, proof */ }
impl<'a> BootTest<'a> {
    pub(super) unsafe fn from_boot(terminal: &'a mut Terminal,
        topology: &Topology) -> Result<Self, Error>;
    pub(super) fn wait(&mut self, case: Case) -> Result<Observation, Error>;
    pub(super) fn pump_one(&mut self) -> Result<Pump, Error>;
    pub(super) fn finish(self) -> Result<(), Error>;
}
```

Only `aarch64::idle_tests` calls the unsafe constructor, documenting actual
initialized BootPlatform topology/mappings, no prior Process entry or concurrent
terminal reader, and no peer/DMA starts. The supplied topology alone is not
proof. Constructor verifies oneCPU and current controls, reserves the real
Terminal and acquires the actual process::Lease; errors undo a partial reservation
only after establishing no hardware was armed. Terminal has private
Monitor/Reserved/Unavailable states at the existing read_byte boundary. Drop
never silently returns Reserved to Monitor; explicit verified finish is required.

The test wrapper reads compile-time profile before construction. BootTest retains
that selected profile and exact next-case index; `wait(case)` rejects a wrong,
repeated or cross-profile case before mutation. It internally creates private
immutable bounds, binding them to this owner/case; no caller passes counts,
frequency, interrupt IDs, arbitrary bytes or numeric duration. After every case,
only the owned test pump may read actual DR under the reservation. Its bounded
one-byte/empty operation is `pump_one`, never a caller callback. It is allowed
exactly once after a successfully restored wait, checks DR error bits before
truncation if a byte exists, and requires empty FR afterwards. No next wait or
finish is allowed until this pump boundary succeeds. Pump is a private-field copied value `{ count: u8, dr: u32, empty: bool }`.
Count0 implies dr0 and an actual empty FR observation; count1 retains the full
checked DR word (error bits clear) and its low byte. No expected host byte is
substituted. The driver emits only those copied facts.
`finish` requires all selected cases consumed, RX empty and verified restoration;
then release reservation and Lease before returning to the retained launcher.
A fatal hardware path never reaches finish. The wrapper has not initialized or
entered any Process; subsequent hello is the first native entry. This feature-only
constructor does not satisfy the future production inactive-Process witness.

## Fixed causal cases

All times below use the actual admitted counter frequency and checked 56-bit
arithmetic. Fixed short deadlines belong only to a sealed boot-test limit
constructor; they must not shorten the production coordinator's 300 s policy.
Case observations and serial formatting occur outside the armed transaction.

1. **pending_byte.** Emit `READY pending_byte`, then host sends exactly byte
   `0x50`. A read-only FR poll confirms nonempty before invoking the transaction; it
   must not read DR. Bound it by both16,777,216 iterations and a checked
   absolute five-second deadline beginning after READY, shortened by the
   immutable wall_end. Each iteration checks an ordered counter for regression
   and56-bit range; limit/failure returns a failed case without arming. Mint
   the one-second transaction limit from the valid count after this poll,
   still <= original wall_end; do not rebase the300-second wall bound. Use a one-second future limit. The final
   pre-WFI check sees data, so entered_wfi=0. After verified cleanup, the owned
   test pump reads exactly `50` and then observes RX empty. This demonstrates
   retained unread data across teardown, not an interrupt wake.
2. **expired_limit.** With RX empty and no host write, mint a limit equal to an
   already sampled valid count. Require expired=1, entered_wfi=0, no timer arm,
   no ACK/EOI, and unchanged controls. No artificial clock regression.
3. **uart_wait.** RX starts empty; use a five-second absolute test limit. Emit
   READY before calling wait_once. Host waits for the actual execution-log
   attempt at the WFI entry TB, then writes exactly `0x55`, recording monotonic
   host timestamps and bytes. Require a completed WFI/resume pair, nonempty
   post UART state, timer not expired, and exact later pump byte `55` with no
   extras. Host scheduling may deliver before WFI actually halts; this case
   claims a real byte across the armed/WFI boundary, **not proof of residence
   in QEMU's halted state**. Missing delivery before the deadline fails.
4. **timer_wait.** No host input, RX empty; use an absolute one-second deadline.
   Require completed WFI/resume evidence, sampled counter >= limit, enabled
   post timer ISTATUS=1, no UART data/error cause, and exact timer cleanup.
5. **production_wall.** Fresh boot, no host input at any time. Mint wall_end by
   checked `start + frequency * 300`, using the production duration constant
   and production limit validation. Require actual completed WFI/resume,
   counter >= wall_end, timer expiry, empty UART, verified cleanup and guest
   success. Preserve host elapsed time as diagnostic evidence; the guest
   counter and real timer cause prove the duration, not the 360 s watchdog.
   Do not divide the duration, rebase the deadline, or reuse the short limit.

For the two timer cases, require actual ACK30 and its exact EOI. For uart_wait,
require ACK33 and its exact EOI. pending_byte may ACK33 if already forwarded;
accept only the recorded legal branch, never an invented EOI. No other valid
ID is accepted. A 1023 read is not an EOI target. An unexpected hint return
without the required cause fails this first fixed profile; coordinator-wide
spurious-return policy is separate work, not simulated here.

Simultaneous raw causes, the narrow recheck-to-WFI instruction race, arrivals
inside teardown, UART error/timeout generation, and nine spurious returns are
**not claimed** by this inventory. Ordinary host scheduling cannot force those
instruction boundaries reproducibly. Additional deterministic observation or
separately reviewed hooks would need a subsequent contract; do not label a
host sleep or a guest-set pending bit as their proof.

## Bounded records and execution evidence

Freeze the following private copied result (names describe real fields, not
public construction authority). `IdleObservation` is Copy and contains no
reference, pointer, lease, Process, Frame or caller-provided MMIO address:

- `entry: Snapshot`, `restored: Snapshot`, `setup: SetupReadbacks`.
- `count: Counts`, `precheck: Causes`, `post: Causes`.
- `acks: [AckObservation; 2]`, `ack_count: u8`, `cleanup: [Cleanup; 2]`.
- `entered_wfi: bool`, `expired: bool`, `spurious: bool`, `controls_restored: bool`.
  Pump is separate copied evidence returned only by the fixed post-restoration
  pump method; it contains no terminal borrow.

Compile-time size limit:1536 bytes total. Unused ACK cells zero; fixed arrays
never grow. Diagnostic storage is initialized outside active hardware ownership
where feasible; the result stays owned by the transaction until full restoration.
No success result exists on partial/fatal cleanup. Counter poison is sticky and
precludes a success observation; errors retain a bounded failure snapshot for
fatal diagnostics without inventing successful return. Future implementation
must seek review if concrete layout cannot meet this bound.

Snapshot fields, in exact wire order, are the existing `process::observe()`26:
`root vbar sp0 daif cntkctl cpacr tpidr0 tpidrro par sctlr tcr mair ttbr1 elr
spsr esr far fpcr fpsr tpidr1 cval ctl currentel spsel dit ssbs`, followed by
`group0 group1 cfg1 cfg2 prio7 prio8 target8 pmr bpr enable0 enable1 imsc dctl cctl`.
CTL is writable bits only; absent DIT/SSBS uses the existing guarded zero cell.
All other fields are actual admitted full readbacks. Nine shared control words
map directly to root's proposed snapshot array. SetupReadbacks contains
`changed` (64-bit dirty-field bitmap indexed by that40-field order) plus
`cval ctl group0 group1 cfg1 cfg2 prio7 prio8 target8 pmr bpr enable0 enable1
imsc dctl cctl`. Unchanged cells retain entry values; changed cells are actual
verified configuration readbacks. Store no third26-register CPU snapshot.
The compiled size bound includes all snapshots and remains1536 bytes.
For the admitted one-interface GIC, target8 is read-only zero with implicit
CPU0 SPI routing. No target write occurs and changed bit32 remains0; all other
configured fields keep their exact checks. This independently reviewed correction
follows the first diagnostic's rejected RAZ/WI target write, preserved in evidence.

Counts fields: `frequency start wall_end limit pre_arm precheck wake final
samples poisoned`. `samples` counts every actual ordered read, including polls;
retain only these named boundary values and final last-valid value (`final`).
`wake=0` if WFI was skipped; `poisoned=0` on every successful case. For expired
no-mutation returns, pre_arm/precheck repeat the actual admission sample and
setup.changed=0; no fictitious register/counter accesses are counted. `start`
is the constructor's checked origin; wall_end is checked start+frequency*300,
frozen once and repeated unchanged across every case. Short limits and readiness
deadlines use current checked counts capped by that original bound. ProductionWall
uses limit=wall_end; ExpiredLimit uses its current construction sample, already
expired at admission. Case transitions never rebase the wall bound.
Causes fields: `sampled fr ris mis rsr ctl count`; sampled0 means all remaining
cells0, not a claimed UART/timer read. `post` sampled1 only after WFI; precheck
is sampled1 if the transaction reaches the hardware recheck, else0. `ctl`
retains enabled ISTATUS before suppression; it is not restoration equality.

Each AckObservation has `iar enable source pending_before active_before
pending_after active_after counter_before counter_after polls_before polls_after`. enable/pending/active are
the selected owned bit normalized0/1, source is CTL&3 for30 or IMSC for33.
Expected successful values: enable0, source2/0, before0/1, after0/0; poll counts
1..1024. Cleanup per owned ID has `id enable pending active counter polls`, all state
bits0 and polls1..1024 when setup occurred, polls0 for unchanged expired path.
These are final observations/counts, never all1024 individual counter samples.

Every line starts `HMIDLE `. Decimal integers have canonical unsigned spelling;
addresses/registers use lowercase `0x` hex with no sign. Booleans are0/1. No extra
fields, duplicate/reordered lines or ignored malformed HMIDLE prefix is allowed.
All40 Snapshot values and16 Setup hardware values are hex; changed is hex.
Counts are decimal. Cause sampled/count are decimal, fr/ris/mis/rsr/ctl hex.
ACK iar/source are hex, all normalized bits/counters/polls decimal. CLEANUP
fields are decimal. WAIT wfi/resume are hex, other fields decimal. PUMP dr is
hex, count/empty decimal, payload hex is exactly none/50/55.
Exact successful serial order:

1. `PREFLIGHT checks=15 irqs=8`.
2. `BEGIN profile=<short|wall> cases=<4|1>`.
3. For each case: `CASE index=<i> name=<fixed name>`; optional
   `READY name=<pending_byte|uart_wait>`; then the records below, in order.
4. `COUNTS index=<i>` plus all Counts fields in the order above.
5. `SNAPSHOT index=<i> phase=entry` plus all Snapshot fields; then
   `SETUP index=<i> changed=<hex>` plus SetupReadbacks hardware fields.
6. `CAUSE index=<i> phase=precheck` plus Causes fields, then phase=post.
7. `WAIT index=<i> entered=<0|1> expired=<0|1> wfi=<hex> resume=<hex> acks=<0..2> spurious=<0|1>`. Spurious is a real1023 IAR read,
   not merely zero ACK count; it produces no EOI.
8. Exactly ack_count `ACK index=<i> ordinal=<0|1>` plus AckObservation fields.
9. Exactly two `CLEANUP index=<i>` records, ID30 then33, plus Cleanup fields.
10. `SNAPSHOT index=<i> phase=restored` plus all Snapshot fields;
    `RESTORED index=<i> controls=1`.
11. `PUMP index=<i> count=<0|1> dr=<hex> hex=<none|50|55> empty=1`, then `PASS index=<i>`.
12. `END cases=<4|1>`.

PUMP is actual bounded DR observation after restored return, never a copy of
host's planned byte. Check all data/error bits before truncation, require exact
expected count and empty FR. No DR/ECR/ICR access occurs inside wait_once. Each
case's successful controls/restoration record must agree with raw GIC evidence
and private copied observations, not merely expected constants printed by driver.

Read-only snapshots and record labels alone are not the hardware oracle.
Bind the linked assembly/MMIO implementation, exact control-flow paths and
retained observation buffer to source, compiler, ELF, disassembly and stack
review. QEMU's existing GIC CPU-write and acknowledge events independently
check ACK/EOI order and forwarding transactions; they do not expose every
Distributor read or system-register write. Do not invent trace events for them.
Additional available QEMU trace events must be discovered from the pinned
binary before use, with their exact grammar reviewed.

Use `-d exec,nochain,in_asm,int`, with `-dfilter` restricted to independently
resolved WFI and immediate resume TB entry ranges, plus existing GIC events.
Verify the actual leaf's WFI opcode and unique continuation from ELF bytes.
Freeze global assembly labels `arm_idle_wfi` and `arm_idle_resume`. The first
label is reached by a direct branch/call and its first opcode is WFI0xd503207f;
the immediately adjacent resume label contains ISB, MRS CNTPCT_EL0 into the
scalar return register, then RET, with no alternate branch target into it. No
stack or device access occurs in this leaf. WFI ends the entry TB. The resume block and completed ordered post-sample must be reached
only through that continuation. No bypass edge, exception PC rewrite, or
stepping is admitted. The first actual short64 raw trace establishes three
distinct blocks: WFI alone at arm_idle_wfi, ISB alone at arm_idle_resume, then
MRS/RET at arm_idle_resume+4. Independent review approves exactly that sequence,
including the separate translated-block geometries. Require all three execution
attempts in order plus the completed private observation for every entered case,
and none for skips. Missing, duplicated, swapped or merged blocks fail; this
supersedes the initial draft's assumed two-block entry/resume shape.
Repeated translations are not repeated
execution; never count `in_asm` as execution.

As the [logger evidence](0031-idle-proof-addendum.md) establishes, a Trace line
precedes TB execution. Reject a stopped-before-execution attempt in the filtered
region in this initial gate; entry logging alone is insufficient. Join entry,
resume, completed private observation and real cause evidence. A buffered log
that prevents timely uart_wait injection is a failed run, not permission to
substitute READY-based injection. Keep the new trace grammar unapproved until
independent inspection of the first actual raw traces.

No EL0 entry, HVC, IRQ exception, synchronous exception, or exception return
is permitted between idle BEGIN and END. GIC ACK/EOI while DAIF is masked is
expected without an IRQ vector. Consume the exact setup/service/restore write
sequence from the reviewed implementation, including skipped branches; do not
use membership-only acceptance or swallow arbitrary writes at phase boundaries.

## Restoration and harness reuse

After END, explicitly complete the RX reservation and release the boot-only
witness/BUSY lease. The existing retained launcher then runs canonical native
hello once: exact payload, two real EL0 SVC transitions (WRITE/EXIT), exit 0,
fresh expected generation and full frame restoration. Call the actual `BootPlatform.native.run("bin/hello")` path through its
existing sealed adapter. Since this is a fresh boot and no Process/Executor has
been initialized by idle tests, normal lazy initialization must perform exactly
one eight-IRQ timer preflight after END and before the first hello EL0 entry.
No optional already-initialized branch is accepted. Emit `HMIDLE HELLO BEGIN`
before that call and `HMIDLE HELLO END status=0 generation=1 restored=1` only
after its actual successful report and exact frame recovery. The native hello
output bytes remain unchanged. Then run the existing full 41-case kernel baseline and privileged final exit.
This proves later user entry and timer delivery still work; no Process is live
while the test-only witness is used.

Reuse artifact binding and live-size polling from `test_rw_copy.py`, the exact
eight-IRQ and baseline/fixup transaction readers from `test_launcher.py`, and
hello's strict lower-EL PC/ESR/GIC rules from existing process/launcher gates.
Extend a dedicated reader with explicit idle events; do not weaken the shared
baseline or strip unknown events before parsing. Baseline has 16 exact timer
IRQs and 24 registered fault/return pairs, not merely matching totals.

A proposed dedicated host runner uses nonblocking serial I/O and polls both
logs every 50 ms. Cap serial at 1 MiB, combined QEMU trace at 16 MiB, and pending
serial parse buffer at 64 KiB. Kill the process group on size limit or deadline;
preserve logs and the failure reason. Record exact stdin writes (only two in
short profiles, zero in wall128), command, environment/tool versions, input
kernel/fixture hashes, actual validator import/source hashes, monotonic host
start/end, return code and limits. Fresh output directories preserve failures.
Host test mutations must reject missing/duplicate/reordered cases, invented
WFI skip/entry, stopped attempts, wrong cause/deadline, lost/unexpected byte,
pre-EOI pending=1, wrong active state, EOI1023, extra ACK, missing restoration,
extra EL0/IRQ during idle, and shortened production duration.

## Required acceptance gates

Before code: independently approve the frozen test-only witness, fixed short
limits, copied observation/API and serial grammar above. Bind every GIC write
sequence to the reviewed transaction implementation; no unexplained optional
write may enter the parser. The selected full raw sequence is initial8IRQ
preflight → idle cases → release → lazy8IRQ preflight → hello2SVC → baseline
16IRQ plus24fault/return pairs → sole privileged successful semihost exit. Before guests: independent
source, compiled MMIO/WFI ordering and stack review. Before acceptance: first
raw trace review and all three profiles, including the uninterrupted production
300 s case. The final review records completion for this fixed profile. A new
production witness or changed protocol must pass the corresponding gates again.
