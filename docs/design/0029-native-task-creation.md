# 0029: native task creation and owned completion

Status: **standalone codec design approved; Session/core integration draft and
unapproved**, 2026-10-26. The scoped [independent review](../reviews/0029-native-task-creation.md)
authorizes only the standalone codec described below. It builds on the locally verified [0028 runtime](0028-native-process-runtime.md),
not a replacement scheduler or another capability metadata model. Independent
review must approve the wire, safe policy extension and core mechanism before
code. Phase 3 exit requirements, services/IPC, Linux processes and SMP userspace
are not claimed.

## First real workload and scope

An actual isolated EL0 supervisor receives explicit creation authority, creates
an independent finite worker and a non-cooperative loop, observes quota
exhaustion, waits for the worker's owned result, requests cancellation of the
loop, waits for its cancellation completion, and exits. Repeat under real pool
exhaustion and after faults. Kernel test code must not create those children on
the parent's behalf outside its actual SPAWN calls. An ordinary `run-task
<path>` launcher exposes this opt-in authority; existing `run` and `run-group`
retain their present contracts.

Initial scope is one ARM CPU, one root supervisor per task-enabled group,
maximum two outstanding children, no child creation grant, no delegation,
handle transfer, shared pages or arbitrary executable bytes. Group capacity
remains four, counting live tasks and unreaped completions; the unused fourth
slot is not implicit authority to create a third child. All normal resource,
W^X, fixed private output and restoration invariants remain those of 0028.

This proposal deliberately uses **grant-local program numbers** instead of the
path-copy sketch in 0028 C. A grant refers to an immutable kernel-admitted
catalog, not an ambient filesystem namespace. Initial catalog numbers 1/2 name
finite worker/loop images. Exact archive paths and hashes are part of the catalog
artifact. This is an explicit design choice requiring review, not a claim that
path-based spawn or a VFS exists.

## Wire proposal

Keep the existing v1 `svc #0`: x0 version 1, x8 operation, x1/x2/x3 arguments.
WRITE 1 and EXIT 2 are unchanged. Add SPAWN 3, WAIT 4 and CANCEL 5 only in a
task-enabled group. Ordinary legacy and 0028-only groups continue to return
Operation for these three operations. A task-enabled child recognizes them but
has no creation grant; forged/foreign handles fail in its own table. All replies
use x0 status and x1 result; x2..x30, SIMD and the documented resumed NZCV
semantics stay unchanged. No compiler Rust layout crosses the boundary.

Existing statuses 0..7 retain their meanings. Proposed additions are Denied 8
(valid capability lacks requested rights), Resources 9 (quota, slot, backing
memory or issuance capacity unavailable), and Pending 10 (poll has no completion).
Stale, foreign, wrong-kind, zero or unknown handles all return Handle 3; there is
no global task lookup or address oracle. Argument 7 covers noncanonical scalar
fields and unsupported catalog numbers. Length 5 covers request/payload lengths;
Address 4 covers a validated-length user range that cannot be completely copied.

### SPAWN

x1 is the launch-grant handle, x2 points to exactly 64 bytes, x3 must equal 64.
All fields below are little endian; no alignment promise is required of the
user pointer. Copy the entire request before interpreting fields.

| Offset | Width | Meaning |
| --- | --- | --- |
| 0 | u32 | request version 1 |
| 4 | u32 | request size 64 |
| 8 | u32 | grant-local program number, initially 1 or 2 |
| 12 | u32 | flags: bit 0 input present, bit 1 request child console; all other bits 0 |
| 16 | u64 | input address |
| 24 | u64 | input byte length,0..256 |
| 32 | u64 | child's maximum charged frames,1..128, inclusive of box storage |
| 40 | u32 | requested completion rights: WAIT 1, CANCEL 2; accept 1 or 3 only |
| 44 | u32 | zero |
| 48 | u64 | zero |
| 56 | u64 | zero |

