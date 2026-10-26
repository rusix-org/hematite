# Progress

Current phase: **runnable local bootstrap; native-first research begins**.
Aaron approved scratch implementation and then changed the primary objective
from strict Linux parity to native OS design with selective compatibility.
[ADR 0005](docs/adr/0005-native-first-design.md) records the new direction.

## Implemented and verified locally

- Stable Rust 1.99.0 workspace, no external kernel crates, locked dependencies.
- Original PVH 32-bit entry into 64-bit Rust, fixed microvm profile, guarded
  stack, supervisor-only 4 KiB mappings, W^X, NX, SMEP, SMAP and CR0.WP.
- Audited boundary structure: `hm-core` owns unsafe operations; `hm-kernel`
  forbids unsafe code. Independent AI design/source review and fixes recorded;
  this is not a human or complete soundness audit.
- Serial monitor: `make run`, then `help`, `status`, `test`; Ctrl-C exits.
- `make test`: formatting, Clippy, safety-boundary checks, 8 host harness tests,
  9 guest tests, 3 missing-feature rejection profiles, and live monitor command,
  oversized-input/recovery and complete-test-report checks. All passed locally.
- Final environment is 1.59 GB uncompressed, pinned Rust base and Debian package
  snapshot, QEMU 7.2.22. One CPU/128 MiB guest; no attached disks or networking.

Evidence: [bootstrap verification](docs/evidence/hematite-bootstrap.md),
[final guest results](docs/evidence/hematite-final-suite/boot/result.json),
[profile results](docs/evidence/hematite-final-suite/profiles/result.json).
The earlier [Asterinas baseline](docs/evidence/asterinas-evaluation.md) remains
an evaluation only; it is not this kernel's implementation.

## Limits and gates

No userspace, syscall ABI, allocator, interrupts/exceptions, SMP, storage,
virtio drivers or networking exists. Page-guard tests inspect mappings; there
is no recoverable page-fault handler. No Linux compatibility claim exists.
The native capability/cancellation proposals are not yet implemented.

CI runs the same command and retains raw evidence, but has not run remotely:
no remote was created or pushed. Therefore the formal CI phase-exit gate is
not claimed passed. License selection remains pending (ADR 0002); Hematite
remains a local codename (ADR 0003). These do not revoke Aaron's authorization
to continue local kernel work; they prevent claiming a licensed/public release.

## Next actions

1. Implement and test ADR 0005's safe capability/resource-lifetime model:
   stale handles, rights attenuation, budgets, cancellation and reclamation.
2. Design/review boot-memory discovery, frame allocation, exceptions and
   scheduling before integrating the model with real resources (Phase 1).
3. Define the native userspace wire contract from those experiments; select
   compatibility workloads and pin a Linux oracle when the adapter exists.
4. Resolve licensing/public naming before distribution; run remote CI only
   after explicit permission to publish/push.

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
