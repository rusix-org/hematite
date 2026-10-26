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

## StageB initial draft review

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

## StageB concrete design review

The appended freeze resolves the initial mechanism/test gaps: exact
prepare_with_input and Platform signatures, private feature-only checked
copy_startup observation, eight independently linked probes in a checked archive,
twelve cases repeated twice, full presence/payload/tail proofs at EL0 and exact
resource/generation records. Nine successful exits per pass issue27 SVCs; two
faults and one deadline make30 slices per pass,60 total. The unchanged old66-run
matrix remains mandatory. The test hook accepts only offsets in the startup
page, prevalidates the complete output interval and returns immutable copied
data under boot mappings; it cannot expose mutable owners or user VA authority.

Invalid257-byte construction is checked before first issuer creation and after
generation24. Budget/exhausted preparation failures consume25/26; successful
inactive prepare/drop must reveal27 and reclaim its page. Comparing the full
page in bounded chunks after source and Input end proves kernel ownership,
while independently executed user probes prevent a kernel-only readback claim.
Absence after binary/fault/deadline cases detects stale copy state. No caller
input borrow is live under the alternate root, including omitted-arena inputs.

A blocking architectural expectation was corrected before implementation:
BR X9 from the ordinary unguarded RX source sets BTYPE01, not guarded-source11.
The NX payload fault therefore requires BTYPE1 (saved0x00400740 after clearing
NZCV), ESR0x8200000f and FAR/ELR=STARTUP+88. The RO byte-write requires
ESR0x9200004f at its named instruction and that same FAR. This follows the
existing0023 reviewed Arm BR rule; no widened syndrome mask is permitted.
The payload's RET encoding is a known instruction, not a claimed safe fallback.

**StageB design approved for the specified implementation.** Confirmed the
coordinated fixed syscall-pointer/newline and exact probe-manifest wording on
disk. The freeze uses payload pointer
STARTUP+88 even for zero-length writes, independent validation of decoded data,
and a distinct syscall for an immutable read-only newline byte. Source review,
actual compiler clients and all affected compiled-stack gates remain required
before any StageB guest.

The ordinary integration gate also now freezes five echo launches (absent,
empty, text, preserved spaces and the64-byte command boundary) plus the five
existing launches: ten generations,21 SVCs and23 total process slices per main
profile. Fresh legacy hello remains, and a separate fresh input-first echo
requires generation1 without warm-up. This closes the usability scope alongside
the separate binary256-byte hardware profile; codec-only evidence cannot satisfy
the milestone.

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

## StageB source review

Reviewed the actual optional preparation/launcher wiring, private startup
observer, guest checks, safe monitor policy and user fixtures. **Core source
approved for the frozen StageB scope**, subject to compiled stack and execution
gates. `prepare_inner` retains no input lifetime; encoding writes directly into
the newly owned zeroed startup frame before leaf publication. Any encoding or
precommit failure drops that local frame and returns the unchanged Space.
Legacy preparation passes None without a page-sized temporary. The observer
validates the entire offset/length interval before delegating to immutable typed
copy, including empty-end, overflow and crossing-end cases.

The guest checks compare all4096 bytes in256-byte chunks after the construction
source and owned Input have left scope. Expected wire bytes are independent of
the encoder. They check absence after binary input and hostile termination,
actual budget/exhaustion rollback, generation consumption and prepared-owner
Drop. The additional by-value helper chain still needs emitted-assembly stack
review; source-level noinline annotations alone do not establish a bound.

The monitor preserves the first-delimiter distinction between absent and empty,
remaining spaces and the existing line limit. The eight host cases import the
actual policy source through a minimal recorder trait; this establishes parsing,
dispatch and error-output behavior, not the real Platform implementation or
hardware integration. The host tool requires exactly those eight cases plus
successful bounded execution. Echo and the eight probes use the actual decoder;
probes validate presence/payload, issue the fixed payload pointer even for empty
writes and use a separately named read-only newline syscall.

Reviewed artifact binding and requested an additional independent check of the
probe X9 target materialization. The builder now checks the three exact RX
MOVZ/MOVK words at its unique internal probe_end symbol, resolving to STARTUP+88,
in addition to STRB/BR, NZCV clear, SVC and loop opcodes. This does not broaden
the frozen public manifest. The delivery parser checks24 ordered runs,54 SVCs,
60 process slices, exact output bytes, absence/empty exit distinctions,
restoration/resource counts and generations through27. Shared TraceReader
retains the existing strict timer/GIC/fixup grammar. Requested exact cleared
NZCV on hostile write/execute/loop STOP records rather than masking those bits;
ordinary exit NZCV remains unconstrained. Actual trace grammar acceptance and
all22 candidate stack gates are pending; no StageB runtime success is claimed.

