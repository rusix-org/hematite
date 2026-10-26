# ARM owned data-page mappings

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0008-arm-owned-mappings.md).
Date: 2026-10-26. Extends [frame ownership](0005-frame-allocation.md) and
[ARM bootstrap](0002-arm64-bootstrap.md).

Implement actual kernel data-page mappings whose lifetime retains their backing
frames. This is a bounded first kernel mapping arena, not a complete address-space
manager: dynamically allocated table pages, EL0, executable mappings, permission
changes, SMP and shootdowns remain later work.

## Arena and authority

Reserve virtual `0x80000000..0x80040000`: 64 slots of 4096 bytes, outside the
existing identity RAM and UART mappings. Add permanent linker-owned, 4 KiB aligned
L2 and L3 tables. Bootstrap root entry 2 points to this L2; L2 entry 0 points to
this L3. All leaves initially contain zero. The unused 448 L3 entries and other
L2 entries remain zero. Both tables lie inside the excluded kernel image and
are never allocator frames. Bootstrap establishes this hierarchy before enabling
the MMU; runtime changes only the 64 permitted leaf entries.

A private constructor creates one `Mapper` after full bootstrap validation.
A permanent atomic construction latch rejects every later construction, including
after the first mapper is dropped. The mapper holds `Cell<[u64; 64]>` recording
the physical page in each occupied slot (zero means vacant). It is not Clone or
Sync. Mapping handles must also be neither Send nor Sync; they borrow the mapper
and retain their allocator-backed frame. No safe API constructs a mapper,
supplies raw page-table addresses, selects physical pages by integer, or edits
arbitrary descriptors. Automatic slot selection is sufficient for this increment.

The mapper's destructor does **not** clear tables, reclaim frames, reset the
latch or reconstruct ownership from the ledger. A forgotten child can leave a
mapping behind after the borrow checker permits its mapper/pool to disappear.
Permanent tables and permanent allocator/mapper latches make this a resource
leak, without reuse of the still-mapped memory. This invariant does not rely on
Drop being called.

## Safe interface and alias proof

The core operation consumes an actual `Frame<'pool>` and borrows `&Mapper`,
returning `MappedPage<'mapper, 'pool>` or `(MapError, Frame<'pool>)`. Full arena
and unsupported-platform failures return the original frame without changing
its contents, allocator accounting, ledger or translation tables. The public
policy boundary uses corresponding associated types; x86 reports Unsupported.

Use a safe `OwnedMapping` trait with `type Frame: OwnedFrame`, the address and
byte methods below, and `unmap(self) -> Self::Frame`. `Platform` supplies
`Mapping<'a>: OwnedMapping<Frame = Self::Frame<'a>>` and
`map_frame<'a>(&'a self, frame: Self::Frame<'a>) ->
Result<Self::Mapping<'a>, (MapError, Self::Frame<'a>)>`. `MapError` distinguishes
Unsupported, Exhausted and CorruptState (a vacant ledger slot with a nonzero
leaf, rejected without mutation); read-only `MapStats { total, free }` enables safe
policy accounting tests. A private core diagnostic handles hardware translation
observations and the one-time forgotten mapping. The public mapping API itself
must support ordinary native kernel clients, not only test booleans.

A mapping exposes its virtual address and physical address as informational
integers, and `bytes(&self) -> &[u8; 4096]` / `bytes_mut(&mut self) -> &mut
[u8; 4096]` through the new virtual address. It never exposes the retained
Frame, its identity-map byte methods, or a dereferenceable raw pointer. All
mapped pages are initialized, normal-cacheable, inner-shareable, EL1 RW,
EL0-inaccessible, PXN and UXN, with the same memory attributes as their identity
mapping. No API requests executable or read-only mappings in this increment.

Moving a frame into a mapping requires all usable prior frame-byte borrows to
end. While mapped, its private retained Frame is used only for allocation
ownership and address metadata, never identity-map byte access. Each physical
frame can have only one mapping owner, because Frame is not cloneable and its
constructor is private. Shared mapped borrows prevent mutable access; exclusive
mapped borrows prevent any other byte access. The direct physical alias remains
in hardware, but no live Rust byte reference or independent safe access path
uses it. Hardware translations alone are not Rust references. Unmap requires
ownership of the handle, so no usable mapped-byte borrow survives it.

`unmap(self)` invalidates the leaf and completes maintenance before returning
the original Frame. Ordinary Drop performs the same invalidation before dropping
the retained Frame. Implement ownership extraction without double-drop: e.g.
private `ManuallyDrop<Frame>` and a single well-reviewed explicit extraction
path. No callback, allocation, formatting or user-driven panic occurs between
publication and constructing the result, or during removal. Frame reclamation
must always follow completed invalidation, not just a descriptor store.

`mem::forget(mapping)` leaves its occupied ledger entry, valid descriptor and
allocated physical frame permanently retained. A later allocation cannot obtain
that frame, and map cannot reuse its slot. Dropping the mapper or pool cannot
change this. Deliberate diagnostic leaks are limited to one mapped frame/slot
per boot; repeated monitor tests do not leak additional resources.

