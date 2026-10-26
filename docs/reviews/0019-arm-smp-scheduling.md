# ARM concurrent scheduling review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness certification. Reviewer performs no builds or guest execution.

## Design verdict

Reviewed [design 0019](../design/0019-arm-smp-scheduling.md) before core edits.
**Ready for the scoped two-CPU scheduling implementation.** This is an
implementation contract for design 0014; it does not relax its concurrency,
lifetime or negative-test requirements, or complete the allocator phase gate.

The private unsafe local-run boundary requires existing joined-session authority
and never owns the global lease or distributor lifecycle. CPU identity, local
stacks, Task records and timer state remain affine; extending CPU identity
recognition must not grant CPU1 access to cooperative CPU0 stacks. Owned fixed
job arrays and a disjoint peer-result cell prevent whole-payload observations
from aliasing remote writes. Shared forwarding persists through both local
completions and peer Returned/OFF; all reuse follows that join.

Secondary IRQ routing preserves the existing complete register save before
scratch use and restores the owning CPU's emergency frame. Requested terminal
output clarification is included: common fatal paths recognize nonzero affinity
before UART output. The full-register sentinel observes only its own counter.
Worker diagnostics grant scalar observations after validated admission, never
raw context or interrupt-control authority.

The concurrent progress test requires second-worker progress on both CPUs and
cannot pass sequential whole-CPU execution. Mutex contention deliberately holds
an inconsistent protected payload while all peers contend. Event fixtures cover
before, during, after, broadcast and unrelated notification, including an entirely
blocked peer. The deterministic wait hook is fixed atomic instrumentation rather
than an arbitrary callback under masked IRQ; requested clarification preserves
its armed state across session admission while resetting only observation masks.

Eight explicit positive cases, bounded report truncation, oversized/empty input,
owned-stack observations and normal destruction complement per-CPU IRQ accounting.
Withheld peer timer, peer guard overflow and both-CPU blocked retention need
strict negative evidence. Live observation masks are atomic diagnostic state,
never completion or payload-lifetime authority. Earlier startup/shutdown failures
and same-guest baseline checks remain required.

## Source verdict

Reviewed the frozen implementation against parent `0fa4fae`, including the local
runner, mailbox/session, CPU-local/stack helpers, IRQ/fault assembly, diagnostic
hooks, safe callback suite and feature integration. **No blocking source finding
remains for this increment.** Builds and execution remain a separate gate.

- The safe CPU0 wrapper retains BUSY and shared controller ownership. The
  private unsafe local runner checks actual CPU association, DAIF, bounded job
  count and CPU-specific stacks before mutation. It neither reacquires BUSY nor
  changes distributor forwarding. Empty local work pairs setup/stop to establish
  explicit quiet state; the existing outer empty single-CPU path remains without
  controller changes. Cooperative construction filters CPU0 despite generalized
  CPU identity recognition.
- Job descriptors are copied into bounded owned arrays. Work and peer result
  occupy distinct UnsafeCells; CPU1 raw-copies only immutable Work and writes
  only its report. CPU0 runs its own owned copy, then acquires Returned and
  observes OFF before peer report access, shared shutdown, coordinator/emergency
  stack cleanup and slot restoration. Per-CPU worker stacks are cleared only by
  their local owner before entry or after normal callback completion. Any
  post-admission local error terminates instead of releasing a live session.
- Full IRQ register preservation remains unchanged: secondary slot 5 routes to
  the common save entry, which selects the owning SP_EL0 and stores all GPRs
  before scratch use. Existing image copy/restore releases that local emergency
  frame. Other peer vectors stay terminal. Common fatal reporting checks full
  affinity before UART output, and the overflow fixture checks the peer guard
  syndrome/range; the host additionally must validate its exact probe PC.
- CPU identity/base validation precedes plain scalar reads and observes only
  the current record under IRQ mask. Worker observations verify active local
  publication and stack range, restore DAIF and expose only scalars. Both full
  sentinel passes poll the owning IRQ counter. Local report sums are checked
  against local counter deltas; no live peer counter is read by CPU0.
- Event registration remains CPU-local while sequence observations are atomic.
  The fixed race hook sits after the final snapshot and before registration,
  synchronizes with the remote notifier using atomics, and intentionally permits
  one stale-snapshot park followed by rescan wake. Admission preserves its armed
  phase. Blocked masks use atomic publication only and never grant payload or
  reclamation authority. Updated comments distinguish local IRQ exclusion from
  possible remote notification.
- Safe callback tests require nonyielding second-worker progress on both CPUs,
  own-counter register probes, exact CPU/task IDs and normal completion. Two
  review findings were corrected: every mixed-case worker now forces timer
  preemption before its 100 yields, and every recorded SP is checked against
  that exact task's declared guard/top bounds, not merely uniqueness/alignment.
  Mutex tests retain an inconsistent payload while all seven peers contend,
  then require 16000 checked updates and eight destructor witnesses. Event
  cases check exact notification increments and expected zero/one park counts,
  including all four blocked peer workers and unrelated-event rescans.
- Oversized lists reject before admission and generation use; empty reuse is
  tested after prior sessions. Negative workloads retain live callback-local
  Events/frames on timeout, require actual peer startup before withheld-timer
  evidence, and use a fixed oversized frame for peer guard failure. Main feature
  dispatch preserves separate lifecycle regressions and exact two-CPU metadata;
  successful SMP sessions precede the normal 39-case baseline.

No compiler or guest execution result was produced by this reviewer. The
integration gate must still establish per-CPU IRQ/vector/ack/EOI correspondence,
all eight cases at both memory sizes, exact negative evidence and the full prior
regression suite. Passing source review alone does not establish concurrent
execution or complete Phase 1 allocator/general resource-management requirements.

Post-build source delta reviewed: initialization now enumerates
`tops.iter().take(jobs.len())` and copies each scalar top instead of indexing
the array. Admission already bounds jobs by four and tops has four entries, so
the same task IDs and stack values are initialized. The local-array references
do not alias Task storage or survive dispatch. **Approved; no behavior or
ownership change.** The actual-type wrapper now also enables `smp-tests`,
covering the selected core feature paths; execution remains parent-owned.
