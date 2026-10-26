# Native startup input codec review

2026-10-26. Independent AI review, not human review or a complete soundness
audit. StageA is the safe codec only; core installation, launcher arguments and
new application behavior require separate integration review and compiled gates.

The proposed owned Input has a private256-byte buffer, length and presence flag.
It does not retain the construction slice. Absent input and present-empty input
remain distinct: absent encodes an entirely zero4032-byte extension; present
uses the explicit versioned header even when length is zero. The payload is
opaque bytes, with no UTF-8, NUL termination, argument-vector or pointer meaning.
Returning an immutable view does not grant user-memory access or execution
authority. Decoded views borrow only the validated input extension.

Keeping the first64 startup bytes unchanged preserves existing native apps'
size/version/reserved checks. The extension has its own magic/version/header
size and reserved bytes. Complete decoding must check the exact extension
length, all fields, bounded payload and the entire remaining zero tail before
publishing a view. It must not accept noncanonical absent encodings or ignore
trailing corruption. Encoder length validation precedes any output mutation;
directly zeroing/writing the caller's destination avoids a4096-byte temporary.

Required tests cover0/1/255/256-byte payloads and257 rejection; all256 byte
values including NUL/non-UTF8; absent/present-empty round trips; changed/dropped
source after owning construction; malformed magic/version/header size/reserved/
payload length; all truncated prefixes and oversized extension; nonzero header
and tail padding; wrong-size destination unchanged sentinel bytes; bounded
arbitrary mutations without panic. Actual compiler clients must reject forged
Input/View, mutation through immutable bytes, and escaped view lifetimes while
accepting an Input independent of its source borrow.

**StageA design approved for implementation of the safe codec and its tests
only.** Read the written [design0027](../design/0027-native-startup-input.md)
and confirmed exact constants, private owned/borrowed APIs, error categories,
canonical little-endian layout and fourteen named host cases. The test plan
includes the requested ownership, whole-tail, exact-extent and transactional
destination checks plus actual-module compiler probes on both targets.
No implementation or execution evidence has been reviewed. Later integration
must validate input before issuing a generation, copy into the retained startup
page without retaining external borrows, keep historical prepare's zero tail,
and explicitly scope the remainder-zero contract amendment to opted-in input.
The native SVC version is unaffected. Fresh source and all affected-profile
stack reviews precede guests; earlier launcher stack headroom is not sufficient
reason to infer that a new temporary will fit.

## StageA source verdict

Reviewed `startup_input.rs`, all14 Rust integration cases and the three host/
compiler/mutation tools. **Source approved for codec-only scope.** Private Input
construction proves length<=256 and presence/owned storage consistency; fixed
indexing uses that invariant. Encoder validates exact destination size before
any mutation, then clears and fills it directly. No page-sized temporary,
allocation, unsafe code or panic path from arbitrary public inputs was found.
Decoder uses checked intervals, admits absence only for the entirely zero
slice, and validates all trailing bytes before returning its private View.

Tests include an independent literal empty-header expectation, all256 possible
payload byte values, all admitted lengths, all4032 truncated extents, unchanged
wrong-size destination sentinels, every tail-byte mutation for empty/small/max
payloads, field/bit mutations, stale-data clearing, source overwrite/drop and
unaltered legacy prefix. Accepted deterministic mutations must reencode exactly,
which detects noncanonical representations. The large buffers are host-only.

The host tool requires the exact14 names/order/results plus exit/deadline/log
conditions. The14 actual-library clients contain four valid ownership/borrowing
examples and ten intended private-field, noncopy/nonclone, escaping-lifetime or
mutation failures; diagnostics must match the specified class. Harness mutation
tests exercise missing/duplicate/failed/skipped/filter/truncation and execution
failures. This is source review, not an assertion those compiler/host cases have
already executed. Runtime installation and input delivery remain unimplemented
by this stage.

## StageB draft review: not yet implementation-approved

The runtime direction is coherent and carries the task beyond codec-only
validation: preserve legacy prepare, add typed opt-in input, copy directly into
the owned startup Frame before publishing its descriptor, then end the Input
borrow before returning Process. Allocation/copy failure must drop the local
page and preserve the original inactive Space. The startup page keeps its
existing RO/NX permissions and one-frame charge; no new address-space region
or authority is required. Monitor first-delimiter parsing supplies owned bytes,
including a distinct empty payload, and actual echo uses the reviewed decoder.

Core implementation is not yet approved because verification remains partly
deferred. Freeze the new feature/profile, exact case inventory/order/repetition,
serial fields, presence/length/payload/zero-tail proof, binary expected outputs,
RO-write and NX target/instruction/syndrome contracts, and generation/resource
checks before core code. Invalid257-byte construction must be observed before
first initialization/issuance and after valid launches without new EL0 entry.
Absent and present-empty must be distinguished by actual user behavior, not
only kernel metadata. This hardware profile supplements the existing66 runs.

Also freeze the precise prepare/Platform signatures and error mapping, and the
read-only observation mechanism for inactive prepared startup checks: Process
currently exposes no byte-copy method. A narrowly private checked copy hook can
support those tests without exposing mutable pages or caller-selected mappings.
Borrowed input may have originated in the omitted owned arena; copying must
finish under boot mappings before execution. Final all-profile stack inspection
is essential with only1,952 bytes of prior normal-launch headroom. No StageA
host result establishes runtime delivery, isolation or input-owner independence
after hardware entry.

## StageA initial actual evidence

Independently inspected ARM batch
`target/evidence/20261026T201039-525539473-aarch64/startup-input-host` and
`startup-input-types`: all14 host cases pass without skips, and fresh validation
of raw stdout plus exit/deadline/artifact conditions passes. Recorded source
hashes match the reviewed files. All14 actual-library clients have the intended
outcome: four accepted; ten rejected with exactly the expected error class in
raw compiler diagnostics. The test-only explicit drop was changed to a lexical
scope, preserving proof that encoding outlives the owned Input; this change is
source-approved.

Independently hashed all21 current runtime ELFs against the reviewed launcher
third-candidate inventory: every byte hash is unchanged, corroborating
[runtime identity](../evidence/startup-input-stage-a/runtime-identity.json).
The prior stack/runtime proof therefore remains bound to identical executables;
this unused codec does not establish input installation or delivery. Final x86
compiler and complete regression/archive evidence remain separate pending gates.

Final [StageA summary](../evidence/startup-input-stage-a/final/verification-summary.json)
records`make test-all` exit0. Independently checked the stable archive's61 ARM
and14 x86 result objects all report success. Inspected both targets'14 actual
compiler-client results and raw diagnostics: four positives compile and ten
negatives fail only with their intended error classes; source hashes match.
Together with the independently inspected14-case host execution and unchanged
21 runtime ELFs, **StageA codec-only final acceptance passes**. This does not
approve or verify StageB startup installation, monitor input or EL0 delivery;
those remain the next separately reviewed integration work.
