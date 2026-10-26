# ADR 0001: evaluate upstream before choosing project direction

Status: accepted — scratch selected by Aaron on 2026-10-25.
Date: 2026-10-25.

## Decision

Aaron's follow-up: "Scratch is the approach, build until we have something we
can run." Proceed with an original local kernel and runnable bootstrap. This
supersedes the contribution-first recommendation below; it does not approve
public release, a license, or claim that scratch development is cheaper.

Concrete design scope: a fixed x86_64 virtual-machine profile and a small
capability-bearing core, with no loadable modules, PCI/physical-device support,
graphics stack or general hardware discovery in the bootstrap. Safe policy
receives a narrow platform interface rather than arbitrary device/register
access. The research goal is to make that smaller workload-specific trusted
surface reviewable; its advantage over Asterinas remains unproven. The cost is
owning every missing Linux ABI, device, scheduler and security implementation.
Aaron chose that cost after the upstream build/boot baseline.

The later [native-first decision](0005-native-first-design.md) supplies a
stronger architectural distinction: native capability/resource-lifetime
contracts take priority, with Linux semantics in a selective adapter. That
research objective is not strict Linux ABI replacement.

## Original recommendation (not selected)

Prefer contributing workload coverage and ABI fixes to Asterinas, subject to
the build/boot experiment below. Do not start a permanent fork or a new kernel
yet. This is a proposed change to the founding from-scratch objective and is
not authorized until Aaron accepts it. The Phase 0 gate remains closed.

Asterinas already proposes the defining design in [AGENTS.md](../../AGENTS.md):
a Rust Linux-ABI kernel with unsafe machinery behind safe framework APIs.
Calling our equivalent boundary `hm-core`, limiting devices, or choosing one
workload does not by itself justify reimplementing the kernel. A permanent
fork adds upstream rebasing, security triage and duplicated review obligations.
Contribution has upstream acceptance and scheduling costs, but shares those
maintenance obligations. No demonstrated architectural incompatibility yet
justifies a fork.

## Evidence and fit

Sources below were consulted on 2026-10-25. These are **upstream reports**,
not locally reproduced compatibility or safety claims. Web snapshots can lag
upstream; reported activity dates are observations, not a claim of latest HEAD.

| Project | Architecture and reported capability | License, governance and activity | Assessment |
| --- | --- | --- | --- |
| Asterinas | Framekernel: OSTD owns unsafe mechanisms, kernel above uses safe Rust. Reports 240+ syscalls and prioritizes x86-64 VMs. Release 0.19.0 reports PVH/Firecracker, a minimal AArch64 port running BusyBox, and self-hosting with Nix/Cargo/rustc/QEMU. | Primarily MPL-2.0, with component exceptions; project book says no CLA. Public RFC process for major decisions. Release dated 2026-10-13 reports 55 contributors to that release. | Closest fit. Verify required workload, core soundness boundary, dependencies, device transport and provenance before adoption. |
| Maestro | Rust Unix-like Linux-compatible kernel; README reports x86/x86-64, SMP, processes, ELF, ext2 and roughly 30% syscall coverage. Also supports legacy devices and modules outside our target. | Repository identifies AGPL-3.0. Observed commit history includes changes by `llenotre` and other contributors through 2026-08-22; no formal governance charter established in this review. | Broader hardware scope and no demonstrated framekernel boundary in consulted docs; less direct fit. README explicitly calls it unstable and incomplete. |
| Kerla | Monolithic Rust x86-64 Linux ABI implementation; reports QEMU/Firecracker, virtio-net and smoltcp. Initramfs only, no disk support; README says most applications lack required features. | MIT/Apache-2.0 dual license. Author-led repository; README explicitly says it is no longer maintained and directs readers to FTL. | Useful historical microVM precedent; adopting it also means taking over maintenance. |
| gVisor | Go application kernel (Sentry) implements Linux behavior inside host userspace; relies on the host Linux kernel. | Apache-2.0 repository. Maintainer Council, public contribution review and documented voting; charter notes all active maintainers currently share one employer. Observed history includes changes through 2026-09-09. | Strong ABI/test precedent, but does not meet the standalone Rust guest-kernel objective. Its tests can inform differential verification. |

The Asterinas safety boundary is an architectural claim, not an audit result.
Its syscall count is not a compatibility percentage. Neither booting a shell
nor upstream self-hosting demonstrates our exact target workload or all ABI
corner cases. Ext4 durability, target server behavior and arm64 parity remain
unmeasured here.

## Required experiment before decision

Build and boot an unmodified pinned Asterinas revision in a bounded QEMU guest.
Record source commit, development-image digest, exact commands, toolchain,
VM configuration, exit statuses and raw build/serial logs. Record observed
shell commands and their output, including failed commands. Independently
identify unsupported target requirements; distinguish an environment failure
from a kernel failure. A failed setup is not a completed build/boot evaluation.

Local result: unmodified commit `26fffae2b7360809cd1a7883cf7e825c8290a056`
built successfully and executed the upstream boot shell script under QEMU
10.2.1/KVM (1 vCPU, 1 GiB), with one boot marker and debug-exit status 33.
See [commands and raw evidence](../evidence/asterinas-evaluation.md). This is
a GRUB/Multiboot2 q35 baseline, not PVH/microvm or workload compatibility.
The upstream OSDK install resolves dependencies without `--locked`; exact
reproduction of that dependency graph remains a limitation. No server workload
or Linux differential suite was run. These results support continued upstream
evaluation and the contribution-first recommendation, not production adoption.
The alternative projects were assessed from documentation only and were not
built or booted for this comparison.

Aaron selected scratch after this experiment; see the decision above. A possible research question is whether
a substantially smaller, workload-specific trusted core can be audited more
effectively; this has **not** been demonstrated and is not the current
recommendation. Any route also needs an explicit target workload, versioned
userspace inputs and pinned Linux LTS oracle before compatibility claims.

## Primary sources

- [Asterinas README](https://raw.githubusercontent.com/asterinas/asterinas/main/README.md): architecture, target and reported syscall coverage.
- [Asterinas 0.19.0 announcement](https://asterinas.github.io/2026/10/13/announcing-asterinas-0.19.0.html): release date, contributors, direct boot, arm64 and self-hosting reports.
- [Asterinas Book](https://asterinas.github.io/book/): component licensing and contributor terms; [RFC overview](https://asterinas.github.io/book/rfcs/): decision process.
- [Maestro README](https://raw.githubusercontent.com/maestro-os/maestro/master/README.md): features, license badge and limitations; [commit history](https://github.com/maestro-os/maestro/commits/master/): observed activity and contributors.
- [Kerla README](https://raw.githubusercontent.com/nuta/kerla/main/README.md): architecture, limitations and maintenance status; [license](https://raw.githubusercontent.com/nuta/kerla/main/LICENSE.md).
- [gVisor architecture](https://gvisor.dev/docs/architecture_guide/intro/), [repository](https://github.com/google/gvisor), [governance](https://gvisor.dev/community/governance/) and [commit history](https://github.com/google/gvisor/commits/master/).

No Linux kernel implementation source was consulted for this record. No
upstream code has been copied into a hematite implementation by this review.
