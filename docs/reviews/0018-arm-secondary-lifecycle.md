# ARM secondary lifecycle review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness certification. Reviewer performs no builds or guest execution.

## Design verdict

Reviewed [design 0018](../design/0018-arm-secondary-lifecycle.md) before core
implementation. **Ready for the scoped secondary startup/return/OFF increment.**

Requested clarifications are included: runtime PSCI major 1, atomic generation
with permanent monotonic allocation, strict phase transitions, owned scalar
reports, CPU1 local timer/interface/PPI quiescence, and post-OFF cleanup limited
to usable stack bytes. The global lease and validated CPU0 capability exclude
other lifecycle/control operations. The peer never receives allocator, mapper,
borrowed callback or serial authority.

Startup validates identity/context before stack access, then installs the
preconstructed root and coherent mappings before reading mutable publication.
The mailbox's narrow Sync proof depends on phase-controlled payload ownership;
no general scheduler/allocator capability becomes shareable. Normal return from
every Rust adapter frame precedes assembly Returned publication. CPU0 requires
both Returned acquisition and PSCI OFF before payload reads, stack reuse or
inactive-record restoration. Uncertain admission/shutdown retains authority or
fails the VM instead of returning an ordinary cleanup error.

Dedicated fatal-only secondary vectors avoid CPU0 exception-recovery assumptions.
Hardware AT probes distinguish usable stack translation from the level-3 guard
and level-1 arena rejection. They do not exercise actual stack overflow or
recoverable fault handling. Stale-generation and simulated CPU_OFF-return
images must fail with their specific evidence and never reach success/baseline.

Consulted [Arm PSCI DEN0022E](https://documentation-service.arm.com/static/640f584656ea36189d4e94a4),
sections 5.2, 5.6–5.7 and 6.4/6.6: HVC call widths follow the specified function
convention; startup passes the context in X0 with MMU/caches disabled and clean,
coherent cache state. AFFINITY_INFO distinguishes firmware-processed OFF from a
software completion flag. The mailbox ownership protocol above is this
project's design, not a guarantee supplied by the firmware specification.

## Source verdict

Reviewed the frozen implementation against parent `6193bb7`: `secondary.rs`,
secondary assembly, CPU-local ownership helpers, boot/test integration and
actual-type compiler fixtures. **No blocking source finding remains for the
scoped lifecycle increment.** Execution remains a separate gate.

- HVC declares x0–x3 inputs/outputs, C caller-clobbers and memory effects.
  VERSION/FEATURES normalize 32-bit results before unsigned version or signed
  status interpretation. Admission checks actual topology, runtime major 1,
  supported functions and CPU0/TPIDR/masked state. BUSY precedes OFF observation,
  generation allocation and any peer storage mutation. The permanent generation
  counter is incremented under that lease and rejects overflow.
- Entry validates EL1, complete affinity, nonzero context and disabled M/C/I
  before stack use. It selects reserved stacks and fatal-only vectors, installs
  the immutable secondary root, completes translation/instruction barriers,
  enables matching memory/control state and installs slot1 TPIDR before mutable
  peer data is read. It neither clears BSS nor rebuilds tables. Preserved x19
  carries the context into Rust; private assembly helpers use caller-saved
  scratch and do not corrupt it.
- The mailbox has asserted phase/generation/payload offsets. Its sole Sync
  implementation covers atomic control and UnsafeCell payload ownership, not
  an allocator or execution capability. CPU0 preparation, CPU1 raw report write
  and CPU0 raw report read are separated by Release/Acquire transitions and
  OFF. The report is four owned scalar words; no Rust payload reference spans
  peer publication or shutdown. Waits accept only their current or next phase.
- The adapter completes its fixed diagnostic and report store, then returns
  normally. Assembly quiesces CPU1 before publishing Returned with STLR, barriers
  and CPU_OFF. Quiescence checks CNTP mask/disable, GICC disable and banked
  PPI30 disable/pending/active state without shared distributor writes or fake
  deactivation. Every CPU_OFF return terminates. CPU0 requires Returned Acquire
  and firmware OFF before copying the report, clearing usable coordinator and
  emergency stack bytes, restoring slot1 canaries and releasing BUSY.
- After attempted CPU_ON, errors and invalid phases fail-stop; waiting for
  uncertain completion cannot return or release possibly live storage. The
  stale fixture passes a different nonwrapping generation. A review finding
  was corrected: invalid initial phase now exits generically; only an actual
  generation mismatch can produce stale exit 3. The off-return fixture reaches
  its dedicated exit after Rust return and Returned publication, without
  issuing CPU_OFF or authorizing CPU0 cleanup.
- CPU1 reads only its own plain record. AT/PAR probes inspect usable stack
  identity, level-3 guard rejection and level-1 arena rejection through its
  actual root. CPU0 renders observations only after OFF. Feature dispatch runs
  two sessions before the 39-case baseline and retains an exact two-CPU
  topology contract. Actual-type compiler fixtures cover non-Send/non-Sync,
  non-Clone and private construction; their execution belongs to integration.

The host harness inspects record ordering, ELF-derived addresses, post-shutdown
baseline, topology rejection and dedicated negative exits. Any claim of visible
HVC trace counts must first be established against the pinned QEMU trace output;
allowing an event is not evidence it occurred. Source review alone claims no
successful boot, instruction execution, compiler probe or test result.

No concurrent peer scheduler, cross-CPU Event/mutex stress, secondary worker
overflow handling or allocator concurrency is established here. Design 0014 and
the Phase 1 exit gate remain open.
