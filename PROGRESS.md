# Progress

Current phase: **Phase 1 locally verified within its ARM scope; Phase 2 native functional experiment verified locally, including archive-backed execution; matching CI remains open**.
Aaron approved scratch implementation and then changed the primary objective
from strict Linux parity to native OS design with selective compatibility.
[ADR 0005](docs/adr/0005-native-first-design.md) records the native direction;
[ADR 0006](docs/adr/0006-arm64-priority.md) makes ARM64 primary now.

## Implementation and verification

- Stable Rust 1.99.0 workspace, no external kernel crates, locked dependencies.
- Original ARM64 direct-ELF entry at EL1, 4 KiB translation tables,
  privileged-only W^X/PXN/UXN mappings, PAN, guarded stack and PL011 console.
- x86_64 regression port: original PVH entry, microvm profile, guarded stack,
  supervisor-only 4 KiB mappings, W^X, NX, SMEP, SMAP and CR0.WP.
- Audited boundary structure: `hm-core` owns unsafe operations; `hm-kernel`
  forbids unsafe code. Independent AI design/source review and fixes recorded;
  this is not a human or complete soundness audit.
- Serial monitor: `make run` defaults to ARM64; `ARCH=x86_64` selects the
  regression port. Commands `help`, `status`, `test`; Ctrl-C exits.
- `make test-all`: formatting, Clippy and safety checks for both targets,
  131 host harness tests, ten native-policy host tests, sixteen initramfs and eight boot-image host cases, 41 guest tests per architecture, ARM no-PAN rejection,
  x86 missing-NX/SMEP/SMAP rejection, and live monitor recovery tests on both.
  ARM discovery matches both 128 MiB and 64 MiB guest configurations.
  Separate ARM images run five scheduling/wait cases at both memory sizes, then
  repeat the 39 baseline cases in the same guest. Forced-transition, withheld
  timer delivery, stack-overflow and all-blocked retention profiles also pass.
  Two-CPU lifecycle profiles at both RAM sizes and three admission/fatal profiles
  also pass; successful shutdown is followed by all 39 baseline cases.
  Eleven concurrent scheduling/allocation cases pass at both RAM sizes, with three strict
  negative profiles for withheld peer timer, peer overflow and both CPUs blocked.
  All passed locally; CI at `d13b032` covers the preceding thread milestone.
  `make test` tests ARM64 alone by default.
- Pinned Rust base and Debian package snapshot, QEMU 7.2.22. One CPU/128 MiB
  guest by default; lifecycle and SMP tests use two CPUs with TCG thread=multi. ARM also
  tests 64 MiB; no attached disks or networking. Both targets use TCG.

Latest execution evidence: [archive-backed native processes](docs/evidence/archive-process-execution.md),
with [complete ARM/x86 regressions](docs/evidence/archive-process-final/verification-summary.json)
and an explicit Phase 2 requirement audit. The normal boot remains a kernel monitor.

Earlier input-boundary evidence: [checked initramfs admission](docs/evidence/initramfs-admission.md),
with [complete ARM/x86 regressions](docs/evidence/initramfs-final/verification-summary.json).
That admission-only milestone left the process executable unchanged; execution
from the archive is now separately verified above.

Earlier execution evidence: [protected native processes](docs/evidence/native-process.md),
with [complete ARM/x86 regressions](docs/evidence/process-final/verification-summary.json).
All three process profiles pass 66 runs each; the reviewed executable matches
the tested ELF. Pinned-emulator coverage limitations are recorded explicitly.

Earlier policy evidence: [native call policy](docs/evidence/native-policy.md), with
[complete local regressions](docs/evidence/native-policy-final/verification-summary.json).
Hardware evidence: [owned ARM address spaces](docs/evidence/owned-address-spaces.md),
[compiled stack review](docs/evidence/space-stack-review.md), and
[three guest profiles](docs/evidence/space-first-qemu/result.json). Full ARM/x86
regressions pass in [the final archive](docs/evidence/space-final/verification-summary.json).
Earlier [safe ELF admission](docs/evidence/elf-admission.md) and its matching
ARM/x86 regressions remain archived under `docs/evidence/elf-final/`.
Earlier [bootstrap verification](docs/evidence/hematite-bootstrap.md) is retained.
The earlier [Asterinas baseline](docs/evidence/asterinas-evaluation.md) remains
an evaluation only; it is not this kernel's implementation.

## CPU-local ARM software state verified locally

Timer/preemption bookkeeping now uses aligned CPU records through TPIDR_EL1.
Boot validates CPU0 affinity before shared writes; Runner rejects absent or
foreign CPU identity before acquiring execution authority. IRQ-masked raw
observations preserve DAIF. Inactive-slot canaries remain intact after real
timer and preemption activity, including forced boundaries; actual-type probes
continue rejecting safe Runner transfer and construction. See
[design](docs/design/0015-arm-cpu-local-state.md),
[independent review](docs/reviews/0015-arm-cpu-local-state.md) and
[evidence](docs/evidence/arm-cpu-local-state.md).
CPU1 now runs a fixed lifecycle diagnostic, with its slot restored only after
confirmed OFF. Concurrent per-CPU stack selection and scheduling are verified below.

## ARM GIC ownership verified locally

