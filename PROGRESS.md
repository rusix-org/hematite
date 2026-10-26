# Progress

Current phase: **ARM64-first runnable bootstrap; native-first research begins**.
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
  12 host harness tests, 9 guest tests per architecture, ARM no-PAN rejection,
  x86 missing-NX/SMEP/SMAP rejection, and live monitor recovery tests on both.
  All passed locally. `make test` tests ARM64 alone by default.
- Pinned Rust base and Debian package snapshot, QEMU 7.2.22. One CPU/128 MiB
  guest per run; no attached disks or networking. Both targets use TCG.

Evidence: [bootstrap verification](docs/evidence/hematite-bootstrap.md),
[ARM guest results](docs/evidence/hematite-dual-final/aarch64/boot/result.json),
[ARM profiles](docs/evidence/hematite-dual-final/aarch64/profiles/result.json),
and matching x86 results under `docs/evidence/hematite-dual-final/x86_64/`.
The earlier [Asterinas baseline](docs/evidence/asterinas-evaluation.md) remains
an evaluation only; it is not this kernel's implementation.

## Limits and gates

No userspace, syscall ABI, allocator, general interrupt/exception handling, SMP, storage,
virtio drivers or networking exists. Page-guard tests inspect mappings; there
is no recoverable page-fault handler. ARM has minimal fatal exception vectors;
x86 unexpected exceptions may triple-fault. ARM bootstrap isolation checks are
not claims about a future EL0 address space or equivalence to SMEP. No Linux
compatibility claim exists.
The native capability/cancellation proposals are not yet implemented.

CI runs the same command and retains raw evidence; the remote CI phase-exit
gate is not yet claimed passed. Aaron authorized public repository creation
and push in [ADR 0007](docs/adr/0007-public-repository.md). License selection
remains pending (ADR 0002); preliminary naming findings remain in ADR 0003.

## Next actions

1. Implement and test ADR 0005's safe capability/resource-lifetime model:
   stale handles, rights attenuation, budgets, cancellation and reclamation.
2. Design/review boot-memory discovery, frame allocation, exceptions and
   scheduling before integrating the model with real resources (Phase 1).
3. Define the native userspace wire contract from those experiments; select
   compatibility workloads and pin a Linux oracle when the adapter exists.
4. Resolve the license selection and review remote CI after publication.

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
