# Physical frame ownership review

Date: 2026-10-25. Independent AI review; not human approval or a soundness
certification. [Design 0005](../design/0005-frame-allocation.md) reviewed before
implementation; no design blocker identified. Core/planner source reviewed;
no remaining implementation defect identified in the reviewed increment.
No builds run by reviewer.

The design provides a coherent initial ownership boundary: bounded safe
selection is metadata only, the core makes a one-shot physical claim, and
non-cloneable frame owners control access and reclamation. Complete-page RAM
containment and any-byte exclusion overlap avoid lossy address rounding.
The requested fixed-profile clarification excludes initrd, firmware-runtime
buffers, DMA and other aliases; the current DTB parser alone does not establish
those exclusions. The boot call must perform current mapping/protection checks
before its unsafe construction call, rather than depending on later tests.

## Review constraints

The core must establish physical authority from the fixed boot profile and
actual trusted discovery. A function accepting arbitrary metadata must not
be a safe constructor for dereferenceable RAM, even when crate-private. An
unsafe constructor needs explicit mapped-memory, exclusion, uniqueness and
concurrency preconditions, discharged at the fixed ARM boot call site.

A one-shot claim must remain claimed after pool destruction, forgotten frames
or initialization failure after the claim. No safe reset, clone or replacement
pool may reissue that memory. Live frames borrow the pool, preventing its
movement or destruction while any frame or frame-derived byte borrow remains
usable. Forgotten frames may leak; they must never become free implicitly.

The bitmap must reserve each page before raw initialization and publication of
its owner. Byte references must borrow the owner, not merely its pool, so
release cannot coexist with live byte access. Dropping a frame releases only
its private recorded slot. No safe operation may free by a caller-supplied
address or slot, alter reservations, or map an arbitrary address.

The single-CPU, interrupt-masked boot profile is the current concurrency
boundary. `Cell` prevents shared pool access across threads through `!Sync`;
borrowed frames consequently cannot move across threads. A movable unborrowed
pool is not itself unsound, provided bitmap ownership and the permanent claim
move with its logical ownership. SMP, DMA and user aliases require separate
designs before they may observe frames exposed as Rust references.

## References consulted

- [Rust `mem::forget`](https://doc.rust-lang.org/core/mem/fn.forget.html):
  unsafe implementations must tolerate omitted destructors.
- [Rust `Cell`](https://doc.rust-lang.org/core/cell/struct.Cell.html):
  interior mutation and `!Sync` boundary.
- [Rust mutable raw slices](https://doc.rust-lang.org/core/slice/fn.from_raw_parts_mut.html):
  initialized storage, single-allocation extent, exclusive access and bounded
  reference lifetime requirements.

## Planner source review

Read `frame_plan.rs` and its initial guest tests. No defect identified. Input
capacity/range/overlap validation precedes fixed bitmap writes; checked page
arithmetic and full-page containment avoid wraparound and partial-page claims.
Eligibility queries outside the bounded arena return false. The oracle uses
clipped interval lengths, separately from the implementation's containment
and overlap predicates. The planner exposes descriptive metadata only.
Compilation and execution are the parent integrator's responsibility.

## Core and boot integration review

Read `frames.rs`, `frame_api.rs`, the platform GAT interface, ARM construction
and x86 Unsupported behavior. The ARM call checks current mappings before
building its actual boot plan. It covers the DTB/kernel range, excludes the
whole prefix and linker extent, and adds every parsed reservation before
unsafe construction. Metadata never supplies an independent dereference API.

Review requested that release-by-index be unsafe with an explicit exclusive
slot/no-surviving-borrow precondition. That change is present; Frame drop and
the private ledger each justify their calls. Allocation claims before raw
zeroing, returns a private non-cloneable owner, and ties all byte references
to that owner borrow. The bitmap and pool cannot be cloned or reset through
safe APIs. The diagnostic cleans only its own newly claimed ledger bits and
retains a deliberately forgotten allocation across repeated tests.

Constructor safety wording must distinguish the first successful latch claim
from a repeated call: physical/exclusive preconditions apply to an invocation
that successfully claims the previously-unclaimed arena. Once claimed, the
constructor rejects before using the new plan. This permits the safe fixed
boot initializer's repeated-call rejection while older frames remain live;
it never permits another successful owner of those pages.

No build or guest execution was performed by this reviewer. Compiler boundary
probes, real frame byte access/reuse, exhaustion, forgotten-owner accounting,
repeat monitor tests and both architecture suites remain integration evidence.

## Final integration source review

Reviewed the added singleton diagnostic: an existing pool establishes that the
permanent latch is already claimed, and its attempted replacement plan has no
eligible pages. The call requires `AlreadyInitialized`; no raw memory operation
occurs in this diagnostic. The constructor now documents this rejection-path
contract explicitly.

The safe frame contract retains a live patterned frame while exhaustion runs,
then verifies its bytes survive ledger cleanup. It checks that repeating the
diagnostic loses no additional capacity, and ARM cannot pass by reporting an
Unsupported allocator. x86 explicitly tests Unsupported behavior instead of
claiming physical allocation support.

The compiler harness builds the actual `frames.rs` in a wrapper library and
checks ten external safe clients. Its positive client prevents unrelated
toolchain failures from satisfying rejection cases; negative clients require
the expected diagnostic codes, and missing/failed/timed-out clients fail the
overall result. The wrapper changes visibility for external-client testing,
not the frame implementation. No new source defect was identified. Parent
execution evidence is recorded separately; this reviewer ran no builds.
