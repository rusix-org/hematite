# CPU topology verification

Executed 2026-10-26 UTC on parent `abf5833` plus this change. Final command
`make test-all` exited 0. [Full output](topology-suite-second.log),
[source hashes](topology-final/source-sha256.txt), commands, compiler results,
ELF identities, raw device trees and serial traces are in
[final evidence](topology-final/). Existing pinned toolchain and resource bounds
apply; no new dependencies were introduced.

Both targets passed formatting, Clippy, safety/repository checks, 41 host harness
tests, 36 baseline guest cases and all existing compiler/monitor profiles. ARM
also passed the five scheduling/wait cases with same-guest baseline regressions,
forced-boundary checks and all existing negative profiles. No secondary CPU ran.

Three new baseline cases cover accepted topology fixtures, malformed inputs and
live metadata. ARM's actual boot tree reports CPU0; x86 explicitly has no ARM
descriptor. Fixtures cover one/two CPU orderings, 32/64-bit address cells,
PSCI 0.2/1.0 advertisements, permitted single-CPU absence of PSCI, reserved
entries and unrelated nodes. Rejection cases cover duplicate identities and
properties, missing/disabled/unsupported CPUs, malformed hierarchy, cell widths,
unit-address mismatches, incompatible or misplaced PSCI descriptions, unsupported
conduits/function IDs, bounds, overlap, depth and termination errors. Every
truncated valid fixture and deterministic byte mutation is exercised; accepted
mutations must retain the admitted topology invariants.

A host executable links the actual `hm-kernel` library and reads newly generated
QEMU trees for one, two and three CPUs. It requires exact accepted affinities,
advertised PSCI 1.0 for the pinned accepted profiles, and Error::Unsupported for
three CPUs rather than truncating the result. [Results](topology-final/aarch64/cpu-topology/result.json)
include each DTB hash and the executable hash; commands and output are adjacent.
QEMU only generated these trees. This does not establish CPU_ON, actual PSCI
version/features, per-CPU interrupt behavior or multicore execution.

The parser uses safe bounded slices, fixed-capacity state and owned Copy output.
It grants no CPU-start capability. Core uses its existing independently bounded
DTB window; unsupported topology leaves None while valid memory metadata can
still initialize the single-CPU system. A future SMP capability must require the
validated descriptor plus the remaining boot/hardware ownership proof.
[Independent AI review](../reviews/0014-cpu-topology-source.md) found no blocking
source issues within this scope; this is not human review or a complete audit.

The [first suite](topology-suite-first.log) stopped at a collapsible-if Clippy
error before guest execution. After that correction and additional reviewed
fixtures, the second full suite passed. Remote publication/CI remains pending
approval; no push retry occurred.
