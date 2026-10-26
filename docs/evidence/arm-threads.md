# ARM cooperative thread verification

Executed 2026-10-26 UTC on parent `345f800` plus this change. Final command:
`make test-all`. [Full output](threads-suite-final.log) and
[archived evidence](threads-final/) preserve source hashes, ELF hashes, exact
commands, compiler diagnostics, serial output and exception/IRQ traces.
Environment: pinned Rust 1.99.0, QEMU 7.2.22, one CPU, TCG.

Both architectures passed formatting, Clippy, structural safety checks, 14 host
harness tests, nine IRQ-trace validator tests and 29 guest cases. Existing five
capability and 22 frame/heap compiler probes passed on both targets; ARM's 21
mapping and 14 new thread compiler probes passed. x86 thread cases explicitly
require Unsupported/absent APIs and do not claim stack switching there.

Safe callbacks execute on four separate guarded 64 KiB stacks. Tests check
exact round-robin order with differing yield counts, stack-local byte arrays,
local destructor counts, return values and no redispatch after completion.
Repeated runs, empty runs, five-job rejection before any callback invocation,
exact trace capacity, trace truncation and 90-dispatch completion all pass.
A live exclusive mapped-byte borrow and frame/mapping accounting remain intact
across callback execution. The runner has no cancellation path that discards
suspended Rust frames; a non-returning callback can stall cooperative execution.

Core diagnostics run all four slots, verify each actual SP lies within its
expected usable stack and remains unchanged, and check x19–x29, D8–D15 and
FPCR/FPSR sentinels across the actual yield/switch path. The switch saves
x18–x30, SP and full Q8–Q15; x18/x30 and vector upper halves are source-reviewed,
not claimed as runtime sentinels across arbitrary Rust calls. Guard AT probes
report faults, nested runner invocation returns Busy, and existing timer/fault
checks pass after the thread tests. CPU rejection, repeated 64/128 MiB monitors
and exact 16/32 IRQ transaction traces also pass.

The [compiler check](threads-final/aarch64/thread-types/result.json) compiles
the actual Runner module, accepts two safe clients and rejects twelve clients
that violate privacy, callback or borrowing constraints. It also compiles a
safe 128 KiB array with release/LTO settings and requires the inline-probe IR
attribute and the emitted 4 KiB decrement/load loop before initialization.
This property is pinned to the tested target/compiler, not assumed for all ARM.

The [overflow profile](threads-final/aarch64/profiles/thread-overflow/result.json)
runs an ordinary safe callback with a 128 KiB local array on worker zero.
Its first fault is an EL1 read translation abort (`0x96000007`) with FAR inside
the exact ELF `__thread0_guard` page. It does not resume, prints one fatal marker
and exits 2 without a test report. The host checks the guard symbol, fault
address, syndrome, absence of recovery and terminal semihosting-exit trace.

## Failures preserved and corrected

The [first suite](threads-suite-first.log) passed the boot inventory but failed
both monitor profiles. A data abort in `Model::stats` hit the normal stack
guard. The monitor loop had absorbed the test inventory through inlining;
its [prologue](threads-investigation/first-monitor-prologue.log) reserved roughly
33 KiB. Marking the test function `inline(never)` reduced the monitor's own
frame to 448 bytes ([final prologues](threads-investigation/final-monitor-test-prologues.log)).
The final complete suite passed without increasing any stack size.

The first overflow guest already faulted in its intended worker guard. Its host
check incorrectly counted QEMU's terminal semihosting call as an additional
fault. The corrected validator requires exactly the guard abort followed by
the exit call; its unit test rejects wrong addresses, write faults, duplicate
faults, missing/incorrect exit records and resumed faults. Original logs and
profile results remain under [first-run evidence](threads-investigation/first-run/).

## Scope

This is bounded cooperative kernel execution, not preemption, SMP, userspace
threads, dynamic stack allocation or a stable userspace ABI. IRQ stays masked
while Rust runs. The separate [AI design/source review](../reviews/0010-arm-kernel-threads.md)
has no remaining findings within that scope; it is not a human or complete
soundness audit.

## Published CI

Source commit `d13b0329bffd5692cee967e8b0e7af6c4634e526` passed
[GitHub run 40324819664](https://github.com/rusix-org/hematite/actions/runs/40324819664).
The pinned environment build, complete `make test-all` step and evidence upload
succeeded. [Exact run/job metadata](threads-ci-result.json),
[raw CI log](threads-ci.log) and [watch log](threads-ci-watch.log) are archived.
This verifies the published cooperative milestone, not the remaining Phase 1
preemptive scheduling/SMP exit gate.
