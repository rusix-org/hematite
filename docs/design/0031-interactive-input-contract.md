# 0031: proposed interactive input contract

Status: **safe codec/policy subset approved and implemented under
[the exact safe API](0031-input-safe-api.md); coordinator/hardware remains draft**. This resolves
policy/ownership choices from [the review](../reviews/0031-interactive-input.md).
The hardware section records primary-source investigation and remains separately
gated. The implemented subset has separate evidence; this broader proposal does
not authorize core copying, hardware waiting or terminal ownership changes.

## Sealed mode and finite profile

Introduce a separate private `Interactive<'exec,'pool>` coordinator borrowing the
one Executor and owning one boxed Process plus one boxed input-state owner. Do
not create a Group, program catalog, launch grant or child slots. Its root identity
is a real TaskId in a checked Executor-issued group namespace, slot0/generation1;
no other task can join that namespace. Share reviewed sealed slice/restore helpers,
not a second Executor or a callable hardware callback. Initial `shell` only runs
its immutable canonical ELF with help/echo/count/exit built-ins.

An exclusive terminal lease transfers RX ownership from the monitor. Admission
checks the actual CPU0 boot context and full archive/ELF before reserving RX;
then reserves the lease, issues namespace/input generation, loads/prepares/boxes
owners and publishes. All failure paths drop owners and roll back the terminal
reservation before returning; issued generations may burn, never roll back. A
failed hardware restoration retires terminal authority and Executor. Input token
uses the existing task-cap issuer with low slot5, explicitly reserved for Input;
lookups check mode, selected root identity, kind, generation and live lease.
Console slot1 remains independently issued. No authority is delegated to children.

Freeze proposed limits: **2 seconds cumulative charged time, 1ms quantum,
4096 lifetime calls, 65536 output bytes, 8192 accounted turns, 300 seconds wall
lifetime**, 512 FIFO bytes and one pending READ. No renewal/reset/credit from input.
Clock frequency and 56-bit domain checks precede publication. Capture `wall_start`
after complete construction immediately before publication; require checked
`wall_start+300*frequency<=MAX_COUNT`. Store one immutable wall_end. Every active,
delivery and idle boundary checks monotonicity against the last observation.
Old modes retain 100ms/64/4096/256 and their exact startup bytes.

Session receives a sealed profile enum, not caller-selected numeric limits; its
one original calls/written/stopped state persists through all read parking. The
interactive prefix keeps version/layout1 but advertises actual configured limits
and feature bits Console=1, Input=2 (flags3). Old preparation still emits flags1
and unchanged limits. No old program is silently assigned the new profile.

## READ wire, statuses and final-call policy

SVC0, x0=1, x8=6, x1=input token, x2=destination, x3=capacity1..256.
Reply x0=status, x1=count; non-Ok count0, Ok count1..capacity. Preserve x2..x30
and the existing sanitized resume policy. No EOF, zero-capacity probe, user timer,
line-mode flag or pointer-to-kernel callback.

Inner 64-byte LE Input envelope: exact magic bytes
`48 4d 49 4e 50 55 54 00` (`HMINPUT\0`); version1:u32@8, size64:u32@12,
input_token:u64@16, flags0:u64@24, max_read256:u64@32, calls4096:u64@40,
output65536:u64@48, reserved0:u64@56. This explicitly corrects the proposal's
nine-byte spelling `HMINPUT\0\0`. These magic bytes match the outer startup-input
codec; the nested location, layout and size distinguish the formats.
Encoding is data, never a capability mint. Outer tail format is unchanged.

Every attempted SVC increments the shared counter once. Check version first,
then operation recognition. A recognized version1 READ on call4096 terminates
CallLimit **before token/range validation, copy, FIFO consumption or parking**.
Wrong version/opcode on that last call follows the existing error-reply then
CallLimit finalization; no subsequent ERET. WRITE/EXIT final-call behavior stays
as already reviewed, using the selected profile's limit. Other valid READs check
input handle, capacity, then complete writable range in that order, returning
Handle3/Length5/Address4. An ungranted legacy READ remains Operation2. Admission
cannot return a detached read guard for the final recognized READ.

READ owns a new non-Clone ReadAdmission of the original Session; parking consumes
it into ParkedRead. No host/user borrow survives. States are Ready, Dispatching,
WaitingInput, DeliveryReady, Closing, Completed. Pending state stores copied
address, capacity, input generation and root identity, never a dereferenceable
user pointer or borrowed mutable frame. The Process owns unchanged mappings and
has no Session while parked. This is independent of Group's WaitingChild.

## Charged delivery and exact commit boundary

READ always validates then provisionally parks, even if FIFO already has bytes.
Its actual SVC turn finishes accounting before WaitingInput/DeliveryReady commits.
A final terminal result discards ParkedRead; no future wake may revive it. When
FIFO is nonempty, select a separate kernel-only Delivery turn before any EL0
resume. It begins/finishes the same Budget and consumes one of the8192 turns,
but does not count another SVC. No synthetic entry/trap/child wake is emitted.

