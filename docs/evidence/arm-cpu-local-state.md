# ARM CPU-local state verification

Executed 2026-10-26 UTC on parent `0f5492d` plus this change. The first full
`make test-all` exited 0. [Full output](cpu-local-suite-first.log),
[source hashes](cpu-local-final/source-sha256.txt), commands, compiler results,
ELF identities and raw traces are archived under [final evidence](cpu-local-final/).
The existing pinned container/toolchain and resource bounds apply.

Both targets passed formatting, Clippy, safety/repository checks, 41 host harness
tests, 37 baseline guest cases and all existing compiler/monitor profiles. ARM
passed five scheduling/wait cases at 128 and 64 MiB, same-guest baseline checks,
forced timer boundaries, withheld-timer/overflow negatives and all-blocked
retention. Exact timer accounting and full register sentinels still pass.

The new ARM baseline case verifies current identity, rejects invalid candidate
record bases and all four foreign affinity levels, and checks sixteen canary
words in the inactive CPU1 record after real timer activity. The dedicated
preemption check repeats it after scheduling/context activity, including the
forced-boundary profile. Invalid bases exercise the validation predicate;
tests do not install an invalid TPIDR_EL1. Private Runner fixtures reject absent
and foreign identity at both execution boundaries before normal successful
execution and nested Busy rejection. Actual-type compiler clients still reject
Runner transfer/sharing and construction. x86's platform hook is explicitly
inapplicable and does not claim ARM state coverage.

[Independent AI review](../reviews/0015-arm-cpu-local-state.md) covered design
before implementation and final source; this is not human review or a complete
soundness audit. CPU1 never ran. Shared stacks, BUSY, GIC distributor lifecycle
and CPU0-only allocator/mapping authority remain unchanged. This establishes a
single-CPU prerequisite, not simultaneous execution or SMP safety. Remote
publication remains pending approval; no push retry occurred.
