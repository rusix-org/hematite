# Independent AArch64 bootstrap design review

Date: 2026-10-25. Reviewer: separate AI agent, not human approval.
Scope: proposed ARM bootstrap and [design 0002](../design/0002-arm64-bootstrap.md),
before code. No build, emitted ELF or QEMU execution reviewed.

The proposed 39-bit L1/L2/L3 geometry is consistent: UART lies at L1[0],
L2[72], L3[0]; the first 16 MiB of RAM uses L1[1], L2[0..8). A fixed bootstrap
without userspace or allocator can use these mappings while device discovery
and memory-map validation remain future work.

The implementation must resolve the following pre-boot requirements, now
incorporated into the design:

1. **Define reset/translation ordering.** Reject an unexpected initial EL or
   enabled MMU/cache state. Table stores, MAIR/TCR/TTBR setup, local TLB
   invalidation and SCTLR enable need DSB/ISB ordering; do not just set M.
   Check 4 KiB granule support and use valid TG1 even with EPD1 set. Ensure
   stack and transition instructions remain identity-mapped across enablement.
2. **Do not leave exception destinations undefined.** DAIF masks do not stop
   synchronous faults. An unspecified VBAR can select an unspecified address;
   a host timeout is not a defined guest failure path. Install aligned minimal
   vectors leading to stack-free failure before risky setup. General recovery
   and scheduling are explicitly out of scope.
3. **Distinguish privilege protections.** UXN prevents EL0 execution, while
   PXN prevents privileged execution. Do not report a UXN check as an ARM
   implementation of x86 SMEP. No user mappings means the bootstrap can prove
   its own isolation layout; it cannot prove future user-memory semantics.
   PAN must be feature-checked before use, set explicitly, and preserved by the
   chosen SPAN behavior.
4. **Respect the compiled-code contract.** Select and align SP_EL1, initialize
   FP/SIMD control/status before compiled Rust, and do not clear a live stack
   during BSS initialization. Bound all storage with linker assertions.
5. **Keep hardware access representable in Rust.** Mutable extern tables with
   raw volatile reads avoid the immutable-static problem identified in the x86
   review. MMIO must stay fixed, aligned, device-typed and reference-free.
   A semihost exit block must remain live through the trap; early errors cannot
   depend on a stack that has not yet been initialized.

No fundamental mapping-design blocker remains if these requirements are
implemented. That conclusion is conditional: code, register constants,
descriptor encodings and actual pinned QEMU entry state still need review and
execution. General ARM hardware, EL2/EL3 handover, SMP and userspace safety are
not established by this design review.

Primary references are linked in design 0002. The review additionally consulted
the [QEMU semihosting invocation documentation](https://www.qemu.org/docs/master/system/qemu-manpage.html)
and Arm's published semihosting interface. No kernel implementation source was
consulted or copied.

## Implementation review and resolution

Subsequently inspected `crates/hm-core/src/boot-aarch64.S`, `aarch64.rs`,
`linker-aarch64.ld`, the shared entry and target configuration. No builds or
QEMU runs were performed by this reviewer.

The implemented constants match the intended translation geometry:
TCR `0x80993519`, SCTLR `0x3058180d`, supervisor-only AF-set RAM leaves,
RO text with UXN, and PXN/UXN on non-executable data/device leaves. The UART
leaf uses MAIR attribute 1; normal RAM uses attribute 0. Parent links and all
unused entries are checked. The 16 vector entries are each 128 bytes in a
2048-aligned table and branch to a stack-free semihost failure path. BSS is
cleared before stack use; the enabled translation retains the executing code,
vectors, stack and UART mappings. The fixed exit blocks and Rust stack block
have the alignment, size and lifetime required by the semihost operation.

Three review findings were reported and then re-inspected as resolved:

1. ARM boot observations initially omitted CurrentEL despite the design's
   contract. They now read CurrentEL and gate the relevant protection results
   on EL1.
2. The individual W^X observation initially omitted SCTLR.WXN while the NX
   observation checked it. W^X now checks it too, matching the stated contract;
   guard observations also require the actual complete mapping check.
3. Address-only linker markers used immutable `u8` extern declarations. Such
   markers can coincide with adjacent mutable storage and should not assert
   byte immutability. Both architecture modules now declare those markers as
   zero-sized arrays, use only their addresses, and never dereference them.
   Actual table arrays remain mutable extern statics with raw volatile access.

No remaining blocker was identified in this limited source review. This is
not proof of all ARM architectural behavior or future concurrency safety.
The main agent owns final compiled-image inspection and rerun evidence after
these edits; earlier test results must not stand in for that rerun.