Preemptive sessions own shared distributor forwarding and SPI state separately
from local GICC/PPI30/timer state. Local teardown preserves shared forwarding.
Two diagnostic local cycles, each including duplicate-setup rejection, preserve
a disabled pending SPI32 until global cleanup. Existing real scheduling and
exact IRQ traces pass. The diagnostic itself dispatches no task or interrupt;
see [design](docs/design/0016-arm-gic-ownership.md),
[review](docs/reviews/0016-arm-gic-ownership.md) and
[evidence](docs/evidence/arm-gic-ownership.md). The older scoped timer diagnostics
still require exclusive whole-controller ownership with CPU1 off. The disabled
SPI diagnostic now explicitly establishes and restores its target on two-CPU
GICs; concurrent interrupt handling is now verified by the SMP gate below.

## Secondary ARM memory verified locally

Six secondary guarded stack reservations and a distinct immutable root now
exist before MMU/cache enable. The root shares device/identity subtrees while
excluding CPU0's owned arena. Exact root entries, every stack leaf, linker
geometry and live frame-pool exclusions pass, including malformed exclusion
queries and an eligible-page counterexample. See
[design](docs/design/0017-arm-secondary-memory.md),
[review](docs/reviews/0017-arm-secondary-memory.md) and
[evidence](docs/evidence/arm-secondary-memory.md). The lifecycle milestone below
installs the root and uses the coordinator stack. The SMP milestone below also
verifies worker stack execution and actual secondary guard overflow.

## Secondary CPU lifecycle verified locally

A private non-Send/non-Sync capability uses a permanent monotonic generation,
Release/Acquire mailbox and PSCI/HVC to start CPU1 twice and confirm OFF before
report consumption or stack reuse. CPU1 runs a fixed Rust diagnostic and returns
normally before assembly publishes Returned and calls CPU_OFF. Actual MPIDR,
TPIDR, TTBR, SP and controls pass; AT/PAR probes validate stack translation and
reject the guard/owned arena. Both RAM profiles pass all baseline cases after
shutdown. One-CPU admission, stale-generation and simulated CPU_OFF-return
profiles fail explicitly with their expected codes. See
[design](docs/design/0018-arm-secondary-lifecycle.md),
[review](docs/reviews/0018-arm-secondary-lifecycle.md) and
[evidence](docs/evidence/arm-secondary-lifecycle.md). This lifecycle evidence precedes the concurrent scheduling milestone below.

## Concurrent ARM scheduling verified locally

Owned job arrays now run simultaneously on CPUs 0 and 1, with separate worker,
coordinator and emergency stacks. Shared interrupt forwarding remains live until
both local runners finish and CPU1 is confirmed OFF. Eight cases at both RAM
sizes cover nonyielding progress/register preservation, 16000 protected updates,
mixed yields and bounded traces, cross-CPU Event races/broadcasts, unrelated
notification and empty reuse. Each CPU's reported interrupts match independent
QEMU IRQ/vector/ack/EOI counts. All 39 baseline cases pass after the final join.

Withheld peer timer and both-CPU blocked fixtures retain live frames until the
bounded host timeout; a real oversized Rust frame on CPU1 faults in its worker
guard at the ELF-identified probe. See [design](docs/design/0019-arm-smp-scheduling.md),
[independent review](docs/reviews/0019-arm-smp-scheduling.md) and
[evidence](docs/evidence/arm-smp-scheduling.md). This establishes the scoped
simultaneous scheduling gate, not native-hardware memory-ordering verification,
task migration, CPU idle, or the whole Phase 1 gate. Concurrent allocation
is verified by the later milestone below.

## Concurrent page-backed allocation verified locally

The actual boot Pool now synchronizes metadata with a private IRQ-masked lock.
Claims commit before zeroing; releases follow byte-borrow termination and typed
destruction. Initialization and destructors run outside the lock. A linear
contiguous-run scan bounds metadata work. Scoped worker access retains the Pool
borrow through normal callbacks and confirmed CPU1 OFF; the higher-ranked
accessor prevents pool/owner/byte borrows escaping. Mapper remains CPU0-only.

At both RAM sizes, eight workers pass mixed frame/block isolation (1024
allocations), two complete exhaustion/reclamation cycles (3314 pages per cycle),
and typed allocation/destructor reentry (1280 allocations, 512 destructor
checks). A retained neighbour and a deliberately forgotten frame remain
unavailable. Masked/enabled IRQ state is preserved on success and failure.
Actual-type clients check 30 frame/heap boundaries, 22 mapping boundaries and
28 thread/Event/scope boundaries. Owner transfer traits are compile-tested;
these workloads allocate/drop each owner on its original CPU.

See [design](docs/design/0020-concurrent-allocation.md),
[independent source/stack review](docs/reviews/0020-concurrent-allocation.md) and
[evidence](docs/evidence/concurrent-allocation.md). The emitted fixed workload
paths conservatively fit below 20 KiB within guarded 64 KiB worker stacks.
This is TCG evidence for shared page-backed ownership, not a scalable allocator,
small-object heap, native ARM memory-ordering proof or published Phase 1 exit.

## Boot-memory discovery verified locally