Delivery validates identities, stages min(capacity,FIFO length) into <=256 owned
bytes, and invokes a private full-range two-pass RW backing copy under boot root.
Reject any non-RW/missing/overflowing page before the first destination store.
No writable Frame/reference escapes. No IRQ timer is enabled to preempt this
bounded kernel operation: its 1ms comparator is an accounted soft bound, not a
claim of interruptible kernel-copy latency. Read final ordered counter after all
copy/Session updates with explicit opaque owners; linked ordering is a gate.

Before removing FIFO bytes or installing a reply, finalize in this order:
invalid clock/restoration/retirement -> Unavailable; observed UART error or FIFO
overflow -> InputOverflow; recorded Ctrl-] or explicit cancellation -> Cancelled;
wall expiry -> WallLimit; exhausted active budget -> Budget; otherwise commit.
A previously finalized terminal reason is immutable. Stage flags before final
counter; sample wall time at the same final counter. IRQs are masked, RX IRQ is
disabled and there is no callback between this check and FIFO/reply commit.
Commit removes exactly the staged prefix, installs original Session and Ok/count,
and publishes Ready last. If commit cannot satisfy its checked invariants, retire
and close; never return a partially installed response. A dying owner's backing
may contain copied bytes, but it cannot resume and FIFO is not consumed.

Cancellation/expiry while WaitingInput drops the parked owner without a delivery
turn. Active EL0 cancellation is observed after the next restored timer/SVC
boundary. Newly arriving bytes after the final bounded device sample belong to
the next event observation; this is not an impossible atomicity claim covering
unobserved hardware arrivals. No dequeue occurs on a failed delivery.

Reports distinguish `UserStep` (real lower-EL data), `Delivery` (begin/end, counts,
accounted flag, entered=false, no PC/ESR/FAR), `IdleWake` (real device/timer facts)
and `Closed`. New interactive-only ends InputOverflow and WallLimit do not alter
existing task completion words. No fake executed root report accompanies an idle
closure. Exact trace grammar must be reviewed before implementation.

## Terminal framing, overload and handoff

The monitor adapter passes whether its command ended in CR. Transfer includes a
single `suppress_next_lf` bit: discard one LF only if it is the next byte; any
other first byte clears the bit and is processed normally. Thus split CRLF is
coalesced without consuming arbitrary queued input. After that, bytes are raw;
userspace owns editing, CR/LF normalization and echo. Ctrl-] (0x1d) is reserved
for cancellation. Inspect receive error bits before truncation and inspect
escape even when the software FIFO is full. For one bounded drain, errors win,
then escape, then overflow; no data is delivered after any terminal flag.

A scheduler boundary drains at most256 hardware bytes; if more remain, a
subsequent bounded boundary is required. Cap total device-service boundaries at
16384 per interactive lease; exhaustion closes as Unavailable, never spins
unboundedly outside accounting. Idle handling admits at most8 consecutive wakes
with no byte/deadline/error event; the ninth closes Unavailable. Progress resets
only this spurious counter, not total work or time budgets.

Closing revokes the grant, masks RX, releases all process/FIFO owners after
restoration, then enters terminal **quarantine**, not ordinary monitor parsing.
Discard at most2048 received bytes over at most250ms, checking actual errors;
require a complete10ms RX-empty quiet interval before restoring the monitor.
Use the same bounded idle timer mechanism, not polling until quiet. Any byte
restarts only the quiet interval, never the absolute250ms deadline. Clear only
observed RX/error causes using reviewed register semantics. Continuous input,
uncleared errors or exhausted discard bound marks terminal Unavailable: print a
bounded diagnostic, mask/quiesce sources, and do not restart command parsing
until reboot. Frames must still reclaim. No bounded drain promises to empty an
infinitely replenished device. Bytes arriving after the successful quiet boundary
belong to the new monitor epoch; announce `hm>` only after handoff commits.

## Hardware investigation and remaining gate

This contract does not authorize a remembered IRQ/WFI sequence. Primary-source
findings and a candidate sequence follow; exact support is still being checked.

### Primary facts and platform limitation

Arm's [PL011 DDI0183F](https://documentation-service.arm.com/static/5e8e3655fd977155116a9042?token=)
§3.2–3.3 identifies DR@0x000, RSR/ECR@0x004, FR@0x018,
IMSC@0x038, RIS@0x03c, MIS@0x040 and ICR@0x044. DR bits11:8
carry receive errors; reading DR precedes associated RSR inspection. ECR clears
errors; ICR clears selected causes. RX is bit4, timeout bit6, errors bits7..10.
The candidate mask is0x7d0, never TX/modem/DMA. Register effects and error capture
must be tested, not inferred from truncating the existing monitor's byte read.

