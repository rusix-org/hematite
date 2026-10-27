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