Safe CPU/PSCI parsing now returns owned, bounded metadata for CPU0 or CPUs0/1.
It validates CPU identities, hierarchy/cell widths, enabled status, duplicate
properties and PSCI compatibility/conduit; malformed or unsupported inputs
return no partial topology. ARM derives this description through the same
existing bounded DTB borrow. Invalid topology leaves no descriptor, rather than
preventing otherwise valid single-CPU memory initialization. The parser itself grants no startup
authority; the lifecycle capability above consumes this metadata. Live boot, malformed fixtures and actual
QEMU one/two/three-CPU trees pass their declared checks; see
[source review](docs/reviews/0014-cpu-topology-source.md) and
[evidence](docs/evidence/cpu-topology.md).

The ARM ELF now starts at `0x40200000`, leaving a 2 MiB DTB prefix. Core reads
only that bounded mapped prefix and returns owned metadata from a safe parser.
It validates RAM/reserved ranges and rejects unsupported memory layouts.
Malformed fixtures, every truncation and deterministic byte mutations run on
both architectures. Discovery describes memory; the allocator below consumes it without expanding
the 16 MiB bootstrap mapping. x86 PVH memory discovery remains unimplemented.
See [design](docs/design/0004-boot-memory.md),
[review](docs/reviews/0004-boot-memory.md) and
[verification](docs/evidence/boot-memory.md).

## ARM frame allocation verified locally

A reviewed core pool allocates uniquely owned 4 KiB pages from the mapped
16 MiB window after excluding the DTB prefix, complete kernel image and all
reservations. Frames zero on allocation and reuse, retain the pool while live,
and return on drop. One permanent construction latch prevents duplicate pools.
Accounting now uses the shared metadata lock described above; frame transfer
traits are checked without weakening ownership or pool lifetimes.

Actual ARM tests cover byte isolation, zeroing, reuse, exhaustion while another
frame remains live, singleton rejection and forgotten-owner retention. The
exhaustion test holds a compact index ledger rather than thousands of frame
objects; it does not touch every claimed page's bytes. Exactly one deliberately
forgotten page remains allocated after the first diagnostic per boot. Repeated
diagnostics preserve the remaining capacity. The combined frame/heap compiler suite now runs 30 safe-client probes
per target using the actual core module.

x86 has no physical allocator yet: its contract case requires explicit
Unsupported, while shared planner and compiler checks run on both targets.
See [design](docs/design/0005-frame-allocation.md),
[review](docs/reviews/0005-frame-allocation.md) and
[evidence](docs/evidence/frame-allocation.md).

## Fallible ARM heap ownership verified locally

The same physical pool now provides contiguous byte blocks and typed owners.
Requests validate size/alignment, claim an entire run transactionally and zero
before publication. Typed owners keep backing bytes private, run T's destructor
before releasing pages, and return the original value on allocation failure.
Reentrant destructors can allocate disjoint memory without held allocator locks.

Tests cover multi-page blocks, over-alignment, zero-size requests, malformed and
unavailable layouts, fragmented free space, byte/typed live-owner preservation,
reuse, borrowed values, exact destruction and value preservation on exhaustion.
One typed owner is deliberately forgotten once per pool in addition to the
frame diagnostic's one page; subsequent tests preserve remaining capacity.

This first heap charges whole pages (including one for zero-size requests).
It has no global allocator or standard alloc-crate collection integration,
and fragmentation may deny requests despite aggregate free capacity. Concurrent
ARM use is verified above; x86 heap requests explicitly return Unsupported.
See [design](docs/design/0006-fallible-heap.md),
[review](docs/reviews/0006-fallible-heap.md) and
[evidence](docs/evidence/heap-ownership.md).

## ARM fixed-site exception recovery verified locally

A separately guarded 16 KiB SP_EL0 emergency stack preserves the interrupted
normal SP_EL1 stack. Recovery requires an exact instruction address, fault
address and syndrome match in a fixed core table; it leaves saved PSTATE intact.
The handler saves all GPRs and performs no FP/SIMD operations or Rust calls.

Actual faults verify null/normal-guard/emergency-guard read rejection, RO-write
rejection and NX execution rejection. A context fixture checks all 31 GPRs,
all 32 Q registers, NZCV, FPCR/FPSR and both stack pointers across recovery.
Separate negative boot images prove fatal handling with an invalid normal SP
and a fault during the emergency-stack save. The SP0 route has a distinct
marker; QEMU exception traces are retained. x86 explicitly lacks this recovery
API; its existing architecture checks still run.
See [design](docs/design/0007-arm-exceptions.md),
[review](docs/reviews/0007-arm-exceptions.md) and
[evidence](docs/evidence/arm-exceptions.md).

## ARM owned data-page mappings verified locally

A 64-page kernel virtual arena consumes frame owners and retains backing until
unmap completes local TLB invalidation. Safe clients access only the mapped
alias; explicit unmap returns the original frame, and Drop invalidates before
reclamation. Forgotten mappings retain their slot and backing, even if the
mapper/pool later disappear. Two permanent table pages remain linker-owned.