Absent input requires address=length=0. Present empty input also requires
address=length=0 and remains distinguishable by bit 0. Present nonempty input
must be copied completely into an owned bounded `Input` before allocation.
The child receives exactly those bytes in the existing 0027 extension, never
an address into the parent. Console is a request for a newly issued scoped
child grant, not permission to copy a parent's token.

On success return Ok 0 and a child-completion handle in x1. Publish the child
and its capability atomically at the kernel-turn boundary: never a runnable
child without an owning completion entry, or a successful handle without its
child. Error returns x1=0 and leaves no child, frame charge or completion slot.
Native console generations may be burned by a late prepare failure, as today;
no generation rolls back.

### WAIT

x1 is a completion handle. x2 is 0 for blocking wait or 1 for poll; x3 must be0.
Only a WAIT right permits either mode. A terminal child returns Ok 0 plus the
completion word below, consumes the handle and reaps its group slot exactly
once. A subsequent wait/cancel using it is Handle 3.

A nonterminal poll returns Pending 10/0 without consuming authority. A
nonterminal blocking wait parks the parent after accounting for this request.
The saved continuation is its SVC successor; no response is fabricated until
completion. Retain only the target internal TaskId and the parent's capability
slot/generation, never a user pointer or output-buffer borrow. On wake, validate
both identities again, consume/reap the completion, and set x0/x1 once in the
stopped parent's owned register context. Do not count wake-up as another call.

Completion x1 uses bits 0..7 kind:0 exited,1 fault,2 active budget,3 call limit,
4 turn limit,5 cancelled,6 unavailable. Bits 8..15 contain exit status 0..255
only for kind 0; otherwise 0. Bits 16..63 must be0. This is a scalar outcome, not
an arbitrary result buffer or IPC payload.

### CANCEL

x1 is a completion handle with CANCEL rights; x2=x3=0. For a live child, mark
pending cancellation and return Ok 0/0. Repeated pending requests return Ok 0/0.
If already terminal but not waited/reaped, return Ok 0/1. Cancellation never
consumes completion authority and never frees active ownership. A future selected
quiescent turn performs reclamation and publishes Cancelled. Existing terminal
outcomes are not rewritten to Cancelled. A WAIT-only handle returns Denied 8.

## Authority delivery and identity

Keep the entire original 64-byte startup prefix unchanged, including size 64,
console feature bits and reserved zero. Do not append fields into padding that
the existing full-tail codec validates. An opt-in root supervisor receives a
present 64-byte 0027 input payload with this separately versioned envelope:

| Offset | Width | Meaning |
| --- | --- | --- |
| 0 | 8 bytes | `HMTASK\0\0` |
| 8 | u32 | envelope version 1 |
| 12 | u32 | envelope size 64 |
| 16 | u64 | actual issued launch capability |
| 24 | u32 | maximum input bytes 256 |
| 28 | u32 | maximum outstanding children 2 |
| 32 | u32 | maximum per-child charged frames 128 |
| 36 | u32 | group frame limit 512 |
| 40 | u32 | finite-worker catalog number 1 |
| 44 | u32 | loop catalog number 2 |
| 48 | u64 | bit 0 permits requesting child console; other bits 0 |
| 56 | u64 | zero |

The payload is constructed from actual core-issued authority and copied into the
root startup frame before any entry. Existing arbitrary input APIs do not mint
this authority: spelling these bytes in an ordinary program cannot create a
matching table entry. Children receive their requested job bytes, not this
envelope, and have no launch-grant entry.

Existing console handle slot 1 stays unchanged. Parent-owned task entries use
low 32-bit slots 2 for Launch,3/4 for its two child completions. Upper 32 bits are
checked nonzero generations from a new persistent checked task-capability issuer
in the existing Executor. This namespace is independent of console Issuer and
internal group/slot TaskId generations. Lookup always uses the actual selected
process's table, then slot, generation, kind and rights. Handles are not bearer
permission to look up another process's table. Copying a token into a child's
input confers no authority there.

