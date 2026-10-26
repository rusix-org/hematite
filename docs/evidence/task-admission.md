# Owned Session admission: local verification

The safe policy portion of the [Session supplement](../design/0029-session-admission.md)
is implemented and independently reviewed. TaskAdmission and ParkedSession own
the original Session and cannot be forged, cloned or reused after consumption.
Both dispatch entry points share one attempt counter. Legacy WRITE/EXIT behavior
is preserved; a correctly versioned task operation at attempt 64 stops before
Host effects or a guard is created. Reply and wake preserve counters and console
identity. No Process/Group storage or actual scheduler path uses this API yet.

The [16-case host run](task-admission/host-first/result.json) verifies literal
legacy outcomes, attempt 63/64 precedence, malformed requests, the shared
64-call/4096-byte budgets, Host borrow release and owner-local abandonment.
Thirteen actual-library clients per architecture verify private construction,
non-Clone ownership, consume-once reply/park/wake and detached Host lifetime.
The [candidate identity gate](task-admission/candidate-identities.json) and
[final linked identities](task-admission/full-second/kernel-identities.json)
show all 23 kernels remain byte-identical to the accepted runtime milestone.
Existing stack and transition proofs therefore remain applicable.

## Preserved runtime failure

The [first full run](task-admission/full-first/result.json) failed in the existing
64 MiB runtime profile after 21 cases. In group 27, slot 2, turn 2, the counter
advanced from 227603833 to 245182292: 17,578,459 ticks, or 281.255344 ms at
62.5 MHz, against 90.69744 ms remaining. The actual timer return correctly
terminated Budget before the worker's first write and reclaimed its 36 frames.
Peers subsequently exited and free frames returned to 3,241. The test requires
normal exits in this scenario, so the guest exited 1. This was not a host harness
timeout or parser artifact. The trace does not establish why that interval grew.
The kernel binary was unchanged; a host pause is a hypothesis, not a diagnosis.
The 128 MiB profile passed; x86 was not reached in this full attempt.

The [isolated recheck](task-admission/runtime-recheck/result.json) passed both
profiles with unchanged commands, kernel, budgets and assertions. The
[second full run](task-admission/full-second/result.json) then passed ARM and x86
in 180.4 seconds: 82 ARM and 17 x86 result files, 210 Python validator cases per
architecture, the 16 policy cases and both sets of 13 compiler clients. Repeat
success does not erase the timing failure or prove its cause. No gate, budget,
assertion or timeout was relaxed and no automatic retry was added to the harness.

Both attempts retain source hashes, command logs and complete raw evidence:
[first-run archive](task-admission/full-first/archives.json),
[final archives](task-admission/full-second/archives.json), and expanded
[ARM](task-admission/full-second/aarch64-results.json)/[x86](task-admission/full-second/x86_64-results.json)
results. Candidate build commands, assembly and source hashes are retained in
[the candidate archive](task-admission/candidate-archive.json).
All build/test inputs remained frozen across both full attempts.

[Independent review](../reviews/0029-native-task-creation.md) accepts this safe
policy/local regression scope with the preserved failure qualification. Remote CI,
actual task creation, capability issuance and hardware parking/wake are still
unverified or unimplemented. The Group dispatch/authority documents are design
work for that integration, not runtime evidence.
