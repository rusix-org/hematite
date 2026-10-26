# Two-CPU ARM preemptive execution

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0014-arm-smp.md). Date: 2026-10-26. Extends designs 0012–0013. No CPU has
been started by this document and no SMP execution result is claimed.

## Scope and entry gate

The milestone is simultaneous timer-preempted safe callbacks on two CPUs,
including shared mutex contention and cross-CPU Event notification. A secondary
hello message is only bring-up evidence, not this milestone's exit gate.
Keep tasks pinned to their initial CPU: four guarded workers per CPU, no
migration, work stealing, CPU hotplug API or arbitrary captured closures.
The existing one-CPU and x86 suites remain required. This does not complete
Phase 1: the allocator is still CPU0-owned, so concurrent allocator exhaustion
and general SMP resource management need a subsequent design and tests.

Use the pinned QEMU `virt`, GICv2, `max`, TCG, secure/virtualization-off profile,
with exactly two CPUs and 64/128 MiB. The parent investigation captured the
unexecuted two-CPU DTB in `docs/evidence/smp-investigation/`: CPU affinities 0
and 1, PSCI/HVC, and a physical timer PPI30 description including both CPUs. Treat that as
configuration evidence only. Parse and validate the bounded topology and PSCI
description before starting a peer; reject duplicates, unsupported affinities,
conduits, disabled nodes or excess CPUs. Do not silently truncate topology.

Add a boot-created, private-field CPU0 `SmpRunner` capability and a synchronous
`run_smp(cpu0_jobs, cpu1_jobs) -> Result<SmpReport, SmpError>`. Each list admits
at most four existing ThreadJobs. Copy admitted jobs into owned session storage
before publication. Results contain two indexed PreemptReports and explicit
CPU identities; add `ThreadContext::cpu_id()` with default 0 for existing
implementations. This is an internal Rust API, not a stable userspace ABI.
SmpRunner is non-Send/non-Sync and checks its CPU0 execution association before
acquiring session authority; it cannot be placed in shared state to grant a
worker permission to control another CPU's lifecycle.
Single-CPU profiles return Unsupported without starting a CPU. TooMany and Busy
are checked before side effects. Empty work on either CPU is valid, but an SMP
verification workload must populate both. No error may abandon live callbacks.

## Boot and translation ownership

CPU0 alone runs `arm_start`, initializes UART, clears BSS and builds tables.
Add a separate secondary physical entry; it never clears global storage or
rebuilds tables. Before its first stack access it masks exceptions, verifies
CurrentEL/MPIDR and a nonzero context generation, then selects its reserved
coordinator stack. The full mailbox generation comparison happens only after
coherent mappings are active, not through an early uncached load. It initializes its
own SP_EL0 emergency stack, VBAR, translation registers, FP/SIMD controls, PAN,
FPCR/FPSR and trusted per-CPU base before entering Rust. Reject an unexpected
CPU identity or boot state through a stack-independent fatal path.

Reserve a second 64 KiB coordinator stack, 16 KiB emergency stack and four
64 KiB worker stacks, each with a 4 KiB unmapped guard. All reservations precede
`__kernel_end` and remain excluded from the frame pool. Extend guard construction,
linker assertions, protection checks and stack-probe evidence to every CPU.
Each CPU owns its emergency stack even when its workers are all blocked.

CPU1 uses a separate immutable root. It shares only the immutable identity and
device subtrees; the root entry for CPU0's owned-mapping arena is invalid.
Build both roots and all stack guards before enabling caches/starting CPU1,
then complete `DSB SY` and never modify their shared identity descriptors. These
initial stores occur with MMU/data cache off and all AF bits pre-set. If a later
implementation instead constructs a secondary root after caches are enabled,
it must add a separately reviewed clean-to-PoC sequence for every relevant table
cache line before CPU_ON; ordinary Release publication cannot replace that.
CPU1 cannot walk the
mutable owned-arena tables or retain their translations. This avoids pretending
that current local `TLBI VMALLE1` implements a remote shootdown. CPU0's existing
Mapper and Pool remain private, non-Sync, unavailable to callback jobs, and no
allocation/mapping operations execute in either local scheduler or IRQ handler.
Exposing those capabilities to peers, mapping the arena on CPU1, changing shared
identity permissions or reclaiming translation tables requires a separate
synchronization/shootdown design before implementation.

No mailbox data is read while the secondary MMU is off. The secondary derives
its fixed boot addresses from trusted image symbols, installs the immutable
root with matching normal-memory cacheability/shareability, completes local
TLB/instruction synchronization, then acquires the published session. Executable
bytes and initial table descriptors are immutable after their boot publication.
Before CPU_ON, CPU0 completes the required publication barrier. PSCI's specified
entry/cache/coherency contract is part of the declared platform precondition;
native hardware support is not inferred from QEMU passing this profile.