No public capability constructor, raw mutable table, borrowed Frame or token-
indexed mutable Process is exposed. Generation wrap retires new issuance rather
than reusing an old token. Root-grant construction can burn an unpublished
reservation on failure; failed SPAWN does not publish or reuse any reservation.
Successful reuse after WAIT requires a fresh task-capability generation as well
as the existing internal slot generation.

Launch policy contains the catalog, child count 2, per-child frame ceiling 128,
group ceiling 512, permitted completion rights 3 and console delegation permission.
A requested WAIT-only completion is useful attenuation; cancellation by the
kernel's structured owner teardown remains possible even without a user CANCEL
right. No descendant receives launch authority, so the initial graph has depth 1.

## Admission ordering and shared call policy

All attempted SVCs share the **same existing Session call counter**. Do not add
another 64-call budget for task operations or intercept them without charging.
The existing console byte counter remains owned by that same Session.

Preserve old WRITE/EXIT and legacy-group behavior exactly. For new task
operations, count the attempt first, then validate version and operation. A
valid new task operation at attempt 64 terminates the parent with CallLimit
**before task side effects, user-memory copies or child issuance**. There is
no 64th SPAWN followed by an immediately unreachable child handle, and no 65th
ERET. Invalid requests still count; call-limit terminal precedence is explicit.
This task-op distinction from legacy 64th WRITE must be covered by policy tests
and actual EL0 exhaustion fixtures.

For attempts 1..63 use this order: version/opcode; capability existence/kind;
rights; scalar flags/length; whole request copy; request header/reserved/canonical
fields; catalog membership and delegated console/rights policy; whole payload
copy; declared child count, completion-slot, group-frame and requested-child-
frame limits; issuer capacity; actual load/prepare/box allocations; commit.
A failed earlier check performs none of the later effects. For WAIT/CANCEL,
validate reserved scalars before looking at child state; a valid handle's missing
right remains Denied rather than exposing target state. Bounds errors return
normally, never a panic. Clock/platform corruption retires authority using 0028,
not a misleading Resources reply.

The exact safe Rust Session accounting extension remains a review item: proposed
nonforgeable, non-Clone turn-local call guard borrows Session, admits one task
operation and is consumed by reply/park/stop completion. It must end before
returning the group turn; **no self-referential borrowed guard is parked**.
An abandoned guard fails closed. The core owns the parked continuation as scalar
state; wake-up installs the already-accounted result without redispatching a
request. Reuse existing Session counters/stop logic rather than duplicating its
policy. Freeze the guard/drop API before implementation, including invalid
version/opcode behavior on attempt 64.

## Core mechanism and scheduling

Prevalidate the complete immutable archive and allowed ELF descriptors before
root EL0 entry. SPAWN performs a bounded catalog lookup, not full archive parsing
on each request. Keep descriptors in bounded owned kernel storage or borrow
immutable artifacts; do not retain caller-supplied temporary Images. Descriptor
storage is charged and its constructor stack audited. Catalog admission failure
occurs before publishing root authority.

A task-enabled group still owns individual boxed Processes. Add only bounded
slot metadata: creation/parent relationship, at most two completion-cap entries,
and Ready/BlockedWait/CancelPending/Closing/Completed state. Tables are not
arrays of inline Process values. Process growth, box rounding and all persistent
metadata storage require a fresh measured-size oracle and accounting review.

The selected parent is already stopped and the boot root restored after SVC.
SPAWN retains its current BUSY lease while executing a **private lease-bound
admission helper**. Public Group::admit reacquires BUSY and cannot be called
recursively. Do not release BUSY to make it work, create another Group, or invoke
a caller callback. End/split field borrows safely so Session/parent Space can
supply the owned request/input, then release those borrows before mutating free
slots and creating a child. The parent Process remains owned throughout.
A private command/result transition between safe policy and the owner group is
preferable to giving a generic Host arbitrary mutable Group authority.

