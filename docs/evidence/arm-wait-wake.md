# ARM event wait/wake verification

Executed 2026-10-26 UTC on parent `dbf0474` plus this change. Final command
`make test-all` exited 0. [Full output](wait-suite-final.log),
[source hashes](wait-final/source-sha256.txt), commands, compiler diagnostics,
ELF hashes, serial reports and hardware traces are in [final evidence](wait-final/).
The existing pinned Rust 1.99.0/QEMU 7.2.22 container and resource limits apply.

Both targets passed formatting, Clippy, safety/repository checks, 41 host harness
tests, 33 baseline guest checks, all prior compiler contracts and monitor/negative
CPU profiles. ARM additionally passed five scheduling cases at 64/128 MiB and
all 33 baseline cases afterward in the same guest. Five new actual-type clients
bring the thread/event compiler inventory to 19: local Event waits and Send/Sync
are accepted; cloning, private sequence mutation and an escaping local Event
reference are rejected for their required diagnostic codes.

## Actual waiting behavior

Event tests verify monotonic notification, MAX-1 to MAX advancement and repeated
exhaustion rejection without wrap. The real wait implementation returns on stale
or arbitrary differing observations without parking, and rejects an unchanged
terminal observation. A local Event can be moved after these completed calls.

Three actual workers park on one event. The notifier yields while notifying a
different event; the waiters must remain absent from dispatch and must not return
against the intermediate predicate. One notification then wakes all three,
including when its producer immediately returns. Park and Wake counts, bounded
dispatch records, result values, stack arrays and destruction counts are checked.
Repeated producer/consumer rounds exercise snapshot-before-predicate usage with
actual blocking, explicit yields and timer-driven progress. Notification and
payload synchronization remain separate contracts.

Ordinary 64/128 MiB scheduling runs each reported 101 scheduler IRQs; with the
later diagnostic probes, traces contain exactly 117 IRQ/vector/ack/EOI
transactions. The boundary image reported 145 scheduler IRQs and 161 total
transactions. Its ELF-resolved interrupted-PC windows contain 13 start, eight
yield, 13 completion and five pre-park hits. A separately required core check
confirms that timer expiry became pending while IRQ remained masked after
BLOCKED publication; the source is disabled before switching away. Counts are
observations, not performance thresholds.

The all-blocked negative image parks a callback on its own stack-local Event.
Only after all unfinished tasks are BLOCKED does the coordinator emit its marker.
The host observes that marker exactly once and kills the guest at two seconds,
with no completion/fatal report. A destructor witness would take a rejected fatal
path if its suspended frame were destroyed. This confirms retained blocked
execution within the test duration, not successful notification or CPU idle.

The callback Event borrow remains live through resumed Acquire observation;
registration is withdrawn before dispatch. [Independent AI review](../reviews/0013-arm-wait-wake-source.md)
inspected this lifetime/transition proof and its tests; it is not human review or
a complete soundness audit. This implementation remains single-CPU, wake-all,
without cancellation, external device producers or SMP wakeup guarantees.

## Earlier attempts and publication

[First build](wait-build-first.log) succeeded with a deprecation warning for
`fetch_update`; the pinned compiler names the operation `try_update`, which is
used by the final source. The [first guest](wait-run-first.log) and
[first complete suite](wait-suite-first.log) passed. The final suite additionally
includes the review-requested private-storage and local-lifetime compiler probes.
There were no failed builds or guest executions in this increment.

Remote CI has not run: public push approval remains pending. No push retry was
made. Phase 1 remains incomplete without preemptive multi-CPU stress and its
required synchronization/ownership mechanisms.
