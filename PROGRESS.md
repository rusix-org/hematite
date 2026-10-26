# Progress

Current phase: **0 — evaluation and proposed decisions**. No approval recorded;
no hematite kernel code implemented. Phase 0 exit gate has not passed.

## State and evidence

| Item | State | Evidence / remaining work |
| --- | --- | --- |
| Existing-project assessment | Implemented, documentation scope | [ADR 0001](docs/adr/0001-project-direction.md); baseline build/boot verified; workload evaluation outstanding |
| License | Proposed, awaiting approval | [ADR 0002](docs/adr/0002-license.md); no LICENSE adopted |
| Name | Blocked for public use | [ADR 0003](docs/adr/0003-name.md); collisions found; candidate needed |
| Core architecture | Proposed; initial AI review done, approval pending | [ADR 0004](docs/adr/0004-core-architecture.md) |
| Repository CI | Verified locally (record presence only); unverified in GitHub | `.github/workflows/checks.yml`; records check only, not a phase exit gate |
| Asterinas build/boot | Verified locally: upstream build and boot smoke only | [Evaluation record](docs/evidence/asterinas-evaluation.md) |
| Kernel, Cargo workspace, pinned build environment, QEMU harness | Planned after decisions | Phase 0 implementation gate in [AGENTS.md](AGENTS.md) |
| Phases 1–7 | Planned | [AGENTS.md](AGENTS.md); no compatibility claims |

## Next actions

1. Review the completed upstream build/boot baseline and its limited scope.
2. Review the evidence and obtain Aaron's direction, license, name, and core
   architecture decisions. Record approvals explicitly in the ADRs.
3. For the approved direction, declare target workload and Linux LTS oracle;
   obtain independent core-design review before implementation.
4. Implement and verify the Phase 0 skeleton only after its approval gate.

## Local checks

- `python3 tools/check_repository.py`: exit 0 on 2026-10-25; required records
  present. Does not validate approval, technical correctness, or runtime gates.

## Sessions

- 2026-10-25: Inspected empty Git history and sole founding file. Created
  proposed ADRs, repository checks, and evidence ledger. Started unmodified
  Asterinas evaluation on disk with bounded build/VM resources. No public
  remote, push, host-kernel change, or kernel implementation.

- 2026-10-25 continuation: Revalidated image pull session `48065` as live;
  no build/boot result yet. Independent AI architecture review recorded in
  `docs/reviews/0004-core-architecture.md`; strengthened DMA authority,
  forgotten-guard and user-copy permission contracts in ADR 0004. Local
  record checks pass; upstream checkout remains unmodified.

- 2026-10-25 continuation: Image pull completed (exit 0); digest recorded.
  Started bounded unmodified upstream build in session `93650`, container
  `hematite-asterinas-eval-20261025`. Initramfs and OSDK completed; kernel
  compilation remains live. Recorded upstream unlocked OSDK dependency
  resolution as a reproducibility limitation. No boot result yet.

- 2026-10-25 continuation: Upstream build completed (exit 0). Bounded QEMU/KVM
  boot completed with one native boot marker and exit 33; host harness exit 0.
  Raw commands, logs, hashes and limitations are in the evaluation record.
  Updated ADR 0001 with measured evidence. No hematite kernel or approved
  direction/license/name/architecture exists yet.
