# Two-CPU preemptive scheduling implementation contract

Status: independently reviewed for implementation, 2026-10-26; see
[review](../reviews/0019-arm-smp-scheduling.md). Implements the concurrent
scheduler milestone in0014 on the lifecycle/memory/GIC prerequisites0015–0018.
It does not finish Phase1's concurrent allocator/general resource-management gate.

## API, authority and ownership

secondary::Runner gains run_smp(&mut self,cpu0_jobs:&[ThreadJob],cpu1_jobs:&[ThreadJob])
returning SmpReport {generation,cpus:[usize;2],reports:[PreemptReport;2],
irq_counts:[u64;2],off:bool,peer_checks,peer_root,peer_sp}. Each list has at most
four function-pointer/u64 jobs. TooMany/authority/Busy fail before effects;
empty lists are valid. Runner remains private-field non-Send/non-Sync, boot-only
construction, and permanent monotonic generations. Existing lifecycle run()
retains its behavior and tests. No Pool/Mapper capability is shared or exposed.

Refactor preempt::run into the existing CPU0 safe global-lease wrapper and a
crate-private unsafe run_local(jobs,cpu) implementation. The latter requires:
caller owns BUSY for the entire joined session; shared forwarding is initialized;
this CPU alone owns its local scheduler and four worker stacks; caller does not
release resources while any callback lives. It validates its MPIDR/TPIDR CPU
association and masked DAIF before mutation. It never acquires/releases BUSY or
starts/stops shared GIC state. It owns local setup/stop and stack cleanup; errors
before tasks run leave local state quiet. After CPU_ON, any local error fails
the VM rather than releasing the session. Single-CPU wrapper preserves existing
admission, rollback and reporting, including Busy on nested calls.

CPU-local validation admits record0 only on affinity0 and record1 only on
affinity1, checking exact base before dereference. Rust observations remain
IRQ-masked and never read remote plain records. Existing cooperative Runner
stays CPU0-only; extending current_id() cannot grant CPU1 its stacks. Fixed
helpers select CPU-specific worker/guard/emergency symbols, rejecting IDs outside
0..2. Each local State remains on its own coordinator stack and never escapes;
Token stores actual CPU identity for ThreadContext::cpu_id(). No migration.

## Mailbox, controller and shutdown

Preserve phase0/generation8 atomic layout. Extend the mailbox with two distinct
UnsafeCells: an immutable owned Work copy (mode, two fixed4-job arrays/counts)
and a peer-written result (lifecycle scalars, PreemptReport, IRQ delta). They do
not overlap. CPU0 prepares both only under BUSY and OFF; CPU1 copies Work with a
raw read after Acquire admission, never takes a whole-mailbox payload reference.
CPU0 retains its own owned job copy. No caller slice, reference, or Event pointer
crosses the mailbox; callbacks use only ordinary Sync shared test objects.

SMP session: CPU0 takes BUSY, validates OFF, resets bounded diagnostic atomics,
starts shared GIC once, prepares records/jobs, Release Prepared and CPU_ON.
Only blocked masks/all-blocked announcement atomics reset at admission; an
explicitly armed wait-race phase is preserved until that wait consumes it.
CPU1 enters its reserved root/stacks, establishes quiet local hardware, acquires
jobs and publishes Online. CPU0 publishes Go then runs its own local scheduler;
CPU1 runs its local scheduler after Acquire Go. Shared distributor forwarding
stays enabled regardless of either local completion. Both local stop paths touch
only own banked PPI/GICC/timer. Each observes local IRQ counter before/after its
run with IRQ masked; deltas must match its own PreemptReport preemption sum.
CPU1 finishes callbacks/local teardown, writes owned result and returns through
all Rust frames; assembly publishes Returned and CPU_OFF exactly as0018.
CPU0 first finishes its own local run, then requires Returned+OFF before peer
result access, shared stop, stack/canary restoration and lease release. Empty
CPU1 work can return before CPU0 completes; this does not allow early global stop.

## IRQ, faults and worker diagnostics

CPU1 vector slot5 (EL1h IRQ) routes to existing arm_timer_irq, which switches to
that CPU's SP_EL0 before its first save. Full image save, own TPIDR publication,
IAR/EOIR, emergency-frame release and coordinator restore remain0012's protocol.
No live remote Task or host context is read. Other secondary vectors remain
terminal; CPU0 synchronous recovery still belongs only to CPU0. In secondary
builds the common arm_exception_fatal entry checks full MPIDR affinity before
printing: nonzero affinity branches to secondary stack-independent fail2, while
CPU0 retains existing serial fault diagnostics. This also covers common timer/
preemption invariant failures on CPU1. Peer fatal paths
abort without serial reporting or reclaim. CPU0 alone writes structured results.

