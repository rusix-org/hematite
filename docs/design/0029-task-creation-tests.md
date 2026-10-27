# 0029: actual userspace task creation acceptance

Status: **draft; pending 0029 wire/API freeze and independent review**. No code,
build, guest execution or passing result is implied. This proposes the hardware
acceptance for [0028 Stage C](0028-native-process-runtime.md#c-actual-scoped-creation-and-owned-completion-requirements).
The eventual 0029 mechanism/wire document owns operation numbers, layouts,
limits, error precedence and lifecycle semantics. This document must be aligned
with that document before either implementation or a supposedly strict parser.

The concrete proposed case inventory and record grammar now live in
[0029 task protocol](0029-task-protocol.md). Its explicit review questions
supersede the exploratory inventory below; neither document authorizes core code.

## What must actually run

An admitted ARM EL0 parent receives explicit launch authority, invokes native
SPAWN at a real SVC, receives an owned completion capability, and uses WAIT and
CANCEL through further real SVCs. At least two children must enter independent
address spaces. One returns an independently checked result while another is
interrupted by the physical timer, resumed at its saved PC, and cancelled. The
parent resumes after waiting, checks the returned outcome in EL0, and exits
successfully. Subsequent ordinary launch must still work with exact original
free frames. Kernel-side admission of several images, mock Host callbacks, a
safe handle model or plausible serial messages cannot substitute for this.

Reuse the independently reviewed Stage B ownership, capture and trace gates.
The new claim is the connection from an authenticated current EL0 caller through
wire decoding, bounded copy, real allocation, scheduler ownership, wait/wakeup
and teardown. Existing register/context isolation remains a required regression;
it need not be reimplemented as an unrelated second scheduler or fixture set.

## Proposed bounded inventory

Run each final scenario twice in one guest at one CPU / 64 and 128 MiB, followed
by the complete current bootstrap baseline. The table proposes twelve scenario
names, not a frozen BEGIN count. Split malformed-input subcases into separate
short-lived parents if the existing 64-call budget cannot contain their exact
inventory. Do not enlarge/reset the native call budget to make the test fit.

| Proposed case | Real execution and acceptance |
| --- | --- |
| `spawn_wait` | Parent spawns two distinct-input workers, obtains different live child capabilities, waits for their independently prescribed outcomes, consumes both completions, and exits 0. Child output/result must establish copied input identity and distinct execution. |
| `wait_ready` | Child completes before the parent's consuming wait. Child frames have already been reclaimed, while its completion/slot remains owned. Wait returns the exact stored outcome once; a repeated wait is stale. Reuse the slot and reject the old capability without disturbing the new child. |
| `wait_parked` | Parent waits while its child is a real polling/busy task. No ERET for the parked parent occurs before the permitted wake event. A peer progresses while the parent is parked. Its next actual entry is the saved post-WAIT SVC PC, with the specified reply, never the entry point or an arbitrary restart. |
| `cancel_live` | A child receives at least two actual timer interrupts at the independently named loop interval, with an exact saved-PC resume and peer progress. Parent CANCEL requests termination; it does not immediately free an owned process. The scheduler services cancellation after quiescence, stores Cancelled, and wakes/answers the parent's wait exactly once. Include the specified repeated-cancel behavior. |
| `copied_request` | Parent places the versioned program-ID/input descriptor and payload in its own RW pages, spawns, then overwrites that storage before a deliberately deferred child consumes startup input. The child verifies the original bytes and presence/length, including an embedded NUL in binary input if the wire permits it. Parent storage is not retained. Absent and present-empty remain distinct under 0027. |
| `authority` | An ungranted child attempts SPAWN using the actual parent's grant token copied intentionally into its startup job; it must fail as the selected child principal. The parent supplies an actual owned sibling-completion token to another child, whose WAIT/CANCEL fail without changing that sibling. Also reject console/launch/child token kind confusion, zero/forged/stale generations, and program IDs outside the actual grant allowlist. The rightful parent subsequently waits/cancels successfully. |
| `bad_request` | Execute exact version/opcode/reserved/length/program-ID/pointer faults through real SVCs, including arithmetic overflow and a descriptor or payload crossing the last mapped byte into a guard. Require the frozen error precedence and zero child entry/ownership effects. A valid spawn after the failures proves caller/runtime survival. |
| `held_capacity` | Parent fills the allowed live/held-completion capacity. A further spawn fails with the specified capacity result even after child frame reclamation when slots remain held. Consuming one completion admits a fresh child; stale old authority remains invalid. Do not confuse frame availability with reusable task/handle slots. |
| `allocation_rollback` | A real EL0 SPAWN reaches actual load, startup and boxed-owner allocation failures under retained Pool pressure. Parent pages, current context, live children and capabilities survive. No failed child enters. Remove pressure only while inactive, retry through a new actual parent SVC, and require successful execution plus exact eventual reclamation. Separately test the declared group/launch frame cap. |
| `parent_exit` | Parent exits with live children and held completions. Launch authority is closed; structured cancellation services every remaining live owner before group destruction. No orphan enters after teardown; all parent/child frames and metadata are reclaimed. A fresh group/parent can spawn successfully. |
| `parent_fault` | Parent faults at a pinned own-RX write with live children; the exact exception triggers the same structured teardown. Include parent budget/call-limit/cancellation variants with children resident, including a parked parent when blocking wait is adopted; shared cleanup code alone does not replace actual terminal-path coverage. Children cannot continue indefinitely because the parent ceased to own their completions. |
| `child_fault` | One child faults at an independently pinned PC/FAR/syndrome while another progresses. The parent receives the fault outcome through WAIT, consumes it, verifies the peer outcome and exits 0. Child failure must not destroy parent authority or another child's session. |

The proposed mechanism selects grant-local program IDs, not caller path lookup.
The host binds each permitted ID to its immutable archive member and ELF hash;
reject unknown IDs before allocation. Two child handles per parent are the
current draft limit, subject to final wire review. Descriptor byte order, size,
reserved fields and copied input remain explicit wire boundaries.

The wire freeze must decide whether WAIT parks or is initially a nonblocking
poll. If it is nonblocking, replace `wait_parked` with exact pending replies,
real parent/child interleaving and eventual consumption; do not claim wakeup,
blocked-parent accounting or parked-PC preservation from a polling loop.

For `copied_request`, the scheduler must provide an existing reviewed way for
parent overwrite to precede child read. If round robin necessarily runs the
child first, use a child that first yields/parks before reading its already
copied startup input, and correlate those actual turns. Do not assume favorable
host timing or add an arbitrary user-memory mutation hook.

## Authority tests without invented identities

The current selected task, restored by the core, is the request principal.
No fixture-provided parent ID authorizes a request. Distinguish at least:
launch grant, parent-owned child completion capability, per-process console
grant and private core TaskId. Their numeric representation is a wire decision;
the host checks kind/owner/generation rather than treating equality of an
unrelated diagnostic number as authority.

Use actual returned tokens. An EL0 parent can first spawn child A, then copy A's
returned completion token and its own launch token into child B's owned startup
job. B attempts forbidden operations against those exact tokens, reports the
expected denial, and exits. The parent subsequently exercises its real rights.
This uses three task slots, needs no shared memory, forged actor argument,
secret token prediction or test-only authority injection. Whether children may
receive attenuated launch grants is unsettled; the first suite can deliberately
grant none. Do not test an unsupported delegation surface by inventing one.

Cross-group stale wire-token tests need a defined generation/namespace contract.
If wire values can repeat safely in distinct owner tables, copying an old scalar
alone does not establish that it must be rejected in every new table. Freeze
whether rejection is globally guaranteed or scoped to the current parent, then
construct the exact test. Within-parent reap/reuse rejection is mandatory. Four group slots include the
parent and held completions, rather than allowing four additional children.
With blocking wait, owned child-tree authority must prevent wait cycles; prove
that the permitted graph cannot put every remaining live task into a cycle,
and reject attempts to wait for self, parent or a foreign peer.

## Real allocation failure and lifetime evidence

Derive child charge from its independently parsed ELF page count, existing
startup allocation and current compiled owner-layout witness. Retain actual
Pool allocation owners while the parent already exists; global free count is
not the group's logical capacity. When the current allocation path remains
Space-load then startup then boxed owner, leave respectively 0, n and n+1 free
frames, where n is that child's actual loaded Space charge. Include any new
owned spawn-request/handle-table allocation explicitly: do not reuse the old
formula after an ownership change. Pressure must reach the claimed stage,
not be intercepted by a total-Pool-free precheck and called partial rollback.

Freeze generation issuance points independently from frame rollback. Failed
prepare may consume an issuer generation even when all frames are restored;
that is neither a successful child nor permission to accept unexplained skips.
Handle-table capacity must reject before child commit, or roll back every newly
owned child allocation if the frozen design permits a later handle-commit
failure. Test the actual reachable path; do not invent a failure seam to claim
an impossible production failure.
Track parent and existing-child charges separately, compare each terminal's
reclaimed owner charge, and compare group teardown with the common pre-group
baseline. Held scalar completions are not live process frames and are not
reusable slots until the consuming operation specified by the wire.

A test driver may install/release pressure only through existing owned Pool
APIs while no process is active. Its trigger is an actual named parent trap
and bounded scenario phase. It may not alter caller registers, saved PCs,
status replies, handle tables or child completion. Any new observation or
control hook needs its own fixed authority/signature and review before use.

## Proposed evidence stream and strict trace gate

Keep diagnostics buffered and emitted after charged execution, following the
reviewed Stage B correction. Console payload output remains actual copied
userspace bytes. No writer callback, serial formatting inside the budgeted
interval, fake completion, relaxed deadline or metadata-only scheduling proof.

Reuse actual CLOCK/ENTER/TRAP/CHARGE/RETURN records and their ordering. Add only
bounded records needed to bind the actual caller and wire effects:

- Request: selected group/slot/generation, per-task call ordinal and actual SVC
  successor PC; frozen version/opcode/scalar arguments. Record copied request
  lengths/digests or bounded exact bytes when needed to establish program ID/input.
- Spawn commit or rejection: actual caller, issued capability kind/generation,
  resulting child identity/fixture/input, independent root and exact charge,
  or the precise error and unchanged ownership. A private runtime fact record
  must be emitted from the real commit path, not a caller-authored prediction.
- Wait pending/park/resume/consume: owner and child identity, retained completion
  state, exact returned outcome and consumption point. A wake record alone
  does not count as execution; require the later matching ERET and EL0 check.
- Cancellation/parent terminal: request versus service, affected real owners,
  each no-entry terminal or final quiescent transition, and exact reclamation.

These are required semantic fields, **not approved new record names or schema**.
Before implementation freeze exact ordered field lists, case count, permitted
per-case action sequence, opcode/status inventory, fixture symbols and bounded
IRQ/turn counts. The existing public Group::admit acquires BUSY; it cannot simply be called
recursively from dispatch while the current turn holds that lease. Any sealed
internal stopped-parent admission phase is a mechanism change requiring
independent design/source review before this test plan can claim its safety.
Every successful SPAWN must match one admitted child; every
child ERET must match a successful admission; every outcome must match an actual
terminal event and one permitted consumer. Unknown, duplicate, missing,
reordered or extra records fail. Caller identity must agree with the immediately
selected task and its actual hardware entry, even when two parents use the
same virtual addresses or SVC instruction site.

At every entry require the exact saved PC and private root belonging to that
owner. Match every lower-EL SVC/fault/timer event to QEMU's real vector, PC,
syndrome where meaningful and exact GIC setup/ACK/EOI/restore sequence. Parent
parking/cancel-only service has no fabricated EL0 transition. Counter timestamps
remain globally monotonic across CPU 0 owners; each owner is charged only its
actual active intervals, including bounded spawn dispatch, not parked gaps.
Preserve terminal-precedence and call/byte limits across spawn/wait/cancel.
All operations, including invalid spawn/wait/cancel requests, must use the
shared native Session call counter; an interception path cannot bypass the
existing 64-call limit. Include a parent whose 64th request is SPAWN, with the
proposed rule that task operations have no effect on the 64th request. Use
separate short-lived parents whose 64th request is SPAWN, WAIT or CANCEL;
require no child admission, completion consumption or cancellation side effect,
then actual parent terminal cleanup. Retain legacy WRITE/EXIT precedence in
the existing process regression gate. A distinct invalid-request variant must
reach the same shared limit. These are distinct bounded runs, not extra calls
packed into an already exhausted parent.
Their exact charging/dispatch-count rules are a wire freeze requirement.

No unknown trace prelude is accepted. Require the existing exact timer preflight,
then declared task activity, then the exact current bootstrap hardware inventory
and sole privileged semihost exit. Keep userspace semihosting explicitly off.
Independently inspect the first pinned actual trace grammar before enabling its
final acceptance flag; preserve failed candidates and raw logs.

## Fixture, ordinary workload and completion gates

Build dedicated static Rust-native parent/child ELFs using the pinned compiler,
with minimal assembly SVC/fault/loop shims and exported exact sites. Bind source,
ELF, symbols, LOAD permissions, call/fault/loop opcodes, manifest and deterministic
archive hashes. Require one whole read-only embedded archive and no separate raw
ELF fallback. Parent/child mutable pages may use the same VAs; no kernel address
symbols or arbitrary polling-release hook should be necessary for these tests.
Use actual user-memory copy for request/input bytes, with no Rust ABI exposed.

An ordinary monitor path should launch one parent program that performs the
same native operations, prints independently checked child results and returns
to the prompt. It should not require the monitor to pre-admit its children.
Freeze program name, allowlist/grant delivery, exact output and command syntax
with the mechanism owner; no unsolicited new shell interface is proposed here.
Require a fresh 128 MiB launch without diagnostic warmup, then repeated 64/128 MiB
runs, failure recovery and a subsequent existing legacy hello. Report this as
native scoped task creation, not Linux fork/exec, general IPC or SMP userspace.

Safe policy unit tests and actual-library compiler ownership/lifetime probes
remain mandatory alongside the hardware matrix. Add parser mutations covering
caller substitution, forged child success, request-copy races, wrong completion
consumer, stale handle reuse, premature frame/slot reclamation, missing wake/ERET,
budget reset, surviving orphan, missing GIC restoration and unexecuted cases.
Review every affected linked stack and actual owner size before boot. Preserve
all previous ARM/x86, native process, startup input, ordinary launcher and Stage B
runtime gates. Only the full accepted evidence establishes this milestone.
