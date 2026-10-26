# First ARM userspace review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit. This reviewer performs no builds or guest execution.

## Scope and staged gate

Review is split between the safe ELF admission stage and the protected runtime.
Only Stage A is approved below. Later runtime stages remain proposed and
unapproved. Parser acceptance
alone cannot establish memory isolation, protected entry or Linux compatibility.

## Existing runtime constraints

Inspection at parent `ac10326` confirms that the kernel currently uses SP_EL0
as its emergency stack and makes all lower-EL vectors fatal. A future EL0 entry
must install dedicated trusted exception stacks/vectors; it cannot route a
user-controlled stack through the current EL1 interrupt-entry assumptions.

The shared kernel identity tables remain immutable under the current Pool
construction proof. Executable user pages need an explicit policy for their
kernel identity aliases while the user root is active. Consuming a Rust owner
alone does not remove a hardware writable alias. Root selection, instruction
cache maintenance, translation invalidation and CPU1 quiescence therefore need
separate concrete review before runtime implementation. Restoring the boot root
and reclaiming pages must follow proven cessation of all EL0 access.

Safe user-memory operations must validate the full requested range and retain
backing ownership. Neither a raw address nor an ELF segment plan is authority
to access memory. Native startup/syscall fields need explicit wire widths and
versioning; the compiler's Rust layout/ABI cannot become the userspace contract.

## Stage A design verdict

Reviewed the complete Stage A contract in
[design 0021](../design/0021-arm-first-userspace.md) before parser implementation.
**Approved for the safe ELF admission implementation only.** This approval
does not authorize the proposed address-space, entry, copy or teardown runtime.

The profile bounds both file size and header/segment counts, requires checked
byte decoding, and returns no partial plan. Image/Segment fields stay private
so later consumers cannot receive a safely forged validation result. The
immutable input borrow remains live while file ranges are exposed. Conversion
from file-width integers to slice indices must also be checked in implementation.

LOAD admission independently checks file containment, memory extent, page
congruence, declared power-of-two alignment and rounded page containment.
Page-level overlap rejection prevents apparently disjoint byte ranges from
merging incompatible permissions. The three exact permission alternatives
exclude writable executable segments and unknown flags. The entry check covers
a complete aligned instruction inside file-backed executable bytes, excluding
zero-fill tails and page padding. Pure BSS is allowed for nonempty segments but
cannot supply the entry point. Empty LOADs are explicitly rejected.

The limited program-header policy is explicit: no interpreter, dynamic section,
TLS or PT_PHDR loading is silently approximated. PT_NULL is ignored without
using its undefined payload; the sole optional GNU_STACK requests no executable
stack or layout. Section-table fields are ignored rather than advertised as
validated. The separately linked fixture must match this declared profile;
rejecting other otherwise-valid ELF files is an admitted limitation.

Required tests combine independently recorded linker output with adversarial
metadata and deterministic truncation/mutation. Compiler tests must target the
actual public types and borrow signatures. Successful parsing must be reported
separately from loading or EL0 execution. No claim about the later physical
permission, cache, trap or reclamation mechanisms follows from this verdict.

Source review and execution evidence remain pending.

## Stage A source review

Reviewed `hm-kernel/src/elf.rs` against the admitted profile. **No blocking
parser-source finding.** The initial input-size/header checks bound fixed
identification reads. Checked table-size/end containment plus the count cap
bound each later header-index addition and fixed-width field read. File offsets
convert to usize explicitly; file, memory and rounded-page ends use checked
arithmetic. The positive memory-size requirement ensures page subtraction is
ordered. Alignment modulus never divides by zero.

Every used descriptor matches one of the three exact permissions. Increasing
virtual order and pairwise rounded-page nonoverlap are checked before insertion.
The private count is capped before indexing its fixed array, and no partial
Image escapes an error. Entry validation covers its full four-byte interval
inside an RX file slice, not merely its allocated page. Public getters lend
immutable input data or copy already validated metadata; none exposes mutable
plan fields or raw memory authority.

Actual-type clients in `tools/check_elf_types.py` import the implementation
directly and use strict JSON diagnostic matching. Positive clients establish
normal use, copied Segment lifetime and data borrows that may outlive Image
but retain the original input lifetime. Negative clients cover field forging,
private-field mutation, input/data lifetime escape, mutation during a live
borrow and mutable access through returned immutable bytes. Compiler execution
remains the parent's gate.

Review found one invalid negative test before guest execution: changing the
first LOAD alignment to 128 KiB still leaves its 4 GiB-based VA and file offset
congruent. It must use a genuinely mismatched power-of-two alignment (8 GiB)
instead of requiring the parser to reject a valid input. The final fixture uses 8 GiB and also adds a file-end overflow with both
file/memory sizes large enough to reach the arithmetic check, plus an entry
whose final instruction would cross the file-backed end. These corrections
were reviewed before the final execution gate.

The independently built fixture uses three explicit RX/R/RW LOADs and retained
BSS, with core-owned entry assembly calling separately compiled Rust code. It
is inspected, not executed. The host reads actual program headers through
readelf and compares segment addresses, sizes, flags, rounded ranges and byte
fingerprints against the parser's report. Exact output inventory and nonzero
subprocess exits/timeouts cannot become successful results. The actual library
is also exercised against capacity boundaries, targeted header mutations and
deterministic raw/mutated inputs; accepted plans undergo independent invariant
checks. Host construction of zero-filled simulated segment buffers is only a
plan-content check and supplies no evidence of a kernel loader.

Review strengthened the every-prefix check: rejection is permitted only before
the end of all required program headers/LOAD bytes. Every later prefix must
parse, because ignored trailing section data cannot affect this profile's load
decision. The original host branch only prohibited rejecting the full file and
could miss erroneous rejection of valid shorter inputs. The corrected branch
was inspected before the final full-suite rerun.

**Final Stage A source verdict: no blocking finding remains.** Builds, guest
cases, compiler clients and host execution remain independently reported by the
parent. Stages B–D remain unapproved and unimplemented by this review.
