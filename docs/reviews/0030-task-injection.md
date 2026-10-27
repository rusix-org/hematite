# 0030 independent review: task finalization and injection tests

## Final-boundary production change: design approved

Reviewed 0030-task-final-boundary against the actual execute_task_turn and caller
at ordinary checkpoint 67b86e1b. The current final accounting branch does not
inspect Executor retirement; the caller can install a provisional Reply/Park
before its later administrative retirement handling. This conflicts with the
specified selected-step failure boundary.

Approve the exact change after genuine finish/Timing recording: accounting error
or actual Executor retirement selects Unavailable; otherwise retain any existing
terminal outcome; otherwise Exhausted selects Budget. A valid clock/accounting
result remains accounted=true even when retirement selects Unavailable. Do not
invent a Clock error, alter counter placement, or expand the scope to other
terminal paths.

The existing caller then bypasses settlement installation, drops the detached
Session/provisional park once, clears Dispatching, and runs its terminal cleanup
under the retained Lease. Root retirement becomes Executed Unavailable; child
retirement retains its actual Step and administratively closes the root before
wake. Existing immutable root closure remains unchanged. No-entry cancellation
and preselection retirement retain their existing separate precedence. Exclusive
Executor ownership and the Lease prevent an interposed user re-entry.

This is implementation design approval only for that final branch. No hook,
seed, mutable observer, new unsafe operation or public API is approved here.
Source review, all affected linked stack/counter-order checks, actual ordinary
and separately reviewed negative traces, and full local regressions remain
required before verified acceptance. The broader task-injection proposal is
pending separate review.

The implemented production change matches the approved branch exactly: after
actual Timing/accounting, result.is_err() || self.executor.retired() selects
Unavailable. No other source path changed in that diff. Source acceptance does
not imply compiled or hardware verification.

## Broader fixed hooks: concrete pending freezes

Reviewed the constructor/provenance and ownership proposal. A one-use private
bootstrap authorization, constructor-selected immutable archive, actual committed
first-child identity, consumed original TaskAdmission and original Session returned
from a deliberately refused wake install are consistent with existing ownership.
No caller seed/address/callback is needed. Three details block implementation:

- Replace the draft's stale pre-counter guard wording with the approved actual
  post-finish/Timing precedence above.
- Freeze an exact archive path list and root mapping. Ten roots plus worker,
  demo/loop and legacy_hello, with explicit bin/demo directories, would be 15
  entries; no implicit extra fixture or pathname authority is acceptable.
- Define real budget-poll failure behavior for regression, range violation or
  polling-bound exhaustion. Consume its authorization without recording a
  successful INJECT/Fired result, preserve cleanup and fail the profile through
  Unavailable/retirement (or explicit fatal failure), never a fabricated Budget.
  A failed hook must not rearm. Freeze copied NegativeObservation fields and its
  access lifetime relative to Authority reap.

The ten hooks remain unapproved pending these narrow corrections. Existing
ordinary source and final-boundary approval are unaffected.

Re-reviewed the revised fixed-hook proposal. The original three freezes are now
resolved: exact 15-entry negative archive, typed read-only observation with Mode
after metadata reap, and ConsumedFailed/retirement cleanup without successful
INJECT or rearming. Existing boot construction now uses a private safe fresh-state
marker instead of adding another unsafe constructor. Shared guard wording links
the approved post-accounting boundary. The original finite-worker wake-install
scenario is retained: actual installed Waiting must be observed; an early worker
completion/missed hook fails rather than substituting a different workload.

One accounting clarification remains: an invalid or regressing budget-poll read
must set the existing sticky clock_failed condition, preserving the last valid
snapshot with accounted=false even if the final read recovers. Exhausting only
the observation-count bound with valid readings can retain genuine accounting
while selecting Unavailable through retirement. Both paths are ConsumedFailed
and cannot pass the intended Budget profile. Other constructor/ownership/trigger
mechanisms are ready for implementation once this precise distinction is frozen.

Verified the final polling distinction on disk. Approve exact fixed ten-hook
implementation design, retaining original finite-worker wake scenario, canonical
one-use constructor, safe fresh-state marker, typed observer and the stated
ownership/trigger restrictions. Root assignment and separate source/layout/
stack/ordering/raw-trace gates remain required; no generic seam or scenario
substitution is authorized.

## Guard-only compiled gate accepted

Independently verified all 24 actual ELF/assembly hashes against guard-candidates-
first. Twenty-one ELFs are byte-identical to the ordinary matrix baseline. The
three changed profiles retain their reviewed bounds including 640-byte allowance:
monitor 64,432, ordinary runtime 65,376, task matrix 61,552. Runtime's task branch
is unreachable through its ordinary constructors; the actual task profile includes
that branch in full. No allowance, stack or path justification changed.

Inspected the changed execute_task_turn branch flow in all three emitted images:
real dispatch returns before final ISB/MRS; budget and Timing stores precede the
Executor retirement byte read; retirement returns Unavailable (6), preventing
caller settlement installation. Invalid accounting follows its existing rejection
path. This approves bounded ordinary regressions for the guard-only candidates,
not hook-feature binaries or dynamic failure-path evidence.

Changed ELF SHA256 values: monitor
fb64ee1247de7df42e8785f8a905ebdae0b45677058c4e1e8e02968d1a6425a3;
runtime 5319dfeae33e6b7abcd180228151fb4c26461bfea5a3666189b5773808be682b;
task matrix 0ba7f356b84bd34ed64b5fff3899e6ea6fa047c2ad641c6aed272dca75225650.
