# Masked-WFI UART/deadline transaction

Status: **Draft, not approved for implementation.** No code, build or guest run.
This refines [0031's input contract](0031-interactive-input-contract.md), using
[primary evidence](0031-idle-primary-evidence.md). It does not authorize a new
interrupt handler or relax the eventual production 300-second wall deadline.

## Sealed ownership and admission

Proposed private API (types have private fields and no Clone/Send/Sync):

```rust
fn wait_once(rx: &mut RxLease, execution: &ExecutionLease,
             limit: IdleLimit) -> Result<IdleObservation, IdleError>;
// IdleLimit: authenticated session wall_end or an earlier quarantine deadline.
// IdleObservation: before/after counts, raw causes, up to two owned IAR values,
//                  entered_wfi, restored; no user buffer or mutable device handle.
// IdleError: Context, Busy, Clock, Configuration, SpuriousLimit.
```

The names are new proposals, not existing exported APIs. RxLease is minted only
by the real BootPlatform's exclusive terminal transfer; the caller retains the
inactive Process, its execution lease and terminal ownership throughout. No
callback, formatting/UART output, allocation, user copy or policy dispatch occurs
inside wait_once. Return only after verified hardware restoration; a restoration
failure or foreign acknowledgement never returns an apparently usable lease.

Require exact oneCPU topology, CPU0, EL1h, masked DAIF, boot root and admitted
MMU controls, fixed VBAR/SP0, no active timer/preempt handler, and held BUSY.
Use the current process admission checks as the specification for GIC quiescence:
GICD/GICC disabled, known GICv2 view, no active IDs, no enabled non-SGI IDs,
zero bank0 pending; preserve unrelated disabled SPI pending bits. Additionally
require UART IMSC=0 and owned IDs30/33 inactive. UART pending33 is permitted as
external data can arrive before entry; it is not cleared to manufacture quiet.
Do not call timer::checks/preemption probes to wait: they intentionally exercise
and modify hardware. Existing read-only admission and ordered-counter helpers
can be factored after review; do not recursively acquire BUSY.

## Exact mapping and saved state

`boot-aarch64.S` supplies leaf descriptors
`0x0060000008000407` (GICD), `0x0060000008010407` (GICC),
`0x0060000009000407` (PL011). Their AttrIndx is1; MAIR_EL1=0xff makes Attr1=0,
Device-nGnRnE. Before MMIO, validate the actual immutable boot root/table paths
and these exact leaves, not only VA ranges. The platform proof excludes DMA,
external table changes and extra CPUs; `context()` alone does not prove that.
[Arm memory attributes](https://documentation-service.arm.com/static/63a43e333f28e5456434e18b)
defines nE as endpoint acknowledgement. [Arm ordering §7](https://documentation-service.arm.com/static/62a304f231ea212bb662321d)
explains why DSB SY on such writes waits for the endpoint response; that does
not prove every asynchronous peripheral side effect has already settled.

Save once per transaction: DAIF, root/control observation, CNTP_CVAL, CNTP_CTL
writable bits0..1, UART IMSC, GICD/GICC CTLR, PMR, BPR, and only the affected
ID30/33 group/priority/config/enable/target fields. Capture pending/active as
observations, not values to replay. Preserve unrelated bits in shared words.
No change to UART baud/LCR_H/CR/FIFO configuration, CNTKCTL, VBAR, SP or TLS.
Compare CTL writable bits only; ISTATUS is sampled while enabled, never treated
as stable restored state. Require entry CTL=2 (disabled/masked), as process
admission already does. The admitted group/security view and EOImode must match
the existing GICv2 process profile; never assume emulator-tolerated bad EOIs valid.

## Configure, recheck and wait

1. With forwarding off and sources masked, configure only ID30 and33 Group0,
   level-triggered, priority0x80, UART target CPU0, PMR0xff and BPR0.
   Read back implemented BPR value as existing code does; require the admitted
   value, not a value silently clamped differently. Read back every changed
   writable field. Do not clear unrelated pending SPI state.
2. Program CNTP_CTL=2; ISB. Write absolute CNTP_CVAL=limit; ISB, read back CVAL.
   Set CTL=1; ISB, verify ENABLE/IMASK. UART IMSC=0x7d0 and enable IDs30/33,
   then admitted distributor/CPU forwarding; DSB SY and bounded readbacks.
3. Sample ordered counter, UART FR/RIS/MIS/RSR and enabled timer ISTATUS.
   If any data/error, expired limit or timer cause is already visible, skip WFI
   and take the same teardown path. Do not consume DR here. A fresh arrival
   after this recheck remains asserted/pending across the following instruction.
4. Otherwise a private compiler-visible assembly boundary performs DSB SY; WFI;
   ISB and an ordered counter read, with named linked WFI/resume labels. Default
   asm memory effects and explicit lease/observation addresses prevent compiler
   movement across the boundary; no `nomem` claim. DAIF stays masked throughout.
   No same-EL IRQ exception is needed. WFI is a hint, not a sleep-duration promise.

CNTP ordering above is a **candidate requiring the remaining primary proof**
below. The existing process assembly already uses ISB after CVAL writes and
when disabling CTL, but that implementation precedent is not sufficient new
architectural evidence. [Arm Generic Timer §4.1](https://documentation-service.arm.com/static/66c4754f32f35b31ceb317ff)
supports ISB-before-counter for speculative reads and absolute CVAL comparison;
it does not by itself establish all write-to-interrupt ordering needed here.

## Observe simultaneous causes; acknowledge sequentially

Immediately after return, before disabling timer, capture counter, CTL.ISTATUS,
FR, RIS, MIS and RSR. Raw cause bits record independently sampled facts, not an
atomic simultaneity claim. Never derive raw timer expiry solely from IAR30 or
UART data solely from IAR33. Preserve these snapshots before any service.

Read IAR once. For owned30: mask timer (CTL=2), ISB, read back writable bits;
for owned33: write IMSC=0 and read it back. Disable that owned GIC enable bit;
DSB SY; write the exact IAR value to EOIR; DSB SY. Then permit the next IAR.
At most two valid acknowledgements, each ID at most once. On1023 stop, never
EOI it.1022 or any unrelated valid ID is fail-closed; no fabricated matching
owned EOI and no attempt to restore an unknown active interrupt owner.
After two owned acknowledgements, do not perform an unbounded drain loop.

Mask both sources, disable both GIC IDs and forwarding regardless of which IDs
were observed. An ID may never be acknowledged because its cause disappeared
or was masked; report raw causes separately from the actual acknowledgement
list. Require owned active bits clear. Only once sources are deasserted/masked,
clear owned stale pending bits if necessary; preserve every unrelated bit.
Readback/quiescence polls have a fixed1024-iteration ceiling, then fail-closed.
Do not clear UART RX/RT ICR here or consume DR: unread data remains owned by the
next bounded RX pump. QEMU's stored RX status need not reassert after premature
ICR clearing. RSR errors and DR error bits must reach that pump before any ECR
clear. No assumptions that timeout/error sources absent in QEMU work on boards.

Restore saved affected fields with forwarding still disabled, then CVAL and
CTL writable bits with synchronization/readback, original IMSC=0 and original
GIC controls. DSB SY/ISB, compare all promised saved controls. Pending UART
state/data arriving during teardown is retained as input, not a restoration
failure merely because a status bit differs. Once restored, return the captured
observation under BUSY to the coordinator; only it may drain bytes or commit READ.
Unexpected IAR, stuck active state or failed restoration masks/disables owned
sources and forwarding best-effort, retains ownership and terminates the kernel
fail-closed. It does not return to monitor, drop owners, or restart user entry.

## Bounds, accounting and the wall deadline

Each call executes at most one WFI; at most8 consecutive returns without data,
error or deadline evidence are tolerated by coordinator state, ninth retires the
session. The total16384 device-boundary limit never resets. Clock regression or
count beyond admitted56-bit MAX_COUNT is sticky failure; do not accept a later
recovered reading. No allocation or active-budget reset occurs during idle.
The immutable wall_end is checked-add(start, frequency*300), checked for overflow
and56-bit range at publication. Quarantine uses min(wall_end, its own absolute
bound); never rebase wall_end after input, spurious wake or retry. Expired limits
skip WFI. Final deadline/cancellation checks still precede delivery commitment.

A broken timer/GIC that never wakes cannot be detected by CPU code while halted.
Guest deadlines require the admitted timer wake mechanism, not a claim of
software timeout independent of hardware. Host watchdog bounds tests but is not
the production300s proof. Acceptance therefore requires a genuine no-input
production-duration300s expiry test as well as shorter dedicated race cases;
QEMU suspension/starvation is not a hard realtime guarantee.

## WFI execution evidence and remaining approval blockers

Pinned [translate-a64.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/translate-a64.c)
marks WFI as a TB terminator and calls its helper after advancing PC. It also
explicitly changes WFI behavior under architectural single-step. Do **not** use
GDB stepping or infer actual waiting from an `in_asm` translation listing.
Candidate observation: an isolated linked leaf whose only guest instruction is
WFI before its next TB, with `-d exec,nochain,in_asm` restricted to that leaf and
resume range. Require actual execution-log entry for that TB, its independently
verified WFI opcode, and the resume TB, paired with private pre/post counters,
GIC/PL011 cause evidence and no IRQ exception. No code from QEMU is copied.

Two blockers remain explicitly open before implementation approval:

- Fetch/inspect the exact pinned `cpu_tb_exec` execution-log placement/filter
  semantics (this session's web fetch of cpu-exec.c failed). Prove the log is
  execution rather than translation and cannot omit chained/repeated waits;
  choose a leaf boundary without globally changing WFI through single-step.
- Obtain primary architectural CNTP write synchronization and interrupt
  deassertion requirements for the proposed ISB/DSB sequence; freeze exact
  readback/EOI timing against them. Also independently review partial-setup
  unwind for every failed configuration/readback position before code.

The prior evidence already establishes masked-WFI wake feasibility on pinned
TCG, not those missing details. Required cases include pending-before-check,
arrival across recheck/WFI, arrival during halt, deadline alone, both raw causes,
spurious returns, unread RX across teardown, exact restoration and subsequent
normal timer/EL0 operation. Source/layout/stack, compiled ordering and independent
raw-trace review remain gates. No interactive userspace functionality is claimed.
