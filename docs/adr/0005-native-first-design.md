# ADR 0005: native-first Rust OS research

Status: direction accepted from Aaron on 2026-10-25; experiments below are
research directions, not proven improvements. The first bounded model is
implemented; see its [design](../design/0003-capability-model.md) and
[verification record](../evidence/capability-model.md).

## Decision

Aaron changed the objective to a Rust OS that explores better designs than
Linux, without requiring its legacy interfaces, while pursuing compatibility
where useful. This supersedes strict Linux ABI parity as the organizing goal
in earlier records. The from-scratch choice remains. The existing bootstrap
is a starting point, not evidence that these designs outperform Linux.

Design native interfaces around explicit ownership, authority and resource
lifetimes. Keep compatibility policy outside the small trusted core. Preserve
the safety boundary and verification discipline in [AGENTS.md](../../AGENTS.md).
No license selection or public-name clearance is implied by this decision.

## Research hypotheses

| Proposal | Hypothesis to test | Cost or unresolved issue |
| --- | --- | --- |
| Scoped capability handles and explicit resource budgets | Rights checks and accountable lifetimes can make authority and exhaustion easier to reason about | Handle transfer, revocation, sharing and budget ownership need precise rules |
| Typed, fault-aware user-memory operations | Checked operations can isolate unsafe address handling while representing partial copies honestly | Pinning, permission changes and faults still require reviewed synchronization |
| Explicit spawn and service communication | Declared startup resources and structured messages may simplify process lifecycle | Existing software may require `fork`, signals or shared-memory behavior; supply adapters only where justified |
| Cancellation-aware asynchronous I/O | Requests with explicit completion and ownership states may make cancellation predictable | Revocation cannot free a buffer while a device or operation still owns access; soundness and quiescence need independent review |
| Observable, testable invariants | Resource counters, lifecycle events and bounded failure reports can make faults reproducible | Instrumentation has overhead and must not leak cross-process information |

None is a demonstrated performance or security advantage. Use failures and
measurements to revise or reject a hypothesis rather than protecting the design
from unfavorable results. A Rust implementation alone is not the comparison.

## First experiment: capability and resource lifetime model

Before adding a broad syscall surface, implement a small safe-Rust model with
64 resource slots, an explicit byte budget, owner-scoped handles and a bounded
in-flight operation count. Start without user pointers, physical DMA or a
scheduler; model completion/cancellation transitions directly so tests can
control their order. Integration with real memory and devices is a later,
independently reviewed step.

Proposed operations are create, grant attenuated rights, begin operation,
revoke, cancel, complete and release. A handle identifies a table slot and a
generation; identity alone grants no authority across owner scopes. Reuse
must invalidate stale handles. Retire a slot rather than wrapping its
generation. Checked arithmetic rejects budget overflow before state changes.
Revocation denies new operations; existing operations retain their storage
and budget charge until completion or confirmed quiescence. Cancellation is a
request with a defined completion outcome, not permission to reclaim early.

The experiment passes only if executable tests show:

- No successful stale, forged-owner or insufficient-rights operation, including
  after slot reuse and a generation-limit case.
- Slot, byte and in-flight limits cannot be exceeded; exhaustion and arithmetic
  overflow return explicit errors with unchanged state.
- Rights can be attenuated but never amplified by transfer or duplication.
- Revoking/cancelling an in-flight operation never frees its storage early;
  completion releases each charge exactly once, including repeated requests.
- Enumerated revoke/cancel/complete orderings preserve these invariants; after
  all resources are released and completions accounted for, counters return
  to baseline. Log the exact ordering for every failed case.

Record capacity, tested transitions, resource counters and failures. These
results establish properties of the model only; they do not establish SMP,
DMA, adversarial isolation or performance. Define workload and measurement
budgets before comparing implementations.

## Native and compatibility boundaries

Native APIs may diverge intentionally. A Linux ABI shim translates declared
surfaces and errors into native operations; it brings marshaling, semantic
impedance, emulation state and maintenance costs. Some Linux behavior may not
fit cheaply or safely. Document unsupported behavior and adapter limits rather
than forcing native invariants to match every historical contract.

For each compatibility surface actually promised, run the same small observable
behavior test against a pinned Linux baseline and the adapter. Linux is an
oracle for that promise, not for native capability/cancellation semantics.
Neither a native test pass nor an adapter subset implies broad Linux parity.

Any future stable native wire ABI must be language-neutral and explicitly
versioned: defined integer widths, encoding, layouts, lengths, errors and
feature negotiation. Do not expose Rust references, enum layout, trait objects,
allocator ownership or the unstable Rust calling ABI across that boundary.
Safe Rust types can wrap the wire format internally. Publishing a stable ABI
is a separate commitment after the experiment informs its design.
