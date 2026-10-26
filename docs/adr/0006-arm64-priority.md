# ADR 0006: prioritize ARM64

Status: accepted direction from Aaron on 2026-10-25; implementation evidence
is recorded separately, not inferred from this decision.

Aaron asked to prioritize ARM support. Make **AArch64 QEMU virt** the primary
local build/run target now, with the working x86_64 microvm port retained as a
regression target. Do not wait until the last roadmap phase to exercise the
architecture boundary. This changes priority, not the unsafe boundary or the
requirement to measure claims.

The first ARM milestone shares safe monitor/policy code and uses an original
ARM entry, translation tables, PL011 console and bounded semihosted test exit.
Test both architectures from the same pinned container with separate output
directories. Default `make run` uses ARM64; `ARCH=x86_64` selects the existing
port. `make test-all` runs both serially.

Scope is a 64-bit virtual ARM machine, not 32-bit ARM, Apple hardware, phones,
Raspberry Pi, physical bootloaders or device support. TCG provides a reproducible
first baseline on this x86 laptop. Native ARM/KVM performance remains untested.

Architecture-specific protection checks must describe the mechanism actually
observed: x86 SMEP/SMAP are not ARM UXN/PXN/PAN. Native user-memory and resource
contracts remain shared. See [ARM bootstrap design](../design/0002-arm64-bootstrap.md)
and [native-first direction](0005-native-first-design.md).