Pinned [QEMU7.2.22 virt](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/arm/virt.c)
places UART at0x09000000 and assigns SPI1 (GIC INTID33), matching local mappings.
Pinned [PL011 model](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/char/pl011.c)
uses a one-byte RX interrupt trigger, does not implement normal enable/loopback
behavior, and initializes CR=0x300. Its FIFO-enable transition resets receive
storage. Consequently current working serial output does **not** prove a physical
PL011 is configured correctly. Do not toggle FIFO/baud/line settings during
handoff or rely on a receive-timeout event the pinned model does not schedule.
Initial admission is explicitly this QEMU profile with actual register readback;
proper physical/ARM-KVM UART configuration remains a separate platform task.

Arm's [Critical Interrupt Prioritization](https://documentation-service.arm.com/static/5ed6547eca06a95ce53f9321?token=),
p7, distinguishes PSTATE masking from GIC priority masking: a pending interrupt
masked by PSTATE can wake WFI, whereas an interrupt blocked by the GIC priority
mask cannot. [ARMv8-A power management](https://documentation-service.arm.com/static/5efdc1b8dbdee951c1cd2baa),
p9, also describes WFI as a hint and masked IRQ wake. This suggests a smaller
candidate than adding an IRQ handler: retain PSTATE.I=1 through WFI and service
the real pending GIC interrupt synchronously afterward. There would be no IRQ
exception/vector switch, and evidence must call this a hardware interrupt wake,
not fabricate an EL1 exception trace. This is an explicit refinement of the
proposal's handler approach, pending review and pinned-emulator confirmation.

Arm's [GICv2 IHI0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=)
§3.2 requires peripheral level-source handling and matching end-of-interrupt
operations after acknowledgment. The candidate uses the existing Group0,
non-split-deactivation profile; it must prove that profile from register readback.
Unrelated pending/enabled state may not be cleared to simplify a test.

### Candidate masked-WFI idle transaction (not implementation approval)

1. Under the exclusive CPU0 BUSY lease, require boot root/normal coordinator
   stack, masked DAIF, exact one-CPU topology, no active thread/preemption window
   and no outstanding process slice. Hold every owner throughout. Save existing
   timer controls, GIC distributor/CPU controls and only the fields for INTIDs30
   and33; save UART IMSC. Reject active owned interrupts, enabled unrelated
   interrupt sources or unknown configuration. Existing vectors/SP/TLS stay put.
2. Keep UART masked during each EL0 slice. In the idle window only, route33 to
   CPU0 as level-sensitive Group0; admit timer30 using existing reviewed timer
   geometry. Program priorities/PMR/BPR and verify readback before forwarding.
   Set the physical comparator to the immutable wall deadline, or the earlier
   quarantine quiet/deadline event. Enable UART mask0x7d0 and only these GIC
   sources. Do not change UART FIFO, baud, line control or DMA.
3. With PSTATE.I still set, recheck FIFO/error flags and the actual counter after
   setup. If ready/expired, skip WFI. Otherwise execute a private memory-clobber
   barrier/WFI boundary; a pending level request arriving before WFI remains a
   wake cause because no handler has consumed it. GIC priorities must permit the
   wake request. This avoids the unmask/handler-return/before-WFI lost-wakeup gap.
4. After wake, still masked, capture actual counter, CNTP status and UART causes
   before disabling sources. Acknowledge at most two owned IAR IDs, masking the
   UART and disabling the timer before matching EOIs. Use the existing device
   completion barrier discipline. Spurious1023 gets no EOI and counts against
   the bounded spurious allowance. Any other ID fails closed. If both sources
   arrived, preserve both facts and apply the policy precedence already stated.
5. Disable owned GIC sources, confirm deassertion/quiescence, clear only owned
   pending state whose actual causes were captured, and restore exact admitted
   writable controls/readbacks before returning to Rust coordination or EL0.
   Do not promise to restore dynamic UART FIFO/status or read-only timer ISTATUS.
   Drain/copy in Rust only after this transaction returns; there is no ISR-held
   borrow or shared Rust queue accessed asynchronously.

Remaining hardware gates are concrete: validate exact GIC field offsets/masks
and Group0 admission; verify device-write completion/barriers from the applicable
architecture spec; inspect pinned QEMU WFI wake with masked IRQ (source fetch
was unavailable, no runtime test was run); define clock/spurious/failed-restore
cleanup; instrument actual WFI interval plus GIC acknowledge/EOI because standard
exception tracing alone will not show a masked wake. No architecture approval is
claimed from Cortex-M/Armv7 examples or old generic barrier summaries. In
particular [Arm issue L.a errata D23233](https://documentation-service.arm.com/static/6824b7398f79851ff2c3e6aa?token=)
removes an older WFI/barrier explanatory section; the final barrier argument must
be tied to current device-access rules and this exact sequence.

## Implementation approval boundary

Policy decisions above are concrete proposals for review, not current behavior.
Before core edits: approve the sealed profile/READ ownership API, mode/lease
rollback, exact two-pass RW-copy contract, delivery/report linearization and
terminal quarantine; then independently approve the primary-spec-backed idle
transaction with pinned evidence. Test native real EL0 input, production300s
silent expiry, overflow/cancel/handoff and repeated monitor recovery; shortened
policy tests do not substitute for the actual production comparator. Preserve
all existing ARM/x86 modes and tests. No implementation or VM was run here.