SPAWN's validation, user copies, ELF/backing copies, allocator work and commit
are part of the parent's active turn. It must not reset the parent's budget
around construction. Actual terminal UART output remains charged; bounded
diagnostic capture/serialization follows the reviewed 0028 rule. No Rust executes
under the user root. New children have their own 100-ms active budget and 1-ms
quantum, never a refreshed portion of the parent's counter. The 512-frame group
cap still includes all roots, user pages, startup and box storage.

Blocking WAIT charges its admission/parking turn; parked wall time is uncharged.
Ready peers continue round robin and an uncooperative child remains timer
preemptible. A blocked parent keeps every frame and its remaining budget.
Completion wake-up and Closing bookkeeping are explicitly uncharged quiescent
management, like existing post-finish reclamation. Each service step examines
at most two child entries and performs only bounded metadata checks, scalar
reply/context updates, and existing quiescent owner release. It performs no
allocation, user-memory or bulk payload copy, UART output, or callbacks.
Diagnostic emission stays separately deferred. All SPAWN validation, input/ELF
copying and allocation remain inside the parent's charged interval. Report this
exclusion plainly: the active-turn budget is not total kernel CPU time.
The awakened parent reenters only through the existing sealed transition with
fixed SPSR sanitization and restored owned register state.

Each task retains the 256-turn cap. Call attempts are counted once at request
admission, including polls and failed requests. A64th task call cannot park.
If the parent budget expires during an otherwise valid SPAWN, apply 0028 terminal
precedence to its normal reply: the parent ends Budget before another ERET,
and structured teardown cancels any newly committed child before that child
can enter. No successful-but-lost child leaks out of the owner scope.

With one-level parent-to-child WAIT and no child creation grant, a dependency
cycle cannot be created. Scheduler Idle must distinguish no runnable tasks from
all terminal: a blocked parent with a live child cannot be silently treated as
complete. Unexpected all-blocked states fail closed with retained ownership;
do not fabricate completion or release frames to manufacture progress.

## Completion, parent death and teardown

Child terminal behavior remains 0028: reclaim its Process and box only after
verified quiescence, retain copied completion plus occupied group slot until
WAIT/reap. Its parent completion-cap entry still owns the result. A suspended
child cannot be freed by a CANCEL request; the selected cancellation turn is
the reclamation boundary. A pending WAIT is woken exactly once only if parent,
capability and child generations still match.

Parent Exit/Fault/Budget/CallLimit/cancellation closes its launch grant and all
user lookup rights immediately. Reclaim the quiescent parent Process, but hold
its scalar final outcome and relationship metadata in **Closing** while children
are cancelled/reaped. That metadata never points into freed parent memory.
Request cancellation of every live child, service each under minimal boot-root
proof, consume already-held completions, then publish the parent's completed
result to its kernel caller. Do not change the parent's original terminal reason.
No orphan policy, reparenting or recursive creation is implied.

Executor retirement prevents new entry/creation across group destruction;
quiescent cancellation/cleanup remains permitted as in0028. Never report final
resource equality or parent completion while a child still owns frames. If the
minimal context proof fails, retain owners and fail closed; uncertainty about
actual restoration is fatal, never an ordinary reclaim path. Group Drop may
release only inactive owners; forgetting an owner can leak bounded claimed
resources but cannot leave an active root or permit allocator reuse.

## Workload and validation proposal

First ordinary workload: `run-task bin/supervisor` receives the real grant,
spawns program 1 with an owned binary job (e.g. sum 1..10 returning exit 55), then
program 2 as a busy loop. A third outstanding spawn must return Resources without
changing generations/frames. Blocking WAIT of worker 1 returns exactly exit 55;
CANCEL of worker 2 is acknowledged, repeated cancellation is safe, and WAIT of
worker 2 returns Cancelled. The parent prints results through its own console and
exits 0. Jobs must be copied before the parent overwrites its original buffer.
No kernel-side loop may substitute direct child admission for observed SPAWN
instructions. This is native task creation, not Linux fork/exec or service IPC.

