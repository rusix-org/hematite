# ARM GIC ownership verification

Executed 2026-10-26 UTC on parent `1a12080` plus this change. The first full
`make test-all` exited 0. [Full output](gic-ownership-suite-first.log),
[source hashes](gic-ownership-final/source-sha256.txt), compiler results,
commands, ELF identities and raw traces are archived in
[final evidence](gic-ownership-final/). Existing pinned container/toolchain and
resource bounds apply; no new dependency was introduced.

Both architectures passed formatting, Clippy, safety/repository checks, 41 host
harness tests, 38 baseline guest cases and all existing compiler/monitor
profiles. ARM also passed five scheduling/wait cases at 128 and 64 MiB with
same-guest baseline regressions, forced-boundary checks, withheld timer and
overflow negatives, all-blocked retention, exact timer traces and full register
sentinels. No secondary CPU ran.

The new gic_ownership case acquires the execution lease on masked CPU0 and
starts shared forwarding. It seeds pending SPI32 only after confirming the
interrupt is disabled. Across two local setup/stop cycles it requires that
pending sentinel and shared forwarding to survive; each cycle also requires
duplicate local setup rejection. Local teardown checks PPI30 and the CPU
interface are quiet. Shared teardown clears the sentinel, disables forwarding
and verifies every implemented SPI bank is disabled, nonpending and inactive.
The diagnostic dispatches no tasks and never unmasks IRQ. x86 explicitly
treats this ARM-only hook as inapplicable.

The duplicate fixture verifies rejection and subsequent cleanup. Source review
establishes that the active-publication guard rejects before mutation; this is
not an independent runtime observation of every live publication field. Real
preemptive workloads exercise the enclosing shared/local session sequence.
The existing scoped timer diagnostic remains an exclusive whole-controller
operation and cannot run during a future SMP session.

[Independent AI review](../reviews/0016-arm-gic-ownership.md) covered the design
before implementation and final source, including rollback and register
liveness. It is not human review or a complete soundness audit. Per-CPU stacks,
secondary translation root, startup, mailbox and confirmed join/OFF remain open.
This establishes ownership separation on CPU0, not concurrent interrupt
delivery. Remote publication remains pending approval; no push retry occurred.
