# Task-negative bootstrap and safe driver

Status: implementation design approved in [review 0030](../reviews/0030-task-injection.md);
source, compiled and runtime acceptance remain separate gates.

Use a separate `task-negative-tests` ARM profile, one fixed compiled case per
boot. Mirror the accepted task-tests boot proof: CPU0-only topology, actual timer
preflight, existing private Factory and Executor constructors, root/vector
addresses obtained inside the architecture adapter. Keep the adapter no larger
than the ordinary adapter and keep initialization out of the retained test stack.
Run the existing 41-check EL1 baseline only after the negative driver succeeds.
Unsupported/missing case, failed construction, exhausted step bound or missing
hook must fail the guest, never skip it. Other feature precedence remains explicit.

The driver is a new `#![forbid(unsafe_code)]` module. It calls the fixed negative
constructor without caller-selected bytes or seed arguments. Bind the reported
case to the copied observation and canonical manifest. Observe the actual root
and namespace, log metadata charges using an independently compiled negative
layout witness, and require physical free-count change to equal live group charge.
Do not embed a second copy of the negative archive in the driver.

Drive at most 1024 selected Steps, borrowing each large Result/Turn in place.
Keep no large copied report in caller frames retained across another group turn.
Use noinline compact-fact inspection, printing and free-count helpers as in the
accepted ordinary driver; use lexical destruction of Group rather than moving
it through explicit drop. Require restored root/vector/protection state and
consistent live-frame counts on actual Steps. Administrative closures must keep
the real child's trigger; never synthesize a parent Step or its timestamps.

At held root publication, require exact expected End and evidence kind from the
approved ten-profile table. Verify the copied hook observation before task_reap:
one Fired, no ConsumedFailed, exact seed kind for exhaustion cases. Compare the
published completion with task_completion and task_reap; require zero remaining
children, metadata-only charge before reap, and zero charge/full baseline after
reap and lexical Group destruction. Record every actual boundary for the host
ownership/first-eligibility checker; guest PASS alone is insufficient.

Capability exhaustion additionally performs a fresh ordinary task construction
with the same Executor and canonical root/archive and requires Resources, then
runs the canonical legacy hello with that same Executor. Those helpers may
expose immutable canonical images/bytes to the safe driver, never mutable state
or a remintable hook token. Require genuine HMPROC execution and full frame
recovery. The reviewed 0030 amendment to the 0029 protocol allows the exact
existing HMPROC hello grammar only inside LEGACY_BEGIN/END for this case, with
its two actual SVCs and payload; reject HMPROC elsewhere. Other retired profiles
must have no later EL0 entry. No failure path
replaces Executor to recover.

Extend the existing compiler layout extractor with an explicit negative-feature
option. Keep ordinary output/commands unchanged. Negative input records include
all new transitive source and canonical fixture inputs; compile actual private
Authority and Group under the negative feature. Preserve the exact reversible
module-copy witness transformation. Generated numeric constants in the negative
fixture directory must agree with extracted bytes. A temporary ordinary-layout
fixture seed may break the archive/witness construction cycle, but is not valid
negative evidence; regenerate and reproduce against the final feature witness.

Before any guest: independent source review of adapter, driver and hooks;
compiler privacy/lifetime probes; exact artifact/layout verification; all affected
linked stack/counter-order checks. After initial diagnostic execution: independent
raw trace review, strict rerun and full ARM/x86 regressions. These gates establish
only the specified injected behaviors, not a completed OS phase.
