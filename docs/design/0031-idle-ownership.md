# Idle ownership and integration boundary

Status: **independently approved design; no hardware implementation**.
See [the final review](../reviews/0031-idle-transaction.md). The concrete
boot-test entry and profile remain pending.
This maps the [transaction](0031-idle-transaction.md) and
[cleanup refinement](0031-idle-cleanup-refinement.md) onto actual existing
owners. It does not authorize another executor or an IRQ handler.

## Existing authority

`aarch64::BootPlatform` owns the actual topology, frame pool and persistent
`launcher::State`. The latter owns the sole terminal `process::Executor`.
`process::Lease` is the existing non-transferable BUSY guard: its private
constructor validates the boot CPU context and atomically acquires
`threads::BUSY`; its destructor releases it. It is not an inactive-process
proof, a topology proof, or an RX reservation by itself.

`process::admission()` is read-only but checks the idle GIC/timer state, so call
it before arming, never midway through the armed transaction. It does not check
the UART mask or exact device leaf descriptors. `user_space::context()` checks
CPU0, root/control values, masked DAIF and PAN; it also does not acquire BUSY.
Reuse these checks without treating them as more authority than they provide.

The existing monitor `BootPlatform::read_byte` directly reads PL011 FR/DR.
Merely introducing a new zero-sized RX guard would not revoke this competing
path. RX exclusivity must be represented at that actual read boundary.

## Proposed sealed construction

Add a private terminal state owned by BootPlatform: Monitor, Reserved, or
Unavailable. No public constructor, reset, cloning or transfer between CPUs.
Monitor reads are permitted only in Monitor; a synchronous interactive launch
first reserves RX, then invokes the retained launcher while keeping BootPlatform
exclusively borrowed. Neither callbacks nor nested monitor commands are allowed.
An error before publication explicitly restores Monitor after owner cleanup.
Generations already issued remain consumed. Unavailable never becomes Monitor.

The RX reservation is a private, non-Clone, non-Send/non-Sync borrowing owner.
Its destructor alone must not restore Monitor: forgetting an owner may leak
Reserved, but must not enable competing readers. Only an explicit verified
completion transitions Reserved to Monitor; terminal failure transitions to
Unavailable or never returns. No raw register access, callback, arbitrary MMIO
address or selectable interrupt ID is exposed through this owner.

The interactive coordinator borrows the existing Executor and owns the actual
boxed inactive Process and input state. It retains one `process::Lease` across
the launch, delivery, idle and closure operations. Do not recursively acquire
BUSY in wait_once. Construct a private execution witness only within the
coordinator's inactive state, borrowing its Process and Lease for the duration
of wait_once. Its constructor is not available to safe kernel policy or other
core modules. The same exclusive coordinator borrow prevents EL0 slice entry,
mapping mutation, reclamation and delivery while this witness exists.

Construction additionally carries the actual one-CPU BootPlatform proof and
the fixed immutable device-mapping proof. Passing an arbitrary `Topology`,
`bool` named inactive, or an integer root does not mint that proof. Before
device MMIO, check root[0] -> arm_device_l2, device_l2[64] -> arm_gic_l3,
device_l2[72] -> arm_device_l3 and the exact GICD/GICC/UART leaves specified in
the transaction. All table pointers originate in retained linker symbols,
not input. Validate each parent before dereferencing the next table. No table
mutation occurs, and Device-nGnRnE attributes follow from checked MAIR/TCR.

IdleLimit is privately minted from the immutable session wall_end or an earlier
quarantine bound. A witness binds it to that coordinator's namespace and
wall_end; a numerically valid limit from another session is insufficient.
Store and check the original wall_end, not a fresh now+duration per wait.

## Proposed result and terminal behavior

Return copied counters, independent raw timer/UART cause samples, entered-WFI
flag, and a fixed two-element acknowledged-IAR list with an explicit count.
No returned mutable register, Frame reference, success token constructor or
user-memory pointer exists. A successful Result itself certifies verified
restoration; do not expose a caller-settable `restored: bool` as authority.

Before mutation, Context/Busy/Clock returns leave the retained owners intact.
After mutation, only exact verified restoration permits a returning
Configuration/Clock error. Clock poison is sticky in the coordinator. Hardware
poll completion remains recognizable after clock poison; each poll still has
its fixed iteration ceiling. Spurious history belongs to coordinator state,
not wait_once, so remove SpuriousLimit from the old draft's error list.

An unexpected IAR or failed restoration enters a private no-return fatal path
while the RX reservation, execution lease, Process and Executor borrow are
still live. Best-effort masking cannot justify returning an error. Do not
unwind, run their destructors, mark the launch completed or print via a caller
callback. Use the kernel's terminal failure mechanism, preserving the evidence
available before termination. Existing WFE fatal loops retain owners but do
not constitute a successful test exit or verified restoration.

## Staged implementation and verification

The first hardware test profile exercises this fixed transaction through a
private BootPlatform test entry with the same permanent topology/mapping proof.
It acquires the real BUSY lease and RX reservation; it does not fabricate a
public interactive Process or bypass hardware admission. Its sealed test-only
inactive witness represents the stronger condition that no Process was entered.
The production constructor still requires the actual retained inactive Process.
The test witness is unavailable without the dedicated feature and cannot reach
ordinary launch code. Independently review that distinction before implementing.

Compile and inspect stack and actual MMIO/system-register/WFI order before
starting new guests. Require real serial input and timer causes, exact saved
control restoration, subsequent ordinary timer/EL0 regression, strict bounded
raw trace review and the full production-duration quiet deadline test. No fake
pending bit, clock, RX byte or restoration callback substitutes for those tests.
The separate safe codec/READ policy and inactive RW-copy approvals remain scoped
to their already recorded tests; they do not establish this ownership protocol.
