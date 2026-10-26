# Native process groups: local verification

Stage B of [design 0028](../design/0028-native-process-runtime.md) is implemented.
The full local regression suite passes. Userspace spawn, IPC and Linux
compatibility remain unimplemented; matching remote CI remains unverified.

The [ordinary group profiles](runtime-stage-b/group-first/result.json) pass on
64/128 MiB ARM guests and a fresh 128 MiB guest. Independent review revalidated
raw serial and exception traces against the exact fourth normal candidate:
two echo instances receive different owned jobs and resume between calls;
the busy loop is interrupted while hello finishes; a failed second admission
reclaims the first owner without entering userspace; counters and free frames
recover, including after subsequent bootstrap tests and legacy hello.
Warm profiles finish with five completions and generation six; fresh with two
completions and generation two. The failed second admission consumes generation
three without a completion. No native spawn operation is exposed.

All [41 compiler clients](runtime-stage-b/buffered-types/result.json) pass, including
affine group ownership, exclusive Executor borrowing, owned-input detachment,
and absence of production test hooks. The current
[compiler layout witness](runtime-stage-b/buffered-layout/result.json) reports
Process 9,832 bytes and three box pages. This is not stack or runtime proof.
Both architectures pass all-feature Clippy, formatting and structural safety
checks in the [static-check archive](runtime-stage-b/seventh-static/result.json).

All 23 [initial linked profiles](runtime-stage-b/third-candidates/inputs.json)
compile. Review rejected the first ordinary admission stack and two runtime
test candidates before guests. Borrowing owned results and splitting preparation,
boxing and rollback phases produced reviewed bounds of 64,288 bytes for the
fourth normal candidate and 65,216 for the fifth runtime candidate, including
the stated conservative allowance, against 65,536 bytes. Exact artifacts and
failure history remain in this directory and the
[independent review](../reviews/0028-native-process-runtime.md).

The [first detailed run](runtime-stage-b/runtime-first/result.json) **fails**
at both RAM sizes. The real 100 ms active-turn budget expires before the
64-call fixture completes: trace printing was inside that interval.
The guest correctly exits failure; no case inventory or budget was relaxed.
The independently reviewed amendment captures bounded diagnostics during the
turn and serializes them after measured work, retaining actual admission,
dispatch and capture costs and charged ordinary terminal output. All 23
[sixth candidates](runtime-stage-b/sixth-candidates/inputs.json) compile; their
linked ordering and stack bounds passed independent review. The
[ordinary rerun](runtime-stage-b/group-second/result.json) passes all three profiles.
The [second detailed run](runtime-stage-b/runtime-second/result.json) still
**fails host validation**: both guests exit zero with all 24 runtime cases and
41 baseline checks, but the driver emits duplicate IDLE records. The strict
validator rejects these; a corrected driver and fresh logs are required.
The corrected seventh driver emits just the final IDLE for each group. Both
[third diagnostic guests](runtime-stage-b/runtime-third/result.json) complete
24 runtime cases and 41 baseline checks; their unchanged serial and hardware
traces passed independent review. The saved host result remains false because
the explicit review flag was deliberately disabled at execution time. Review
accepted 520/514 slices (286 SVCs and two faults each), full context copies,
64-call termination with remaining time, cancellation, slot reuse and real
allocation pressure; every case returns to 3,241 free frames. The flag is now
enabled for subsequent tests. The [full local ARM/x86 suite](runtime-stage-b/full-first/result.json)
then passed in 263.7 seconds. This run includes 200 Python validator tests per
architecture, all ownership/compiler gates, fixture reproduction, the existing
process/startup/monitor regressions, the three group monitor profiles and both
runtime profiles. Build/test source hashes remained unchanged during the run;
normal and runtime ELFs match the reviewed candidates. Complete raw evidence is
preserved in the [hashed archives](runtime-stage-b/full-first/archives.json),
with expanded [ARM results](runtime-stage-b/full-first/aarch64-results.json) and
[x86 results](runtime-stage-b/full-first/x86_64-results.json). Matching remote CI
remains unverified.
