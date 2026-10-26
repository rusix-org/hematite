# Initial physical frame ownership

Status: independently reviewed implementation contract, 2026-10-25; see
[review 0005](../reviews/0005-frame-allocation.md). Review is by a separate AI
agent, not human approval or a soundness certification. Extends [boot-memory discovery](0004-boot-memory.md)
and the ownership constraints in [ADR 0004](../adr/0004-core-architecture.md).
This increment allocates actual ARM RAM within existing mappings. It does not
add a heap, new mappings, userspace access, devices, preemption or SMP.
The later [scoped timer contract](0009-arm-timer-irq.md) permits an assembly-only
IRQ window while owners exist; all Rust allocator operations remain masked.

## Selection and authority

The initial arena is the already mapped ARM interval
`0x40000000..0x41000000`: 4096 pages of 4096 bytes. Start with no eligible pages.
A page is eligible only when its entire half-open interval lies in an advertised
RAM range and it intersects none of:

- The entire pre-kernel DTB window `0x40000000..0x40200000`.
- The kernel interval `0x40200000..__kernel_end`, including all linker sections,
  tables, BSS, allocator metadata, current stack and its guard.
- Any reservation returned by the DTB parser, including intervals not contained
  in RAM and overlaps between reservation sources.

Testing complete-page containment implements inward RAM rounding. Rejecting
any byte overlap implements outward reservation rounding without constructing
potentially overflowing rounded addresses. A one-byte reservation excludes its
whole page. Exact adjacent boundaries do not overlap. Pages outside the arena
remain unavailable even when the DTB reports more RAM.

A safe `hm-kernel` planning module produces an immutable fixed eligibility
bitmap and descriptive counts. It accepts bounded slices of half-open regions,
checks malformed/overflowing inputs and arena geometry, and never truncates
capacity failures. Its output is freely inspectable metadata, not permission
to dereference memory. Empty usable memory is represented explicitly and
allocation then returns exhaustion; invalid descriptions return errors.

Only the trusted ARM initialization path may turn a plan into a pool. It uses
the actual parsed boot tree, fixed arena constants and address-only linker
symbols, after the DTB borrow has ended. Validate that the actual RAM map covers
the complete kernel and DTB intervals and that every chosen page is inside
initialized, mapped normal RAM exclusively available to this pool. Perform the
boot-map checks before construction, not merely in an optional test: usable
leaves must be writable, nonexecutable, supervisor-only and not aliases of kernel
code. No public safe constructor accepts caller-created maps or physical
addresses. A private unsafe construction boundary states these obligations;
safe plan construction alone does not satisfy them.

Construction claims a permanent one-shot latch before returning a pool.
Use an atomic compare/exchange or an equally reviewed primitive, not an
unguarded mutable static. A second construction fails, including after pool
drop or after a failed post-claim initialization. The latch is never reset:
leaking or dropping owners cannot authorize a replacement pool over the same
physical pages. Validation should precede the claim where possible; failure
may prevent future allocation but must not duplicate authority.

Authority also depends on the exact current boot profile: no initrd, firmware
runtime buffer, DMA-capable device or additional virtual alias is introduced.
The current parser ignores unknown `/chosen` properties, so its reservation
list is not evidence that an arbitrary initrd is excluded. Any future initrd
or firmware handoff must extend the reservation proof before allocation.

## Pool and frame contract

`hm-core` owns the concrete `Pool` and `Frame<'pool>` types. Pool fields and
Frame fields/constructors are private. Neither type implements Clone or Copy.
The pool keeps immutable eligibility and a `Cell` bitmap of unavailable/live
pages; bitmap storage is fixed and inside the excluded kernel/stack storage.
No heap allocation is needed. Allocation scans bounded bits, marks one eligible
page allocated, zeroes its full 4096 bytes, then returns its sole frame owner.
Allocation from an exhausted pool is a normal error with unchanged bitmap.
Zeroing must finish before exposing a byte reference or returning the owner.

`Frame<'pool>` contains a shared borrow of its originating pool and one private
page index. Allocation takes `&Pool`, so multiple independent frames can live
at once. The `Cell` makes Pool non-Sync; the contained `&Pool` makes frames
non-Send and non-Sync. No unsafe trait implementation changes these properties.
Current execution has one CPU and masked interrupts, with no reentrant caller
between bitmap claim and zeroing. This is not an interrupt/SMP allocator.

