# Fallible heap review

Date: 2026-10-25. Independent AI review, not human approval or a soundness
certification. Design/source review and runtime evidence are separate.

## Review constraints

The heap must retain the frame pool's physical authority proof and permanent
single-instance claim. It may aggregate only consecutive eligible, unclaimed
pages; no success or failure may clear another frame/block's bits. Layout
validation and complete-run search precede the bitmap mutation. Commit the
entire run before zeroing and publishing a private non-cloneable owner.

References into byte blocks must be tied to the block borrow. Typed owners
must additionally retain the logical ownership/lifetime of `T`, initialize it
exactly once, and expose only typed borrows after initialization. Moving a
Rust value may leave its padding uninitialized; a byte-slice accessor over a
live `T` would therefore be invalid even if its block was initially zeroed.
The owned backing block stays private until after `T` is dropped.

Destruction runs `T` while its storage is still charged and alive. No bitmap
borrow or lock may span `T`'s destructor: it can reenter the allocator or drop
other owners. Reclamation follows the value destructor; forgetting an owner
must retain every backing bit. Allocation failure returns the original `T`
without running its destructor or changing accounting.

Zero-sized values still require non-null aligned typed pointers and exactly
one destructor call. The proposed one-page minimum is a defined cost, not a
license to disregard alignment. Size/alignment validation must reject invalid
Layout requests and keep every pointer/slice extent inside the owned run and
within `isize::MAX`. Oversized or impossible aligned runs fail normally.

Safe-client compiler probes should reject forged owners, duplicate owners,
escaping pool or payload lifetimes, overlapping mutable borrows and cross-thread
transfer/sharing. A valid client must also compile. Actual ARM tests must retain
an existing frame while allocating and releasing blocks, test mixed sizes and
over-alignment, and verify drop/failure/reentry behavior without exceeding the
64 KiB test stack. x86 must explicitly report Unsupported rather than claiming
physical heap coverage.

## Reference checks

Consulted the Rust primary documentation for
[Layout](https://doc.rust-lang.org/core/alloc/struct.Layout.html),
[drop_in_place](https://doc.rust-lang.org/core/ptr/fn.drop_in_place.html), and
[PhantomData/drop checking](https://doc.rust-lang.org/nomicon/phantom-data.html).
The documentation supports the layout, destructor and ownership constraints
above; executable evidence must use the project's pinned compiler.

## Design conclusion

Reviewed [design 0006](../design/0006-fallible-heap.md) before implementation.
The contract is ready for implementation: it explicitly retains the existing
physical authority boundary, validates/claims complete runs transactionally,
keeps raw backing private after typed initialization and preserves storage
through reentrant destruction. The page-granularity and zero-size costs are
stated rather than presented as a finished efficient heap.

No design blocker identified. No builds were run by this reviewer.

## Source review

Read the implementation in `frames.rs`, the ARM/x86 Platform adapters and the
safe heap API/tests. No memory-safety defect identified. Claiming a run checks
all eligibility, occupancy and address-alignment constraints before a single
bitmap update. Layout checks precede claiming; raw zeroing covers the bounded
owned run before publication. The unsafe release contract is discharged by
private Block ownership, and no new arbitrary-memory constructor appears.

KernelBox moves the valid supplied value directly into an aligned owned pointer,
keeps its Block private, and exposes only typed owner-bounded references. Its
Drop runs T before the backing field drops, without a Cell borrow or partially
committed state spanning user destructor code. The reentrant fixture checks
retained accounting, separate allocation and restoration. The failed typed
allocation diagnostic returns a non-Copy marker with no early destructor, then
checks exactly one destructor on explicit drop. Leaked owners remain charged.

Safe tests explicitly reject unsupported x86 allocation, check page charges and
live-Frame separation, dirty/reuse zeroing, zero-size alignment and destruction,
and borrowed values. Review requested additional explicit coverage for a valid
but impossible alignment and fragmented free space: aggregate free capacity
must not permit a multi-page allocation unless a complete run exists, and
failure must preserve the exact bitmap. These are verification additions rather
than identified implementation defects. Runtime and compiler results remain
the parent integrator's responsibility.

## Final source pass

The core now constructs fragmented free space by releasing and clearing only
even ledger-owned indices, leaving every adjacent candidate blocked. It requires
at least two free pages, rejects a two-page request with an unchanged bitmap,
and then releases only remaining ledger bits. Existing allocations are never
part of that cleanup. Safe policy retains a live typed value through the
exhaustion/fragmentation check and verifies it can still be read and mutated.
No new defect was identified.

The actual-module compiler harness includes 22 clients across frame and heap
APIs, requires positive controls, rejects missing outcomes, and matches the
intended diagnostic categories for negative clients. It covers private backing
bytes, borrowed payload escape, duplicate/forged boxes, borrow/drop conflicts
and Send/Sync. The final source also contains the requested valid-but-impossible alignment
fixture (`size = 1`, `align = 1 << 40`) requiring Exhausted and unchanged
statistics. Its runtime result is recorded by integration. The reviewer did
not run builds or QEMU.
