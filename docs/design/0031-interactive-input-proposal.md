# 0031: an owned interactive console-input session

Status: **proposal, unapproved; no implementation authorization**. This follows
native task creation, not a claim that Phase 3, services or Linux compatibility
is complete. ARM64/QEMU remains primary; x86 advertises Unsupported until its
mechanism is separately designed. Existing native interfaces and tests retain
their current contracts.

## Useful first behavior and current limits

Add monitor command `shell` that starts an actual archive-backed EL0 program.
The program prints its own prompt, reads terminal bytes, edits a bounded line,
and implements `help`, `echo <text>`, `count`, and `exit`. `count` maintains state
across requests; this demonstrates one continuing userspace owner, not repeated
kernel-launched command fixtures. A later step gives this program the existing
scoped launch grant so it can run/wait/cancel workers. Do not call the first
built-ins-only experiment a POSIX shell or a filesystem/process shell.

Current monitor `read_byte` polls PL011, and run_monitor spins when empty.
Native Session only copies user bytes outward; no general writable-user-copy
API exists. Group Waiting owns a detached Session for child completion, not
console input. A process has 100ms cumulative charged execution, 64 calls,
4096 output bytes and 256 turns. Merely adding READ to this arrangement would
produce a short-lived demonstration or a busy-loop program, not useful input.

## Proposed authority and wire

Use an opt-in interactive constructor, not a global input syscall enabled for
all native programs. The existing Executor owns a single terminal-input lease;
monitor and interactive program cannot consume RX concurrently. No child inherits
that lease. Root principal identity and an exact nonzero generation are checked
on every read. Reuse the checked task-cap issuer, with low slot **5** for input;
console remains slot1, launch slot2 and completions slots3/4. Exhaustion refuses
new interactive sessions; it cannot wrap or reset existing namespaces.

Proposed READ operation is SVC0/version1/opcode6: x1=input token, x2=destination,
x3=capacity1..256. Success returns x0=Ok, x1=number of bytes1..capacity; other
register behavior matches existing replies. No zero-length success, EOF fiction,
line-mode flag, timeout pointer or caller-supplied callback. Invalid capacity is
Length; invalid principal/token is Handle; non-RW/unmapped/overflowing destination
is Address. Reuse Version/Operation precedence and count every attempt once.
Opt-in mode recognizes READ; legacy modes continue returning Operation.

The input grant is serialized as an application-specific envelope **inside** the
existing owned Input payload, not in unused startup-page bytes. Proposed exact
64-byte LE envelope: magic `HMINPUT\0\0` at0, u32 version1 at8, u32 length64 at12,
u64 input token at16, u64 flags0 at24, u64 maximum_read256 at32, u64 lifetime_call
limit4096 at40, u64 output_limit65536 at48, u64 reserved0 at56. The startup prefix keeps its existing 64-byte layout/version, while its advertised
call/byte limits must match this interactive Session's real configured limits;
legacy preparation retains its exact old bytes. The canonical tail codec remains
unchanged. This envelope grants
no authority by itself; only the matching core table does. Combining it with a
launch grant later requires a separately versioned envelope, not silently
reinterpreting the current 64-byte task grant.

## Bounded owners and read settlement

Propose one individually boxed `InteractiveState`, never a large retained stack
array: actual root TaskId, input grant, a 512-byte FIFO, monotonic event sequence,
optional pending read and explicit overflow/cancel/closing state. All storage is
charged to admission and reclaimed after RX quiescence. One pending read per
principal, one terminal owner globally; no public byte-injection method.

PendingRead owns the original Session and copied address/capacity/token identity.
It retains neither a Rust borrow of userspace nor an arbitrary pointer to access
later. Process/Space remains owned, mapping permissions immutable while parked.
A new private two-pass `Space::copy_into_rw` validates the complete range and all
owned RW backing pages before modifying any byte, then copies through those
owners under boot mappings. It never exposes mutable Frame storage, writes RX or
startup RO pages, or dereferences an EL0 virtual address directly.

READ admission validates version, shared call limit, handle, length and whole
writable range before parking or consuming bytes. Invalid requests leave FIFO
unchanged. Empty valid READ creates provisional ReadPark; final accounting and
retirement precedence apply before publishing WaitingInput. A terminal selected
turn discards provisional state and never subsequently wakes it. WaitingChild and
WaitingInput are distinct owned variants, never simultaneously populated.

If bytes already exist, copy at most256 into a bounded owned turn buffer, copy
into validated stopped-user backing, and commit FIFO removal/reply only after
successful final accounting. If the turn becomes terminal, its inaccessible
process may contain copied bytes but FIFO is not consumed and no reply is installed.
For a parked reader, arrival creates DeliveryReady, not a synthetic SVC or free
bulk-copy wake: the next selected **charged kernel delivery turn** performs the
same copy/commit. It has no invented EL0 entry/trap and no second syscall count.
Scalar notification may be uncharged bounded management; user copies may not be
hidden in the existing uncharged child-wake bookkeeping. Failed revalidation
retires the affected interactive authority and closes the owner safely.

