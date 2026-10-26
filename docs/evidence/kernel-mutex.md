# Kernel mutex verification

Executed 2026-10-26 UTC on parent `f681dcb` plus this change. Command:
`make test-all`. The first full suite passed; there were no failed build or
runtime attempts in this increment. See [full output](mutex-suite-first.log)
and [archived evidence](mutex-final/) for source hashes, exact commands, ELF
identities, compiler diagnostics, serial logs and hardware traces.

Both architecture suites passed formatting, Clippy, safety hygiene, 23 host
harness tests and 32 guest cases. All existing capability, frame/heap, mapping
and thread compiler checks passed within their declared architecture scope.
The new mutex compiler suite passed 22 clients per architecture, including
accepted Mutex<Cell<T>> sharing and rejection of non-Send payload sharing,
guard transfer/sharing/forging/cloning, invalid borrows and escaped references.
The implementation and cooperative fixture received independent
[design/source review](../reviews/0011-kernel-mutex.md).

## Guest behavior

Generic safe policy tests run on both ARM and x86. They exercise mutation under
ownership, failed acquisition while held, cooperative retry through a scheduling
context, reacquisition observing writes, exclusive get_mut and consuming
into_inner. Borrowed/non-Copy payloads are destroyed exactly once. A deliberately
forgotten local guard keeps shared acquisition unavailable; later exclusive
ownership permits extraction without forced unlock or leaked payload storage.

The ARM-only cooperative fixture uses four real guarded-stack workers and a
static instance of the actual core mutex protecting two Cell fields. Each worker
performs 16 updates, deliberately leaves the pair inconsistent while yielding
with the guard held, checks it remains unchanged by other workers, repairs it
and releases ownership. The test requires observed contention, all four normal
completions and the exact final count/checksum. Policy invokes this fixture twice
with unchanged allocator/mapping accounting. Repeated 64/128 MiB monitor tests
pass, as do later thread/timer/fault checks and exact IRQ trace validation.
The x86 worker-specific case checks absence of ARM execution support; x86 still
executes the generic mutex ownership/lifetime cases.

## Hosted parallel stress

The harness compiles the actual `crates/hm-core/src/mutex.rs` as a host library,
then runs a separate safe Rust executable against it. No duplicate mutex model
is used. The executable uses four OS threads with 20,000 updates each, holds an
initial guard until every worker has observed rejected acquisition, and checks
an interior-mutable payload, critical-section exclusion, exact final count and
checksum, and exactly-once destruction. The host process has a 20-second bound;
its initial contention handshake also has a five-second deadline.

Observed [stdout](mutex-final/aarch64/mutex-types/host/run-host.stdout.log):
four workers, 80,000 updates, four forced rejections, 3,846 retry attempts and
one payload destructor. Retry count is an observation, not a deterministic
performance threshold. [Result metadata](mutex-final/aarch64/mutex-types/result.json)
records the host target `x86_64-unknown-linux-gnu`, Rust 1.99.0, panic=abort,
executable SHA256 and all command exits. Sources, commands and raw logs are
archived; compiled executables/rlibs are reproducible build output and omitted.

This supplies actual host parallel execution evidence for Acquire/Release
ownership; it is not an exhaustive memory-model proof or QEMU guest SMP test.
The container remains limited to two CPUs and 4 GiB while running four threads.

## Limits

The mutex does not disable preemption or interrupts, provide wait queues,
guarantee fairness, detect deadlock or poison application state. Forgetting a
guard can permanently block shared acquisition. Existing IRQ handlers remain
mutex-free; they must not wait for interrupted owners. The kernel still uses
cooperative execution and panic=abort. AI review and tests are not a complete
soundness audit. Remote CI is recorded separately once run.
