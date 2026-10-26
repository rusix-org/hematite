# Persistent active-turn accounting

Implementation: `313ba353`.

Stage A of [runtime design 0028](../design/0028-native-process-runtime.md) is a
safe, allocation-free time ledger intended for actual resumable process turns.
It does not own processes, pages, capabilities or hardware. The current monitor
and executor remain unchanged; resumable execution, groups and userspace spawn
are subsequent implementation gates, not claimed by these tests.

The ledger retains remaining ticks and completed-turn count across calls. A
successful begin checks monotonic time and both quantum and whole-budget end
arithmetic. Finish charges only the active interval, saturating a bounded overrun
at the total budget; parked time is excluded. Zero elapsed time still counts a
turn. Invalid transitions, regressions and overflows permanently poison the
ledger without partially changing its diagnostic snapshot. Stop cannot claim
quiescence for an active turn. Existing native Session byte/call accounting is
not duplicated or changed.

[Independent AI review](../reviews/0028-native-process-runtime.md) covers the
contract, source and verification tools. Fourteen integration cases test the
public production library; a fifteenth uses an explicitly separate private-counter
seam compiled from the actual source to reach u64 turn-count boundaries. The seam is
absent from production. Ten compiler clients per target reject forged/copied
owners, reset attempts, use through shared borrows and production access to the
test seam. Public snapshots cannot restore or modify an accounting owner.

The [full local summary](process-budget-stage-a/final/verification-summary.json),
[raw output](process-budget-stage-a/final-test-all.log),
[source hashes](process-budget-stage-a/final/source-sha256.txt) and
[runtime identity comparison](process-budget-stage-a/runtime-identity.json)
bind the results. All 22 kernel ELF files remain byte-identical to the reviewed
startup-delivery milestone. Full ARM/x86 regressions retain all prior profiles
and add five strict harness mutation tests, totaling 169 Python cases per target.
The [tested design snapshot](process-budget-stage-a/design-at-full-test.md)
preserves the specification before its verification status is updated.

Commands used the pinned repository container: `check_process_budget.py`,
`check_process_budget_types.py`, the preserved all-profile build script, and
`make test-all`. Exact commands, source identities, diagnostics and raw results
are archived. These are local results, with no new remote CI or publication.

The [hardware test proposal](../design/0028-runtime-tests.md) requires actual
EL0 interleaving, busy-loop peer progress, resumed register state, persistent
session limits, quiescent cancellation and separate frame reclamation/completion
reaping. Those requirements prevent this safe component from becoming a
substitute for the requested multi-process runtime.

A separate [compiler layout measurement](process-budget-stage-a/layout-extracted/result.json)
records Process at 9,832 bytes, Space at 8,936 bytes, and its KernelBox/optional
owner at 40 bytes. Current box backing rounds to three pages. This informs
owned-storage planning; it does not establish allocation success or stack bounds.
The first attempt emitted no unused functions; the second parser expected numeric
words but Rust emitted a byte string. Both attempts are preserved. Assembling
and extracting that exact read-only section produced the recorded five values.
