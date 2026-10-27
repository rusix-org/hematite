# Review 0031: interactive input proposal

Status: **direction reviewed; not implementation approval**. Read-only review of
[proposal 0031](../design/0031-interactive-input-proposal.md) against the current
Session, Space, Executor/Group, task-turn and monitor sources. No code, build,
VM or architecture experiment was performed. This review does not certify the
proposed UART/GIC/WFI sequence; that sequence is not yet specified.

## Scope and existing evidence

An archive-backed, continuing EL0 line program is a useful bounded next step.
Keep the initial `shell` command limited to help/echo/count/exit, without launch
rights, filesystem semantics or a POSIX claim. ARM-only implementation with
explicit x86 Unsupported fits ADRs 0005/0006. Existing project authorization
still requires a concrete independently reviewed core design before code.

The identified gaps are real:

- `native.rs` hard-codes limits in issuance, startup, admission, resume and
  final-call settlement. Its owned TaskAdmission/ParkedSession pattern can
  inform READ, but ParkedSession currently represents an already-counted WAIT.
- `user_space.rs::read_into` already validates a whole readable range before
  copying through owned frames. It does not provide writable-user copying;
  a new private operation must check every page is ReadWrite before any write.
- Group and task turns use the persistent Budget, a 1 ms quantum and a fixed
  256-turn stop. Waiting and wake reports currently describe child completion.
  Input delivery cannot be represented as a fabricated child wake or EL0 SVC.
- `aarch64.rs::read_byte` polls flags/data and truncates data to a byte;
  `run_monitor` spins when empty. Neither is an RX error-aware interrupt lease.
  Existing slice handling must retain its strict timer-only contract.

## Decisions required before implementation

1. **One sealed interactive mode.** Freeze its constructor, root identity,
   input-generation allocation/rollback, terminal lease acquisition and release.
   A root TaskId does not itself imply a task launch grant. State whether this
   mode uses Group without task authority or a separate owned coordinator; do
   not construct unused catalog/child authority merely to obtain an identity.
   Slot 5 must be explicitly reserved in the shared typed namespace. Validate
   principal, mode, kind and generation, not just equality with envelope bytes.
   Admission failures may burn generations but must not retain the RX lease.

2. **Wire and policy.** Freeze the envelope's exact eight magic bytes, error
   precedence, READ feature advertisement and new termination/report values.
   The proposed magic resembles the existing startup-input codec; tests must
   distinguish the inner envelope from that outer codec. Preserve old startup
   bytes and legacy READ=Operation. Use a sealed validated resource profile,
   not caller-selected unlimited counters. Decide the final READ attempt's
   no-copy/no-park behavior, including invalid version/opcode precedence, while
   preserving legacy WRITE/EXIT final-call behavior. Input delivery must neither
   mint another Session nor count a second syscall.

3. **Delivery linearization and accounting.** Specify states and reports for
   ReadPark, WaitingInput, DeliveryReady, charged delivery and terminal closure.
   A delivery turn must have actual begin/end counters, consume the same active
   budget and turn allowance, and have entered=false with no invented trap or
   saved user PC. Freeze whether its quantum is merely a checked bound or an
   interrupting kernel deadline; do not silently reuse EL0 timer assembly.
   Perform final clock, wall-deadline, cancellation and retirement checks before
   FIFO removal and reply installation. On failed settlement, an inaccessible
   dying process may contain copied bytes, but no reader resumes with a partial
   result. Freeze precedence for simultaneous data, escape, overflow and expiry.

4. **Terminal handoff.** Define what happens to already-buffered bytes after
   `shell\r\n`, especially the second newline, and to bytes arriving during
   teardown. Bounded discard cannot promise to empty an endlessly replenished
   UART. Give an exact handoff boundary, error clearing and fail-closed condition;
   otherwise leftover application input could become monitor commands. Explicitly
   approve Ctrl-] reservation and overflow termination. Detect UART error bits
   before truncation, and check escape even when the software FIFO is full.

5. **Idle hardware contract.** Freeze CPU0/peer exclusion, BUSY ownership,
   boot-root/vector/stack state, UART configuration and interrupt identification,
   GIC save/restore and acknowledge/EOI order, deadline programming, barriers,
   interrupt-mask/WFI/recheck order and simultaneous/spurious-source handling.
   Verify each new architectural claim against primary specifications and pinned
   QEMU behavior. No approval is inferred from remembered WFI semantics. An IRQ
   handler that only records bounded scalar state still needs explicit lifetime
   and alias rules; it cannot access a live Rust FIFO borrow.

6. **Finite lifetime.** The proposed 2 s / 4096 calls / 64 KiB / 8192 turns /
   5 minute wall lease needs explicit acceptance. Freeze wall-clock start and
   checked counter-domain arithmetic, including admission near MAX_COUNT and
   monotonicity across idle. Poll/drain, idle retries and cancellation scans
   need bounds too; uncharged management must not become an unbounded work path.
   A blocked owner must expire without host input. Shortened test deadlines, if
   desired, need an explicitly sealed profile and cannot substitute for testing
   the production wall limit or its actual timer configuration.

## Smallest proper implementation sequence

1. Freeze the decisions above and exact safe codec/profile/owned-read admission
   APIs. Implement independent literal-wire and Session lifetime tests first:
   old startup parity, invalid requests, final-call precedence, persistent totals,
   no retained Host/user borrow, and no Session cloning/reset through park/wake.
2. Independently review private full-range RW copy, including cross-page RW/RO,
   guard, wraparound, zero-capacity rejection and unchanged destination on invalid
   range. Compile ownership clients against the actual library and inspect layout.
3. Separately review and implement the exclusive RX/idle lease from primary
   specifications. Establish real UART arrival and silent deadline wake without
   userspace first; do not present that component test as a shell outcome.
4. Integrate the single interactive owner and charged delivery; inspect every
   affected stack path, allocation rollback and closure. Then add the actual
   userspace line editor and monitor adapter. Launch rights remain a later scope.
5. Freeze a strict interactive host protocol and run real serial sessions only
   after source/layout/stack approval. Preserve failed runs and old regressions.

## Required acceptance evidence

Bind actual ELF instructions/symbols, archive, kernel and resource profile to
records. Correlate READ SVC/continuation PC, real RX or deadline IRQ, parked state,
charged delivery, FIFO commit and resume. No EL0 transition while parked, no
second call count on wake, and no entry after closure. Demonstrate one root and
generation across multiple commands, monotonic cumulative charges and unchanged
frames after teardown. Missing, duplicate, reordered and post-cancel delivery
records must fail the host parser.

Use delayed ordinary serial bytes for blocked read, partial and 256-byte reads,
editing, binary data, queued input, escape and overflow; dedicated hostile ELFs
exercise invalid/foreign/stale grants and writable-range failures. Test cancel
before commit, fault/exit, deadline with no input, and a fresh session after each
closure. Race tests need an observable ordered boundary, not a guessed host sleep.
Do not claim every lost-wakeup interleaving from a few successful serial runs.

Measure idle activity separately from proof of WFI/IRQ ordering. QEMU host CPU
usage is noisy and is not a hardware power measurement. The existing monitor
will still spin outside the interactive lease unless a separately approved scope
changes it; compare the actual interactive waiting interval to that baseline.
All existing ARM task/legacy gates and x86 regression behavior remain required.
