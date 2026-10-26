# Progress

Current phase: **Phase 1 underway: ARM64 frame allocation verified**.
Aaron approved scratch implementation and then changed the primary objective
from strict Linux parity to native OS design with selective compatibility.
[ADR 0005](docs/adr/0005-native-first-design.md) records the native direction;
[ADR 0006](docs/adr/0006-arm64-priority.md) makes ARM64 primary now.

## Implemented and verified locally

- Stable Rust 1.99.0 workspace, no external kernel crates, locked dependencies.
- Original ARM64 direct-ELF entry at EL1, 4 KiB translation tables,
  privileged-only W^X/PXN/UXN mappings, PAN, guarded stack and PL011 console.
- x86_64 regression port: original PVH entry, microvm profile, guarded stack,
  supervisor-only 4 KiB mappings, W^X, NX, SMEP, SMAP and CR0.WP.
- Audited boundary structure: `hm-core` owns unsafe operations; `hm-kernel`
  forbids unsafe code. Independent AI design/source review and fixes recorded;
  this is not a human or complete soundness audit.
- Serial monitor: `make run` defaults to ARM64; `ARCH=x86_64` selects the
  regression port. Commands `help`, `status`, `test`; Ctrl-C exits.
- `make test-all`: formatting, Clippy and safety checks for both targets,
  12 host harness tests, 19 guest tests per architecture, ARM no-PAN rejection,
  x86 missing-NX/SMEP/SMAP rejection, and live monitor recovery tests on both.
  ARM discovery matches both 128 MiB and 64 MiB guest configurations.
  All passed locally. `make test` tests ARM64 alone by default.
- Pinned Rust base and Debian package snapshot, QEMU 7.2.22. One CPU/128 MiB
  guest by default; ARM discovery also tests 64 MiB; no attached disks or networking. Both targets use TCG.

Latest evidence: [frame verification](docs/evidence/frame-allocation.md),
[ARM guest results](docs/evidence/frame-final/aarch64/boot/result.json),
[ARM profiles](docs/evidence/frame-final/aarch64/profiles/result.json),
and matching x86 results under `docs/evidence/frame-final/x86_64/`.
Earlier [bootstrap verification](docs/evidence/hematite-bootstrap.md) is retained.
The earlier [Asterinas baseline](docs/evidence/asterinas-evaluation.md) remains
an evaluation only; it is not this kernel's implementation.

## Boot-memory discovery verified locally

The ARM ELF now starts at `0x40200000`, leaving a 2 MiB DTB prefix. Core reads
only that bounded mapped prefix and returns owned metadata from a safe parser.
It validates RAM/reserved ranges and rejects unsupported memory layouts.
Malformed fixtures, every truncation and deterministic byte mutations run on
both architectures. Discovery describes memory; the allocator below consumes it without expanding
the 16 MiB bootstrap mapping. x86 PVH memory discovery remains unimplemented.
See [design](docs/design/0004-boot-memory.md),
[review](docs/reviews/0004-boot-memory.md) and
[verification](docs/evidence/boot-memory.md).

## ARM frame allocation verified locally

A reviewed core pool allocates uniquely owned 4 KiB pages from the mapped
16 MiB window after excluding the DTB prefix, complete kernel image and all
reservations. Frames zero on allocation and reuse, retain the pool while live,
and return on drop. One permanent construction latch prevents duplicate pools.
Cell-based accounting is single-CPU only; frames cannot be sent across threads.

Actual ARM tests cover byte isolation, zeroing, reuse, exhaustion while another
frame remains live, singleton rejection and forgotten-owner retention. The
exhaustion test holds a compact index ledger rather than thousands of frame
objects; it does not touch every claimed page's bytes. Exactly one deliberately
forgotten page remains allocated after the first diagnostic per boot. Repeated
diagnostics preserve the remaining capacity. Ten safe-client compiler probes
run per target, using the actual core module.

x86 has no physical allocator yet: its contract case requires explicit
Unsupported, while shared planner and compiler checks run on both targets.
See [design](docs/design/0005-frame-allocation.md),
[review](docs/reviews/0005-frame-allocation.md) and
[evidence](docs/evidence/frame-allocation.md).

## Limits and gates

