# Ordinary task authority and lifecycle matrix

The [strict second run](task-matrix/guest-second/result.json) passes 50 ordinary
cases (25 cases repeated) and 41 bootstrap checks at each of 64/128 MiB on
one ARM64 CPU. [Review 0029](../reviews/0029-native-task-creation.md) accepts
source, linked stack bounds, actual layout and independent raw exception/serial
correlation. The exact case inventory is in the
[protocol](../design/0029-task-protocol.md); ten injection profiles remain excluded.

The guests exercise copied absent/empty/256-byte startup inputs, ready and
parked waits, restricted and foreign handles, stale reuse, malformed descriptors,
copy faults, child faults, held capacity, logical frame caps, actual physical
allocation failures, parent termination, call limits and dropping a waiting
group. Cleanup returns physical frames to the measured baseline; fresh launches
follow failure and cancellation. This is bounded native task behavior, not
arbitrary program launch or Linux compatibility.

The [first guest run](task-matrix/guest-first/result.json) intentionally fails
only the raw-review acceptance flag; its untouched traces were independently
reviewed before enabling the gate. The second run validates fresh traces without
relaxing assertions. Parser tests include missing, duplicate and reordered
ownership records. [Compiler probes](task-negative-types-first/result.json)
validate 53 ownership/privacy cases including production absence of test observers.

Three oversized linked stack candidates are preserved under `task-matrix/`.
The proposed boxed-result core change merely moved stack storage and was reverted.
Safe test-driver changes and a test-only initialization boundary bring the
[fourth candidate](task-matrix/task-matrix-fourth/) to 61,552/65,536 bytes,
including the unchanged helper allowance. All 24 profiles in the
[fifth inventory](task-matrix/task-matrix-fifth/) compile; the other 23 remain
byte-identical to previously accepted profiles. Both-architecture all-feature
[static checks](task-matrix/static-final/result.json) pass.

The [full ARM/x86 regression](task-matrix-full-first/result.json) passes in
272.0 seconds with every captured input unchanged. Command, source hashes,
output and both raw evidence archives are retained alongside the result.
All 308 captured inputs match local implementation commit `67b86e1b`;
see [committed-blob verification](task-matrix-full-first/verified-commit.json).
Remote CI, ten injected failure cases, interactive input, storage and networking
are not established by this evidence.
