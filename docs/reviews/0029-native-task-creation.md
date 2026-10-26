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