## Real device waiting, cancellation and overflow

No EL0 retry loop and no kernel spin-until-byte loop. At bounded scheduling
boundaries, core checks/drains at most256 actual PL011 RX bytes into its FIFO.
While a process can enter EL0, UART RX interrupts stay masked and the existing
1ms timer quantum returns control. When no owner is runnable, a reviewed kernel
idle transition enables only the admitted receive wake source and a wall-deadline
timer, rechecks readiness, and uses WFI with a lost-wakeup-safe handshake. The
handler masks/acknowledges the source and returns to the boot-root coordinator;
Rust drains bytes afterward. The handler does not mutate a shared Rust FIFO.

This is a **new interrupt/idle mechanism**, not permission to reuse the timer
probe's busy wait. Exact PL011 source/configuration, GIC routing, acknowledge/EOI,
DAIF/WFI ordering, register save/restore and simultaneous-source rules need a
primary-spec-backed implementation design and independent review before code.
Existing process slices must continue rejecting unexpected IRQs; UART is enabled
only in the isolated idle window. WFI returning spuriously permits one bounded
readiness check, then re-arm; repeated hardware failure retires the session.

Raw bytes are delivered without kernel echo or CR/LF translation; the userspace
program owns editing/echo. Reserve byte0x1d (Ctrl-]) as an explicit terminal-owner
cancel escape, never delivered as application data. RX overrun/error flags or a
full kernel FIFO cause an observable input-overflow termination, not silent loss
or fabricated EOF. This acknowledges finite UART buffering and paste bursts;
lossless flow control is a later device capability, not an assumed guarantee.

Cancellation wins before a pending delivery commits; the FIFO prefix is not
consumed and the parked Session is dropped exactly once. During a charged copy,
CPU0 serialization completes restoration/finalization before teardown. Root exit,
fault, budget stop, overflow or host escape masks RX, quiesces the idle source,
revokes the input grant, drops pending state and drains/discards remaining input
with a bounded operation before returning terminal ownership to the monitor.
If quiescence cannot be established, fail closed rather than resume the monitor
with two readers. Normal children use existing structured Closing semantics.

## Longer interactive lifetime without a reset loophole

Propose an explicit sealed **interactive resource profile**: 2 seconds cumulative
active time, 1ms quantum, 4096 lifetime syscall attempts, 64KiB successful console
output, 8192 accounted turns and 5 minutes wall lifetime. These are proposed
initial bounds to measure, not a performance claim. Publish the profile in startup
metadata and copied reports; preserve the legacy 100ms/64/4096/256 profile exactly.
Parameterize the one actual Session counter policy; do not swap Session, reset
Issuer, renew budgets on READ, or replace the Process after every line.

WaitingInput/WaitingChild and kernel idle do not charge active execution. The
fixed wall deadline includes parking and requires a real idle wake timer, so a
silent terminal cannot retain owners forever. No automatic credit from attacker
supplied bytes. At expiry or resource exhaustion, terminate visibly, restore the
monitor and permit the user to start a fresh explicitly granted session. A later
long-running service design needs explicit budget sponsorship/renewal authority;
this finite interactive lease does not pretend to solve that policy.

## Stages and acceptance gates

1. Review/freeze input envelope, status/termination representation and configurable
   Session admission, including final-call READ no-copy/no-park semantics. Pure
   codec/policy tests are a preparatory component, not the milestone outcome.
2. Review/implement private writable-owner copy and the IRQ-backed idle lease.
   Verify lost-wakeup cases, simultaneous deadline/RX, cancellation, device error,
   guard/RO/cross-page addresses, no partial copy, full-range transactional FIFO
   removal, and no callbacks/borrowed user storage. Inspect actual owner layouts,
   pointer/counter ordering and every affected 64KiB stack path before guests.
3. Run the real EL0 line program through the normal terminal transport: delayed
   bytes, CR/LF/editing, multiple commands, empty line, max line/overflow, binary
   bytes, Ctrl-] while blocked, deadline while silent, bad/foreign/stale input
   tokens, faulting reader and monitor `test` followed by a fresh session. Host
   writes ordinary QEMU serial bytes; no guest-memory pokes or test injection API.
4. Require traces of actual READ SVC, parked interval, hardware RX/deadline wake,
   checked delivery and resumed PC; no repeated EL0 polling while idle. Show one
   process/generation and cumulative counters across commands, output accuracy,
   zero retained frames after every closure, exact monitor recovery and all
   legacy/task/ARM/x86 regressions. Measure idle instruction/CPU activity against
   the current busy monitor and report terminal latency/overflow behavior.

Decisions still requiring approval: finite interactive limits and explicit
termination codes; Ctrl-] reservation; UART-overflow-as-termination; idle IRQ
mechanism and its exact primary-spec protocol; initial built-ins-only scope versus
simultaneous launch-grant envelope. No IPC/VFS/thread/SMP/compatibility work is
silently included. The next service-facing extension is a scoped receive endpoint
with owned messages and cancellation, using the same pending-owner discipline.
