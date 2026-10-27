# Fixed native task failure boundaries

All ten [strict negative profiles](task-negative/guest-second/result.json) pass
on one ARM CPU with 128 MiB RAM, each followed by the 41 bootstrap checks.
[Review 0030](../reviews/0030-task-injection.md) independently accepts source,
artifacts, actual layout, emitted stack/counter ordering and both diagnostic and
strict raw traces. These are deliberately injected failures and fixed generation
seeds, not observed spontaneous hardware faults or general fault-tolerance proof.

The profiles cover final-deadline exhaustion after SPAWN/WAIT, executor retirement
before reply/park installation, abandonment of an owned syscall admission,
failure to install a genuine finite-worker completion wake, capability/slot
exhaustion, and child retirement with its parent Ready or Waiting. Trace review
checks actual provisional settlement, original counters, no forbidden later EL0
entry, preserved root closure, cleanup and physical-frame recovery. Capability
exhaustion also runs the real legacy hello with the same Executor.

The exact [protocol](../design/0029-task-protocol.md) and
[hook design](../design/0030-task-injection-hooks.md) define the cases. Hook
selection is fixed at compilation, privately authorized once, and invalidated by
intervening ordinary preparation. Copied observers grant no mutation. The
[59 compiler clients](task-negative/types-first/result.json) include absence of
negative constructors/observers in ordinary builds. Private Authority measures
968 bytes, one page; metadata remains three pages. All ten linked negative
profiles fit 61,136/65,536 bytes including the unchanged allowance; the legacy
recovery chain fits 53,088. No stack gate was relaxed.

The [artifact binding](task-negative/artifacts-first/result.json) checks every
candidate against the canonical 15-entry/13-ELF archive. The ordinary-layout
seed is only construction history; the
[final negative-feature witness](task-negative-layout-fresh/result.json) records
actual private-type compilation and current source hashes. The initial missing
observer-feature compile failure is preserved under `task-negative-layout-first`.

Initial diagnostic results intentionally fail only the trace-review flag:
[SPAWN deadline](task-negative/guest-first-final_budget_spawn/result.json),
[other nine](task-negative/guest-first-rest/results.json). After independent
raw acceptance, all ten fresh strict reruns pass. Parser review found and fixed
a boot-root alias gap in admitted userspace roots and legacy recovery; coordinated
record mutations now reject it. The pinned
[44-test host batch](task-negative/host-second/result.json) passes. Raw QEMU
exception/GIC traces establish executed transitions; internal ownership and page
root claims additionally depend on source-bound serial evidence, not independent
TTBR observation.

The standard `make test-all` now builds and runs these ten cases from the same
canonical inventory, reproduces fixtures and records a feature-specific layout.
The [first full run](task-negative-full-first/result.json) fails after 293.2
seconds, with every captured input unchanged. The ordinary 64 MiB matrix passes;
the 128 MiB copy-input case stops at call 7 with genuine Budget. Its selected
turn costs 39.203 ms with 36.395 ms remaining; earlier allocation turns also
cost more than in the 64 MiB run. The actual child commit is cancelled/reaped,
the provisional reply is discarded, and metadata-only closure is published.
The test then fails before root reap/baseline; do not claim those later checks.
The emitted image matches the reviewed candidate. Deferred UART serialization
is outside accounting; the available evidence cannot separate emulation cost
from host scheduling. No budget or assertion was relaxed and no blind rerun was
performed. The suite stopped before its integrated negative guests and x86 leg.

The subsequent [bounded audit optimization](task-budget.md) preserves this
failure, reduces repeated validation work and passes the full ARM/x86 suite,
including these ten profiles. This closes the local regression gate for the
tested checkpoint. This milestone does not
establish interactive input, a shell, storage, networking, Linux compatibility,
remote CI or completion of the useful-system objective.