Tests cover live contents and protection checks, warmed-slot replacement with
different physical pages, translation faults after unmap/Drop, zeroed physical
reuse, full-arena failure returning the original frame, and one bounded forgotten
mapping per boot. Twenty-one ARM compiler probes check consuming lifetimes and
private authority. There are now three deliberately retained diagnostic pages
per boot across frame, heap and mapping checks. x86 mapping is unsupported.
See [design](docs/design/0008-arm-owned-mappings.md),
[review](docs/reviews/0008-arm-owned-mappings.md) and
[evidence](docs/evidence/owned-mappings.md). Dynamic table allocation, address
spaces, executable mappings and SMP shootdowns remain unimplemented.

## ARM scoped timer IRQ delivery verified locally

The pinned GICv2 profile delivers eight physical timer interrupts per scoped
assembly probe. The handler preserves GPRs on the emergency stack, rearms the
timer and acknowledges/completes interrupt 30. It never calls Rust or touches
allocator/mapping metadata. Rust remains masked around this diagnostic path;
the separate preemptive runner below enables IRQs in worker callbacks.

Runtime checks cover x4–x30, all 32 Q registers, NZCV, FPCR/FPSR and both stack
pointers. Polling scratch x0–x3 preservation is source-reviewed, not a sentinel
test. A second probe withholds PPI enable, requires a bounded counter timeout
and zero delivered interrupts, then checks shutdown. Both probes verify timer,
controller, pending/active and DAIF state before returning. Repeated monitor
tests preserve allocation/mapping counts and continue to pass fault recovery.

Strict host traces require 16 complete IRQ/vector/ack/EOI/return transactions
for boot and 32 for each repeated 64/128 MiB monitor. Nine trace-validator tests
reject missing, malformed, duplicate and reordered records. See
[design](docs/design/0009-arm-timer-irq.md),
[review](docs/reviews/0009-arm-timer-irq.md) and
[evidence](docs/evidence/arm-timer.md).

## ARM cooperative kernel threads verified locally

A boot-authorized runner executes up to four safe Rust callbacks on distinct
64 KiB guarded stacks. Callbacks yield cooperatively; the coordinator dispatches
runnable jobs round-robin and returns only after normal completion of every
callback. Private context records and a global execution lease prevent forged
resumption, overlapping stack use and nested runs. No suspended Rust frames
are discarded. Inputs/results are integers; captured or borrowed closures are
not accepted by this initial internal API.

Tests verify dispatch order, locals and destructors across yields, completion,
capacity rejection before callbacks, trace truncation, repeated runs and
unchanged frame/mapping ownership. Core diagnostics check register sentinels,
actual SP identity on all four stacks, guard translations and Busy rejection.
Nineteen compiler probes now enforce thread/event authority and lifetime boundaries.
The pinned compiler's page probing is checked in emitted IR/assembly; an actual
128 KiB callback frame faults in worker zero's guard and exits fatally.
See [design](docs/design/0010-arm-kernel-threads.md),
[review](docs/reviews/0010-arm-kernel-threads.md) and
[evidence](docs/evidence/arm-threads.md). Cancellation and dynamically allocated thread stacks remain unimplemented.
Concurrent scheduling is verified by the later SMP milestone below.

## Mutex ownership and contention verified locally

Both architectures expose the same allocation-free generic core mutex through
the safe platform interface. Acquire/Release ownership protects values through
a private non-Send/non-Sync guard; cooperative acquisition yields when busy.
Safe tests cover contention, publication, borrowed/non-Copy values, destructor
counts and forgotten guards. Twenty-two compiler clients per architecture check
authority, borrowing and Send/Sync bounds, including Mutex<Cell<T>> sharing.

Four actual ARM workers hold guards across yields while their protected Cell
pair is temporarily inconsistent; only the holder can access it until repaired
and unlocked. Repeated runs complete with exact totals and unchanged resource
accounting. The same core module passed a hosted four-thread, 80,000-update
stress test with forced initial contention and exactly-once payload destruction.
This earlier stress test supplies host concurrency evidence; the SMP milestone
also verifies actual guest contention. See
[design](docs/design/0011-kernel-mutex.md),
[review](docs/reviews/0011-kernel-mutex.md) and
[evidence](docs/evidence/kernel-mutex.md). IRQ-safe locking, wait queues,
preemption-disable ownership and fairness guarantees remain unimplemented.

## Event waits and wakeups verified locally

Preemptive workers can register a borrowed Event and become BLOCKED, excluded
from dispatch until its monotonic sequence changes. Final observation and
registration are IRQ-masked; the scheduler clears its pointer before making the
worker runnable, and the returning worker performs its own Acquire observation.
Notification wakes all matching waiters; exhausted sequences never wrap or admit
another indefinite wait. The cooperative context explicitly returns Unsupported.

Tests cover immediate/stale/terminal observations, multiple waiters, unrelated
events, notifier completion, repeated producer/consumer waits, preserved locals
and destructors, and exact park/Wake dispatch accounting. Forced timer expiry
before registration and while registration is masked passes. A callback-local
event in a separate all-blocked guest retains its stack/borrow until host timeout.
This is not CPU idle, cancellation, device notification, mutex queue integration
or general SMP resource ownership. Cross-CPU Event wakeups are verified below.
See [design](docs/design/0013-arm-wait-wake.md),
[source review](docs/reviews/0013-arm-wait-wake-source.md) and
[evidence](docs/evidence/arm-wait-wake.md).

## Limits and gates

