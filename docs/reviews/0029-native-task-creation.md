# Native task creation: independent review

2026-10-26. Independent AI review, not human review. Standalone safe wire codec design approved; Session and core design approval
remain pending. No implementation or hardware result is implied.

## Existing implementation constraints

The real native Session owns the 64-attempt and 4096-byte counters, version/error
precedence and sticky terminal state. New task operations must use that same
attempt authority. Existing 64th WRITE reply and valid EXIT behavior must remain
compatible; suppressing new task side effects on their 64th attempt needs explicit
status/termination ordering rather than an out-of-band dispatcher bypass.

Group::turn retains BUSY and a mutable selected Slot/Process through the sealed
transition, dispatch and final accounting. Public Group::admit acquires BUSY
itself. Syscall admission therefore needs a separately reviewed private
lease-bound phase; recursive public admission would fail Busy. Safe borrows must
end or be split before mutating another slot, without releasing the exclusion
lease or placing large Process/Space values into a scheduler array. Current
four-slot capacity includes held completions, not merely runnable processes.

Session console tokens, Group TaskId, launch authority and child-completion
handles are distinct principals/kinds. The selected task supplies caller identity;
user bytes cannot nominate a parent. Retirement and overflow must fail closed
without token reuse. Failed creation must account for actual load/startup/box
rollback, any irrevocable issuer consumption, and handle/slot commit ordering.
No completion or child may retain a borrow of the parent's input/memory.

The startup prefix remains 64 bytes. The 0027 decoder validates the entire 4032-byte
extension including zero padding; silently adding capability fields to that tail
breaks existing apps. A separately versioned payload within owned opaque input
or an explicitly revised startup profile is required. Program IDs local to an
immutable launch allowlist are a reasonable bounded alternative to path lookup,
provided their namespace, mapping and missing-ID error precedence are explicit.

Wait must distinguish parked owners from completed/empty slots. No runnable work
must not silently become successful Idle while a parent waits for a child.
Budget charging excludes parked time, but syscall admission/cleanup work and
terminal precedence need a defined accounting boundary. Parent termination must
close grants and reclaim/cancel descendants under a bounded policy before the
owner group can be considered complete. Child completion delivery, consuming
wait and cancellation must each have exact once-only behavior under faults,
budget exhaustion, 64th attempts and cancellation of an already-waiting parent.

Existing linked coordinator-stack headroom is narrow. Any new wire-copy buffers,
parent metadata or callback adapter must be measured in actual linked call chains.
The previous runtime's verified matrix is a regression requirement, not evidence
that new creation paths are sound or executable.

## Initial concrete wire review

The proposed grant-local program IDs and opt-in run-task path are acceptable
bounded design choices. They do not imply a VFS or ambient executable authority.
The separately versioned 64-byte HMTASK payload fits inside existing owned input
and preserves the original startup prefix and full-tail codec. Child input stays
opaque and cannot create a matching authority-table entry merely by copying bytes.
The 64-byte SPAWN descriptor and scalar WAIT/CANCEL/completion formats provide
bounded copy surfaces without output-buffer lifetimes.

Standalone codec implementation approval still requires exact Rust APIs, malformed
field precedence, encoder transactionality and test inventory. That approval can
be independent of the unresolved Session/core mechanism. No Session or Group
change is approved by a pure-codec decision.

A concrete core blocker remains: the draft attributes wake work to completing or
cancelling turns, but current cancellation has no Budget::begin and completion is
resolved after final accounting. Specify a feasible attribution before promising
charged wake work. Bounded post-finish management can be explicitly excluded and
reported separately, or a new timing transaction needs its own review. Parent
Closing must retain the original outcome while hiding final completion until all
children are reclaimed; wake must validate the stopped parent/cap/child identities
without a second syscall count or fabricated active turn.

## Standalone codec approval

The separately scoped `task_wire` proposal is approved for implementation. It
exports only safe, allocation-free byte codecs and copied metadata. Private
validated Spawn/Grant fields prevent invalid encoder state; exact destination
length is checked before mutation and successful encoding overwrites every
reserved byte. Decode order distinguishes size, header, flags, reserved fields,
rights, frame bounds, input canonicality and range overflow. Unknown program
numbers remain metadata until actual grant lookup. A canonical numeric launch
token is not evidence of issuance or authority.

Completion decoding rejects unknown kinds, high bits and non-exit status bytes.
WAIT/CANCEL scalar helpers confer no handle rights. The proposed literal-byte,
unaligned, exhaustive bounded-length, malformed-combination and ownership tests
are appropriate. Concrete exported names/derives and exact test inventory should
be recorded with the implementation before source acceptance. Approval excludes
native Status/Session changes, capability issuance, core dispatch and startup
format changes; none is necessary for this codec stage.

