# Fallible page-backed heap ownership

Status: independently reviewed implementation contract, 2026-10-25; see
[review 0006](../reviews/0006-fallible-heap.md). Review is by a separate AI agent,
not human approval or a soundness certification. Extends [frame ownership](0005-frame-allocation.md)
without changing its boot-memory, singleton or single-CPU preconditions.

This layer allocates contiguous owned blocks and typed values from the existing
ARM pool. It is a first kernel heap with page-granularity charges, not a global
allocator or an `alloc`-crate integration. Every nonempty request rounds up to
whole 4 KiB pages; a zero-size request deliberately reserves one page. Small
objects therefore waste space, and fragmentation can deny a request despite
enough aggregate free pages. Record those costs; add measured size classes or
slabs before claiming suitability for service workloads. Multi-page objects
and alignments greater than one page are supported when a suitable run exists.

## Allocation and reclamation

Frames and heap blocks use the same pool bitmap and immutable eligibility map.
Neither path may bypass the other's live bits. A safe block request accepts
size and alignment integers, validates them with `Layout::from_size_align`,
and returns InvalidLayout for invalid geometry. Determine charged pages from
`max(size, 1)` using checked rounding and division. Reject requests larger than
the bounded arena as an ordinary unavailable/exhausted result, without mutation.

Scan only bounded candidate runs. Each candidate must be wholly eligible and
free, its physical start divisible by the requested alignment, and its checked
end inside the arena. For a valid Layout whose alignment cannot be satisfied
there, return exhaustion. Align the actual address, not merely the page index.
Do not calculate a huge rounded physical address that could overflow.

Validate the entire run before changing any bits; commit its occupied bits in
one bitmap update. This is a transaction in the current single-threaded model,
not an atomic/SMP allocator claim. No callback or destructor runs between the
scan, claim and initialization. Zero every charged page before publishing the
owner. All ordinary errors leave the bitmap and existing allocations unchanged.
The trusted mapping contract makes zeroing valid; hardware faults are not an
allocation-error recovery path in this increment.

`Block<'pool>` has private pool borrow, start index, page count and requested
length. It cannot Clone or Copy. Its shared/mutable byte slices cover exactly
the requested length and borrow the Block, never the entire pool lifetime.
Size zero returns an empty slice at the actual aligned, non-null owned address.
No safe raw-address constructor or release-by-index method is exposed.

Drop returns precisely this run after all usable byte borrows have ended.
The internal release primitive is unsafe: callers prove exclusive ownership of
the full claimed run and absence of surviving aliases. Frame and Block drop
provide separate proofs. Forgetting a Block leaves every charged bit occupied;
pool movement or drop does not reset the permanent construction latch. The
borrow of a Cell-containing Pool keeps blocks non-Send and non-Sync. No unsafe
trait implementation overrides that restriction.

## Typed ownership

`KernelBox<'pool, T>` owns one private Block, an appropriately aligned pointer
to its initialized `T`, and `PhantomData<T>` to express value ownership for
drop checking and auto traits. Only sized T is supported initially. A fallible
constructor obtains a block using `Layout::new::<T>()`, then moves the caller's
valid value with `ptr::write`. On allocation failure it returns the original
value together with the error; it must not drop, copy or partially initialize T.
There is no fallible step between writing T and returning its unique owner.

Deref and DerefMut create references tied to the respective KernelBox borrow.
The backing Block stays private and its bytes must not be exposed or inspected
after typed initialization: Rust moves can leave padding bytes uninitialized,
even though the allocation was initially zeroed. Never derive a byte slice
over the live T as an intermediate step in these operations. Use the retained
owned pointer and type validity directly. No raw extraction, unsizing, cloning,
pinning guarantee or `into_inner` API is included in this first contract.

Drop invokes `drop_in_place` exactly once while the backing run is still owned;
only afterward does the Block field release its pages. T's destructor may
allocate or drop other owners from the same pool. No bitmap borrow, mutable
pool guard or partially committed allocator state may be held across that call.
The current profiles abort on panic; no unwinding or destructor-panic recovery
claim is made. Forgetting the box leaks both its pages and T's destructor.

Zero-sized T follows the same ownership and destructor rules: allocate one
page at T's alignment, retain a non-null aligned pointer, and run its destructor
once on normal drop. Zero size is not permission to use an unaligned pointer.
Borrowed values need only outlive their box, not be `'static`. The typed owner
cannot outlive either its pool or any reference contained in T.

## Safe policy surface

Add a safe `OwnedBlock` trait with diagnostic address plus `bytes`/`bytes_mut`
slice methods, and a Platform GAT for its owned block type. A fallible block
method takes size/alignment. A second GAT owns `T` and implements
`DerefMut<Target = T>`; the corresponding fallible method returns that owner or
`(HeapError, T)`. The pool borrow and `T: 'a` bound must follow the actual method
borrow, permitting short-lived referenced values. These are internal Rust
interfaces, not a stable userspace ABI.

ARM supplies actual owners. x86 returns explicit Unsupported and preserves a
failed typed request's value; no physical x86 heap support is claimed. No heap
owner grants access to userspace, DMA, page tables or alternate mappings.
Those uses require ownership transfer and a separately reviewed aliasing model.

## Verification required

- Invalid alignments, overflowing sizes and unavailable valid layouts fail
  without changing accounting. Cover zero size, page edges, multi-page requests,
  over-alignment, fragmented runs and reservations. Range tests remain bounded.
- Live Frames and Blocks never overlap. Dirty every requested byte across page
  boundaries, retain a neighbour during drop/reuse and verify isolation and
  zeroing. Exhaustion does not invalidate existing owners; restoring only test
  claims returns accounting to its prior state.
- Forgetting owners retains their allocation. Diagnostic leak checks must run
  once per pool or track their loss so repeated monitor tests do not leak more.
- Typed fixtures cover mutation, borrowed values, over-alignment, zero-sized
  destructors, exactly-once destruction, value preservation on failure and a
  destructor that reenters allocation while its own pages remain occupied.
- Compiler probes include a valid client, rejected cloning/field forging,
  pool/value lifetime escape, overlapping mutable borrows, drop while borrowed,
  and forbidden Send/Sync transfers. Compile the actual core implementation.
- Preserve existing frame, boot, parser, protection and capability tests on
  both targets. Exercise ARM 64/128 MiB profiles. Keep stack use bounded and
  retain exact source identities, commands, raw logs and failures.

## Provenance

Consulted 2026-10-25; no Linux implementation source consulted or copied.
The project remains pinned to Rust 1.99.0; the linked moving docs supply API
contracts, not a toolchain upgrade or dependency selection.

- [Layout](https://doc.rust-lang.org/core/alloc/struct.Layout.html): checked
  size/alignment construction, including zero-size layouts.
- [ptr::write](https://doc.rust-lang.org/core/ptr/fn.write.html): move a valid
  value into aligned owned storage without dropping uninitialized contents.
- [drop_in_place](https://doc.rust-lang.org/core/ptr/fn.drop_in_place.html):
  valid aligned non-null value and exclusive destructor access, including ZSTs.
- [PhantomData](https://doc.rust-lang.org/core/marker/struct.PhantomData.html):
  express logical ownership and its type-system consequences.