Safe private worker_observation()->Option<(u64,u64)> masks briefly, checks IRQ
was enabled, exact own CPU identity, local active+published task and actual SP
inside that CPU's reserved worker range, then restores DAIF and returns scalar
CPU/SP. worker_register_probe(seed)->u64 runs the existing full register sentinel
only after this admission; both sentinel polling loops already read own TPIDR
IRQ count. A coordinator or outside worker receives failure, not a hanging probe.
Task callbacks cannot choose raw contexts, alter TPIDR, disable IRQ or access
allocator/mapping capabilities through these diagnostic helpers.

## Deterministic concurrent tests and hooks

Parent's safe smp_checks module owns all callback test state and labels results.
Required positive sessions (repeated where indicated):
- Two nonyielding workers per CPU publish unique START bits, wait for all4, then
  publish SECOND bits and wait again. Each runs its own register sentinel and
  records CPU/SP. This deadlocks if CPUs run sequentially or either timer cannot
  dispatch its second worker. All4 require nonzero own preemptions, zero yields,
  normal return, correct CPU IDs and disjoint declared worker stack ranges.
- Four workers per CPU contend on one Mutex<Cell-based count/checksum>. CPU0
  worker0 holds an intentionally inconsistent pair until all7 peers observe
  contention, forcing preemption of the owner and remote execution. Then8x2000
  updates produce exact16000 with consistent checksum and8 destructor witnesses.
- A mixed voluntary-yield/forced-preemption workload on4workers per CPU exceeds
  the64-entry trace bound; both reports must mark truncation while exact completion
  and yield totals remain correct. A final empty0/0 session verifies empty reports
  after prior work and rejects oversized job lists without consuming a generation.
- Remote Events cover notification before wait, notification in the final-check/
  registration gap, notification after all remote workers park, broadcast,
  unrelated events and an entirely blocked CPU waking from a remote notifier.

Only smp-tests builds include fixed atomic race/observation hooks; no arbitrary
callback runs under an IRQ mask. arm_wait_race() requires maskedCPU0/BUSYfalse
and arms a fixed CPU0task0 wait. After that wait's final Event snapshot but before
registration stores, CAS phase1→READY2 Release signals CPU1 and spins masked
until finish_wait_race() CAS2→DONE3 Release. CPU1 waits Acquire READY, performs
one actual Event.notify, then finishes. CPU0 deliberately uses its stale snapshot,
parks once, and coordinator rescan must wake it without a second notification.
No Event pointer is stored by the hook; the normal wait borrow remains live.

Each local coordinator publishes a per-task blocked mask through own AtomicU64
after rescanning registrations; blocked_masks() uses Acquire reads only. Separate
atomic all-blocked bits record the actual no-runnable condition. CPU0 initializes
these under BUSY/OFF before each SMP session, and both local exits clear own
observations. Parent tests can wait for CPU1 mask0xf before one broadcast. These
are strictly diagnostic observations, not scheduling pointers or lifetime proof.

## Negative fixtures and evidence

smp-tests implies secondary-tests. smp-no-peer-timer-test withholds only CPU1 PPI30
in dispatch; otherwise identical nonyielding work. CPU1's first callback publishes
its real SP then CPU0 emits a started marker after Acquire, before the inevitable
hang waiting for the missing second CPU1 worker. Host requires CPU0 IRQ progress,
no CPU1 timer IRQ and deadline failure, with no returned report/reuse.
smp-peer-overflow-test runs a fixed oversized Rust stack frame on CPU1worker0.
Only a CPU1 EL1h data abort with FSC7 and FAR inside that worker's guard can take
negative exit5; other faults use generic failure. Host also checks fault PC in
the oversized-frame probe and FAR/CPU attribution. No recovery or OFF is claimed.
smp-all-blocked-test parks callback-local Events without notifiers on both CPUs;
CPU0 emits BOTH-BLOCKED only after both coordinators publish all-blocked bits.
Host requires timeout, no destructor-return/PASS/OFF/reuse. CPU0 marker helpers
are fixed core diagnostics; CPU1 never prints. Existing stale-generation and
CPU_OFF-return lifecycle negatives retain their exact outcomes.

Exact positive inventory in order: progress_contexts(2/2), mutex(4/4),
mixed_truncation(4/4), event_before(1/1), event_during(1/1),
event_after_broadcast(1/4), event_unrelated(1/2), limits_reuse(0/0).
Host validates reports per CPU, exact IRQ sum/IAR/EOIR attribution, expected
case inventory and39-case post-OFF baseline. Run pinned TCG with two vCPUs,
MTTCG and64/128MiB, preserving all prior one-CPU/x86 and negative suites. Record
failures and avoid interpreting passing lifecycle alone as concurrent progress.
Sources remain0012–0018's ARM/GIC/PSCI contracts; no Linux source.
