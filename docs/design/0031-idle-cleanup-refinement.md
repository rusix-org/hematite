# Idle cleanup by observed GIC quiescence

Status: **Fixed transaction implemented and accepted in its boot-only scope**
by [the final review](../reviews/0031-idle-transaction.md). Production interactive
input remains separate. Read with the [transaction draft](0031-idle-transaction.md)
and [timer/logger addendum](0031-idle-proof-addendum.md). This proposal removes
an assumed instantaneous timer-to-GIC propagation guarantee, not the requirement
to clear an interrupt source before deactivation.

## What the architecture exposes

[Arm GICv2 IHI0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=),
§3.2.2 (3-40), §4.3.8 (4-100/101), and §4.4.5 (4-138/139), establishes:

- Disable controls forwarding; it does not prevent pending-state changes.
- Level-sensitive pending follows the input signal, independently of the
  software-set pending latch. Asserted level cannot be hidden by ICPENDR.
- Pending and active are independently readable, including active-and-pending.
- A valid EOI must match the last valid acknowledgement; EOImode0 also deactivates.

**Design inference:** for an acknowledged level interrupt, disable forwarding,
request source suppression, then observe its pending bit clear before issuing
EOI. Waiting for this observation replaces assuming ISB has already propagated
the source change. Device-nGnRnE accesses plus DSB/readbacks still order actual
MMIO; they do not themselves establish asynchronous line deassertion.

The pending read is an observation of architectural state, not a direct wire
probe or promise against future assertions. Software must retain suppression
and exclusive ownership until cleanup ends. Bounded failure is preferable to
retrying indefinitely or pretending a delayed device has restored successfully.

## Fixed owned fields and bounded primitive

Only CPU0 one-CPU topology, existing boot mappings, masked DAIF, exclusive RX
lease and execution/BUSY lease are admitted. The transaction owns only IDs30/33.
Use 32-bit MMIO and preserve every unrelated bit:

| Observation/action | Timer30 | UART33 |
|---|---|---|
| ICENABLER write | GICD+0x180, bit30 | GICD+0x184, bit1 |
| ISENABLER read | GICD+0x100, bit30 | GICD+0x104, bit1 |
| ISPENDR read | GICD+0x200, bit30 | GICD+0x204, bit1 |
| ISACTIVER read | GICD+0x300, bit30 | GICD+0x304, bit1 |
| ICPENDR write, only owned latch | GICD+0x280, bit30 | GICD+0x284, bit1 |

A private poll performs at most1024 iterations. Each iteration reads the exact
required enable/pending/active bits and separately samples an ISB-ordered
counter. Hardware completion depends only on the required device bits. Clock
regression/range failure sets permanent poison but never prevents recognizing
hardware completion, including after a later counter happens to recover. The ceiling is a finite
work bound, not a claimed hardware propagation time. It never WFI-waits for
quiescence, prints, allocates, touches user memory or calls a callback.

For an acknowledged ID, freeze this sequence:

1. Retain the entire valid IAR value locally. Do not read another IAR yet.
2. Write its ICENABLER bit, DSB SY, require matching ISENABLER bit zero.
3. For30 write CNTP_CTL=2, ISB, verify writable bits equal2. For33 write
   UART IMSC=0, DSB SY, verify zero. Preserve captured enabled ISTATUS/RIS/MIS/FR
   from before suppression; do not recalculate the original cause afterward.
4. DSB SY, bounded-poll its pending bit zero while its active bit remains1.
   No pre-EOI ICPENDR clear is needed for a normally acknowledged source;
   an unexpected sticky software pending latch causes failure rather than
   concealing an unexpected actor. Check enable remains0 throughout.
5. Only on observed pending0/active1, DSB SY then EOIR=the retained IAR,
   followed by DSB SY. Poll pending0/active0/enable0 with the same finite bound.
6. Only after this completes may the next IAR be read. At most two owned IDs,
   each acknowledged once; never two same-priority IAR reads before EOI.

This ordering deliberately differs from EOI-first-then-poll: that alternative
can deactivate while the line is still observed asserted. The new proposal
keeps the acknowledged interrupt active while waiting for deassertion.
A count expiry during this cleanup marks the session's wall deadline expired
but does not skip hardware cleanup or justify an early lease release.

## Simultaneous causes, no acknowledgement and late arrivals