## Per-CPU execution authority

Replace singleton scheduler/timer state, IRQ counters and diagnostic flags with
two permanently allocated, aligned CPU-local records. Install a trusted record
base in TPIDR_EL1 during each entry and validate its MPIDR association. IRQ and
switch assembly obtains local publication through that base, never an unchecked
caller index. Keep TPIDR_EL1 fixed across callbacks; safe code cannot set it.
Record-layout offsets and bounds have compile-time checks, plus runtime CPU
identity checks at boot and public execution boundaries. Protection observations
must expect the correct root for each CPU, verify that CPU1's arena entry stays
invalid, and identify every CPU's own guard and emergency-stack top.
Plain CPU-local records and counters are never read by CPU0 while CPU1 writes
them. CPU1 copies its observations into its owned report; CPU0 reads that report
only after Returned and OFF confirmation. Any genuinely live remote observation
must instead use a separately identified atomic field with its ordering stated.

The global execution lease still excludes overlapping cooperative, preemptive
and SMP sessions. Inside one SMP session, a private local runner on each CPU
owns only its own contexts and stacks; it does not reacquire the global lease.
Local capabilities and tokens stay non-Send/non-Sync and tasks never migrate.
All mutable local run state has one CPU owner and is mutated with that CPU's
IRQ masked. Peers never access each other's Task records or borrowed Event
registrations. No blanket unsafe Sync implementation may be applied to the
existing Runner, Pool or Mapper to make the code compile.

Shared safe callback state must already satisfy Rust's Sync rules: e.g. static
Events, atomics and the reviewed Mutex<T: Send>. Function-pointer jobs cannot
capture a peer's stack or CPU0's BootPlatform. A worker may retain a mutex guard
while preempted; other workers yield on contention and IRQ handlers never acquire
that mutex. The complete asynchronous register image, CLREX behavior, emergency
stack release and normal-return completion rules remain those of design 0012.

## Interrupt controller ownership

Split existing timer setup/stop into global and CPU-local operations. Current
code disables GICD_CTLR and clears every enable/pending bank; it must not run
unchanged while a peer is active. CPU0 configures global distributor ownership
once while CPU1 is confirmed OFF, before CPU_ON. Enable forwarding for the
session and do not disable it until both local runners are quiescent and CPU1
is confirmed OFF again.

Each CPU configures only its banked SGI/PPI word, PPI30 group/priority/trigger,
GICC interface and local physical timer. Local teardown disables/deasserts its
timer, completes any exact IAR/EOIR pair, clears its local pending PPI and checks
local inactive/disabled state. It never clears peer SPI state or global
forwarding. Keep EOImode and Group0 explicit, with readbacks and failures as in
the single-CPU tests. Per-CPU trace validation must attribute IRQ, acknowledgment
and EOI to the same CPU; a total count alone is insufficient.

Existing scoped diagnostic probes execute only outside the SMP lease, with CPU1
confirmed OFF, and retain their exact one-CPU trace contract. CPU0 alone emits
normal serial reports. Peer reports use the mailbox; a fatal peer fault aborts
the VM rather than pretending to complete its callbacks or releasing resources.

## Session mailbox and shutdown

Use one bounded permanent mailbox with a never-wrapping session generation,
atomic phase words and UnsafeCell payload slots. CPU0 owns preparation and
reuse; CPU1 owns its result slot only after admission. No shared/exclusive Rust
reference to a mutable payload crosses publication or remote execution.
Copy only owned jobs and reports through this mailbox; it never contains a
pointer to a caller's temporary jobs slice or a borrowed Event.

CPU0 takes the global lease before checking PSCI AFFINITY_INFO(1) == OFF or
changing any session/controller state, then prepares the
immutable job copy and generation, then publishes Prepared with Release before
CPU_ON(affinity 1, fixed secondary entry, generation). CPU1 validates its context
ID against the Acquire-observed generation after installing its MMU, publishes
Online and waits for CPU0's Release Go. CPU0 starts its own local run only after
Online; Go lets both local schedulers run independently. A stale generation,
duplicate start or unexpected phase is fatal, never permission to reuse storage.

Each local run returns only after all its callbacks return normally. CPU1 writes
its owned report, quiesces local timer/interrupt state, and returns from its Rust
adapter to assembly. Only then does assembly publish Returned with Release and
invoke CPU_OFF. At that point no Rust worker or adapter frame is abandoned.
CPU_OFF returning is a fatal platform failure. CPU0 waits for both its local
completion and the peer's Acquire Returned, then requires AFFINITY_INFO == OFF
before reading/reusing peer storage, clearing stacks or releasing the lease.
Returned alone is not proof that the peer stopped executing on its stack.

