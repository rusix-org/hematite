# ARM GICv2 session and local ownership

Status: independently reviewed for implementation, 2026-10-26; see
[review](../reviews/0016-arm-gic-ownership.md). Increment of design0014;
CPU1 remains off. This changes ownership boundaries, not interrupt delivery.

The global BUSY lease still encloses one synchronous preemptive session.
The Rust caller validates CPU0/TPIDR via cpu_local::current_id() before
acquiring the lease; diagnostics do the same and reject unmasked coordinators. Split
assembly into arm_gic_session_start/stop and arm_preempt_setup/stop. Only the
session functions write distributor CTLR or SPI banks. Session start validates
masked DAIF, the existing GICv2/no-security-extension profile, disabled GICC and
distributor, and no active SPI before mutation. It also rejects enabled non-timer PPIs,
pending non-timer SGI/PPI and any active SGI/PPI in bank0; read-only SGI enable
bits are not rejected. ITLinesNumber must be at least 1
(so SPI32 exists). It disables/clears SPI banks
1..ITLinesNumber (never bank0), then enables shared forwarding. No task runs yet.
Session stop requires the local CPU interface and PPI30 disabled, with no PPI30
pending/active state, then disables shared forwarding, clears pending SPI banks, and verifies
readback (including the diagnostic SPI32 bit).
These functions are private: caller owns BUSY and proves CPU1 is off. They must
not be reused for a future peer without the joined/off proof in design0014.

Local setup retains timer frequency/range/profile checks and CPU-local record
validation, additionally requires shared forwarding already enabled. It disables
only local GICC and CNTP, clears only PPI30 enable/pending bits in bank0, programs
only PPI30 group/config/priority using read-modify-write, then enables GICC and
publishes the CPU-local preempt record. Before mutation it rejects enabled
non-timer PPIs, any pending non-timer SGI/PPI, or any active SGI/PPI. SGI enable
bits can be read-only ones; the supported profile has no SGI sender and CPU1
stays off. This explicitly preserves exclusion of other local interrupt sources
without trying to clear active state. It never writes distributor CTLR, SPI
banks, or other SGI/PPI enable/pending bits. Local stop deasserts CNTP, disables
and clears PPI30, verifies no active/pending PPI30, disables GICC, withdraws the
local record, and verifies distributor CTLR is still 1. Timer quiet per-return
behavior stays local. A local setup rollback invokes local stop only.

Rust holds BUSY across shared start, local setup, callbacks, local stop and
shared stop. Shared start failure is before any mutation and releases BUSY.
Local setup failure always invokes idempotent local stop followed by session
stop before releasing BUSY; no live task exists. Fatal quiescence failure still
terminates rather than returning/reclaiming. Normal completion stops local,
then shared, before wiping stacks and releasing BUSY. Empty jobs retain their
existing no-side-effects success path.

The existing scoped timer diagnostics retain exclusive whole-GIC ownership:
they run outside all Runner leases on the masked CPU0 test coordinator before
preemption, with CPU1 off. Their global setup/teardown remains unchanged and is
not an API for future concurrent CPUs. No timer diagnostic is added to workers.

New preempt::gic_checks() validates CPU0/TPIDR and masked DAIF, acquires BUSY, starts shared forwarding, performs two
local setup/stop cycles, and checks each local stop left distributor CTLR==1 and
GICC/PPI30 quiet before global stop. It seeds pending SPI32 through ISPENDR1 bit0 only after confirming SPI32
is disabled and its pending bit is zero (session start already clears it).
Each cycle also attempts a duplicate local setup while its first setup is
active, requires rejection, then stops normally to prove cleanup remains usable.
After each local cycle it checks SPI32 remains disabled and pending; shared
stop removes that diagnostic pending state. The fixture never creates active
state or changes interrupt targets. With SPI32 disabled it cannot be delivered
even though shared forwarding is enabled. It never unmasks IRQ or dispatches tasks.
After global stop it verifies existing full control-state checks and releases
BUSY. The baseline gic_ownership case invokes this; repeated preemption suites
exercise the real session wrapper and retain their IRQ-count/context contracts.
No additional IRQ is generated, preserving exact scoped timer trace counts.

Source basis: [ARM GICv2 IHI0048B.b](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=),
sections4.3.6 and4.3.8 (banked register0, shared SPI banks), existing GICv2
MMIO contract/designs0009/0012/0014 and previously reviewed register addresses; no Linux source. Remaining blockers for CPU1 are
private stacks/entry, CPU-local mappings, mailbox/lifetime protocol and a global
session coordinating both CPUs. This change does not establish SMP execution.