The NZCV finding is resolved in source: no-call write/execute/loop cases now
require their exact saved PSTATE, while ordinary exits mask only NZCV. Reviewed
mutations inject each NZCV bit into each of the six hostile STOP records and
retain an accepted ordinary-exit NZCV variant. No source-review blocker remains.

StageB's initial new-profile compilation exposed75,712/76,176-byte preparation
chains, rejected before guests. The reviewed owner-helper refactor preserves
source/Input lifetime-end and full-page readback while removing redundant
Process-return frames. Independently inspected and hash-bound all22 third
candidates: **compiled-stack gate now passes**, with the normal monitor's largest
conservative bound64,128 below65,536. The new profile's larger baseline bound is
62,448. Exact chains, rejected evidence and final hashes are in the
[stack review](../evidence/startup-input-stage-b/stack-review.md). First guest
execution is approved for this inventory; actual delivery/interactive traces
and regression results remain pending.

## StageB first actual runtime acceptance

Independently revalidated the raw serial and full exception/GIC traces for both
[first delivery profiles](../evidence/startup-input-stage-b/first-delivery/result.json)
and all four
[first launcher profiles](../evidence/startup-input-stage-b/first-launcher/result.json).
All six pass the frozen strict validators without overrides. Independently
matched recorded raw-log and kernel hashes, canonical ELF bytes against GNU
stdout extraction, and unique complete read-only kernel archive embedding with
no individual-ELF fallback. Delivery archive SHA-256 is
`8585f4de0f690effd2074a449dd46a17954555c55953d984a93b3284f1cc5798`
(114,176 bytes at kernel offset380095); normal archive is
`ea708fb72cd2e9cd277de319bd6e9697c54872472eaa1e3cc4a8f13ca4a9169d`
(743,936 bytes at offset371352). These bind the reviewed c200f695… and e33eb4ae…
kernels respectively.

Each64/128MiB delivery profile executes24 runs and60 slices:54 exact SVCs,
four actual startup-page faults and two deadline IRQs. Raw traces show the
write permission fault ESR0x9200004f and execute permission fault0x8200000f at
STARTUP+88, with the exact saved hostile PSTATE/BTYPE. Full256-byte binary output,
empty/absent exit distinctions and recovery cases pass. Both rejected257-byte
inputs leave the3280-frame baseline unchanged; generations1..24, failure25/26
and inactive-drop27 complete with reclamation. Eight preflight and16 baseline
IRQs plus the strict registered fault transactions are present.

Both ordinary main profiles complete37 commands and ten launches through
generation10 with3126 free frames after warm-up. Five actual echo runs cover
absent, empty, ordinary text, retained spaces and the64-byte monitor line limit.
Existing native info/hello, fault/deadline containment and post-test recovery
remain exercised. Fresh hello and fresh input-first each start at generation1
and retain3129 frames; inspected input-first transcript prints `first`, reports
exit0 and returns the live prompt. No warm-up is needed to initialize this path.

**StageB first runtime acceptance passes for these pinned one-CPU QEMU inputs.**
This establishes actual protected startup delivery and interactive use, not just
a codec. Final full-suite/regression archival acceptance is still pending;
Linux compatibility, services and native ARM/KVM remain separate gaps.

## StageB final local acceptance

Independently inspected the stable
[final archive](../evidence/startup-input-stage-b/final/verification-summary.json):
all67 ARM and14 x86 result objects report success. Fresh validation of all six
final raw delivery/launcher transcripts and full traces passes, with exact
reviewed kernel identities and independently matched raw-log hashes. The two
compressed final kernels hash to their reviewed third candidates; canonical
archive/ELF hashes, GNU extracted payloads and unique complete read-only
embedding also match. These are final-suite logs, not reused initial execution
claims. Inspected actual26-client compiler outcomes and expected rejection
classes in raw diagnostics. The x86 monitor explicitly prints unsupported for
`run bin/echo text`, then accepts status at its recovered prompt.

The final summary records `make test-all` exit0,164 Python cases per target,
48 input-delivery runs,22 ordinary launcher runs and198 existing process runs.
Audited the [milestone evidence](../evidence/native-startup-delivery.md): its
claims and limitations agree with the reviewed source and actual evidence.
**StageB final local acceptance passes.** This closes this startup-input
milestone only; CI phase closure, Linux compatibility, concurrent services,
storage/networking and ARM/KVM execution remain separate work. No broader goal
completion, human audit or performance advantage is inferred.
