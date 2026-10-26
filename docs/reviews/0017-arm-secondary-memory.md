# ARM secondary memory review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness certification. Reviewer performs no builds or guest execution.

## Design verdict

Reviewed [design 0017](../design/0017-arm-secondary-memory.md) before runtime
edits. **Ready for the scoped inactive-secondary memory prerequisite.**

Six contiguous guarded reservations have explicit usable sizes, alignment and
containment before `__kernel_end`. Existing frame-pool exclusion therefore
covers their complete storage. A separate page-aligned root shares the exact
immutable device/identity subtrees while leaving every other root entry zero,
including the mutable arena slot. Guard invalidation and root construction
occur before the existing DSB and cache/MMU enable; no live translation update
or remote shootdown is introduced.

Diagnostics must inspect actual leaf/root contents and the live pool's immutable
eligibility Plan, not just linker addresses. The proposed exclusion query
rejects invalid or out-of-window ranges before walking bounded page indices,
preventing a vacuous success from `eligible()` rejecting an invalid index.
Full-tree boot checks must include the new guards before allocator creation.

This excludes the mutable arena from the secondary translation tree; it does
not isolate privileged CPUs from shared identity RAM or CPU0 stacks. Pool and
Mapper authority must remain unavailable to CPU1. No second CPU, secondary
translation root, stack switch or fault test is executed by this increment.

## Source verdict

Reviewed the frozen implementation against parent `75d0a6a`: linker layout,
coldboot assembly, full-tree and secondary diagnostics, read-only Pool helpers,
and platform/test inventory integration. **No blocking source finding remains.**

- The linker reserves six page-aligned guard/stack ranges immediately after
  CPU0 worker3. Assertions fix every size and adjacency, with the final top at
  `__kernel_end` below the identity-map limit. The existing BSS clear includes
  these bytes before guards become invalid; existing allocator construction
  excludes the entire image through that same end marker.
- A distinct page-aligned root is allocated between primary root and device
  tables. Boot writes only its device/RAM entries; BSS initialization leaves
  its arena slot and other entries zero. The six guard writes index the known
  RAM leaf allocation using linker-bounded addresses. All occur before the
  original DSB and cache/MMU enable. No secondary TTBR installation, live table
  mutation or CPU startup was added.
- Boot validation now includes exact secondary range sizes/adjacency, invalid
  guards, every usable page's full RW/NX identity descriptor, secondary-root
  placement and all 512 root entries. The existing complete subtree checks
  still establish shared table structure and W^X permissions. These checks
  execute before Pool and Mapper construction, and recur in the test suite.
- `excludes_region` validates ordered nonempty aligned bounds, checked Plan
  window arithmetic and full containment before converting to page indices.
  Its immutable Plan query cannot confuse currently allocated pages with
  permanently excluded pages. The diagnostic rejects malformed/outside queries
  and requires a real eligible page to return false. It checks every full
  secondary reservation and the root page against the live pool. No allocation,
  ledger mutation or new address-bearing capability is introduced.
- The new baseline case follows actual CPU0 mapping hardware tests, inspecting
  secondary root exclusion after arena activity. Table reads use bounded raw
  volatile scalar accesses, form no references to hardware storage, and are
  limited to tables that remain immutable after coldboot.

Build, strict host/guest suites, compiler probes and negative images remain the
integrating agent's execution gate; this reviewer ran none. These checks prove
constructed table contents and reserved storage only. CPU1 has not installed
the root, used these stacks or exercised their guard faults, and the shared
identity map still exposes privileged kernel RAM. No SMP execution or Phase 1
completion claim follows from this change.
