# ARM owned mapping review

Date: 2026-10-26. Independent AI review of
[design 0008](../design/0008-arm-owned-mappings.md), not human approval or a
soundness certification. Reviewer inspected the existing allocator, ARM boot
tables, mapping checks, linker reservations and frame compiler probes.

## Design review

Ready for implementation within the stated single-CPU scope. No blocking
design finding. The bounded arena provides actual mutable page mappings while
preserving the allocator's authority over physical storage.

The consuming Frame interface is essential: a mapped owner must hide the
retained Frame and its identity-map byte methods. Earlier frame borrows cannot
survive movement into the mapping, and mapped borrows cannot survive consuming
unmap. Identical cache/shareability attributes on the identity and arena aliases
avoid inconsistent physical-memory attributes. Hardware aliases alone provide
no independent Rust access path. No other core API may construct a byte
reference to the retained physical frame while the mapped owner exists.

Forget is safe because a forgotten mapping loses both its Frame destructor and
its slot reclamation path. Mapper destruction must remain inert: permanent
linker tables, permanent mapper/allocator construction latches and unavailable
allocation bits prevent reuse even when safe code later drops the mapper or
pool after forgetting a child. A live child instead retains both borrows. The
implementation must not introduce automatic table cleanup or an index-based
recovery API that circumvents this argument.

Only initially-invalid or fully-invalidated leaves become valid. Removal must
store an invalid descriptor, complete DSB/TLBI/DSB/ISB, and only then clear the
slot and extract/drop the retained Frame. Local VMALLE1 invalidates all relevant
levels and global/non-global entries for the sole active PE. It does not grant
SMP support. Compiler memory effects are required as well as CPU barriers; the
maintenance assembly must not carry nomem/readonly/pure options. Explicit unmap
and Drop should share one extraction/removal path so each owner is released
exactly once.

The existing Pool constructor contract must be updated narrowly to permit this
consuming alias protocol while retaining unchanged identity mappings. Mapping
arbitrary physical integers, blocks, typed values, kernel storage or executable
memory remains outside the safe interface. Fixed table storage remains reserved
by the linker-image exclusion, including the two new table pages.

Boot checks must compare every new table entry against independently retained
ownership metadata: root entry 2, the single L2 link, all occupied leaves and
all unused entries. Initialization must require an entirely empty arena before
claiming the singleton. Full-arena failure must return the supplied frame with
all state and contents unchanged. If an expected vacant leaf is unexpectedly
valid, map must reject without overwriting it or losing the input owner.

The required live byte accesses, warmed-slot replacement with a different
physical frame, and AT observations cover distinct properties. An AT result
must first check PAR_EL1's fault bit before interpreting the output address.
After unmap, observing a fault supplements the ordering argument; passing TCG
tests cannot prove native hardware coherency or replace the source review.
Compiler rejection probes need positive controls and actual implementation
types, not replicas of the ownership interface.

## Source review

Reviewed `mappings.rs`, the changes to `aarch64.rs`, `boot-aarch64.S`, `main.rs`
and the `frames.rs` constructor contract, plus the safe mapping trait/tests,
Platform integration and the mapping compiler harness. No blocking core source
finding. No build or guest execution was performed by this reviewer; runtime
claims belong to separate integration evidence.

- `Mapper::map` selects a bounded vacant slot, rejects a mismatching valid leaf
  without changing ownership, consumes the actual Frame into private storage,
  updates the ledger and publishes the exact normal RW/PXN/UXN descriptor.
  No potentially fallible callback intervenes between publication and return.
- Both explicit unmap and Drop call the same private removal routine before
  extracting or destroying the retained Frame. The invalid store and full
  local DSB/VMALLE1/DSB/ISB sequence precede ledger release. Explicit unmap
  suppresses automatic Drop after extracting the Frame, avoiding double removal
  or release. No public method exposes identity byte access while mapped.
- Mapper has no destructor and never resets its singleton latch. Its metadata
  uses Cell, and handles borrow both mapper and allocator through their owned
  Frame, preserving non-Send/non-Sync handles. Forget leaves both allocation
  and slot unavailable; later pool/mapper destruction cannot free the backing
  or permanent tables.
- The bootstrap installs the two aligned BSS table pages before enabling the
  MMU. Both remain inside `__kernel_end` and outside allocator eligibility.
  Initial checks require all arena leaves zero before Mapper construction;
  later checks compare every L2/L3 entry with the live ledger and also retain
  the original full root, RAM, guard, privilege and device checks.
- Translation diagnostics validate the slot, issue AT/ISB, test PAR_EL1's
  fault bit and only then mask its physical result. They exercise actual byte
  accesses, repeatedly replace a warmed slot while retaining the former frame,
  observe translation failure after removal and confirm zeroing on reuse.
  The deliberate forgotten mapping is bounded to one slot/frame per mapper.
- Safe policy tests exercise ordinary frames and heap blocks alongside multiple
  mappings, full-arena unchanged-input failure, and retained contents after
  repeated remapping. x86 unsupported scope remains explicit. The compiler
  harness compiles the actual modules and checks specific diagnostic codes,
  including positive map/unmap, shared-read and forget-then-drop clients.

Verification follow-up closed: direct forged `MappedPage` construction was
added alongside private backing access and the private Mapper constructor.
The reviewer inspected the final generated client, command, raw compiler
diagnostic and result under `target/evidence/owned-mapping-types-final/`.
The compiler rejected its private `mapper`, `slot` and `frame` fields with
E0451; the final result reports all 21 client probes passed, including three
positive controls. Additional probes reject integer backing and double unmap.
This confirms the requested compiler boundary check, not runtime translation.

## References checked

- [Arm memory management guide 101811_0105_00_en, version 1.4](https://documentation-service.arm.com/static/68e680a2587fd957f5efd742),
  page 30: descriptor removal and completed invalidation before relying on new
  translation state; break-before-make for replacement.
- [Arm TLBI VMALLE1 reference](https://support.arm.com/documentation/101550/0001/AArch64-registers/AArch64-register-descriptions/AArch64-System-instruction-register-description/TLBI-VMALLE1--TLB-Invalidate-by-VMID--All-at-stage-1--EL1):
  local PE, all translation levels, global and non-global EL1 stage-1 entries.

Only primary architecture documentation informed the maintenance review; no
Linux implementation was used or copied.
