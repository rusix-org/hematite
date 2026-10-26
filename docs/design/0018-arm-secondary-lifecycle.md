# ARM secondary lifecycle (no peer scheduler)

Status: independently reviewed for implementation, 2026-10-26; see
[review](../reviews/0018-arm-secondary-lifecycle.md). Builds on0014–0017.
The exit is two actual CPU1 startup/callback/shutdown sessions, not concurrent
preemptive workers or the complete0014/Phase1 gate.

## Authority and admission

Feature secondary-tests owns a private non-Send/non-Sync Runner created only
from CPU0 boot authority and the parsed actual DTB topology (exact affinities
0,1 and HVC PSCI). CPU0/TPIDR and masked DAIF are checked at each entry. Runner
uses a permanent mailbox-global monotonic generation starting1; reject MAX
exhaustion before work. Reconstructing a capability cannot reset generations.
A global BUSY compare-exchange excludes all existing scheduler/controller tests.
Both CPUs keep IRQ masked throughout; no GIC setup, timer scheduling, serial
peer output, Pool/Mapper authority or borrowed user job is passed to CPU1.

HVC calls use fixed PSCI IDs, x0..x3 explicit in/out, C caller-clobbers and
compiler memory effects. Probe VERSION(0x84000000), require runtime major1 (minor0 or newer),
and FEATURES(0x8400000a) for CPU_ON64(0xc4000003), CPU_OFF32(0x84000002), and
AFFINITY_INFO64(0xc4000004), requiring status0. Check CPU1 affinity level0 OFF
(result1) under BUSY before preparation. Pre-admission unsupported results may
return an error; once CPU_ON is attempted any nonzero result, invalid phase,
timeout/uncertain lifetime or CPU_OFF return fails the guest without releasing
possibly live storage. The host bounds runtime; live-session loops never return
an ordinary error or free the lease.

## Mailbox and cold entry

One permanent repr(C,align64) mailbox has AtomicU64 phase at0, AtomicU64 generation at8,
and UnsafeCell<Copy report> at16. A narrowly justified unsafe Sync applies only
to this mailbox. Phases: Idle0, Prepared1, Online2, Go3, Returned4. CPU0 owns
preparation while peer confirmed OFF; it writes generation, zeros report and
CPU1 local record (identity1), then Release-publishes Prepared and DSB SY before
CPU_ON(1, fixed arm_secondary_start, generation). No caller slice/pointer enters
the mailbox. Report contains only owned Copy scalar fields. Generation never wraps or repeats within the Runner lifecycle.

Secondary entry masks DAIF and verifies EL1, full MPIDR affinity1, nonzero X0
context, and SCTLR M/C/I all off before stack access. It sets its reserved SP1,
SP0, dedicated VBAR, immutable secondary TTBR0 root, MAIR/TCR, TLBI/IC barriers,
SCTLR/PAN/CPACR/FPCR/FPSR matching CPU0, and TPIDR to slot1. It reads no mutable
mailbox/local data until coherent normal-memory mappings are enabled. Root and
executable bytes were built/published caches-off by CPU0 in0017; this entry
never rebuilds tables or clears global BSS. The context generation stays in
callee-saved x19 until passed to the Rust adapter.

Wait loops accept only the expected waiting phase or its next phase: CPU0
Prepared→Online and Go→Returned, CPU1 Online→Go. Repeated waiting phases are
allowed; every other phase fails-stop.

Rust adapter Acquire-loads Prepared and validates the atomic generation against
its context; mismatch is fatal. It Release-publishes Online, waits for Acquire
Go, runs a fixed Rust diagnostic function synchronously, writes its owned report
through a raw pointer and returns normally through every Rust frame. CPU0 waits
Acquire Online then Release Go. Assembly alone publishes Returned with STLR
only after the adapter returns, issues DSB SY, and invokes CPU_OFF. Any return
from CPU_OFF is fatal. CPU1 entry and pre-Returned assembly explicitly deassert/disable CNTP, disable
GICC, disable/clear its banked PPI30 and verify disabled/nonpending/inactive
state. No distributor CTLR/SPI bank is changed; reset state is not assumed.
CPU0 requires Acquire Returned and AFFINITY_INFO==OFF
before reading the plain report, clearing only the usable coordinator/emergency stack bytes (not guards), restoring
CPU1 slot canaries, publishing Idle or releasing BUSY. Returned alone never
licenses access. CPU0 does not inspect CPU1 plain state while it runs.

## Architectural and fault diagnostics

The fixed Rust callback records actual MPIDR, TPIDR, TTBR0 and current SP; verifies
CPU1 identity, exact root, coordinator SP bounds, emergency SP, EL1, masked DAIF,
PAN, TCR/MAIR/SCTLR WXN+MMU/cache controls and FP/SIMD enable. Exact checks mask:
bit0 MPIDR=1, bit1 TPIDR and CPU-local identity1, bit2 root, bit3 SP1/SP0 bounds,
bit4 privilege/control state, bit5 guard translation rejection, bit6 owned
arena translation rejection, bit7 fixed Rust callback returned normally. Expected255.

Dedicated secondary vectors are fatal-only; current CPU0 recovery code is never
used by the peer. The callback uses AT S1E1R + ISB + PAR_EL1 observations for its
coordinator guard and VA0x80000000, requiring translation failure with FSC7 and5
respectively. It also translates its usable SP page and requires success with
the expected identity physical page, included in bit3. AT never dereferences a
Rust pointer or generates a recoverable exception. These are CPU1 hardware
translation probes, not actual stack overflow or fault-handler execution. The
probe cannot read/write CPU0's mutable arena descriptors because CPU1 root[2]
is absent. Shared identity RAM remains accessible as documented0017.

## Observability and negatives

A one-CPU/missing unsupported topology emits only HMSMP REJECT TOPOLOGY
and exits unsuccessfully before BEGIN, INFO, CPU_ON or baseline. Runtime PSCI
probe rejection uses the distinct HMSMP REJECT PSCI record.
CPU0 emits BEGIN2, INFO runtime version and three signed feature statuses, then
two SESSION records (generation, cpu1, on0, returned1, off1, checks255, actual
root and SP). Records are emitted only after each peer OFF. Finally PASS lifecycle
and END2 precede the existing39-case baseline under the exact two-CPU metadata
contract; ordinary one-CPU builds remain unchanged. Host compares root/SP with
ELF symbols and checks CPU1 HVC/semihost trace evidence where applicable.

secondary-stale-generation-test and secondary-cpu-off-return-test imply
secondary-tests. CPU0 emits EXPECT STALE-GENERATION or EXPECT CPU-OFF-RETURN
before CPU_ON. The stale fixture passes generation+1, and the adapter detects
mismatch only after coherent startup, terminating via CPU1 semihost exit3.
The off-return fixture follows normal callback return and Returned publication
but injects the CPU_OFF-return path instead of issuing HVC, terminating via
CPU1 semihost exit4. Neither fixture emits PASS or reaches baseline; normal
unexpected faults use the existing failure exit2. Negative priority is stale
before off-return when both features enabled. No peer serial output is added.

Sources: [ARM PSCI DEN0022E](https://developer.arm.com/documentation/den0022/e/)
sections5.2,5.6–5.7,6.4 (including6.4.3),6.6 and existing ARM
translation/exception specifications cited by0014; no Linux source. QEMU virt
TCG secure/virtualization-off with max CPU is the only claimed boot profile.