Required hardware matrix, to be frozen in a separate test contract:

- Parent's actual SPAWN SVCs precede distinct child first ERETs; root/input/frame
  identities and hashes come from independently built fixtures and archive.
- Finite worker plus loop demonstrates both copied job isolation and timer peer
  progress. Blocking WAIT resumes only after the matching child completion.
- Poll returns Pending without consuming; completed WAIT consumes once; stale,
  foreign, wrong-kind and missing-right handles have no cross-owner effects.
- Child attempts parent's launch/completion tokens and cannot create/cancel/wait
  outside its table. WAIT-only cap cannot cancel; forged catalog numbers fail.
- Empty/absent/max 256 payloads, oversized 257, overflow/cross-page request/input,
  reserved bits and malformed lengths fail transactionally with exact precedence.
- Quota/held-result exhaustion plus actual physical pressure reaches child load,
  startup and box failure while the parent remains runnable. No output/handle
  success or leaked child follows rollback; generation burns are explicit.
- Parent attempt 64 SPAWN performs no child side effect; all earlier WRITE/error/
  task calls share one limit. No65th ERET or extra quota after wake-up.
- Parent faults/exits/is cancelled while a child loops or a completion is held:
  bounded cascade, no later child entry, exact reclamation before root completion.
- Cancel while parent is blocked, child completion near cancellation, slot reuse,
  and stale completion tokens preserve exactly-once delivery and quiescence.
- Every existing legacy/process/startup/group profile remains required. Audit all
  affected compiled stacks before guests; never increase 64 KiB to fit tables or
  nested child construction. A real ordinary command and strict raw exception
  trace gate accompany the special hardware profile.

Safe-stage work can implement exact endian request/envelope/completion codecs,
rights validation and shared-call admission tests once reviewed. Those helpers
must be directly used by actual core dispatch, not presented as completed spawn.
Core work then introduces bounded catalog/cap owners, lease-bound admission,
parking/wake/Closing transitions and actual parent fixtures. No core change or
safe-policy API is approved yet.

## Standalone safe wire codec: proposed implementation boundary

This subsection can be approved separately from Session and core integration.
Implement `hm_kernel::task_wire` with `#![forbid(unsafe_code)]`, no allocation,
callbacks, process references, capability table, or mutable global state. These
values describe bytes; constructing or decoding one grants no authority.
Do not change native Session, native Status, core dispatch, or startup encoding.
Only export the module from the safe crate.

Constants are `SPAWN`, `WAIT`, `CANCEL` (u64), `SPAWN_SIZE` and `GRANT_SIZE`
(usize), `MAX_INPUT` and `MAX_CHILD_FRAMES` (u64), `MAX_CHILDREN`,
`GROUP_FRAMES`, `WORKER_PROGRAM` and `LOOP_PROGRAM` (u32), with values from
these tables. All public enums and value structs derive Clone, Copy, Debug,
Eq and PartialEq. Spawn getters are `program`, `input`, `console`, `max_frames`
and `rights`; Grant getters are `handle` and `console_allowed`. Getters and
encoders consume copied values; `Rights::bits()` returns the explicit u32 wire
value. Neither Rust enum discriminants nor struct layouts encode wire data. `Rights` is a closed value enum `Wait` / `WaitCancel`, encoded as 1 / 3.
`Spawn` owns private program (`u32`), optional input `(u64 address, u64 length)`,
console (`bool`), maximum frames (`u64`), and rights fields, exposed by copied
getters. It is Clone/Copy metadata. `Spawn::new(program, input, console,
max_frames, rights)` validates canonical input and frames; any program number
is representable because membership is checked only against the actual grant.
`Spawn::decode(&[u8])` and `encode_into(&mut [u8])` use exactly 64 bytes.
An encoder rejects a wrong destination size before touching any byte. A
successful encoder overwrites every byte, including all reserved fields.