No userspace, syscall ABI, kernel heap, general interrupt/exception handling, SMP, storage,
virtio drivers or networking exists. Page-guard tests inspect mappings; there
is no recoverable page-fault handler. ARM has minimal fatal exception vectors;
x86 unexpected exceptions may triple-fault. ARM bootstrap isolation checks are
not claims about a future EL0 address space or equivalence to SMEP. No Linux
compatibility claim exists.
The safe capability model now checks bounded resource accounting, attenuated
rights, stale generations and cancellation/completion lifetimes. Five compiler
probes per target verify the safe-client type boundary. This is metadata only: owner
labels are trusted inputs and completion is a simulated backend acknowledgment.
See [model verification](docs/evidence/capability-model.md).

Published bootstrap CI passed at `b1b824c` ([run 40276324135](https://github.com/rusix-org/hematite/actions/runs/40276324135)).
The capability, boot-memory and frame changes passed locally; remote CI has not
run these changes. Aaron authorized public repository creation
and push in [ADR 0007](docs/adr/0007-public-repository.md). License selection
remains pending (ADR 0002); preliminary naming findings remain in ADR 0003.

## Next actions

1. Design/review kernel heap and memory ownership for page tables; then
   recoverable exceptions, timers and scheduling. Add interrupt/SMP-safe
   ownership before concurrency (Phase 1).
2. Define the native userspace wire contract from those experiments; select
   compatibility workloads and pin a Linux oracle when the adapter exists.
3. Resolve the license selection; run remote CI for the new changes when published.

## Session ledger

- 2026-10-25: Evaluated Asterinas, recorded proposed decisions and initial CI;
  upstream build/boot baseline committed as `2492ebb`.
- 2026-10-25: Aaron selected scratch and requested a runnable kernel. Wrote and
  independently reviewed bootstrap design. Fixed hardware-mutated page-table
  declarations and strengthened alias checks before boot. Preserved initial
  code-model/LTO build failure; corrected target configuration.
- 2026-10-25: Original kernel booted, then passed the complete suite in the
  smaller pinned environment. Exercised `make run` interactively and exited
  cleanly. No host boot configuration or kernel changed.
- 2026-10-25: Aaron revised the objective to explore native Rust OS designs
  with best-effort Linux compatibility. Updated AGENTS/ADRs and proposed a
  measurable capability/lifetime experiment. No performance advantage claimed.

- 2026-10-25: Aaron explicitly authorized public `rusix-org/hematite` creation
  and push. Recorded ADR 0007; preserved existing ARM64 work and evidence.
  Publication checks: 12 host harness tests, repository and safety hygiene,
  and whitespace checks passed. No kernel changes or fresh QEMU run in this
  publication session; existing dual-architecture logs are preserved.

- 2026-10-25: Aaron prioritized ARM support. Added an original reviewed ARM64
  bootstrap and made it the default. Fixed review findings around CurrentEL,
  WXN observations and address-only linker symbols. Final dual-architecture
  suite and interactive ARM monitor passed. Physical ARM and native ARM/KVM
  performance remain untested. This implementation session made no publication
  changes; separate publication authorization/work is recorded in ADR 0007.

- 2026-10-25: Implemented and independently AI-reviewed the safe capability
  model. Both architecture suites pass 14 guest cases and five compiler probes,
  including all 24 revoke/release/cancel/complete permutations. Preserved initial
  Clippy failure and final evidence. No unsafe/core or hardware changes.

- 2026-10-25: Implemented reviewed ARM DTB discovery and safe parser. Fixed
  missing DTB by relocating the ELF; documented QEMU property-padding deviation.
  A fixture stack overflow hit the guard and was fixed by bounded test frames.
  Final dual suite passes 16 guest cases; ARM reports configured 64/128 MiB RAM.
  Preserved failures, exception trace, source hashes and final evidence.

- 2026-10-25: Implemented independently reviewed ARM frame pool and safe
  planning/API boundary. Fixed safety-comment checker findings; full dual suite
  passes 19 guest cases and ten frame compiler probes per architecture.
  ARM 64/128 MiB monitor profiles independently verify the eligible count
  against the ELF end marker. x86 allocation remains explicitly unsupported.
