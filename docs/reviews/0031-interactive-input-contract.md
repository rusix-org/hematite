# Review 0031: interactive input contract

Status: safe policy direction accepted; exact API freeze required before code.
Hardware, mutable user-copy, coordinator and terminal handoff implementation are
not approved by this review. Read current native.rs/startup_input.rs and the
contract draft; no builds, VM runs or source changes were performed.

A separate single-root coordinator avoids granting unused task authority. The
explicit cumulative/wall/call/output/turn/service bounds, provisional READ park
and separately charged delivery are appropriate. Retaining the original Session
through ReadAdmission/ParkedRead is essential; neither parking nor delivery may
issue a new allowance. Hardware eligibility remains independent of this policy
review, including the proposed masked-WFI transaction.

## Narrow safe-stage freeze needed

Keep existing Admission::{Complete,Task} unchanged so existing exhaustive core
matches do not acquire an unhandled READ path. Introduce a separate InputAdmission
with Complete { session, outcome } and Read(ReadAdmission), returned by an opt-in
Session::admit_input. Use a private sealed SessionProfile selected only by a new
fixed Issuer issuance method; keep issue(console) and old startup bytes unchanged.
No caller-selected numeric limits or setters. The new profile can alter existing
Session layout, so compiled identity/layout/stack gates are still necessary before
using rebuilt kernels.

Before source implementation, specify:

- Exact issuance, admission, copied getters, reply/park and completion signatures;
  constructors/fields remain private and owning guards remain non-Clone/non-Copy.
  Dropping a guard loses the original Session; it does not restore or mint one.
- Cross-entry semantics: recommend READ recognition only in admit_input on the
  interactive profile; its task opcodes remain Operation. Legacy sessions reject
  READ, including when passed to admit_input. Existing dispatch/admit_task retain
  their explicit operation sets, with the selected profile's shared limits. State
  the decision rather than letting API reuse accidentally grant another mode.
- Whether startup Input flags describe configured features or a live grant after
  termination. Existing Session::stop_calls clears the console. A new profile bit
  must not accidentally be interpreted as retaining input authority after closure.
- Exact envelope codec type/functions/errors and decode precedence, including
  zero/wrong-slot/wrong-generation token treatment, exact size, reserved fields,
  immutable owned result, unchanged output on encoding errors and literal wire
  roundtrips. Encoding metadata never grants terminal authority.
- Exact test inventory: old startup/last-call behavior, READ 4095/4096, bad version
  and opcode precedence, WRITE/EXIT limits, shared counters through park/delivery,
  exhausted profiles, no task effects in input admission and ownership/type
  rejection clients. Host adapters test policy only, not writable user ranges.

The draft's inner eight-byte HMINPUT magic is byte-identical to the existing
outer startup-input magic. Correct the sentence claiming otherwise: their offsets,
header lengths and enclosing payload context distinguish these formats. A codec
must not confuse the inner 64-byte envelope with the outer 24-byte header plus
payload. No compatibility or authority may be inferred from magic alone.

## Later core and hardware gates

Delivery requires the separately reviewed two-pass RW copy, stable retained
mappings, final counter/retirement/terminal precedence and Ready-last publication.
The new metadata stage cannot establish these properties. Likewise exact
quarantine/quiet deadlines, real hardware wake/error evidence, finite-service
bounds and restoration before monitor parsing require their own core contracts,
compiled ordering/stack proof and real negative tests. No hardware sequence is
approved here merely because the draft cites architectural documents.

## Exact safe API approval

Reviewed 0031-input-safe-api.md and approved its codec for implementation:
private immutable Copy Grant, exact 64-byte encoding, slot 5/nonzero-generation
token shape, complete decode precedence, checked canonical constants and
unchanged destination on wrong encode size. Identical inner/outer magic bytes
are now correctly distinguished by their nested location and layout. Metadata
construction does not establish input authority.

Separately approved the specified safe policy/ownership implementation. The
private two-profile selector and fixed issue_interactive method preserve the
shared nonwrapping issuer; existing issue and Admission remain compatible.
Explicit cross-entry opcode rules prevent accidental TaskAdmission issuance in
Interactive mode. All entrypoints retain the selected shared allowance; final
recognized READ cannot return a guard, and terminal state clears advertised
grants without resetting profile limits.

ReadAdmission reject uses only three typed errors. park validates capacity and
returns the unchanged owner on failure; ParkedRead finish validates positive
bounded count/capacity and resumability before returning the original Session.
Neither operation increments calls, charges output bytes or retains caller data.
No success/park metadata result independently authorizes user copying or wake.
No blocker remains for this narrow safe implementation. Freeze actual integration
test names before strict execution; positive lifetime tests should establish that
owners retain no source/Host borrow, while negative clients target real private
fields, cloning and moved-owner misuse.

Hardware, core writable copy, interactive coordinator, delivery scheduling and
terminal quarantine remain separately unapproved. Safe implementation still
requires source/test review and all affected compiled identity/layout/stack gates
before any rebuilt guest execution.

## Codec and verification source review

Approved interactive_wire.rs and its 13 integration tests. Exact-size conversion
precedes all private constant-offset reads; malformed data cannot select an
unchecked offset. Token shape and complete field precedence match the contract.
Encoding checks destination size before mutation. The independent literal,
field-bit defects, precedence chain, all single-byte mutations, owned result and
nested-format confusion cases substantively cover canonicality and totality.