A safe policy trait, `OwnedFrame`, supplies an address for diagnostics plus
`bytes(&self) -> &[u8; 4096]` and
`bytes_mut(&mut self) -> &mut [u8; 4096]`. Reference lifetimes are tied to the
specific frame borrow, never extended to the pool or to `'static`. The core's
unsafe conversion uses only the internally proved page index/address. All
bytes are initialized and mutable references cover a uniquely owned page.
The identity map is a translation, not an additional Rust reference. No other
core API may construct aliases or expose arbitrary physical memory.

Dropping a frame returns precisely its own eligible bit to the same pool.
No public release-by-address/index operation exists. Safe Rust prevents drop
or movement while a returned byte reference is still used; private non-cloneable
owners prevent double release. Reallocation zeroes prior contents even when
the former owner dirtied the entire page. Forgetting a frame leaks its bit;
forgetting or dropping a pool never reconstructs ownership. A forgotten frame
whose object is inaccessible cannot yield further references, while any live
reference borrowed from it prevents the relevant move/drop. This contract
does not rely on destructors running for safety.

Pool movement is allowed only when Rust's borrows permit it; its fixed physical
arena does not move. A pool cannot be cloned or recreated to bypass outstanding
or leaked allocations. Before later mapping/DMA support, ownership must transfer
into types that retain the frame until all CPU/device access is quiescent.
This initial API must not be reused to give `&mut` access to a frame visible to
userspace, devices or another address space without a new review.

## Policy boundary and architecture scope

Extend the safe platform trait with a GAT `type Frame<'a>: OwnedFrame` bounded
by `Self: 'a`, `allocate_frame(&self) -> Result<Self::Frame<'_>, FrameError>`,
and descriptive pool statistics. ARM implements it using its single pool;
the safe kernel can exercise real ownership and data access without unsafe
code or knowledge of raw pointers. x86 returns an explicit Unsupported error
and may name the same concrete Frame type without constructing a pool. Do not
report x86 physical allocation as passing; only shared selection logic is
tested on both architectures until PVH discovery is implemented.

## Verification required

- Safe planner fixtures on both architectures: clipping to arena, complete-page
  containment, empty RAM, disjoint/overlapping reservations, one-byte exclusions,
  adjacent boundaries, reservations outside RAM, kernel/DTB exclusion, malformed
  ranges and checked arithmetic/geometry limits. Enumerate candidate pages and
  compare eligibility with an independently expressed interval predicate.
- ARM actual-memory tests: allocate distinct pages, mutate one while retaining
  another, verify isolation and initial zeros; drop and reuse, verify zeros;
  exhaust the exact eligible count without duplicates or excluded addresses;
  repeated exhausted calls do not change statistics. Keep test stack bounded.
  Do not stack-allocate an array of 4096 frame objects. A core-only exhaustion
  check may claim indices through the allocator's same private claim primitive,
  record only those indices in a 512-byte ledger, then verify the public frame
  allocation path returns Exhausted. Cleanup returns only ledger-owned indices;
  it never clears preexisting allocations or leaked bits. This test claims no
  initialized byte access for its internal indices. Actual byte/zeroing paths
  remain covered by the separate owned-frame tests above.
- Exercise safe forgetting: leak a frame, then exhaust all remaining pages and
  prove the forgotten address is not reissued. Record intentional lost capacity;
  monitor tests must not assume a fresh pool or require ever-growing leaks on
  repeated `test` commands. Prefer a one-time boot leak test plus stable runtime
  exhaustion/reuse checks, or explicitly track the one deliberate leak.
- Compiler probes: frames/pools cannot clone, frames cannot outlive the pool,
  byte borrows prevent premature drop/reuse, no public constructor/fields mint
  ownership, and frames cannot cross Send/Sync boundaries. Positive clients must
  compile so unrelated failures do not count as boundary enforcement.
- Preserve the existing protection, parser, capability and monitor tests on
  both targets; exercise ARM 64 MiB and 128 MiB profiles. Record raw logs,
  source/ELF identities and any failures. Independently review every raw
  zeroing/reference operation and the one-shot construction path.

No performance advantage, physical-hardware support, heap, preemption or SMP
claim follows from this first allocator. Planning and ownership tests establish
separate properties: neither alone proves the trusted boot-memory contract.
