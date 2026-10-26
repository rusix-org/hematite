# ARM timer interrupt verification

Executed 2026-10-26 UTC on parent `80ab24e` plus this change. Final command:
`make test-all`. See [full output](timer-suite-final.log) and
[archived evidence](timer-final/) for source hashes, ELF hashes, exact expanded
commands, environment, compiler diagnostics, serial output and interrupt traces.
The pinned container uses Rust 1.99.0 and QEMU 7.2.22 with one CPU and TCG.

Both architectures passed formatting, Clippy, safety hygiene, 13 existing host
harness tests, nine new trace-validator tests, 26 guest cases, five capability
compiler probes and 22 frame/heap compiler probes. ARM's 21 mapping compiler
probes also passed. x86 timer checks explicitly require unsupported behavior.

ARM positive probes receive eight physical timer IRQs, rearming after each of
the first seven. Policy invokes two positive/blocked-delivery pairs while an
exclusive mapped-byte borrow remains live. The blocked probe withholds PPI
enable and must reach its 100 ms counter deadline with zero delivered IRQs;
iteration limits also bound waits if the counter stops. Early failure or the
iteration limit cannot satisfy the expected timeout observation.

Runtime sentinels verify x4–x30, Q0–Q31, NZCV, FPCR/FPSR and both stack pointers.
x0–x3 are polling scratch and are not runtime sentinel checks; their save/restore
is independently source-reviewed. Every return to Rust masks IRQ, disables the
timer and controller, clears pending state after source deassertion and checks
inactive/not-pending PPI state. A bounded quiet interval verifies no new delivery.
Existing synchronous fault tests run after the timer contract.

The independent host validator checks ordered CPU0 IRQ entry, current-EL SPx
vector offset 0x280, physical ID30 acknowledgment, matching complete EOIR value
30 at GICC offset 0x10, then exception return. It rejects missing, duplicate,
reordered or malformed events. Observed exact counts:

| Profile | Complete IRQ transactions | Result |
| --- | ---: | --- |
| [Boot](timer-final/aarch64/boot/timer-trace.json) | 16 | Passed |
| [128 MiB monitor](timer-final/aarch64/profiles/monitor/timer-trace.json) | 32 | Passed |
| [64 MiB monitor](timer-final/aarch64/profiles/memory-64m/timer-trace.json) | 32 | Passed |

Monitor profiles now run `test` twice in one boot, including after oversized
and unknown command recovery. Full inventories and continued prompt delivery
are mandatory. All previous CPU rejection and unhandled/nested fault profiles
still pass. The actual pinned QEMU GIC register ranges and PPI wiring were
captured from its [generated device tree](timer-investigation/gicv2-layout.json),
with [command and raw DTB](timer-investigation/).

The [first full suite](timer-suite-first.log) passed before source review
requested explicitly clearing/checking GICD_IGROUPR0 bit30. That fix avoids
depending on the reset interrupt group; final evidence includes it. A final
test invocation was rejected before launch because automatic permission review
timed out. Its permitted single retry launched and passed; there was no failed
kernel build or guest execution in this increment. The trace-validator author
found and fixed acceptance of a duplicate exception return in its initial host
mutation test; the final host suite includes that regression.

Scope remains a fixed GICv2 emulator profile and a bounded assembly IRQ window.
Rust runs masked; no handler calls Rust or touches allocator/mapping state.
This is not preemption, general IRQ dispatch, SMP, native hardware validation,
timer precision or latency evidence. See the independent
[design/source review](../reviews/0009-arm-timer-irq.md). AI review and QEMU
tests are not a full soundness audit. Remote CI is recorded separately once run.
