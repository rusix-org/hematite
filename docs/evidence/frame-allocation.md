# Frame allocation verification

Executed 2026-10-26 UTC (2026-10-25 local) against parent `7271014` plus this
change. Command: `make test-all`. Source hashes are in
[frame-final/source-sha256.txt](frame-final/source-sha256.txt); exact environments,
commands, ELF hashes, raw serial output, compiler diagnostics and results are
in [frame-final/](frame-final/). Final full log: [frame-suite-4.log](frame-suite-4.log).

Both targets passed formatting, Clippy with warnings denied, structural safety
checks, all 12 host harness tests, all 19 guest cases, five capability compiler
probes and ten frame compiler probes. CPU-feature rejection and interactive
monitor tests passed. ARM 64 MiB and 128 MiB guests report discovered RAM
correctly; the harness computes expected usable pages independently from the
ELF `__kernel_end` symbol and fixed mapping limit. Those generated QEMU profiles
have no additional reserved ranges; synthetic planner cases test reservations.

The ARM `frame_contract` case exercises real zeroed byte access on distinct
pages, dirty-page drop/reuse, and preservation of a live page through exhaustive
claim/release. The exhaustion check uses the allocator's private claim primitive
and a 512-byte index ledger, avoiding a large array of frame objects. It holds
every free page, requires repeated public allocation failures, and cleans up
only its own claims. It does not test byte access to every eligible page.

One frame is deliberately forgotten once per pool. Its bit stays occupied,
cannot appear in the subsequent exhaustive claims, and survives repeated
diagnostics. Repeated diagnostics preserve the remaining capacity. An empty
metadata plan tests that a second construction returns AlreadyInitialized
without touching memory or resetting the permanent latch.

The x86 `frame_contract` case specifically checks Unsupported; it is not an
x86 allocation test. Shared planner tests cover whole-page containment,
clipping, one-byte exclusions, overlap, empty capacity and upper-address bounds
on both architectures. Planning metadata alone conveys no memory authority.

Compiler probes compile the actual core frame module behind a separate library
boundary. A valid two-frame client passes. Expected diagnostics reject cloning
frames/pools, escaping the pool lifetime, dropping a borrowed frame, simultaneous
mutable borrows, forging private fields, calling the private constructor,+sending a frame between threads, and sharing a pool via Sync. All probes must
run and produce the expected outcome; arbitrary compiler failures do not count.

Initial runs failed the repository's nearby SAFETY-comment check before guest
execution; those logs remain in [frame-suite.log](frame-suite.log) and
[frame-suite-final.log](frame-suite-final.log). The latter filename was chosen
before execution and is a failed attempt, not the final result. Intermediate
passing runs are retained in frame-suite-2.log and frame-suite-3.log.

Independent AI design/source review is recorded separately. It is not a human
audit or proof of full soundness. No heap, new mapping, DMA, user address space,
interrupt-safe allocator or SMP exists. Only ARM physical frame ownership in
the current fixed guest profile is claimed. Remote CI has not run this change.
