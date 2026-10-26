# Independent review of ADR 0004

Date: 2026-10-25. Reviewer: separate AI agent; not human review or Aaron's
approval. Scope: the proposed architecture text, not implementation soundness.
No kernel code exists for this review, and no boot experiment was performed.

The capability boundary is a reasonable outline. Resolve the following in
the relevant API/boot designs before implementing those mechanisms:

1. **MMIO authority needs more than register bounds.** ADR 0004 permits
   checked widths/offsets inside an acquired device region. That alone does
   not prevent a safe caller from programming an arbitrary DMA destination
   or descriptor address through a legitimate register. Specify how safe
   device code can supply only owned DMA capabilities, including indirect
   descriptors, queue reset and completion. Raw device configuration that
   can violate memory ownership cannot be an unrestricted safe API. This is
   an inference from the ADR's own no-arbitrary-physical-mapping invariant;
   it is not an audit finding against implemented code.
2. **Guard correctness cannot depend on destructors running.** Explicitly
   account for forgotten guards and requests, nested guards, and cancellation.
   Rust permits safe callers to bypass `Drop` with
   [`mem::forget`](https://doc.rust-lang.org/core/mem/fn.forget.html).
   Leaking a resource may preserve memory safety; prematurely reclaiming a
   DMA buffer or allowing overlapping CPU-local mutable access does not.
   The eventual public API review must check these paths, not just normal
   scope exit. Non-transferability alone is insufficient.
3. **Pinning must distinguish lifetime from permissions.** A frame pin keeps
   storage alive but does not itself serialize `mprotect`, unmapping or address
   space replacement. The user-copy design must define its synchronization
   and permission-change linearization, and keep actual copying free of Rust
   references that imply user memory is immutable or exclusive. The ADR
   already anticipates faults and partial transfer; specify the pin/lock
   contract when those APIs are designed.
4. **PVH entry is not a complete microvm discovery contract.** The
   [PVH specification](https://xenbits.xen.org/docs/unstable/misc/pvh.html)
   describes entry state, while [QEMU's microvm documentation](https://www.qemu.org/docs/master/system/i386/microvm.html)
   describes firmware and machine options. These do not establish that the
   proposed pinned firmware supplies every required boot datum. Record the
   exact firmware/QEMU pair, RAM and reserved-region source, CPU enumeration,
   interrupt routing and virtio-mmio discovery. Verify PVH loading, the ISA
   test exit device, and serial output on that pair. The ADR correctly leaves
   this unverified; it must remain a feasibility item until measured.

Primary sources above were consulted on the review date. Moving documentation
is evidence for constraints, not a selected toolchain or machine version.
No Linux implementation source was consulted. These findings do not request
new permission beyond the existing decision/core-design gates in
[AGENTS.md](../../AGENTS.md), and do not alter the project direction.
