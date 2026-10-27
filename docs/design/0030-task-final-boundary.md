# Task executor retirement at finalization

Status: independently approved in [review 0030](../reviews/0030-task-injection.md);
implemented, pending compiled and runtime verification.

The ordinary task matrix is accepted at `67b86e1b`. The remaining protocol
profiles `platform_reply`, `platform_park`, `child_retire_ready` and
`child_retire_waiting` retire the actual Executor after a real lower-EL return
and before the final accounting read. Current `execute_task_turn` checks clock
and budget results, but not retirement at this boundary. `task_step` can thus
install a provisional Reply/Park before its later retirement check closes the
root administratively. That does not meet the recorded executed-step contract.

## Proposed production change

After the final counter read, actual `Budget::finish` (or stop on clock failure)
and Timing recording, treat `self.executor.retired()` like a failed accounting
result: return `Some(End::Unavailable)` before any provisional settlement is
installed. Preserve actual call/write counters, timer observations and the
accounted result. Do not invent a clock error when the clock is valid.

Precedence at this boundary is Unavailable on invalid accounting or retirement;
otherwise preserve an existing terminal outcome; otherwise exhausted accounting
becomes Budget. The task_step caller drops provisional settlement, clears
Dispatching, and handles this actual selected step through its existing terminal
path. A root gets Executed Unavailable. A child gets its actual terminal Step,
then the root closes administratively with that child's TaskId. An already
recorded root closure remains immutable. No later EL0 entry is allowed once the
Executor is retired.

No new user-accessible API, mutable observer, or unsafe operation is required.
The final counter read remains after dispatch/provisional settlement and before
accounting. The retirement read follows accounting, before settlement install;
there is no intervening path that re-enters userspace. Executor access remains
exclusive under the existing Group borrow and active Lease.

## Verification gate

Implement only after independent source/design review. New negative fixtures
must cause actual retirement at the specified boundary, record its exact
selected TaskId/event, and prove no installed reply, parked state, wake or later
ERET. Ordinary faults must still remain contained without retiring Executor.

Compile all affected profiles, independently inspect linked stack bounds and
counter/settlement ordering, then run fresh ordinary and injected hardware
traces. Run the full ARM/x86 suite after raw negative trace review. Static tests
or a manually constructed End value do not establish this behavior. Until those
gates pass this is only a proposed correction, not a verified fix.