The codec error enum is `Length, Header, Flags, Reserved, Rights, Frames,
Input, Address, Handle, Limits, Completion, Arguments`. It is metadata, not a
new syscall status enum. Future dispatch maps Length to native Length and
Address to native Address; all other descriptor errors map to Argument.
Envelope/completion decode failures are not themselves syscall responses.

Spawn decode validation order is: exact slice size (Length); version and size
fields (Header); unknown flag bits (Flags); all reserved fields (Reserved);
rights encoding (Rights); frame range (Frames); input length <= 256 (Length);
input presence/empty canonicality (Input); nonempty range addition overflow
(Address). Absent input and present-empty input require both address and length
zero. A nonempty present range may contain any non-overflowing address, including
zero: hardware accessibility is exclusively a later core-copy check. The
constructor checks frames, then the same input rules. Encoding a private
validated value has no further fallible checks beyond destination size.

`Grant::new(handle, console_allowed)` describes an envelope for the fixed initial
limits/catalog in the table. It accepts only a nonzero upper-32 generation and
low-32 slot 2, with no implication that the token has been issued. Its private
fields have copied getters. Grant decode order is exact slice size (Length),
magic/version/size (Header), reserved u64 (Reserved), feature bits (Flags),
canonical handle (Handle), then every fixed limit/catalog value (Limits).
Encode/decode uses exactly 64 bytes and the same no-mutation-on-size-error rule.
The ordinary Input codec remains the sole outer envelope encoder.

`Completion` is a copied scalar enum `Exited(u8), Fault, Budget, CallLimit,
TurnLimit, Cancelled, Unavailable`. `encode() -> u64` and
`decode(u64) -> Result<Completion, Error>` use the exact bit layout above;
unknown kinds, nonzero high bits or an exit byte on a non-exit kind return
Completion. This carries no child identity and cannot consume a real result.

`WaitMode::decode(mode, reserved)` accepts only mode 0/1 and reserved 0,
returning Blocking/Poll or Arguments. `cancel_arguments(address, length)`
accepts only two zero values. These helpers do not validate handles or rights.

Required standalone tests use literal independently specified wire bytes,
unaligned slices, exact full-buffer overwrite and unchanged wrong-size output,
all input lengths 0..256 plus oversized and overflowing ranges, absent versus
present-empty, every reserved region/unknown flag, invalid rights/frame bounds,
unknown program numbers preserved, non-authoritative grant token/limit checks,
all exit statuses and malformed completion words, and scalar WAIT/CANCEL
arguments. Combined-defect tests pin precedence. Mutating/dropping source bytes
must not change decoded owned values. This test scope establishes codecs only;
the hardware acceptance remains the separate 0029 test contract.

## Explicit decisions still requiring review

1. Approve grant-local program IDs versus bounded path strings, and opt-in
   `run-task` authority issuance rather than changing ordinary run semantics.
2. Freeze the safe Session guard/command API, including abandonment and 64th
   invalid-version/opcode behavior, before editing native.rs or core dispatch.
3. Confirm limits 2 children/128 frames each/512 group and the exact 64-byte spawn
   and authority envelopes. Catalog-storage placement/charge needs concrete
   compiled-size and construction-stack analysis, not a large local array.
4. Freeze syscall trace birth/park/wake/reap/closing records and deterministic
   fixture scripts with independent review before hardware implementation.
5. Decide whether future creation grants can be delegated. This first version
   explicitly cannot; do not accidentally implement recursive authority now.

These are design review decisions, not evidence of an implemented or verified
Stage C. The [0029 test draft](0029-task-creation-tests.md) is complementary;
it must agree on the final wire and ownership semantics before code.