Pre-start validation failures return ordinary errors without publishing work.
After CPU_ON might have admitted a peer, timeout, ALREADY_ON/ON_PENDING, missing
acknowledgment or uncertain shutdown cannot release the mailbox/stack lease.
Fail the guest or retain the session indefinitely for the host deadline; never
return an error that drops possibly live state. There is no cancellation API.
When either callback never returns, its local run and the overall session remain
owned. Repeated successful sessions increment the generation; at MAX refuse a
new session rather than wrapping. CPU1 starts from its secondary entry each time.

## Cross-CPU waiting

Event notification remains a checked Release/Acquire sequence change, with no
remote scheduler pointer. Each CPU scans only its own blocked registrations.
The existing all-blocked coordinator scan remains active with local IRQ masked,
so it can observe a notification published by the other CPU without an IPI or
idle kick. This is CPU spinning, not a low-power idle protocol. The park-side
final check and subsequent rescans cover a remote notification before, during
or after registration: a changed generation must not require a second notify.
No assumption that IRQ masking excludes remote notification is permitted.
If all workers on both CPUs block without a notifier, retain both runs and their
borrows; a host timeout is only negative-test evidence, never successful wakeup.
WFE/WFI idle, SGIs and device producers remain outside this milestone.

## Required evidence and implementation sequence

1. Validate topology/PSCI parsing with malformed, duplicate and unsupported
   fixtures. Record exact two-CPU DTB, machine command, PSCI version/features,
   CurrentEL/MPIDR and CPU_ON/OFF status observations. Prove secondary entry never
   clears shared BSS or reinitializes the distributor. This alone is not SMP.
2. Refactor per-CPU records and local/global GIC ownership under all existing
   one-CPU tests before admitting simultaneous execution. Independently source-
   review every assembly offset, trusted CPU-base path and mailbox transition.
3. Run at least two nonyielding callbacks per CPU that require local peer
   progress. Require timer suspension and distinct actual SP/CPU identity on
   both CPUs. Register probes must wait for their own CPU's counter to advance;
   peer ticks cannot satisfy a local preemption sentinel. Cross-CPU atomic
   handshakes must require overlapping progress;
   sequentially running one CPU's complete workload then the other's must fail.
   Exercise full register sentinels, guards, mixed yields, normal destructors,
   report truncation and repeated CPU_ON/run/CPU_OFF sessions.
4. Exercise the same shared Mutex<Cell-based payload> concurrently across both
   CPUs with exact final count/checksum and destruction. Verify cross-CPU Event
   notification before/during/after park, broadcast and unrelated-event cases,
   including a CPU whose every worker is blocked. No lost wake may require
   periodic artificial notification. Test the no-notifier retained-session case
   separately with strict launch/blocked markers and a bounded host timeout.
5. Require exact per-CPU IRQ/vector/ack/EOI counts, independent session generation
   and OFF confirmation, and source/ELF identities. Inject secondary startup,
   withheld timer, guard overflow and shutdown failures; none may produce pass,
   premature lease release or an ordinary successful return. After successful
   join/OFF, run the complete CPU0 regression suite in that same guest.

Use explicit TCG multi-thread mode for the two-CPU evidence and retain the
two-host-CPU/four-GiB container limit. Emulator interleaving proves the scoped
functional contract, not native hardware memory ordering, performance or the
full Phase 1 allocator-concurrency gate. No physical hardware or host boot
changes are part of this design.

## Sources and provenance

Consulted 2026-10-26; no Linux implementation source used or copied.

- [Arm PSCI DEN0022E, sections 5.5–5.7 and 6.4](https://documentation-service.arm.com/static/640f584656ea36189d4e94a4):
  CPU_ON supplies a physical entry/context ID; initial EL, masked state and
  disabled MMU/caches are specified; PSCI handles power-state coherency duties.
  CPU_OFF success does not return, while AFFINITY_INFO supplies the observation
  needed before reuse. The mailbox/generation protocol above is our design.
- [QEMU virt documentation](https://www.qemu.org/docs/master/system/arm/virt):
  virtual board, generated DTB and configurable GIC/CPU profile. Current docs
  provide context; the checked-in pinned-QEMU DTB is the selected profile's
  configuration evidence, not a claim about every virt version.
- Existing reviewed designs 0005, 0008, 0009, 0012 and 0013 supply the current
  ownership boundaries; this document identifies which assumptions change.
