# Progress

Current phase: **Phase 1 underway: single-CPU ARM timer preemption verified locally**.
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
  40 host harness tests, 32 guest tests per architecture, ARM no-PAN rejection,
  x86 missing-NX/SMEP/SMAP rejection, and live monitor recovery tests on both.
  ARM discovery matches both 128 MiB and 64 MiB guest configurations.
  Separate ARM images run four preemption cases at both memory sizes, then
  repeat the 32 existing cases in the same guest. Forced-transition, withheld
  timer delivery and preemptible stack-overflow profiles also pass.
  All passed locally; CI at `d13b032` covers the preceding thread milestone.
  `make test` tests ARM64 alone by default.
- Pinned Rust base and Debian package snapshot, QEMU 7.2.22. One CPU/128 MiB
  guest by default; ARM discovery also tests 64 MiB; no attached disks or networking. Both targets use TCG.

Latest evidence: [preemption verification](docs/evidence/arm-preemption.md),
[ARM scheduling results](docs/evidence/preemption-final/aarch64/preemption-128m/result.json),
[ARM profiles](docs/evidence/preemption-final/aarch64/profiles/result.json),
and matching x86 results under `docs/evidence/preemption-final/x86_64/`.
Earlier [bootstrap verification](docs/evidence/hematite-bootstrap.md) is retained.
The earlier [Asterinas baseline](docs/evidence/asterinas-evaluation.md) remains
an evaluation only; it is not this kernel's implementation.

## Boot-memory discovery verified locally

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
Cell-based accounting is single-CPU only; frames cannot be sent across threads.

Actual ARM tests cover byte isolation, zeroing, reuse, exhaustion while another
frame remains live, singleton rejection and forgotten-owner retention. The
exhaustion test holds a compact index ledger rather than thousands of frame
objects; it does not touch every claimed page's bytes. Exactly one deliberately
forgotten page remains allocated after the first diagnostic per boot. Repeated
diagnostics preserve the remaining capacity. The combined frame/heap compiler suite now runs 22 safe-client probes
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
and fragmentation may deny requests despite aggregate free capacity. It is
single-CPU only. x86 heap requests explicitly return Unsupported.
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
Fourteen compiler probes enforce the public authority/lifetime boundary.
The pinned compiler's page probing is checked in emitted IR/assembly; an actual
128 KiB callback frame faults in worker zero's guard and exits fatally.
See [design](docs/design/0010-arm-kernel-threads.md),
[review](docs/reviews/0010-arm-kernel-threads.md) and
[evidence](docs/evidence/arm-threads.md). SMP, cancellation and dynamically
allocated thread stacks remain unimplemented.

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
This is host concurrency evidence, not guest SMP. See
[design](docs/design/0011-kernel-mutex.md),
[review](docs/reviews/0011-kernel-mutex.md) and
[evidence](docs/evidence/kernel-mutex.md). IRQ-safe locking, wait queues,
preemption-disable ownership and fairness guarantees remain unimplemented.

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
[evidence](docs/evidence/arm-preemption.md). This does not satisfy Phase 1's
multi-CPU gate; wait queues, SMP-safe resource ownership and SMP remain ahead.

No userspace, syscall ABI, general interrupt/exception handling, SMP, storage,
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

## Next actions

1. Design/review wait/wake queues and SMP-safe resource/scheduler ownership before
   enabling additional CPUs. Preserve the restricted allocator/mapping capability
   boundary until its synchronization is implemented. Extend recovery to fault-aware user operations only with
   address-space and lifetime proofs. Add interrupt/SMP-safe ownership before concurrency;
   implement measured small-object allocation before service workloads.
2. Define the native userspace wire contract from those experiments; select
   compatibility workloads and pin a Linux oracle when the adapter exists.
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
