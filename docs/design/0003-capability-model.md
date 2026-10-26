# Capability and resource-lifetime experiment

Date: 2026-10-25. Scope: the safe-Rust model proposed in
[ADR 0005](../adr/0005-native-first-design.md), not real memory/device ownership.
Independent design review: separate AI agent; not human approval or a soundness
certification. No `hm-core` or hardware changes are part of this experiment.

## Boundary and representation

One model owns 64 resource slots, 128 grant slots and 64 operation slots.
Each live resource records a byte charge; this is accounting metadata, not an
allocated buffer. Configured byte and in-flight budgets are bounded using
checked arithmetic. A small configurable generation limit permits retirement
tests without billions of operations. Zero generation limits and in-flight
limits above 64 are invalid; a zero in-flight limit is permitted.

Handles and operation tokens contain private slot/generation identity and an
invariant lifetime brand. A higher-ranked scoped constructor creates exactly
one model for that brand; no public constructor or cloning operation may create
a second instance with the same brand. Tokens cannot be mixed across nested
model scopes or escape their owning scope. Within a model, slot generations
invalidate stale tokens after release/completion; a generation is never wrapped.
Retire a slot when its generation limit is exhausted.

Owner labels select model principals. The kernel-side caller supplies the
actor label, and methods verify it against the grant or operation owner.
These labels are **not authenticated userspace identities**. A future syscall
adapter must derive actor identity from trusted execution context rather than
accepting a caller-selected label. Likewise `complete` represents a trusted
backend acknowledging completion/quiescence, not a user authorization to free
hardware-visible memory.

Rights are explicit sets (`NONE`, `READ`, `WRITE`, `GRANT`, `ALL`). Delegation
requires `GRANT` and can only select a subset of a valid parent grant's rights. A delegated grant never gains the original root grant's
resource-wide revocation authority, even if it receives every ordinary right.
Releasing a root grant does not implicitly destroy its delegated grants.

## Invariants

1. Every live grant and operation refers to one live resource. Liveness is
   recomputed from bounded tables rather than maintained in separate refcounts;
   no stale internal reference can survive resource reuse.
2. Charged bytes equal the checked sum of live resource charges, independent
   of how many grants refer to each resource. The sum never exceeds its budget.
3. Live in-flight operations never exceed either configured or physical table
   capacity. Grant/resource table exhaustion is explicit, never a panic.
4. A resource is reclaimable exactly when it has neither grants nor operations.
   Closing/revoking the last grant while operations remain retains its byte
   charge and slot. Completion releases the operation exactly once.
5. A revoked resource has no valid grants and cannot admit new operations.
   Existing operation identity/owner remains valid until completion.
6. Every operation failing validation or admission leaves the entire model
   unchanged, including generation counters and retirement state. Validate all
   capacities, permissions and checked arithmetic before committing mutations.
7. Cancellation is a request. It cannot alone reduce resource charges or
   in-flight counts. It may be idempotent while the operation is live;
   cancellation after completion is stale, not a second completion.

The audit operation must independently scan cross-references, generations and
byte totals from the tables, not just compare the stored byte counter to its limit.
It is a diagnostic invariant checker, not evidence about unmodeled concurrency.

## Transition and failure matrix

| Operation | Required preconditions | Successful transition | Failure behavior |
| --- | --- | --- | --- |
| Create | Valid config, free resource and root-grant slots, checked byte charge within budget | Allocate resource and root grant atomically; charge bytes once | Exhaustion/overflow leave state unchanged |
| Grant | Live actor-owned parent grant with GRANT; requested rights subset; available grant slot | Add a recipient-owned non-root grant; bytes unchanged | Wrong owner, stale token, amplification or full table changes nothing |
| Begin | Live actor-owned grant with requested operation right; in-flight capacity available | Allocate operation token; retain resource independently of grant | No token/refcount/charge mutation on denial |
| Release | Live actor-owned grant | Invalidate that grant; reclaim resource only if no references remain | Stale/wrong-owner release cannot decrement counters |
| Revoke | Live actor-owned original root grant | Mark resource revoked and invalidate all its grants; retain operations and byte charge | Delegated/stale/wrong-owner token changes nothing |
| Cancel | Live actor-owned operation | Record cancellation request without releasing any charge | Repeated live cancellation is idempotent; stale/wrong-owner calls change nothing |
| Complete | Live operation token, supplied by trusted model backend | Return defined completed/cancelled outcome; remove operation once; reclaim if last reference | Duplicate/stale completion cannot free again |

Revocation does not request cancellation. A retained operation completes as
`Completed` unless explicitly cancelled; cancellation changes its eventual
outcome to `Cancelled`. Losing a grant never proves that an operation has
quiesced. No model method may equate a cancellation request with actual
physical DMA completion.

## Required experiment evidence

Exercise all stated admission limits and stale generations, wrong-owner access,
rights attenuation/amplification, root-only revocation, last-grant release with
outstanding work, duplicate completion and reuse, and generation retirement.
Enumerate meaningful revoke/cancel/complete orderings and run the invariant
audit after every step; preserve the exact failing ordering. Require final
byte, resource, grant and operation counters to return to baseline after
cleanup, excluding intentionally retired slot availability.

Tests must also establish that cross-model tokens do not type-check. A runtime
suite alone cannot demonstrate lifetime-brand separation. Measure the model's
size and bound test stack use against the 64 KiB bootstrap stack.

No allocator, userspace ABI, SMP ordering, DMA isolation, cryptographic handle
secrecy or performance advantage is established by these tests. Those require
separate designs and evidence when the model is integrated with real resources.

## Independent implementation review

A separate AI agent reviewed `capability.rs` and its initial guest tests on
2026-10-25. The source matches the transition contract: one invariant brand
per scoped constructor, no model cloning, checks before mutation, bounded table
scans, independent root authority, retained operations after revocation and
nonwrapping retirement. No implementation defect was identified in that review.
Review requested tests for same-slot operation reuse, root release with a live
delegate, and create failure when the grant table is full. Counter snapshots
plus `audit` test observable accounting; they do not prove byte-for-byte state
identity on failure. Execution evidence is recorded separately by integration.
