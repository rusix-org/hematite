# Bounded executable-alias lookup

Status: implemented and locally verified; see
[review 0032](../reviews/0032-task-budget-reproducibility.md) and
[measurement/full-suite evidence](../evidence/task-budget.md).

The three fixed unchanged-binary trials produce one Budget failure in
allocation_rollback and two complete passes. The original copy_inputs case
completes in all three. They do not resolve the preserved full-suite
failure or attribute any interval to host scheduling. Review0032 identifies a
concrete repeated cost: Space::audit scans512 owners for each of4096 identity
leaves, twice during SPAWN and again at first execution. Every check is required;
its lookup algorithm need not perform roughly2.1million owner predicates.

## Proposed equivalent lookup

Keep every existing context, frame-count, table, user-leaf, ownership, permission
and executable-entry check. Replace only the final4096-by512 RX membership loop.
Process the4096 identity leaves in64 windows of64 pages. For each window, build
one u64 executable mask by scanning all512 retained owners/permissions. Only
ReadExecute owners contribute. Then validate that window's64 actual table words
against RO for set bits and the exact boot_ram value otherwise.

Factor the pure mask operation into a small forbid-unsafe hm-kernel helper taking
an iterator of physical addresses and a window base. It allocates nothing and
returns one u64. Exact signature:
`pub fn executable_mask(window_base: u64, physical: impl Iterator<Item = u64>) -> u64`.
Alignment is relative to window_base, preserving exact address equality even
for arbitrary helper inputs; the real core bases are page aligned. Iterator
behavior is caller-defined; bounded work applies to the actual core iterator.
Use checked subtraction, reject non-page-aligned offsets, require
offset<64*4096 before shifting; arbitrary input must not panic or wrap. Core
passes only addresses of retained RX Frames through an immutable iterator.
No address list,512-byte bitmap, persistent cache, permission mutation or new
userspace/kernel authority is introduced. The window base is the same fixed
RAM range already checked; no change to mapping geometry or public wire layout.

Equivalence: bit j is set exactly when an RX-owned physical address equals
window_base+j*4096. This is the original existential predicate for that leaf.
OR handles duplicates identically. Every original leaf is inspected exactly once
with the same expected descriptor. Earlier audit checks continue rejecting
invalid frame addresses and inconsistent owner/permission entries. Worst-case
owner iterations fall from4096*512 to64*512, with4096 descriptor comparisons
unchanged. All state is local and immutable except the scalar mask.

## Verification

Before core edits, independently approve this equivalence and API/stack scope.
Host-compile the actual pure helper and compare it against the original
per-leaf existential predicate over empty/full/singleton and sparse sets,
window boundaries, duplicates, unaligned/out-of-window addresses, zero and u64
limits, and deterministic generated cases. Do not substitute the reference
algorithm in production or declare the guest fixed from host tests.

Run both-architecture static gates, refresh actual layout/source witnesses,
compile all34 existing profiles, and independently recheck every affected stack
and final-counter ordering. The160-byte runtime margin makes source-only size
reasoning insufficient. No new guest before compiled acceptance.

Then repeat the predeclared three same-workload trials using the optimized
binary, retaining all original failures. Compare actual charged copy_inputs
SPAWN/entry intervals and original budgets/counters, not merely aggregate pass.
Run full ARM/x86 regressions including the ten strict injections. Keep100ms
budget, failure semantics, task limits and expected scripts unchanged. This
optimization does not establish real-ARM performance or eliminate all possible
host-scheduling timing failures; publish the actual measurement scope.