The revised management boundary resolves the earlier accounting objection:
wake/consume/Closing bookkeeping is explicitly uncharged quiescent management,
examining at most two child entries with scalar context/metadata updates and
existing owner release. It performs no allocation, user-memory or bulk payload
copy, UART output or callback. SPAWN validation, input/ELF copying and allocation
remain charged to the parent. This is consistent with existing post-finish
reclamation and does not claim total kernel CPU accounting. It does not by itself
approve the still-unfrozen Session guard or core ownership implementation.

## Safe codec source review

Reviewed `crates/hm-kernel/src/task_wire.rs` and its sole safe-crate module export.
No source blocker found. Every byte decoder establishes the exact 64-byte extent
before fixed-offset access. The private helper offsets are bounded and all public
encoder errors precede writes. Private Spawn/Grant fields and validated constructors
preserve encoder invariants; copied values retain no source-byte borrow. Range
addition uses checked arithmetic after the specified length/canonicality checks.
Grant validates only numeric token shape, not issuance. Completion decoding admits
all 256 exit statuses and only the six exact non-exit words, rejecting high bits
and non-exit status bytes. WAIT/CANCEL helpers enforce their complete scalar shape.

This is source acceptance of the standalone codec only. Independent tests, actual
compiler clients and regression/runtime-identity evidence remain pending. No native
Session, status, core dispatch or execution-authority change is covered.

## Independent tests and host gate

Reviewed all 18 integration cases and the host validator, five validator mutation
cases, six actual-library compiler clients, and Makefile integration. No blocking
coverage or validator issue found. The independent byte examples and explicit
error chains cover the promised wire semantics; exhaustive completion codes,
reserved regions, input lengths and sentinel output checks go beyond round trips.
The existing startup codec integration checks the unchanged prefix and full zero
tail without pretending that a numeric Grant issues authority. Compiler clients
exercise private struct construction/mutation and detached Copy metadata against
the actual library. Makefile requires both target compiler runs and the host suite.

Independently revalidated `target/evidence/task-wire/host-first`: the exact 18-case
raw stdout passes, process exit is zero, both raw log hashes match, and all recorded
source hashes match current inputs. This verifies host codec behavior only; final
compiler-client and full regression evidence remain pending. Suggested adding
`native.rs` to this host gate's input hashes because the startup integration case
uses its Issuer; the enclosing full-suite input inventory also binds source files.

## Owned Session admission proposal

Reviewed the separate `0029-session-admission.md` against current native dispatch.
The owned guard is an acceptable safe-policy design: it carries the actual
non-Clone Session, preserving console identity and both counters without a token
that could be applied to a different Session. Consuming reply/park/wake operations
avoid self-reference and count a parked WAIT only once. Task guards cannot exist
at attempt 64. Correctly versioned task operations then stop before side effects;
wrong-version/unknown-opcode requests retain legacy error Reply plus terminal
CallLimit, and legacy WRITE/EXIT retain existing behavior.

This approves the policy direction, not core integration. Before core approval,
freeze the abandonment cleanup mechanism without borrowing the entire selected
slot/Group across child allocation; a proposed Drop guard must not recreate that
borrow conflict. Explicitly handle final budget/platform termination after a WAIT
has produced a ParkedSession: discard that state, preserve terminal precedence,
and never install a reply or resume a terminal parent. Every core exit must either
restore the original Session once or enter bounded teardown. Source, compiler
ownership tests and actual parking/wake evidence remain required.

## Final standalone codec acceptance

Accepted the local codec milestone using durable evidence in
`docs/evidence/task-wire/full-first`. The complete local `make test-all` exited
zero in 259.4 seconds. Independently inspected the archived inventories: all 80
ARM and 16 x86 result files pass. The log contains 205 Python validator cases per
target. Revalidated the exact 18-case host stdout and execution envelope; both
architectures' six actual-library clients pass, with raw negative diagnostics
limited to the expected private-construction E0451 or private-field E0616 errors.
The two positive clients compile owned detached Copy metadata.

All recorded full-suite source hashes still match. Both durable archive hashes
match their manifest. Independently hashed all 23 current linked kernels against
the recorded identities: they remain byte-identical to the previously reviewed
runtime candidates, including the seventh runtime-test binary. The codec stage
therefore does not alter their linked execution or stack proofs; repeating the
previous detailed raw runtime audit is unnecessary for this unchanged mechanism.

This acceptance establishes safe wire behavior, ownership/API boundaries and
local regression results only. No EL0 task creation, launch-capability issuance,
Session extension, parking/wake integration or new kernel execution path is
implemented by this stage. Remote CI and the full 0029 core/hardware design remain
unverified or pending, respectively. The standalone codec review is now frozen.

## Safe Session implementation scope