Raw timer/UART snapshots precede service; they are separate from the ACK list.
After one owned ID finishes, the other can be acknowledged and serviced using
the same sequence. A vanished cause can yield1023; no EOI is issued for it.
After the bounded acknowledgement phase, suppress both sources and owned GIC
IDs, DSB/read back, then disable forwarding and require both active bits zero.
For never-acknowledged owned pending state, suppress first, then allow one
owned ICPENDR write to remove a stale software latch and poll pending zero.
This clear cannot conceal an asserted level under the primary semantics above.
No unrelated pending bit is written and no active bit is forcibly cleared.

Never clear PL011 RX/RT ICR or consume DR during this transaction. Unread input
and error-bearing data remain for the bounded RX pump. IMSC0 suppresses output
without deleting FIFO bytes; new bytes after a snapshot belong to the next
pump observation. Source suppression remains in force through restoration.
All restored enable/forwarding/IMSC values are the admitted disabled values.
A later-arriving owned pending indication cannot reach the CPU while disabled;
next admission rechecks pending/active/source state. A zero sample does not
prove no future delayed indication. If a platform cannot support these bounded
observations, it is unsupported/fail-closed, not accepted by extending a delay.

Unexpected valid IAR, duplicate owned IAR,1022, wrong active state, failed mask
readback or nonquiescence enters fatal cleanup: best-effort mask owned sources
and both forwarding paths, retain all leases and owners, terminate without
returning to monitor or resuming EL0. No EOI is invented for an unowned ID;
no successful-restoration report is emitted. This is a terminal kernel failure,
not a recoverable result whose Drop might release live hardware ownership.

## Return/error phases and clocks

| Phase | Permitted outward result |
|---|---|
| Before first MMIO mutation, no IAR read | Context/Busy/Clock with unchanged hardware and retained caller ownership |
| Partial configuration, no ACK yet | Restore original disabled state and verify every changed field; only then Configuration/Clock; failed restore is fatal |
| WFI returned, before/after owned ACK | Complete the suppression/quiescence/EOI protocol; only then return observation or Clock; any unverified cleanup is fatal |
| Foreign ACK or failed restoration | No returning Result; retain ownership and fail closed |

Clock failures are sticky coordinator retirement, never recoverable interactive
continuation. Record validity separately from the raw sampled value. Capture
and check at pre-arm, final pre-WFI recheck, immediately after WFI, every bounded
cleanup poll, and final restoration. Every sample compares against the last
valid sample; regression or >MAX_COUNT marks failure even if later samples
recover. Hardware cleanup still runs with its iteration bound after clock
failure; it must not depend on trusting that counter to terminate.

IdleLimit has private provenance: session wall_end or a coordinator-minted
quarantine bound, always <= immutable wall_end and <=MAX_COUNT. Reject forged,
out-of-range or later limits before MMIO. If now>=limit, skip WFI and return
an expired observation without arming. A quarantine limit may shorten but never
extend the original300s session limit. `SpuriousLimit` is removed from the
one-transaction result: only the coordinator owns the history of eight empty
returns and retires on the ninth. Existing total boundary/work limits persist.

## Evidence and remaining review obligations

The [addendum](0031-idle-proof-addendum.md) closes the logger-source question:
execution logs precede TB execution and can describe stopped attempts. Require
WFI-only entry TB, unique resume path, completed post-observation, and rejection
of stopped attempts; no stepping, debugger PC writes or inferred WFI from mere
absence of exceptions. Preserve the production300s deadline/no-input test and
short race cases. A host watchdog is test containment, not the deadline proof.

Required review obligations, now discharged for the fixed boot-only scope:

- The updated [proof addendum](0031-idle-proof-addendum.md) now traces pinned
  `gic_test_pending`, distributor reads and input updates: revision2 includes
  level independently of enable/active/forwarding. Bind the explicit guest
  command to `virt,secure=off,gic-version=2` and retain the accessible-view and
  level readbacks below. Independent review must accept that precise premise;
  no assumed timer propagation interval is introduced.
- Independently review the complete field/setup/rollback freeze below against
  the actual implementation, including every partial-setup failure position.
- Actual compiled ordering and no-return fatal ownership; counters must not
  allow accepting a recovered invalid sample. No arbitrary test hook may fake
  pending/active success or a timer wake.

