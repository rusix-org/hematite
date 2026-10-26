# ARM preemption prerequisite audit

Date: 2026-10-26. Independent AI source audit of commit `ee6b8a4`;
not human approval or a complete soundness certification. No kernel builds,
guest execution or runtime implementation performed for this audit.

## Existing source findings

1. **The cooperative context is insufficient for interruption.**
   `threads.rs::Context` and `threads-aarch64.S::arm_context_switch` preserve
   the ordinary call ABI, not all live machine state. A timer can interrupt
   between any two callback instructions. The preemptive path must preserve
   x0–x30, all Q0–Q31, FPCR/FPSR, interrupted SP, ELR and the complete saved
   PSTATE. An ordinary Rust call to the current switch cannot provide that
   contract. Register tests must cover caller-clobbered registers and vector
   upper halves while another worker changes them.

2. **The emergency stack cannot retain suspended interrupt frames.**
   `timer-aarch64.S::arm_timer_irq` selects SP_EL0, allocates 256 bytes and
   restores that allocation only immediately before ERET. ELR/SPSR remain in
   CPU registers. This works because the current handler neither calls policy
   nor dispatches another worker. Suspending this handler and allowing another
   worker to interrupt would retain shared emergency-stack state and overwrite
   exception return registers. Copy asynchronous state to exclusively owned
   per-task storage and restore the emergency-stack top before leaving the
   handler. Handler faults must retain the existing stack-independent fatal
   route; no recoverable nested scheduling is implied.

3. **Scheduler transitions must become indivisible with respect to IRQ.**
   `Token::yield_now` writes RUNNABLE before saving its continuation;
   `arm_thread_start` writes DONE before switching off its worker stack.
   The coordinator marks RUNNING before restoring that context. Enabling IRQ
   indiscriminately makes these intermediate states visible and can permit
   resumption from an incomplete context or cleanup of a still-active stack.
   Mask IRQ before any scheduler metadata transition, keep the coordinator
   masked, and restore the intended worker state only through a fully specified
   assembly transition. Do not keep Rust references into scheduler-mutated
   storage live across callback execution or a switch.

4. **The diagnostic timer is not yet a scheduler timer.**
   The current handler requires an active diagnostic lease, saved EL1h state
   with only IRQ unmasked, and an interrupted PC inside its fixed assembly
   window. It stops after eight deliveries. Merely enabling IRQ in Rust will
   take a fatal path. Add an exclusive scheduler mode with its own checked
   authority/lifetime and teardown, retaining the diagnostic path and its
   existing tests. Complete source deassertion and GIC acknowledgement before
   dispatch; do not hold an active interrupt across arbitrary worker execution.

5. **Cell accounting is conditional on capability reachability.**
   `Pool::claim`, `claim_run`, `release` and `release_run` use whole-ledger
   read/modify/write. Mapper allocation/release similarly mutates Cell state
   and hardware tables. If two preemptible workers could invoke these APIs on
   the same instance, stale ledgers could grant overlapping ownership or lose
   allocation bits. That is not a demonstrated reachable race in the current
   public worker API: jobs carry function pointers and integers, cannot capture
   BootPlatform, and its pool/mapper fields are private. Pool/Mapper are not
   Sync; frame references are not Send; BootPlatform contains a non-Send,
   non-Sync Runner. Preserve this boundary and keep the coordinator masked and
   handlers allocation-free for the initial preemptive milestone. Any later
   worker allocator authority needs an independently reviewed synchronization
   design. Update existing constructor comments that currently promise all
   Rust execution is masked rather than silently contradicting them.

6. **Ordinary mutex ownership can survive preemption; IRQ blocking cannot.**
   The implemented mutex publishes through Acquire/Release and keeps its guard
   non-Send/non-Sync. A worker may be preempted while holding it, and another
   worker can yield on contention. An IRQ handler must never wait for that
   interrupted owner. No public RAII mask token should be added incidentally:
   arbitrary safe forgetting and out-of-order destruction must not undermine
   exclusion or accidentally re-enable IRQ in a critical section.

7. **Exception and CPU-state assumptions need explicit treatment.**
   Fixed-site synchronous recovery currently accepts only fully masked saved
   DAIF. Preserve that restricted diagnostic contract or deliberately revise
   and test it; do not silently make arbitrary callback faults recoverable.
   The scheduler must specify exclusive-monitor handling across switches and
   preserve the pinned target's supported architectural state. Existing stack
   probe/guard evidence remains required for safe callbacks, including when a
   timer arrives near a stack limit. No guest SMP guarantee follows from a
   single-CPU preemptive implementation.

These findings identify requirements for a new design, not defects in the
currently masked cooperative implementation. All conclusions above derive
from repository source; no Linux implementation source was consulted.

## Design review

Reviewed [design 0012](../design/0012-arm-preemption.md), including revisions
requested during this audit. **Ready for implementation at the design level.**
No blocking design finding remains. This verdict is independent AI review of
the contract, not runtime implementation authorization or source approval.

The design preserves a task-private full asynchronous image and restores the
shared emergency-stack top before resuming coordinator Rust. It explicitly
saves a fresh coordinator context on every dispatch, including ERET dispatch;
first entry publishes a valid worker SP and coordinator continuation before
unmasking. Explicit yield restores its own prior mask, preserving the existing
cooperative mode. Completion publishes DONE only after the callback returns and
IRQ is masked. These address the critical transition hazards identified above.

Requested timer-lifecycle clarifications are present: coordinator-driven fresh
rearming on every dispatch, disabled/deasserted source on return, pending-PPI
cleanup, setup failure rollback before callbacks, spurious-ID return without
EOI or a scheduling event, and publication withdrawal only after quiescence.
The mask transitions also require compiler memory barriers. The ownership
boundary explicitly keeps allocator/mapper access outside preemptible workers
and requires a new review before exposing it.

Implementation review must still verify every assembly offset and register
restore, actual DAIF/ELR/SPSR transitions, absence of retained emergency frames,
fresh host continuation saves, timer/controller instruction ordering and
exclusive-monitor clearing. The design deliberately leaves exact hardware
instruction sequencing to that review; the text alone does not establish it.
Run the proposed non-yielding Rust workloads, complementary full-register
sentinel probes, mixed-yield/mutex tests, negative delivery/overflow cases and
all existing regression profiles before claiming preemption verified. No tests
were executed by this reviewer for the proposed behavior.