Reviewed both strict host tools, their five-case mutation suites and the 22-client
actual-library type checker. Exact 13/18 named inventories and summaries reject
missing/duplicate/extra/skipped/filtered records; successful text cannot hide a
failed launch, exit or timeout. Compiler clients use the actual rlib and cover
private construction, non-Clone/non-Copy owners, consumed-owner reuse and borrowed
moves; positive clients establish that source and Host lifetimes are not retained.
These tools are source-approved. Their execution and the separate Session policy
implementation remain to be reviewed; no hardware or EL0 input claim follows.

## Session policy source approval

Reviewed native.rs's profile and READ additions separately from the codec.
The original Admission variants remain unchanged. Legacy issue selects the old
limits; Interactive is reachable only through fixed issue_interactive using the
same checked issuer. All admission paths count once and finalize against the
selected limit. Only interactive admit_input recognizes READ; interactive
admit_task cannot produce task guards, and other entrypoints retain WRITE/EXIT
operation evaluation. Recognized final READ stops before guard creation.

Owned reject/park/finish preserve the original Session and counters. Invalid
capacity/count returns the same owner; successful finish checks positive bounded
count against original capacity and resumability, without output-byte charge.
Terminal startup flags and console are zero while configured limits remain.
The generation-MAX addition exercises both issuance methods after retirement
inside the existing named unit test. No unsafe or hardware access was added.
Source approved for safe testing/compilation; the independent 18 policy cases
remain to review. The new profile field can affect Session/Process layout and
compiled callers, so existing guest binaries cannot be assumed unchanged.

Reviewed the independent 18 policy integration cases: they exercise shared
allowances across 100 READ/WRITE cycles, 65,536-byte exhaustion without READ
credit, typed rejection, unchanged owners on invalid park/finish, host-borrow
release, drop isolation, terminal stickiness, last-call malformed/version behavior
and denial of task guards through interactive entrypoints. Requested one adjacent
boundary addition within the existing inventory: successful READ at call 4095,
park/finish retaining resumability, followed by recognized READ 4096 stopping
without a guard. Existing final-call tests cover rejection but not that immediately
preceding successful READ.

The initial codec integration compile failure used a nonexistent View::parse;
its correction calls the actual startup_input::decode API and leaves production
codec semantics unchanged. Preserve that failure separately from the passing
13-case codec and 18-case policy host runs; refreshed test evidence is required
if the requested boundary case is added.

The adjacent boundary test is now present: READ 4095 parks and finishes with
unchanged counters and resumability before READ 4096 rejects all five tested
capacities without Host effects. Replayed the exact codec-second/policy-second
host inventories; all 13/18 pass and every captured source hash is current.
Makefile retains all prior gates and adds both mutation suites, both strict host
integration/Clippy targets and 22 actual-library type clients per architecture.
Safe source/tests/integration are approved for the 34-profile compiled evaluation.
No hardware input or rebuilt guest eligibility is established by this approval.

## Compiled safe-policy regression gate

Verified all 34 actual ELF and archived assembly hashes in input-safe/candidates-
first against the accepted optimized/rebound baseline. Nineteen ELFs are identical.
Changed modes retain the prior bounds including the unchanged 640-byte allowance:
monitor 64,432; ordinary runtime 65,376; process/startup-input 62,480; negative
profiles 61,136. Task matrix grows 16 bytes in dispatch_task, yielding 61,568.
No stack limit or helper allowance changed.

Reviewed emitted accounting paths: final-counter/order instruction streams are
unchanged apart from the explicit profile-dependent 64/4096 resumability check
in monitor/negative task execution. No policy work crosses counter boundaries.
Core constructors still issue only Legacy; no new interactive execution path is
introduced by these safe APIs. Process __bss_start remains 0x40394000, matching
the reviewed canonical fixtures.

The fresh actual ARM layout witness has current source hashes and unchanged
Process 9832/three pages, Authority 928/one page, Group 1960 and metadata three
pages. All 22 actual-library type clients pass on each architecture. Approved
existing-mode regression guests with normal artifact binding and refreshed
negative-feature layout checks. This approval does not cover interactive EL0,
UART receiving, writable user copy or masked-WFI behavior.

## Final local safe-stage regression acceptance

Independently checked `docs/evidence/input-safe-full-second`: `make test-all`
returned zero in 198.875 seconds, all 342 captured source hashes still match,
and all 34 actual linked ELF hashes equal the independently reviewed candidates.
Both raw evidence archive SHA-256 values match `archives.json`. Reading their
result records found 116 ARM and 18 x86 records with no false pass status or
reported errors. The new strict host inventories contain exactly 13 codec and
18 policy cases; both architectures retain the 22 actual-library input clients.

Replayed the complete saved serial and exception traces through the strict
ordinary task and negative validators using current fixture metadata, actual
kernel fixup tables and recorded layout witnesses. Both 50-case ordinary RAM
profiles pass with 2,021 slices each: 1,056 SVCs, 961 timer IRQs and four faults,
plus the required preflight and bootstrap IRQ sequences. All ten fixed negative
profiles independently pass (65 total slices), including real parked wake
rejection, final-budget accounting, seed exhaustion, retirement and legacy
recovery. These are existing-mode regression results; none exercises an
interactive READ hardware path.

The first full attempt remains a failure: an old `forge_session` compiler client
listed fields explicitly and became stale when the private profile field was
added. Replacing that client with `Session { ..session }` preserves the exact
E0451 privacy rejection and avoids an unrelated missing-field diagnostic. No
production source or compiled kernel changed to make the retry pass.

Accepted for the local safe codec/profile/owned READ-admission milestone.
Writable user copy, UART receiving, parked process delivery, interactive EL0 and
idle/interrupt handling are still unimplemented or separately gated. This record
makes no new remote-CI or native ARM performance claim.