The exact owned-admission API in the Session supplement is approved for safe-policy
implementation independently of core integration. This includes statuses Denied 8,
Resources 9 and Pending 10; copied TaskOperation; owned non-Clone Admission,
TaskAdmission and ParkedSession; and shared private counting/finalization helpers.
Session layout, Stop and existing dispatch semantics must remain unchanged.
Only WAIT may park, with rejection returning the same guard. Consuming reply/wake
returns the original Session without another count or console effect. Dropping
an unfinished guard merely loses that Session; it performs no hardware action.

This approval does not claim that losing a guard already tears down a real Process.
The core abandonment mechanism, Process state transitions and terminal-after-park
handling remain unimplemented and unapproved. Independent source and policy tests
must check both legacy regressions and the newly specified admission precedence
before accepting this safe-policy stage.

## Safe Session source review

Reviewed the `native.rs` implementation diff. No source blocker found. Session
fields/order and existing evaluate/write behavior remain unchanged. The extracted
begin/finish/stop helpers preserve the original single attempt increment,
console revocation and sticky terminal ordering. Opt-in admission recognizes
correctly versioned task operations only, suppresses all guards/Host effects on
attempt 64, and routes other requests through existing evaluation/finalization.

TaskAdmission owns the original Session and copied Request. Neither it nor
ParkedSession has a public constructor, Clone/Copy implementation or borrowed
user state. Non-WAIT parking returns the original guard; reply and wake consume
ownership without counters or effects. Dropping these safe metadata owners does
not perform hardware cleanup. This accepts source within the approved policy
scope only. Independent tests, actual-library compiler probes and linked runtime
identity/stack checks remain necessary; legacy dispatch refactoring may change
code generation even when its source semantics are preserved.

## Safe Session tests and initial evidence

Reviewed all 16 public-API integration cases, the strict host runner and its five
mutation cases, 13 actual-library ownership clients, and Makefile wiring. No
blocking issue found. Literal expected legacy outcomes supplement direct/owned
parity checks; tests cover task attempt 63/64 precedence, no Host effects for task
admission, the shared attempt and 4096-byte counters, original console continuity,
owned Host-independent guards, non-WAIT rejection and owner-local abandonment.
The compiler clients test private construction/extraction, no Clone, moved Session
ownership, reuse after reply/park and double wake; positive clients check actual
owned results rather than a substitute type.

Independently revalidated the exact 16-case raw host report and all recorded
inputs in `target/evidence/task-admission/host-first`. The new policy is locally
host-tested, while compiler execution and full-suite acceptance remain pending.
Initial linked candidates, including normal ARM, match their reviewed baseline
ELF hashes; final all-profile identity review waits for the complete inventory.

The completed Session-policy candidate inventory contains exactly the prior 23
profiles, each with build exit zero. Independently hashed every actual linked ELF
from `target/evidence/task-admission/first-candidates/inputs.json` and compared it
with the accepted task-wire/runtime baseline: all 23 are byte-identical. This
includes the previously narrow runtime and normal launcher stack paths. Existing
compiled stack and transition proofs therefore remain applicable without a new
stack interpretation. Approved these unchanged binaries for full-suite guests;
this does not turn the unused safe admission API into executed core integration.

## Lease-bound core dispatch draft review

The sequential owned-command proposal addresses the original wide-borrow and
provisional-WAIT problems in principle. Short selected-Space borrows end before
sequential Group mutation; an outer finalizer replaces a Group-borrowing Drop guard.
A provisional Park is discarded before Waiting publication if final accounting
selects a terminal outcome. This is not yet core implementation approval.

Concrete remaining refinements sent to the authors:

- Carry copied calls/written metrics through every Settlement and DispatchFailure.
  Once an error drops the owned Session and Process.session is None, the outer
  finalizer cannot recover its actual admitted counter from the Process.
- Carry the trusted capability slot/generation in the WAIT command alongside its
  child TaskId; Park must retain the exact lookup identity.
- Define a checked Waiting-to-Dispatching installation transition or separate wake
  installation helper. The proposed put_session accepts only Dispatching, while
  the wake description currently starts in Waiting.
- Preserve Fault when final accounting exhausts budget. Actual run_turn substitutes
  Budget only when no terminal reason exists; the draft's broad “otherwise” rule
  must not override that existing behavior.
- Extend the compiler-ordering proof to detached Session/Command/Settlement storage.
  Existing group_counter exposes opaque Process/Capture pointers, which must not
  silently be assumed to cover newly detached stack owners. Validate the chosen
  ordering in actual linked code before claiming admission/commit work is charged.

Catalog/capability commit rollback, structured Closing and exact hardware protocol
also remain separately pending; no core source edits are approved by this review.

