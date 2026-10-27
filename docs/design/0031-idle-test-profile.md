# First fixed idle hardware profile

Status: **Proposed test contract; no implementation, build, or VM execution.**
Scope is the [idle transaction](0031-idle-transaction.md), with its
[cleanup refinement](0031-idle-cleanup-refinement.md) and
[ownership boundary](0031-idle-ownership.md). This tests a private transaction
before introducing interactive process delivery. It does not establish READ,
FIFO ownership across sessions, quarantine, or a userspace shell.

## Admission and profile inventory

Use pinned QEMU 7.2.22, TCG, one CPU, `virt,secure=off,gic-version=2`, existing
ARM CPU model and boot mappings, and semihosting `userspace=off`. No debugger,
architectural single-step, synthetic clock, injected pending bit, or fake UART
byte is permitted. The boot-only test entry borrows the actual BootPlatform,
reserves its actual monitor RX boundary, and acquires the real BUSY lease.
Its private feature-only witness means **no Process has been entered**. It is
not a substitute constructor for the production inactive-Process witness.
No new Executor is minted to bypass the retained launcher.

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
EL1 interrupts. Record this explicit test preparation rather than assuming
firmware leaves CNTP_CTL=2.

## Fixed causal cases

All times below use the actual admitted counter frequency and checked 56-bit
arithmetic. Fixed short deadlines belong only to a sealed boot-test limit
constructor; they must not shorten the production coordinator's 300 s policy.
Case observations and serial formatting occur outside the armed transaction.

1. **pending_byte.** Emit `READY pending_byte`, then host sends exactly byte
   `0x50`. A bounded read-only FR poll confirms nonempty before invoking the
   transaction; it must not read DR. Use a one-second future limit. The final
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

Proposed serial envelope is `HMIDLE BEGIN profile=<name> cases=<count>`, one
ordered `CASE index=<i> name=<name>` followed by its observation records and
`PASS index=<i>`, then `HMIDLE END cases=<count>`. READY appears only for the
specified input cases. Exact field spelling freezes with the result API before
coding; required information is fixed here:

- Immutable frequency, start, wall_end/selected limit; every checked counter
  sample with validity and phase. Clock poison cannot be erased by later reads.
- Original and final saved writable controls, including affected GIC fields;
  actual root, DAIF, mapping admission, and terminal/RX ownership result.
- Precheck and immediate post-WFI FR/RIS/MIS/RSR and enabled CNTP_CTL samples;
  entered_wfi and the linked entry/resume PCs. Raw causes stay separate from ACKs.
- Up to two full IAR values in observed order; each acknowledged source's
  disable readback, source-mask readback, pre-EOI pending=0/active=1 and post-EOI
  pending=0/active=0 observations, with bounded poll counts. Unacknowledged
  cleanup uses active=0. Capture fixed successful/final samples and counts,
  not an unbounded per-poll text stream.
- Exact bytes pumped only after restoration, and RX-empty observation. No DR,
  RX/RT ICR, ECR, formatting, or serial callback occurs inside wait_once.

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
The entry must start a TB and contain WFI as its first instruction; WFI ends
that TB. The resume block and completed ordered post-sample must be reached
only through that continuation. No bypass edge, exception PC rewrite, or
stepping is admitted. Require exactly one completed entry/resume pair for each
entered case and none for skip cases. Repeated translations are not repeated
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
fresh expected generation and full frame restoration. Its normal lazy timer
preflight, if required by the actual retained state, must be a frozen exact
prefix; choose that path before implementation, not an optional parser branch.
Then run the existing full 41-case kernel baseline and privileged final exit.
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

## Remaining concrete freeze points

Before code: approve the test-only witness and short-limit constructor; freeze
bounded observation record field names against the private API; select the
exact retained launcher's post-idle preflight path; and bind every GIC write
sequence to the reviewed transaction implementation. Before guests: independent
source, compiled MMIO/WFI ordering and stack review. Before acceptance: first
raw trace review and all three profiles, including the uninterrupted production
300 s case. None of these gates is represented as completed by this proposal.
