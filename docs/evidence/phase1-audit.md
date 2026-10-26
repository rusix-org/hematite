# Phase 1 evidence audit

Audited 2026-10-26 at `ac10326`. The preceding goal turn is **progress**:
it committed shared allocator implementation and matching execution evidence.
This audit makes no Phase 1 completion claim and performs no new build or VM run.

All 87 entries in [the final source manifest](allocation-final/source-sha256.txt)
match the `ac10326` Git objects. Serial/exception artifact hashes for all five
SMP profiles match their result records; re-running `test_smp.validate` over
those archived logs passes both positive and all three negative profiles.
The [full command output](allocation-full-suite-attempt2.log), environment,
expanded guest commands and exact ELF identities remain the execution evidence.

| Phase 1 requirement | Actual implementation and evidence | Status and limits |
| --- | --- | --- |
| Physical frame allocator | `frames.rs` shares the actual boot Pool; both [SMP profiles](allocation-final/aarch64/smp/result.json) retain 3314 real frames per exhaustion cycle, reclaim exactly that capacity twice, and preserve live/forgotten allocations. | Verified locally on ARM's bounded 16 MiB managed window. x86 runtime allocation explicitly returns Unsupported; compiler checks are not physical-allocation evidence. |
| Paging and kernel address space | Immutable privileged identity mappings, W^X/PAN/guards, owned CPU0 mappings and secondary root exclusions; `mapping_hardware`, `secondary_memory` and fault cases appear in the [39-case baseline](allocation-final/aarch64/boot/result.json). | Verified scoped kernel mappings. No user address space, executable user mapping or cross-CPU mapping updates. |
| Kernel heap | Same Pool provides checked contiguous blocks and typed owners; 1024 mixed allocations and 1280 typed/reentrant allocations pass per SMP profile. `heap_blocks`/`heap_values` retain existing failure, alignment, leak and destructor coverage. | Verified page-backed heap. Size classes and standard collections are absent; these are efficiency/integration work, not missing page ownership. |
| Exceptions and interrupts | `exceptions-aarch64.S` recovers only whitelisted EL1 probe faults; timer IRQ context preservation passes. Unexpected, nested and stack-overflow [profiles](allocation-final/aarch64/profiles/result.json) fail explicitly. | Verified fixed kernel contract. General user traps and process-local fault termination are absent. |
| Timer | ARM physical timer/PPI30/GICv2 drives both CPUs; [64 MiB](allocation-final/aarch64/smp/smp-64m/result.json) has 169/147 and [128 MiB](allocation-final/aarch64/smp/smp-128m/result.json) 168/145 per-CPU IRQ/vector/ack/EOI transactions. | ARM-specific realization under ADR 0006. The founding “APIC timer” wording is x86-specific; no x86 APIC/preemptive-scheduler parity is established. |
| Kernel threads and preemptive scheduler | Four guarded callbacks per CPU; nonyielding progress barriers, full context probes, mixed yields, normal drops, bounded trace truncation and repeated reuse. Oversized input rejects before admission; baseline also tests Busy rejection. | Verified bounded scheduler capacity/exhaustion and concurrent stress. No dynamic stacks, captured jobs or migration. |
| Spinlocks, mutexes and waits | Private IRQ-masked metadata spin lock; shared Mutex<Cell<_>> contention; Event blocking/rescanning with before/during/after notification, broadcast and unrelated-event cases. | Verified scoped mechanisms. Event is a monotonic observation, not a queued permit; all-blocked coordinators spin and retain borrows. Device waits, idle wakeups and fairness are not established. |
| SMP bring-up | PSCI start/stop and owned mailbox, per-CPU stacks/state, shared/local GIC ownership; [lifecycle](allocation-final/aarch64/secondary/result.json) and all eleven concurrent cases pass, followed by the baseline after OFF. | Verified two-CPU MTTCG profile at both RAM sizes. No native ARM/KVM memory-ordering or performance claim. |
| Exit gate in CI | Local `make test-all` passed; current workflow invokes that command and preserves artifacts. [Archived remote metadata](threads-ci-result.json) proves success at `d13b032`, the earlier cooperative-thread milestone. | **Not closed.** No recorded remote CI result verifies `ac10326`. Local success must not be relabelled as CI completion. |

The local functional exit clauses have concrete evidence for the declared
bounded ARM target: actual simultaneous preemption plus allocator/scheduler
exhaustion and concurrency. That does not erase the CI requirement, x86 scope
qualification, or any later-phase requirement. No additional missing local
Phase 1 mechanism was identified that must precede a bounded first ARM process.

## Next work that is actually required

Prepare an independently reviewed first-userspace design, then implement its
safe parsing/policy pieces and trusted ownership boundary. Existing lower-EL
vector slots all route to fatal handling, and SP_EL0 currently names the kernel
emergency stack. Entering EL0 therefore requires a new trap/stack protocol;
reusing that register as a user stack without redesign would invalidate the
current exception proof.

The first process also needs owned user page tables/frames with permissions,
static ELF validation/loading, protected entry/return, bounded fault-aware user
copy, versioned startup/console/exit operations and owned-handle validation.
Termination must restore kernel execution and reclaim frames only after user
access is quiescent. Tests must establish bad-pointer/handle containment and
repeated reclamation, not merely a successful print. Initramfs remains a Phase 2
requirement; an embedded test ELF alone would not complete that phase.

A CPU0-only first process can leave CPU1 confirmed OFF and preserve the existing
Mapper confinement. Small-object allocation, CPU idle/IPIs, migration,
cross-CPU mappings and broad device interrupts are useful later work, but are
not prerequisites for that bounded ownership model. Keep Phase 1's formal gate
open until authorized publication and matching CI evidence exist; meanwhile,
local first-userspace preparation advances the existing objective without
claiming a completed phase. Linux hello-world remains a separate compatibility
claim, never implied by native execution.

## Rechecking this audit

The source comparison is against the audited commit, so unrelated later edits
do not silently redefine the claim. This read-only command checks the manifest
and the five archived SMP profiles without compiling or launching QEMU:

```sh
python3 - <<'PY'
import hashlib, json, subprocess, sys
from pathlib import Path
root = Path('docs/evidence/allocation-final')
for line in (root / 'source-sha256.txt').read_text().splitlines():
    expected, name = line.split('  ', 1)
    data = subprocess.check_output(['git', 'show', 'ac10326:' + name])
    assert hashlib.sha256(data).hexdigest() == expected, name
sys.path.insert(0, 'tools')
import test_smp
for name in ['smp-64m', 'smp-128m', 'no-peer-timer', 'peer-overflow', 'all-blocked']:
    directory = root / 'aarch64/smp' / name
    result = json.loads((directory / 'result.json').read_text())
    for key, filename in [('serial', 'serial.log'), ('exceptions', 'exceptions.log')]:
        assert hashlib.sha256((directory / filename).read_bytes()).hexdigest() == result['artifacts'][key]['sha256']
    checked = test_smp.validate((directory / 'serial.log').read_bytes(),
        (directory / 'exceptions.log').read_bytes(), result, result['symbols'],
        None if name.startswith('smp-') else name)
    assert checked['passed'], name
print('Audited source and five SMP profiles match')
PY
```

The validator here is the checked-out tool; reproduce its original semantics
with the `ac10326` checkout. This audit did not query live GitHub state and makes
no claim that archived metadata is a newly observed remote run.
