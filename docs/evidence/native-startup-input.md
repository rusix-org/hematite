# Safe owned startup-input codec

Implementation: `409e76e`.

Stage A of [design 0027](../design/0027-native-startup-input.md) adds an owned,
allocation-free input value and a strict byte-format encoder/decoder. It is a
preparatory native interface, not yet a runtime input facility. Existing startup
pages, monitor commands and native programs retain their verified behavior.

The input holds up to 256 arbitrary bytes and distinguishes absence from an
explicitly empty payload. Construction copies the caller's bytes; decoding
borrows only the validated immutable destination. The 4032-byte extension has
an explicit versioned header and zero padding. Encoding writes directly into
caller-provided storage, with no page-sized stack temporary; a wrong-sized
destination remains untouched. The first 64 startup bytes are outside the
codec's destination.

[Independent AI review](../reviews/0027-native-startup-input.md) covers the design,
source and verification tools. Fourteen host tests exercise literal wire bytes,
all allowed payload lengths, arbitrary byte values, all truncated input extents,
invalid header fields/lengths, every padding position, source mutation/lifetime,
failed-write transactionality and deterministic mutation/canonical re-encoding.
Fourteen compiler clients per target check valid owned-copy use and reject
forged fields, copying/cloning the owner, escaped borrows and mutation through
immutable views. These are concrete codec/type guarantees, not process isolation
or service execution claims.

The [full local summary](startup-input-stage-a/final/verification-summary.json),
[raw suite log](startup-input-stage-a/final-test-all.log) and
[source hashes](startup-input-stage-a/final/source-sha256.txt) bind verification.
The [design snapshot](startup-input-stage-a/design-at-full-test.md) preserves the
specification hash recorded during the full suite; later Stage B draft edits
are not runtime source changes. The complete ARM/x86 suite retains all prior runtime profiles and adds five
harness mutation tests (147 Python cases per target). All 21 runtime ELF files
are byte-identical to the preceding reviewed launcher milestone; the new codec
is not called by the kernel yet. The emitted build artifacts are recorded in
[candidates](startup-input-stage-a/candidates/inputs.json).

The first host run passed. The initial Clippy check rejected an explicit
`drop(input)` in the ownership test because Input has no destructor. A lexical
scope now ends that owner before decoding the independent destination. The
[failure](startup-input-stage-a/build-check.log) and subsequent successful checks
are retained; no kernel logic changed for that fix.

Commands: `tools/dev.sh python3 tools/check_startup_input.py --output <fresh>`,
`tools/dev.sh python3 docs/evidence/startup-input-stage-a/build-candidates.py`,
then `make test-all`. All use the pinned repository container. Expanded commands,
compiler diagnostics and raw results accompany each record. No remote CI or
publication is claimed.

Next is the independently reviewed Stage B integration: copy the extension into
an owned read-only startup frame, expose opt-in launch input, run a native echo
consumer and verify maximum-size payloads and protection faults at EL0. Spawn,
IPC, concurrency and a useful server workload remain later requirements.