The revised core-dispatch supplement resolves those five specific findings at the
design level. DispatchResult snapshots actual post-admission counters before any
fallible task helper or guard drop. WAIT owns the trusted PendingWait identity.
Wake validates first, takes Waiting into Dispatching, consumes once and publishes
Ready last. Fault precedence now exactly follows existing run_turn. A private
TurnWork stores the detached result before the final counter; explicit opaque
Group/Process/TurnWork/Capture addresses establish a new compiler-ordering review
obligation instead of assuming the old proof covers detached storage.

Approved this ownership/finalization boundary as one design component. This is
not complete core implementation approval: catalog/capability commit rollback,
typed internal errors, complete lifecycle and hardware protocol remain pending.
Actual source must still establish every exit reaches finalization, error snapshots
remain accurate, wake install failures close safely, and compiler ordering plus
Process/Slot size and 64 KiB stack bounds hold in linked artifacts.

## Preserved Session-stage full-suite failure

Independently inspected the first Session-stage runtime failure at
`target/evidence/20261026T225647-084005624-aarch64/runtime/runtime-64m`.
This is a genuine failed guest, not a timeout or merely a parser rejection:
QEMU exited 1 after about 4 seconds. The raw stream contains 21 completed HMRT
cases, then case index 21 `completion_tokens` fails its expected outcome.

Group 27 slot 2 turn 2 starts at counter 227603833 and finishes at 245182292:
17,578,459 ticks, or 281.255344 ms at 62.5 MHz, against 5,668,590 ticks
(90.69744 ms) remaining. The actual raw trace shows EL0 IRQ at 0x10001099c,
PPI 30 acknowledgement/EOI and restored controller sequencing. Budget accounting
terminates that owner with zero calls/written bytes and reclaims its 36 frames.
Its peers subsequently exit; free frames reach 3241 before the held-completion
assertion expects all four owners to have exited normally. The tail is an EL1
failure semihost exit. Recorded raw artifact hashes match.

The kernel binary is identical to the previously reviewed runtime. These records
establish an unexpectedly long measured interval and correct fail-closed budget
outcome, but cannot establish a historical host pause or locate the delay within
that interval. Current host pressure readings are not proof of its cause. The
128 MiB sibling passing does not erase this failure. Full-suite acceptance for
the Session stage is therefore pending; preserve this result and do not weaken
budgets or expected outcomes to manufacture a pass.

## Final safe Session acceptance

Accepted the safe Session policy stage with the first full-suite failure explicitly
retained. The unchanged isolated runtime rerun passes both profiles; the subsequent
complete `make test-all` exits zero in 180.4 seconds. Durable evidence is in
`docs/evidence/task-admission/full-second`. Independently checked all 82 ARM and
17 x86 archived result files, the exact 16-case host stdout/execution envelope,
and all 13 actual-library compiler clients per architecture. Negative raw
compiler diagnostics contain the expected private-field/construction, no-Clone
or moved-owner errors; three ownership-positive clients compile. The full log
contains 210 Python validator cases per target.

All recorded source hashes and both archive hashes match. Independently hashed
all 23 linked kernels again: each remains identical to the prior accepted runtime
baseline. The new Session admission remains unused by core execution. This
supports local policy/API and regression acceptance, not EL0 task creation or
core waiting/teardown acceptance. The repeat pass does not explain or erase the
preserved first runtime timing failure, and no historical host-pause cause is
claimed. Remote CI remains unverified.

## Authority/catalog draft findings

The proposed boxed catalog/authority lifetime and local scalar reservation before
atomic child/cap publication are reasonable design components. Full authority
approval remains pending two concrete policy details: store the console-delegation
permission in the actual authority (or explicitly freeze it as always enabled),
and define existing public Group mutation behavior while task authority exists.
Unrestricted external child reap could invalidate a parent's capability or
PendingWait; ordinary admission could consume reserved capacity. Freeze a private
setup/empty-group precondition plus restricted root-only administrative operations
or exact relationship reconciliation. Typed setup errors and complete hardware
protocol remain pending. No core implementation approval follows from the safe
Session stage's passing evidence.

The revised authority supplement resolves those two component findings. Persisted
LaunchCap.console_allowed governs both the actual envelope and console-delegation
checks. A fresh task-mode constructor cannot convert populated ordinary groups;
TaskActive blocks public admission and restricts administration to the exact root.
TaskClosed permanently rejects mutation after root reap, preventing accidental
ordinary-mode fallback. Private child operations remain relationship-aware and
lease-bound. Root completion stays hidden during Closing; metadata charge persists
until root reap or Group Drop. These are acceptable component design choices.

Before concrete API freeze, name the constructor's executor borrow lifetime in
its returned Group instead of ambiguous `'_` across multiple reference inputs;
root_image must remain an independent temporary borrow. Typed errors, exact root
Closing reports, actual charge/layout proofs and the complete hardware protocol
remain pending before core implementation approval. The logical 512-frame test
must use measured legal image charges and is distinct from physical Pool pressure.
