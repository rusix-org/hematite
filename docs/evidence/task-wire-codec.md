# Native task wire codec: local verification

The standalone portion of [design 0029](../design/0029-native-task-creation.md)
is implemented in safe Rust and independently reviewed. It encodes and validates
owned Spawn/Grant values and scalar completion/wait/cancel values. These values
confer no authority. Session, Group, syscall dispatch and monitor behavior are
unchanged; userspace task creation is not implemented by this milestone.

The [full local suite](task-wire/full-first/result.json) passes on ARM64 and
x86_64 in 259.4 seconds. It includes 18 independent Rust codec cases, six actual
rlib compiler clients per architecture, and 205 Python validator cases per
architecture. Coverage includes literal endian bytes, all input lengths,
unaligned/source-detached values, malformed-field precedence, every reserved
region, overflow, unchanged output on encoding errors, the existing startup
prefix/full-tail codec, all exit codes, and malformed scalar replies. Compiler
clients reject struct forgery and private-field mutation while accepting copied
metadata detached from source bytes. Negative host-gate tests reject missing,
duplicate, skipped, reordered, failed and truncated reports and execution errors.

All [23 kernel identities](task-wire/full-first/kernel-identities.json) match
the independently reviewed process-group milestone exactly. This is no new
hardware-path claim. Existing ARM/x86 runtime regressions also pass. The full
[command/source record](task-wire/full-first/inputs.json),
[raw evidence archives](task-wire/full-first/archives.json), and expanded
[ARM](task-wire/full-first/aarch64-results.json)/[x86](task-wire/full-first/x86_64-results.json)
results are preserved. Build and test sources stayed frozen during execution.
Remote CI has not been run for this change.

The [initial 18-case host run](task-wire/host-first/result.json) is retained with
its original input hashes. A later [formatting preflight failure](task-wire/preflight-first.log)
prevented Clippy and Python checks in that command; formatting was corrected
before the successful full suite. No failed evidence is relabeled as passing.

[Independent review](../reviews/0029-native-task-creation.md) separates codec
acceptance from pending Session/capability/Group integration. The next mechanism
must preserve the original Session across admission and parking, implement real
owned children, and pass the separate EL0 parent/child hardware contract.