ARM's separate preemptive runner now suspends arbitrary safe function-pointer
callbacks through timer IRQs, preserving full register images in task-owned
storage. The shared emergency stack is released before coordinator execution.
Four nonyielding workers, mixed yields, mutex ownership during preemption,
destructors, context sentinels and exact trace accounting pass. Forced expiry
hits all three start/yield/completion windows. No public interrupt-mask guard
is exposed; allocator/mapping capabilities remain unavailable to workers.
See [design](docs/design/0012-arm-preemption.md),
[source review](docs/reviews/0012-arm-preemption-source.md) and
[evidence](docs/evidence/arm-preemption.md). The later SMP milestone verifies simultaneous execution; general wait queues
and SMP-safe mapping ownership remain ahead. Shared allocation is verified above.

No userspace, syscall ABI, general interrupt/exception handling, storage,
virtio drivers or networking exists. ARM recovery is restricted to the fixed
EL1 probe sites above; general user-copy and process-fault handling remain
unimplemented. x86 guards are checked through mappings and unexpected x86
exceptions may triple-fault. ARM bootstrap isolation checks are
not claims about a future EL0 address space or equivalence to SMEP. No Linux
compatibility claim exists.
The safe capability model now checks bounded resource accounting, attenuated
rights, stale generations and cancellation/completion lifetimes. Five compiler
probes per target verify the safe-client type boundary. This is metadata only: owner
labels are trusted inputs and completion is a simulated backend acknowledgment.
See [model verification](docs/evidence/capability-model.md).

