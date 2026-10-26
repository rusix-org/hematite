# ARM CPU-local software state (single-CPU prerequisite)

Status: independently reviewed for implementation, 2026-10-26; see
[review](../reviews/0015-arm-cpu-local-state.md). A bounded increment of
[0014](0014-arm-smp.md); it does not start CPU1 or establish SMP safety.

Reserve two 128-byte, 64-byte-aligned records in assembly-owned BSS. Rust has
no reference to either mutable record. Exact offsets: timer five u64 words at
0 (delivered, interval, active, deadline, error); preemption four u64 words at
40 (host pointer, task pointer, active, interval); IRQ count at 72; boundary
flags at 80; masked wait expiries at 88; CPU identity at 96; padding to 128.
Rust repr(C, align(64)) layout assertions and global_asm const substitutions
bind assembly offsets to the single layout definition. All previous mutable
timer/preemption globals, including sentinel observations, use TPIDR_EL1 plus
these offsets. Existing instruction scratch registers remain unchanged.

Cold entry validates CurrentEL and MPIDR affinity (Aff3:Aff0 all zero) before
any stack/BSS write. Architectural MPIDR non-affinity bits are ignored. After
BSS clear, CPU0 installs record0's exact base in TPIDR_EL1; its zero identity is
CPU0. Initialize all sixteen words in inactive record1 to a fixed nonzero
canary. No secondary entry installs or accesses record1. Interrupts remain
masked until existing scoped timer/worker entry. Existing SP/exception vectors
and cache/page-table sequencing remain unchanged.

Private Rust validation first checks MPIDR affinity zero and TPIDR exactly
equals record0, then reads its identity with IRQ masked. It never dereferences
an unvalidated base. Rust observations mask IRQ, preserve/restore previous DAIF,
and read only raw scalar fields; no mutable references or whole-record reads
are exposed. CPU0's inactive-record canary diagnostic is the sole exception:
it reads separately owned, never activated record1 with IRQ masked. Checks
exercise candidate bases (null, misaligned, foreign slot) through the same
validation predicate without installing invalid TPIDR. Public Runner run and
run_preemptive boundaries compare the validated current CPU with their stored
boot identity before acquiring BUSY or touching stacks; mismatch returns
ThreadError::Unsupported.
Private rejection fixtures exercise both entry points with foreign and absent
Runner identity before the existing normal and nested-run checks.
Nested callbacks remain supported: affinity observation saves/restores DAIF and
BUSY still rejects nested scheduling. Runner remains non-Send/non-Sync.

Expose cpu_local::checks() to the platform. It validates identity, rejects the
bad candidate bases, verifies all sixteen inactive-slot canaries, and verifies
layout/address separation. Parent invokes it both in baseline and after real
preemption tests, while existing timer/context/IRQ accounting and boundary
negative suites run unchanged. This proves CPU0 uses its own record and leaves
the inactive slot intact, not concurrent CPU behavior.

Remaining hazards are explicit: global BUSY, shared worker/coordinator/emergency
stacks, global GIC distributor setup/stop and single-CPU mapping/allocator
ownership remain unchanged. CPU1 must stay off. A later reviewed increment must
split GIC lifecycle and stacks before allowing a second CPU to execute. Sources
are the existing reviewed designs 0009/0012/0014 and ARM architectural register
contracts already used by those designs; no Linux implementation source.
