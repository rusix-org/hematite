# AArch64 bootstrap design

Status: implementation contract for the ARM-first bootstrap; no boot or
soundness result claimed. Date: 2026-10-25. Independent review:
[0002-arm64-bootstrap](../reviews/0002-arm64-bootstrap.md).

Aaron prioritizes ARM for the native-first Rust OS. Keep the existing safe
policy/trusted-core boundary; architecture code remains in `hm-core`. This
experiment covers one QEMU CPU, fixed boot mappings, serial monitor and tests.
It does not implement userspace, allocation, DMA, scheduling or SMP.

## Pinned entry profile

Use the project's pinned QEMU with `virt,virtualization=off,secure=off`, CPU
`max`, one CPU and 128 MiB RAM. Record the exact emulator version and command;
`max` is not a portable hardware baseline. Direct-load an AArch64 ELF linked
at physical/virtual `0x40200000` (originally `0x40080000`;
[boot-memory design 0004](0004-boot-memory.md) reserves the larger DTB prefix). All bootstrap allocations must fit below
`0x41000000`; linker assertions cover file-backed and NOBITS storage.

Require `CurrentEL == EL1` and initial SCTLR M/C/I clear. Other initial ELs or
already-enabled translation/caches fail rather than attempting an undocumented
handover. Mask DAIF, select SP_EL1, clear owned BSS before using it as a stack,
then initialize a 16-byte-aligned 64 KiB stack. Never treat incoming registers
as validated boot metadata. This fixed QEMU profile is not a device-discovery
or physical-hardware protocol.

Before risky setup, install a 2 KiB-aligned minimal exception-vector table.
Each of its 16 128-byte entries branches to a stack-free failure path. This
provides deterministic abort behavior, not recoverable exception handling.
Leaving VBAR unspecified is not a fail-closed contract.

Require 4 KiB translation granule support and PAN from the architectural ID
registers before using those mechanisms. Enable FP/SIMD through CPACR_EL1,
initialize FPCR/FPSR, and issue the required instruction synchronization before
entering compiled Rust. No FP context switching exists in this phase.

## Translation and permission contract

Use TTBR0_EL1 with a 39-bit VA space and three 4 KiB levels (L1/L2/L3).
TCR_EL1 uses T0SZ=25, 4 KiB TG0, inner-shareable normal write-back table walks,
32-bit IPS and EPD1=1. Give disabled TTBR1 a defined zero value and a valid
TG1 encoding (4 KiB is `0b10`), with T1SZ=25. Leave top-byte ignore and optional
hardware permission-update modes disabled. MAIR attribute 0 is normal WB
(`0xff`); attribute 1 is Device-nGnRnE (`0x00`).

| Mapping | Translation path | Leaf permission |
| --- | --- | --- |
| PL011 `0x09000000..0x09001000` | L1[0] → device L2[72] → device L3[0] | EL1 RW, no EL0 access, Device-nGnRnE, PXN+UXN |
| RAM `0x40000000..0x41000000` | L1[1] → RAM L2[0..8) → 4096 RAM leaves | Normal memory, no EL0 access; permissions below |
| Everything else | Invalid entries | No translation |

Text is EL1 RO/executable and UXN. Rodata is EL1 RO/PXN/UXN. Other RAM is
EL1 RW/PXN/UXN. The stack guard is a separate invalid leaf; page zero is
unmapped because the only low-address leaf is PL011. All section/guard edges
are page-aligned. Set leaf AF explicitly. No second alias or user mapping is
created; every unused table entry stays zero.

Keep table arrays mutable in Rust's foreign declarations, access them only
through checked raw volatile operations, and never manufacture references to
hardware-observable storage. Verify complete table hierarchy, including unused
entries. The core alone can change mappings; safe policy receives observations.

With the required initially disabled caches/MMU, finish table stores with DSB,
program MAIR/TCR/TTBR, synchronize, invalidate local EL1 TLBs, and complete
DSB/ISB before enabling translation. Initialize/invalidate instruction-cache
state for the loaded image as required by this reset-entry contract. Write
SCTLR with M/C/I/WXN/SA enabled and architecture-required RES1 bits, preserving
neither unknown feature enables nor endianness. Clear SPAN and set PAN=1.
Follow the SCTLR change with ISB before normal translated execution. No table
or memory-attribute changes occur after this transition.

## Safe operations and host reporting

The platform implements bounded PL011 read/write, read-only boot checks and
the owned boot-memory metadata described in design 0004. MMIO uses aligned 32-bit raw volatile operations on fixed registers
inside the mapped page; arbitrary addresses, widths and DMA are not exposed.
UART polling is bounded on transmit. The single CPU and disabled interrupts
bound the current concurrency model; they are not an SMP synchronization plan.

Exit uses Arm semihosting `SYS_EXIT_EXTENDED` (`0x20`) with `HLT #0xf000`.
Only core-owned, aligned, initialized two-u64 blocks are passed: reason
`0x20026`, status 0 for success, 1 for explicit failure, or 2 for an unexpected
exception. Early-failure blocks live in
owned image data and do not require a stack. Configure QEMU semihosting for
privileged trusted guest use; no user-supplied operation or pointer reaches it.
If the trap returns unexpectedly, halt/loop rather than returning through an
uninitialized call frame. Semihosting is test instrumentation, not native ABI.

## Verification scope

Boot checks read actual CurrentEL, SCTLR, TCR, TTBR, MAIR, PAN and the complete
table tree. `write_protect` means the active EL1 AP permissions match the
required RO/RW layout; `nx` means active translation and expected PXN/UXN
restrictions, not merely the presence of a feature bit. W^X also requires
WXN and no alias. Guard checks inspect invalid translation paths.

Architecture-neutral user-protection names must retain explicit scope. UXN
denies EL0 execution; it is not equivalent to x86 SMEP. Privileged execution
of future EL0-accessible mappings must be denied by PXN. The initial tests
prove only that this bootstrap has no EL0-accessible mappings and that PAN is
enabled; no EL0 transition or fault-recovery behavior is established.

Require the full named serial test inventory and semihost success status.
QEMU exit 0 alone is insufficient. Record CPU-feature rejection and interactive
monitor results separately, retaining failures, timeouts and exact ELF hashes.
Pin and validate architecture-specific exit semantics in the host harness.
The missing-PAN profile must report `HMBOOT REJECT PAN` after UART setup and
exit 1, never a successful test inventory. This distinguishes intended CPU
rejection from an emulator launch error with the same numeric exit status.

## Primary references

Consulted 2026-10-25; no Linux or other kernel implementation copied.

- [QEMU virt board](https://www.qemu.org/docs/master/system/arm/virt.html):
  board, CPU and direct-boot constraints; moving manual does not pin QEMU.
- [Arm AArch64 memory management guide](https://documentation-service.arm.com/static/68e680a2587fd957f5efd742):
  translation geometry, permission and attribute design reference.
- [Arm Cortex-A57 TRM](https://documentation-service.arm.com/static/5e906b3d8259fe2368e2aa8a):
  baseline TCR encodings; newer feature handling must follow the selected CPU.
- [Arm semihosting specification](https://github.com/ARM-software/abi-aa/blob/main/semihosting/semihosting.rst):
  AArch64 trap, registers and extended exit block.