Published kernel CI passed at `d13b032` ([run 40324819664](https://github.com/rusix-org/hematite/actions/runs/40324819664)),
including capability, memory, heap, exception, mapping, timer and cooperative threads.
The exact commit/job result and raw logs are archived in
[thread verification](docs/evidence/arm-threads.md). Aaron authorized public repository creation
and push in [ADR 0007](docs/adr/0007-public-repository.md). License selection
remains pending (ADR 0002); preliminary naming findings remain in ADR 0003.

Mutex source/evidence is committed locally as `ee6b8a4`. Automatic approval
review rejected its public push, requiring direct authorization in the current
conversation despite ADR 0007. No push retry or remote mutex CI run has occurred;
publication awaits Aaron's response. Local kernel development remains authorized.

## Phase 1 audit and first userspace preparation

The [Phase 1 audit](docs/evidence/phase1-audit.md) matches the local ARM
mechanisms to actual source hashes, strict guest results and raw traces. It
finds evidence for the bounded local functional exit clauses, while the formal
CI gate remains open at the earlier published milestone. Small-object heaps,
CPU idle and task migration are not prerequisites for one CPU0-only process.

[Design 0021](docs/design/0021-arm-first-userspace.md) separates safe ELF
admission from owned page tables, EL0 exception entry, native calls and teardown.
Its parser stage is independently reviewed and verified: bounded ELF64/LE
AArch64 static plans, checked ranges/permissions/entry, ignored section tables,
private metadata and immutable input lifetimes. A real separately linked ELF
matches readelf; 32 invalid mutations, all 13649 prefixes, 4096 deterministic
random/mutated inputs and file-capacity boundaries pass. Eleven actual-library
compiler clients pass on each target. This is admission evidence, not loading
or EL0 execution. See [verification](docs/evidence/elf-admission.md).
[Design 0022](docs/design/0022-arm-owned-address-spaces.md) now implements owned
CPU0 user spaces and bounded assembly-only translation probes. Actual ELF bytes,
BSS, permissions and stack pages load into owned frames; private roots protect
RX identity aliases and restore the boot root before Rust resumes. Three ARM
profiles pass five ownership cases, 142 before/active/after translation queries,
and 41 baseline cases each. All budget prefixes and real allocator exhaustion
roll back; maximum-size and forgotten-owner cases pass. Sixteen compiler clients
verify authority/lifetime boundaries. See [evidence](docs/evidence/owned-address-spaces.md).
That milestone used SP_EL0 only as a kernel emergency stack. Design 0023 now
adds a separate trusted exception frame, user stack and instruction-cache
protocol for actual protected execution; checked archive input now follows
designs 0024/0025.

The versioned native console/exit policy is now implemented in safe Rust and
verified with ten host cases and twelve compiler clients per target. It defines
startup bytes, scoped generation handles, all-or-nothing copy-before-output,
independent byte/call limits and terminal revocation. The 64th returning call
records a Reply and stops; replies never grant hardware resume authority.
[Evidence](docs/evidence/native-policy.md) explicitly excludes the real user-copy
adapter and EL0 execution. Those now have separate
[native process evidence](docs/evidence/native-process.md): 33 real Rust ELF
fixtures run twice on each of three ARM profiles. Strict traces verify native
calls, protection faults, timer termination, terminal no-resume behavior and
reclamation. Nineteen compiler clients check the actual process API; preparation
budget/exhaustion failures and inactive drop preserve ownership. Kernel source,
compiled stacks and actual traces have independent AI review. Pinned-QEMU DAIF
syndrome and DIT coverage limitations are explicit; no hardware or Linux claim.

The bounded newc initramfs parser is implemented and independently reviewed.
Sixteen host tests validate complete admission before lookup, GNU cpio reference
payloads, traversal/alias rejection, all declared limits and borrowed lifetimes.
Eleven compiler clients per target check encapsulation and input ownership.
That parser-only milestone is recorded in [archive evidence](docs/evidence/initramfs-admission.md).
The process profile now selects every program and preparation input by exact
archive path. Four admission failures precede allocation, and the full 66-run
matrix passes at all three configurations. Exact embedded bytes, kernel address
bindings, compiled stacks and raw traces were independently checked. See
[execution evidence](docs/evidence/archive-process-execution.md).

## Next actions

1. Design and independently review an ordinary ARM monitor launcher with archive
   listing, exact program selection, readable output and prompt recovery after
   exit/fault/deadline. Retain one Executor/issuer across launches and preserve
   all existing isolation gates. Concurrent userspace remains outside scope.
2. Use these experiments to refine the native wire contract; select compatibility
   workloads and pin a Linux oracle when the adapter exists.
3. Resolve the license selection; keep remote CI current as further milestones are published.

## Session ledger

- 2026-10-25: Evaluated Asterinas, recorded proposed decisions and initial CI;
  upstream build/boot baseline committed as `2492ebb`.
- 2026-10-25: Aaron selected scratch and requested a runnable kernel. Wrote and
  independently reviewed bootstrap design. Fixed hardware-mutated page-table
  declarations and strengthened alias checks before boot. Preserved initial
  code-model/LTO build failure; corrected target configuration.
- 2026-10-25: Original kernel booted, then passed the complete suite in the
  smaller pinned environment. Exercised `make run` interactively and exited
  cleanly. No host boot configuration or kernel changed.
- 2026-10-25: Aaron revised the objective to explore native Rust OS designs
  with best-effort Linux compatibility. Updated AGENTS/ADRs and proposed a
  measurable capability/lifetime experiment. No performance advantage claimed.

- 2026-10-25: Aaron explicitly authorized public `rusix-org/hematite` creation
  and push. Recorded ADR 0007; preserved existing ARM64 work and evidence.
  Publication checks: 12 host harness tests, repository and safety hygiene,
  and whitespace checks passed. No kernel changes or fresh QEMU run in this
  publication session; existing dual-architecture logs are preserved.

- 2026-10-25: Aaron prioritized ARM support. Added an original reviewed ARM64
  bootstrap and made it the default. Fixed review findings around CurrentEL,
  WXN observations and address-only linker symbols. Final dual-architecture
  suite and interactive ARM monitor passed. Physical ARM and native ARM/KVM
  performance remain untested. This implementation session made no publication
  changes; separate publication authorization/work is recorded in ADR 0007.

- 2026-10-25: Implemented and independently AI-reviewed the safe capability
  model. Both architecture suites pass 14 guest cases and five compiler probes,
  including all 24 revoke/release/cancel/complete permutations. Preserved initial
  Clippy failure and final evidence. No unsafe/core or hardware changes.

- 2026-10-25: Implemented reviewed ARM DTB discovery and safe parser. Fixed
  missing DTB by relocating the ELF; documented QEMU property-padding deviation.
  A fixture stack overflow hit the guard and was fixed by bounded test frames.
  Final dual suite passes 16 guest cases; ARM reports configured 64/128 MiB RAM.
  Preserved failures, exception trace, source hashes and final evidence.

- 2026-10-25: Implemented independently reviewed ARM frame pool and safe
  planning/API boundary. Fixed safety-comment checker findings; full dual suite
  passes 19 guest cases and ten frame compiler probes per architecture.
  ARM 64/128 MiB monitor profiles independently verify the eligible count
  against the ELF end marker. x86 allocation remains explicitly unsupported.

- 2026-10-25: Implemented independently reviewed fallible page-backed heap and
  typed ownership. Final dual suite passes 21 guest cases and 22 frame/heap
  compiler probes per target. Verified failure atomicity under fragmentation,
  borrowed values, reentrant destruction and original-value return on exhaustion.
  Page-level allocation cost, diagnostic leaks and missing global allocator are
  explicit; no workload or performance advantage claimed.

- 2026-10-26: Added reviewed ARM fixed-site EL1 recovery, dedicated exception
  stack and live protection/context tests. All 22 guest cases and 13 host
  harness tests pass on both targets; ARM invalid-normal-stack and nested-fault
  images exit through their required fatal routes. Preserved raw QEMU exception
  traces, exact source/ELF identities and compiler evidence.

- 2026-10-26: Implemented independently reviewed ARM owned data mappings with
  completed invalidation before backing reclamation. Dual suite passes 25 guest
  cases and 13 host tests; final 21 mapping compiler probes pass. Preserved raw
  results and source identities. The bounded arena uses permanent table storage;
  no general address-space teardown or concurrency support is claimed.

- 2026-10-26: Implemented reviewed scoped ARM timer IRQs, bounded disabled-PPI
  timeout and context/quiescence checks. Made GICv2 explicit and captured its
  DTB. Review prompted explicit Group0 configuration. Final dual suite passes
  26 guest and 22 host cases; independent trace checks pass exact 16/32 IRQ
  transactions. No preemption, SMP or IRQ-enabled Rust execution is claimed.
- 2026-10-26: Published through timer commit `8014e2f` under ADR 0007 authorization.
  GitHub run `40320742456` completed successfully, including environment build,
  all tests and evidence upload. Archived exact run/job metadata and raw logs.
- 2026-10-26: Added reviewed cooperative ARM threads with four guarded stacks.
  Boot passed initially; monitor testing found stack exhaustion from inlined
  test fixtures. Separating the test function fixed it without enlarging stacks.
  Final dual suite passes 29 guest/23 host cases, 14 thread compiler probes,
  compiler stack-probe checks and actual worker guard-overflow rejection.
  Preserved initial failures, final traces and exact source/ELF identities.
- 2026-10-26: Published thread commit `d13b032`; GitHub run `40324819664`
  passed the complete environment/build/test job and evidence upload. Archived
  exact commit/job metadata and raw logs. Phase 1 remains incomplete without
  preemptive multi-CPU execution and its required stress/synchronization tests.
- 2026-10-26: Added reviewed mutex/guard ownership on both architectures and
  actual contending ARM workers. First full suite passes 32 guest/23 host cases,
  22 new compiler clients per target and hosted four-thread 80,000-update stress.
  Preserved exact source, commands, host binary hash and raw results. Ordinary
  cooperative mutexes do not authorize blocking acquisition in interrupt handlers.
- 2026-10-26: Audited the existing scheduler/IRQ ownership boundaries and completed
  independently reviewed design 0012 for actual timer-driven ARM preemption.
  Review clarified fresh coordinator saves on every dispatch, emergency-stack
  reclamation, cooperative mask preservation and timer setup/teardown failures.
  This is design evidence only: no preemption implementation or runtime tests
  added. Public mutex push remains pending approval; local work continues.
- 2026-10-26: Implemented and independently source-reviewed ARM preemption.
  Initial compilation exposed PAN encoding/FFI issues; first IRQ exposed an
  invalid EL1 banked-SP read, fixed with a memory-free SPSel observation. Fixed
  all-features negative-fixture selection. Final full suite passes 40 host and
  32 baseline guest cases per architecture, four ARM scheduling cases plus
  same-guest regressions, forced-transition delivery and both negative profiles.
  Preserved failures, exact source/ELF hashes and full traces. Remote publication
  remains pending approval; no push retry occurred.
- 2026-10-26: Implemented independently reviewed borrowed Event waits and actual
  BLOCKED/Wake scheduling. First guest and full suites passed; initial compilation
  warned about an atomic API renamed in the pinned compiler, corrected before
  strict lint verification. Final suite passes 41 host/33 baseline guest cases,
  five ARM scheduling cases, park-boundary pressure and all-blocked retention.
  Added five actual-type compiler clients (19 thread/event total). Archived exact
  source/ELF identities and full logs; no remote publication attempted.
- 2026-10-26: Completed independent prerequisite audit and design 0014 for
  true two-CPU preemptive execution. Captured the pinned QEMU two-CPU DTB without
  executing a kernel: affinities 0/1 and PSCI/HVC are described by the profile.
  [Configuration evidence](docs/evidence/smp-investigation/README.md) includes
  raw bytes, commands and reproducible decoding. Review required per-CPU stacks/
  counters, immutable secondary tables excluding the owned arena, shared GIC
  ownership and confirmed CPU_OFF before reuse. No CPU_ON, SMP implementation
  or multicore execution verification occurred in this design increment.
- 2026-10-26: Implemented reviewed safe CPU/PSCI topology parsing and extraction
  through the existing bounded ARM DTB borrow. First suite stopped at a Clippy
  style error; fixed it and strengthened legacy-alias/malformed fixtures after
  review. Final full suite passes 41 host/36 baseline guest cases per target,
  all scheduling/negative regressions, and actual QEMU trees accepting one/two
  CPUs and rejecting three. Archived source/ELF identities, raw DTBs and logs.
  This is descriptive metadata only; secondary startup remains unimplemented.
- 2026-10-26: Implemented independently reviewed CPU-local ARM timer/preemption
  records, boot affinity validation and Runner CPU association checks. First full
  suite passes 41 host/37 baseline guest cases per target and all existing
  scheduling, forced-boundary, monitor and negative profiles. Inactive CPU1
  canaries remain intact after actual IRQ activity; CPU1 was never started.
  Archived exact source/ELF identities and raw logs. No public push attempted.
- 2026-10-26: Split reviewed preemption GIC ownership into shared session and
  CPU-local lifecycles. First full suite passes 41 host/38 baseline guest cases
  per target and every existing scheduling/negative profile. New repeated local
  setup/stop fixture preserves shared forwarding and disabled pending SPI32;
  duplicate local setup rejects, and global teardown clears the sentinel.
  Archived exact source/ELF identities and raw logs. CPU1 remains off and public
  publication remains pending; no push was attempted.
- 2026-10-26: Reserved six guarded secondary stacks and an immutable secondary
  root excluding the owned arena. Reviewed diagnostics validate exact geometry,
  descriptors and live frame-pool exclusion, including invalid-query rejection.
  First suite stopped at Clippy identity_op; simplified the equivalent constant.
  Second full suite passes 41 host/39 baseline guest cases per target and all
  scheduling/negative regressions. Archived failures, source/ELF identities and
  raw results. CPU1 never installed the root or used these stacks.
- 2026-10-26: CPU1 now starts, runs a Rust diagnostic and shuts down twice via
  PSCI with generation checks and confirmed OFF before reuse. First guest passed
  lifecycle but exposed a one-CPU target assumption in the old GIC sentinel;
  reviewed explicit target setup/restoration fixes it. Retained failed guest,
  register traces and a failed halted-QEMU inspection. First full suite stopped
  at comment placement; corrected it. Final full suite passes 53 host/39 baseline
  guest cases, 23 actual thread/event/secondary type clients, every prior profile
  and five lifecycle profiles. Actual two-CPU traces/ELF identities archived;
  concurrent preemptive workers remain the next design0014 gate.

- 2026-10-26: Verified concurrent ARM scheduling under design0019: eight cases
  on two CPUs at both RAM sizes, three negative profiles and post-join baseline.
  Full regression evidence and the corrected Clippy failure are retained in
  [SMP verification](docs/evidence/arm-smp-scheduling.md). Pool/Mapper remain
  CPU0-only; concurrent allocation is the next design task. No public push.

- 2026-10-26: Verified shared boot-pool allocation under design0020, including
  eight-worker retained owners, exact two-cycle exhaustion, zeroing/isolation,
  typed destructor reentry, scoped lifetime rejection and IRQ-state restoration.
  Full ARM/x86 suite passed before and after deterministic peer-reuse
  strengthening; final compiler assembly and
  independent stack review match the tested ELF. See
  [allocator evidence](docs/evidence/concurrent-allocation.md). No public push.

- 2026-10-26: Audited bounded Phase 1 local evidence against ac10326; formal
  matching CI remains absent. Implemented the independently approved ELF
  admission stage of design0021 and real linked-fixture/readelf verification.
  Full ARM/x86 suite passes 41 guest cases and all prior profiles; first suite
  stopped on two test-only Clippy warnings, preserved and corrected. Protected
  runtime stages remain proposed. See [ELF evidence](docs/evidence/elf-admission.md).

- 2026-10-26: Implemented independently reviewed owned ARM spaces and bounded
  installed-translation probes (design 0022). All three QEMU profiles pass five
  ownership cases, 142 PAR triples and 41 baseline cases; all 33 failure budgets,
  real exhaustion, maximum loading and forgotten ownership are covered. Sixteen
  compiler clients and the full ARM/x86 regression suite pass. Preserved the
  initial PAN encoding failure, Clippy warnings, and an oversized compiled stack
  caught before boot; split test cases resolve it. Exact source/ELF identities,
  assembly, raw logs and review are archived. EL0 execution remains next; no
  remote push or new CI claim. See [evidence](docs/evidence/owned-address-spaces.md).

- 2026-10-26: Designed the protected native process transition and strict
  execution evidence, and implemented its independently reviewed safe wire
  policy. Ten policy tests, twelve compiler clients per target and all ARM/x86
  regressions pass; 97 Python harness tests include seven new strict-inventory
  cases. Fixed ambiguous call-limit resume semantics before integration; the
  64th reply immediately revokes the grant and stores termination. Archived raw
  logs/source identities. Core EL0 execution remains implementation work, with
  no new kernel entry/exception code or remote publication in this milestone.

- 2026-10-26: Implemented reviewed protected native ARM ELF execution. Three
  profiles pass 66 runs each, including actual console/exit calls, invalid
  arguments and handles, bounded authority, protection faults, timer deadline,
  register restoration and exact reclamation. Source/compiled-stack/trace review
  preceded acceptance. Preserved fixture/build/lint failures and first guest
  validator failures; resolved exact BTYPE and pinned-QEMU DAIF expectations
  without changing runtime code. Explicitly recorded QEMU's DIT coverage limit.
  Full ARM/x86 suite passes 115 Python cases per target, 19 process compiler
  clients and all prior profiles. See [evidence](docs/evidence/native-process.md).
  Initramfs remains next; no new remote CI or public push.

- 2026-10-26: Implemented reviewed allocation-free newc archive admission with
  complete validation before lookup. Deterministic and independently GNU-built
  archives match payloads; sixteen real parser tests and eleven compiler clients
  per architecture pass. Full ARM/x86 suite passes 123 Python tests per target
  and all prior runtime profiles; reviewed process ELF remains byte-identical.
  Preserved initial formatting and test-only Clippy failures. The positive
  directory-link profile was settled before implementation from actual GNU
  filesystem behavior. See [evidence](docs/evidence/initramfs-admission.md).
  Archive-backed execution remains next; no new remote CI or push.

- 2026-10-26: Connected checked archives to all native process and preparation
  paths. Four preallocation errors preserve frames; all three profiles pass
  66 runs with exact reclamation and unchanged hardware transaction rules.
  GNU payload extraction and unique read-only kernel embedding checks exclude
  individual ELF fallbacks. One rebinding iteration stabilized BSS40393000;
  source/stack review preceded guests. Full ARM/x86 suite passes 131 Python
  cases per target, eight composition tests and six type clients per target.
  Local native Phase 2 requirements audited; matching CI and Linux adapter
  remain absent. Next is ordinary monitor launch. See [evidence](docs/evidence/archive-process-execution.md).