This protocol tolerates delayed deassertion by observation. It makes
no normative latency claim, no physical-board/KVM claim, and no assertion that
finite polling proves an instantaneous external signal transition. The final
independent review accepts implementation and execution only for the declared
boot-only test profile; production integration needs its own ownership proof.


## Complete admitted register profile and snapshot

This section replaces the earlier transaction draft's implementation-dependent
BPR wording. It selects the existing process profile exactly, rather than
silently adapting to a different controller. Read the fixed boot Device mappings
before touching MMIO; retain the immutable mapping proof throughout.

Before any bank1/config/target access or write, require
`GICD_TYPER.ITLinesNumber >= 1` (at least64 interrupt IDs), so SPI33 exists.
Reject a32-ID controller before snapshotting any SPI33 register. Also require
GICD_TYPER.SecurityExtn bit10=0 and CPUNumber bits7:5=0, GICC_IIDR version
bits19:16=2, GICD_CTLR=0 and GICC_CTLR=0. Therefore the admitted CPU view has no
security banking and starts in combined EOI/deactivation mode; setup only writes
GICC_CTLR=1, which leaves EOImode clear. Reject every other view. Require all
implemented non-SGI enable bits zero and all implemented active bits zero;
scan exactly `(GICD_TYPER & 31)+1` banks. Bank0 pending must be zero. Preserve
pending SPI bits, including UART33, and never snapshot/replay them as controls.
Entry CNTP_CTL writable bits must equal2; UART IMSC must equal0. This matches
the current read-only process admission, with the additional UART ownership
check. Fixed GIC identity/enable scans are read-only, never IAR reads.

Snapshot the following into one fully initialized private local object before
first mutation. For shared words save all32 read bits; change only the mask in
the table, write `(saved & !mask) | requested`, and require exact whole-word
readback. This checks preservation of unrelated implemented bits too.

| Register | Change mask | Requested setup bits | Restore/readback |
|---|---:|---:|---|
| GICD+0x080 IGROUPR0 | 0x40000000 | 0 | full saved word |
| GICD+0x084 IGROUPR1 | 0x00000002 | 0 | full saved word |
| GICD+0xc04 ICFGR1 | 0x20000000 | 0 (level30) | full saved word |
| GICD+0xc08 ICFGR2 | 0x00000008 | 0 (level33) | full saved word |
| GICD+0x41c IPRIORITYR7 | 0x00ff0000 | 0x00800000 | full saved word |
| GICD+0x420 IPRIORITYR8 | 0x0000ff00 | 0x00008000 | full saved word |
| GICD+0x820 ITARGETSR8 | none; RAZ/WI in admitted UP profile | no write; implicit CPU0 routing | exact0 entry/setup/restored |
| GICC+0x004 PMR | 0x000000ff | 0x000000ff | saved low8; upper bits must0 |
| GICC+0x008 BPR | 0x00000007 | 0 | saved low3; upper bits must0 |
| GICD+0x000 CTLR | admitted whole values0/1 | 1 | exact0 |
| GICC+0x000 CTLR | admitted whole values0/1 | 1 | exact0 |
| GICD ISENABLER0/1 | bits30/1 via set/clear aliases | set owned bits | full saved read words |
| PL011+0x038 IMSC | 0x000007ff | 0x000007d0 | exact saved0; upper bits must0 |
| CNTP_CVAL_EL0 | all64 | authenticated absolute limit | exact saved64 |
| CNTP_CTL_EL0 | 0x3 | 1 after programming | exact saved2 under mask3 |

No target write is attempted. The first diagnostic rejected the earlier SPI33
target write before WFI: pinned QEMU's one-CPU GICv2 makes ITARGETSR RAZ/WI and
internally routes SPIs to CPU0. The independently reviewed
[source refinement](0031-idle-proof-addendum.md) replaces that unsupported
programmable-target assumption. Require the whole target8 word zero, omit its
write/dirty bit32, and preserve every other exact whole-word check.
No writes to GICD_TYPER/IIDR, active-bit
registers, UART CR/LCR_H/baud/ICR/ECR, CNTFRQ/CNTKCTL or unrelated enable bits.
The two enable snapshots include existing SGI enables; restore by clearing only
owned bits and compare full words. Read-only ISTATUS, UART raw/status and GIC
pending/active are observations, not fields restored to old values.
Require setup PMR readback0xff and BPR readback0; a clamped different value is
Configuration, not acceptance under an inferred alternate priority model.
Current process-aarch64.S already requires these exact readbacks; copying that
profile is deliberate, not a new general GIC configuration API.

