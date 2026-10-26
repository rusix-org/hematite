# Runnable bootstrap verification

Date: 2026-10-25 America/Vancouver / 2026-10-26 UTC.
Scope: original ARM64-first Rust bootstrap and x86_64 regression port.
**Verified locally; no Linux ABI, userspace, performance or full audit claim.**

## Source and environment

Implementation is committed in `b5e5e0b2b02cbe7dd477a2f8048d7a79b56a43f1`.
The final tests ran on the implementation worktree before that commit; a
subsequent diff against that commit confirmed no changes to kernel, Cargo,
Docker, Make or VM-harness inputs. The preceding x86-only implementation is
`1cba732`. Concurrent publication work is recorded separately in ADR 0007;
it did not perform this session's QEMU runs.

Dockerfile pins Rust image
`sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`
and Debian snapshot `20261025T000000Z`. Stable Rust 1.99.0, both bare-metal
targets, QEMU 7.2.22. Kernel dependencies are only the two workspace crates.
Observed local environment image ID after final builds:
`sha256:9dfd24b1c4e7a35bf605d70b6a479101c6b3e81f12cc41a0759c732cfde5d03b`
(1,759,596,753 bytes). Local image IDs may vary with build attestations; source
inputs, package snapshot and actual tool versions are the reproducibility pins.

## Commands and results

From the repository root:

```sh
make test-all > docs/evidence/hematite-dual-suite-final.log 2>&1
make run
# At the ARM monitor: status, test; then Ctrl-C to terminate QEMU.
# Earlier also exercised make run ARCH=x86_64 equivalently.
```

`make test-all` exited 0. It sequentially builds/tests ARM64 and x86_64 using
the same container, offline Cargo, two build CPUs, 4 GiB container memory, and
bounded QEMU guests with one CPU and 128 MiB each. TCG only; native ARM/KVM
performance is untested. The ARM VM uses direct ELF on `virt`; x86 uses PVH
on `microvm`. No guest disks or network devices. Both builds preserve the
core/policy unsafe boundary.

| Verification | Result and scope |
| --- | --- |
| Formatting / Clippy / repository / safety hygiene | Pass on both targets; structural checks are not soundness proofs |
| Host harness | 12 negative/positive validation tests pass, run in each architecture invocation |
| ARM guest | All 9 cases exactly once, semihost success exit 0, no timeout |
| x86 guest | All 9 cases exactly once, debug-exit 33, no timeout |
| ARM unsupported CPU | Cortex-A72 produces exact `HMBOOT REJECT PAN` marker and exit 1; launch errors cannot pass this case |
| x86 unsupported CPUs | Missing NX, SMEP or SMAP each produces bootstrap rejection exit 35 |
| Live monitors (both) | Help/status/test work; 65-byte input is rejected, unknown command handled, subsequent help works; host then terminates the still-running VM |
| Interactive `make run` | ARM64 banner/prompt, status, full test report, clean Ctrl-C exit observed |

Raw final evidence is copied without normalization:

- [Full command output](hematite-dual-suite-final.log).
- [ARM boot](hematite-dual-final/aarch64/boot/result.json),
  [ARM profiles](hematite-dual-final/aarch64/profiles/result.json).
- [x86 boot](hematite-dual-final/x86_64/boot/result.json),
  [x86 profiles](hematite-dual-final/x86_64/profiles/result.json).
- Each case directory includes `command.json`, `input.json` (ELF hash), raw
  `serial.log`, and structured results. Environment versions are beside them.
- Final ELF layouts: [ARM64](hematite-arm64-elf-final.log),
  [x86_64](hematite-x86-elf-final.log); separate RX, R and RW load segments.

## Failures and review

[First x86 build](hematite-build-01.log) failed because a small code-model
override conflicted with precompiled `core` under LTO. Removing the unnecessary
override kept the built-in target's code model; the next build passed.
An initial QEMU 10.2.1 environment was replaced by the smaller frozen Debian
one; its earlier logs are retained, not represented as final-environment tests.

Independent AI reviews [0001](../reviews/0001-bootstrap.md) and
[0002](../reviews/0002-arm64-bootstrap.md) found and rechecked corrections for
hardware-mutated foreign statics, untested alias entries, address-only linker
markers, ARM CurrentEL observations and WXN status. Final tests above follow
those fixes. No human review or complete unsafe audit is claimed.

## Limits

Guard tests inspect active mappings, not recoverable fault delivery. ARM has
minimal fatal vectors; x86 unexpected faults may triple-fault. ARM UXN/PXN/PAN
observations do not prove a future EL0 memory model or equivalence to SMEP.
No allocator, scheduler, SMP, userspace ABI, native capability model, virtio
transport, filesystem or networking exists. Differential Linux testing starts
when a specific compatibility interface is implemented. Formal remote CI
status is tracked separately from these local results.
