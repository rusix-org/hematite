# ARM secondary memory reservations

Status: independently reviewed for implementation, 2026-10-26; see
[review](../reviews/0017-arm-secondary-memory.md). This is a prerequisite
from design0014; CPU1 remains off and its translation root is never installed.

Append a secondary stack block immediately after CPU0 worker3, before
__bss_end/__kernel_end. In order reserve coordinator (4KiB guard+64KiB usable),
emergency (4KiB+16KiB), workers0..3 (each4KiB+64KiB). Every guard/top is 4KiB
aligned; every SP top is therefore 16-byte aligned. Each range is half-open
[guard,top); usable bytes start at guard+4096. Linker assertions fix sizes,
adjacency, separation from CPU0 worker3, and final containment below0x41000000.
The existing BSS clear covers all reserved bytes before translation is enabled.
No secondary stack is accessed or lent to callbacks in this increment.

Reserve a distinct 4KiB arm_secondary_root in .bss.tables. CPU0 initializes
entries0 and1 to the same device/RAM L2 descriptors used by arm_root, leaving
all other entries zero. In particular entry2 at virtual0x80000000 excludes the
mutable CPU0 owned-mapping arena. This does not isolate privileged CPUs from
shared identity RAM or CPU0 stacks; the secondary capability model must still
exclude Pool/Mapper authority, and a later secondary-entry design must install
this root. The shared identity/device subtrees remain immutable after bootstrap;
only CPU0's separately reachable owned arena may later change.

After constructing the existing identity leaves, invalidate exactly the six
secondary guard leaves before the existing DSB SY and MMU/cache enable. All
usable stack pages retain exact identity EL1 RW, AF, inner-shareable normal-WB,
PXN/UXN descriptors. There is no live table edit, TLBI, cache publication or
CPU1 startup added here: these are CPU0 coldboot writes with caches off. Existing
MMU sequencing publishes both roots and all shared subtrees before Rust begins.

Extend boot full-tree validation to compare every secondary-root entry and all
secondary guard leaves. Add read-only secondary_memory_check diagnostics:
verify ordered nonoverlapping linker ranges and exact sizes/alignments; verify
every guard invalid and every usable page exact RW/NX identity; compare all512
secondary-root entries against exact shared device/RAM descriptors and zero
elsewhere; verify root alignment/disjointness/containment. The inherited full
boot mapping checks still validate shared subtrees and CPU0's own root.

Add Pool::excludes_region(start,end) as a narrow crate-private read-only query
of the actual immutable allocation Plan. Reject empty, unaligned, overflowing
or out-of-window ranges; inspect every covered page through Plan::eligible.
The diagnostic verifies all six complete stack reservations (including guards)
and the secondary-root page are ineligible in the live pool. The existing
[0x40200000,__kernel_end) exclusion remains the allocator mechanism; the query
observes it without allocating, modifying the bitmap or exposing addresses.
A read-only helper diagnostic rejects empty, unaligned, outside-window and
overflow-shaped queries, and finds an actually eligible page that must not be
reported excluded. Missing eligible pages fail the test rather than skipping it.

Run the new diagnostic after existing mapping hardware tests so real CPU0 arena
activity precedes the exclusion check, then retain all normal/negative ARM tests
and x86 regression. This establishes reserved storage and constructed page-table
contents only; it is not CPU1 execution, translation-fault, stack-switch or SMP
concurrency evidence. Sources: existing reviewed designs0003/0005/0008/0014 and
their ARM translation architecture specifications; no Linux source.