Revise the frame-pool constructor contract narrowly: identity mappings remain
unchanged for the pool's lifetime. Additional aliases are permitted only through
this consuming ownership protocol, with matching attributes and no simultaneous
identity access. Existing Block/KernelBox backing is not mappable. Do not replace
the current invariant with a general permission to mutate mappings.

## Publication and removal

Run on one CPU, EL1, asynchronous exceptions masked, with no DMA or other
translation-table writer. No table references are formed: aligned descriptor
loads/stores use raw volatile operations. TCR hardware access/dirty updates are
disabled and AF is set in valid leaves.

For map, select a vacant ledger entry, verify its leaf is invalid, retain the
Frame, record its physical address, and publish that page's fixed RW/NX leaf.
Then execute `DSB SY; ISB` before returning access. For removal, store zero,
execute `DSB SY; TLBI VMALLE1; DSB SY; ISB`, then clear the ledger slot and
return/drop the Frame. Inline assembly must retain compiler memory effects:
do not mark this sequence `nomem`, `readonly` or `pure`.

These are deliberately conservative full barriers and a local all-stage-1 EL1
invalidation. Every reused slot has completed the invalidation sequence before
another descriptor is published; no valid-to-valid replacement occurs. The
protocol follows break-before-make. It does not establish multi-CPU safety:
enabling another CPU or asynchronous access requires a separately reviewed
ownership and invalidation protocol first.

## Mapping checks and evidence

Extend BootChecks to validate permanent root/L2 links and all 512 arena leaves.
For occupied slots, require the exact descriptor dictated by the mapper ledger;
vacant and out-of-arena leaves remain zero. Before Mapper construction, all
arena leaves must be zero. Preserve complete checks of identity RAM, code/data
permissions, both guards, null exclusion and UART. Checks must succeed while
several owned mappings are live, not only after cleanup.

Add private bounded hardware-translation observations: `AT S1E1R` followed by
`ISB` and reading `PAR_EL1`, for a validated arena slot only. A successful probe
must report the retained physical page; after removal it must report a fault.
These observations do not dereference unmapped memory or broaden exception
recovery. They supplement actual mapped loads/stores, not replace them.

Required guest coverage:

- Allocate a frame, write a pattern, map it, verify the pattern through the new
  VA, mutate there, explicitly unmap, and verify the returned identity-mapped
  frame contains the mutation. Confirm hardware translation and BootChecks
  while mapped, then a hardware translation fault after unmap.
- Keep multiple mapped pages and ordinary Frame/Block owners live; verify
  disjoint contents, distinct slots and unchanged ownership/accounting.
- Warm the TLB with actual byte accesses. Unmap and hold the returned frame;
  map a different physical frame into that same first-free slot. Repeated
  distinguishable patterns must reach the new frame while the old frame's
  bytes remain unchanged. Check fault state between mappings.
- Drop a mapping, observe hardware translation failure before slot reuse,
  and verify backing capacity returns. Reallocate the physical page and check
  allocator zeroing, including bytes previously mutated through the mapping.
- Fill all available slots with real mapped pages. One further map must return
  its input Frame unchanged. Clean up and prove full non-leaked capacity returns.
- Forget one mapping once, prove its slot/descriptor/frame stay retained through
  later allocations, and verify repeated diagnostics preserve remaining capacity.
  Keep the leak bounded across repeated `test` commands.

Compiler probes must reject use-after-map of Frame, use-after-unmap of a mapped
reference, duplicate mutable mapped borrows, mapping escape past mapper/pool,
Clone, Send/Sync, and construction of arbitrary mappings. Include positive
controls for consuming map/unmap and shared reads. Source review additionally
checks forgetting and mapper destruction, which borrow-negative tests alone
cannot prove. Run existing allocator/heap/fault checks and both architecture
suites; preserve raw failures and source/ELF identities. x86 unsupported results
do not claim ARM mapping coverage.

## Provenance and limits

Consulted 2026-10-26; no Linux implementation source used or copied.

- [Arm AArch64 memory management, 101811_0105_00_en, version 1.4](https://documentation-service.arm.com/static/68e680a2587fd957f5efd742),
  page 30: translation removal, completed invalidation and break-before-make.
- [Arm TLBI VMALLE1 reference](https://support.arm.com/documentation/101550/0001/AArch64-registers/AArch64-register-descriptions/AArch64-System-instruction-register-description/TLBI-VMALLE1--TLB-Invalidate-by-VMID--All-at-stage-1--EL1):
  local EL1 stage-1 invalidation scope. The one-CPU restriction is intentional.
- [Rust Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html):
  shared/exclusive reference obligations; this ownership proof applies to all
  safe clients, including callers that forget handles.
- [Arm system instruction reference](https://documentation-service.arm.com/static/606f045785368c4c2b1c198c),
  AT S1E1R entry, page 140: EL1 read translation result reported in PAR_EL1.

Passing QEMU tests establishes behavior for this bounded emulated profile,
not an independent soundness proof or native-ARM cache/errata validation.
Permanent table storage is explicit overhead; reclaimable table trees and
address-space teardown require a later design retaining all walkable tables
until they are detached and invalidated.