## Ordered setup and partial-failure rollback

Maintain a private `dirty` bitset, marked **before** each first write, plus
`ack: Option<u32>` and `clock_poisoned`. All snapshots exist before dirty becomes
nonzero; a failed write/readback cannot leave an uninitialized restore value.
No callbacks, public unwind guard or caller-visible partially armed state exist.

| Stage / first writes | Required readback before advancing | Failure action |
|---|---|---|
| S0: no mutations; validate lease, mappings, profile, snapshots and limit | current counter valid; expired limit returns no-WFI observation | return Context/Busy/Clock; no device writes |
| S1: group, config, priorities, PMR/BPR in table order; target8 read-only0 | DSB SY after each write; exact readback | common rollback; restore dirty configuration only |
| S2: CTL2; CVAL limit | ISB after each system-register write; CTL&3=2, exact CVAL | common rollback including dirty timer fields |
| S3: owned ISENABLER30 then33 | DSB/readback both full words with expected owned additions | common rollback clearing both owned enables |
| S4: GICD_CTLR1 then GICC_CTLR1 | DSB/exact1 after each write | common rollback with forwarding suppression |
| S5: UART IMSC0x7d0, then CTL1 | DSB/IMSC readback; ISB/CTL&3=1 | common rollback including source suppression |
| S6: final ordered counter/cause recheck; optional WFI | valid clock and no ready cause to enter WFI | clock or ready cause skips WFI and cleans up |
| S7: post-WFI observations, sequential ACK/service | exact owned ACK and pre/post-EOI conditions above | cleanup if provable; foreign/duplicate ACK or unverifiable state is fatal |

S3 enables cannot deliver while forwarding is off. S4 enables forwarding while
the peripheral sources are still suppressed; a previously pending UART33 may
assert CPU IRQ, which remains PSTATE-masked. It is serviced at S7, never via an
ISR. S5/S6 preserve the already-pending wake case. This setup order supersedes
the less specific order in the initial transaction draft.

Common rollback after any dirty write first marks its own suppression writes
dirty, masks both owned GIC enables, disables CNTP (CTL2+ISB) and UART IMSC0,
and disables GICC/GICD forwarding. DSB/readback all these controls. With no ACK,
require both active bits0; permit only owned pending-latch clear and bounded
pending0 checks after suppression. With an outstanding owned ACK, use the
pre-EOI pending0/active1 protocol and matching EOI before any field restoration.
No new IAR is read just to make a partial-setup rollback look complete.

Once hardware is quiescent, restore dirty shared group/config/priority/target
words, PMR and BPR in table order with DSB/exact readbacks. Restore dirty CVAL
then CTL2 with ISB/readbacks. Original owned enables, IMSC and both forwarding
controls remain0; compare their saved values and the complete saved enable
words. Verify unchanged boot CPU controls/DAIF. Every rollback stage has the
same1024 poll ceiling and fatal fallback; no retry invokes setup again.
Unchanged fields need not be rewritten, but final saved-value comparison covers
all promised controls. Late UART data is preserved; pending is observed, never
silently restored by replaying an old snapshot.

A successful rollback returns Configuration if setup failed and clocks stayed
valid, or Clock if any sample poisoned the clock. Configuration is not permission
to keep using an unsupported input session; coordinator retires it. A Clock
return proves only verified hardware restoration, never accepted time accounting.
If restoration cannot be verified, no Result returns and no owner/lease drops.

## Hardware completion is independent of clock validity

The poll has two logically separate outputs: `hardware_complete: bool` and a
sticky `clock_poisoned` bit in the transaction. Each iteration samples device
bits and counter; a valid counter advances `last_valid`, an invalid one only
sets poison. Once set, poison cannot be cleared, even if subsequent values
appear monotonic. Hardware bits may still complete the poll immediately.
If the iteration bound expires without the required device state, failure is
fatal independently of clock validity. If cleanup and full restoration finish,
poison selects Clock rather than a successful IdleObservation. No fabricated
zero elapsed time or recovered counter is substituted into a success report.
The same rule applies to pre-WFI/post-WFI and partial-configuration failures.
