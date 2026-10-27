# Task budget reproducibility investigation

Status: read-only investigation. No timing policy, expected result, core source,
build or QEMU configuration change is approved by this review.

The preserved ordinary 128 MiB copy_inputs failure is genuine Budget, with the
reviewed candidate binary. Its third SPAWN turn used 2,450,180 counter ticks
(39.203 ms) with 2,274,684 ticks (36.395 ms) remaining. Earlier SPAWN turns used
24.858 and 26.200 ms. The passing 64 MiB run used 19.980, 19.382 and 18.907 ms
for the corresponding three SPAWNs. The failed turn exceeds the remaining total
by 2.808 ms; this is cumulative charged cost, not evidence of a particular host
pause. See [review 0030](0030-task-injection.md) for exact counters and cleanup.

The interval includes actual syscall admission, typed descriptor/input copies,
frame allocation and zeroing, private table construction/audit, startup
preparation, Process boxing and commit. Trace capture copies are charged;
diagnostic UART formatting/serialization is deferred until after the final
counter. Reviewed source and assembly show no misplaced diagnostic serialization
or erroneous accounting. Cleanup discards the provisional reply, cancels the
unentered child and publishes the real Budget closure with metadata remaining.
The failing driver does not report final reap/baseline success.

## Concrete cost found in source

Space::audit (user_space.rs) visits all 4,096 identity leaf entries. For each it
scans the 512 page/permission slots to determine whether that physical page is
owned executable memory. Most physical pages have no executable owner, so this
requires approximately two million slot predicates per audit. Factory::load
calls audit after table construction, and install_startup calls it again before
startup allocation/commit: about four million such predicates per SPAWN, in
addition to other work. prepare_instructions performs another audit at the
child's first entry, charged to that child's turn. This explains a concrete
source of legitimate computation and a plausible optimization target; it is not
a measured attribution of all observed milliseconds.

The allocator also searches its eligibility/used bitmap from the beginning for
each frame, zeroes every newly owned page, and zeroes the three-page Process box
before moving the value. Private table construction writes 4,096 identity
leaves. Those are real costs. Skipping zeroing, ownership checks or the audit
would change the safety argument and is not an acceptable timing workaround.

## Next decisions supported by evidence

The fixed three unchanged-binary trials can establish recurrence and variation.
Process CPU versus wall time and recorded host/cgroup observations can constrain
possible explanations, but whole-run metrics cannot attribute a stall to one
charged turn. A passing trial cannot erase the full-suite failure or prove a
host scheduling cause. Keep every outcome and its exact binary/command.

A promising production experiment is a semantics-preserving audit lookup change:
construct a bounded 4,096-bit executable-physical-page bitmap from validated
owned RX frames, then retain the full 4,096-leaf comparison using that bitmap.
This replaces repeated owner searches with one owner pass and one leaf pass,
while retaining all table, permission, bounds and alias checks. It needs a short
written equivalence argument, independent core review, meaningful malformed-table
coverage, and all affected emitted stack checks. A 512-byte local bitmap is not
free: some existing call chains have only 160 bytes of remaining stack margin;
actual placement/call-chain measurement must precede any guest. A scalar
alternative can validate every owned RX alias separately and search ownership
only for identity leaves changed from their boot value, but its equivalence is
more subtle and must not accept an omitted RO alias restriction.

If operation-level attribution is necessary first, propose a separately reviewed
bounded timing diagnostic around existing load/audit/prepare/box boundaries,
with copied scalar timestamps and deferred emission. Its overhead and changed
binary must be disclosed; it is not interchangeable with the unchanged trials.
Do not preemptively increase budgets, accept Budget for copy_inputs, exclude
allocation from charge, remove checks, or switch to another emulated time model
merely to obtain a pass. Native ARM/KVM measurements remain a separate future
question; current evidence concerns pinned QEMU TCG only.

## Windowed-mask proposal review

The revised proposal in design 0032 avoids a 512-byte bitmap: build one u64
mask for each of 64 windows, scanning the 512 owners once per window. This
preserves the exact predicate for all 4,096 leaves, including duplicate owner
addresses, with 32,768 owner visits instead of approximately two million.
Approve this implementation scope after the proposed exact safe helper signature
is recorded: executable_mask(window_base: u64, physical: impl Iterator<Item=u64>)
-> u64. Checked subtraction yields a bit only for a relative page-aligned delta
below 64*4096. Relative alignment preserves exact equality even for arbitrary
bases; actual core bases are page aligned. Reference tests should use checked
addition at u64 limits. The bounded-work claim applies to the actual finite core
iterator, not arbitrary user-provided iterators. All prior audits and each actual
leaf comparison remain; no cached authority, allocation or page mutation.

Read the three unchanged-trial records: one failure and two passes. Recorded
cgroup throttling counters remain zero; the failed run consumed about 3.544 CPU
seconds in 3.574 wall seconds, but whole-process accounting cannot exclude or
locate individual scheduling delays. The results establish reproducibility and
variation, not a confirmed host-stall explanation. They do not replace the
preserved failed full suite. Actual timing improvement remains unmeasured until
the reviewed algorithm is implemented, compiled and compared under the declared
unchanged workload/budget protocol.

The exact helper signature/relative-alignment contract is now written; design
implementation approval is unconditional within that narrow scope. Clarification
from actual trial-1 serial: its failure is allocation_rollback, group 22 turn 7,
not copy_inputs. All three unchanged trials completed copy_inputs. Thus these
trials reproduce budget fragility elsewhere in the matrix, not recurrence of the
same case; preserve that distinction in subsequent comparisons.

## Source review

Approved mapping_audit.rs and the narrow final-loop replacement in user_space.rs
for compiled evaluation. checked_sub rejects preceding addresses; offset below
262144 and divisible by 4096 bounds the shift to 0..63. This equals the independent
checked_add/contains reference even near u64::MAX and for unaligned bases.
The RX filter preserves the original Some(ReadExecute)+Some(Frame) predicate;
window*64+bit visits each original index 0..4095 exactly once with its original
expected descriptor. Earlier validation is unchanged. No allocation, unsafe
addition or persistent state was introduced.

The three host tests cover all singleton bits/full windows, duplicate/empty and
unaligned inputs, overflow boundaries, and 2048 deterministic generated windows
against a separately expressed reference. They establish lookup equivalence,
not hardware performance. Their placement as unit tests expands the existing
exact native library-test inventory; require explicit named inventory integration
rather than accepting arbitrary extra tests. Source acceptance does not infer
actual scalar stack placement or timing improvement; the 34-profile emitted gate
and preserved-trial comparison remain mandatory.

Reviewed the strict host gate update: its exact sorted inventory is the original
10 fully qualified native tests plus the three named mapping_audit tests. Summary
counts, zero ignored/filtered/measured/failed checks and unexpected-output rejection
remain strict. Mutation coverage removes/duplicates every named record, including
all three new cases. The pinned native-host-first actual run passes all 13 tests;
its captured source hashes still match current files. No host pass is presented
as a kernel timing or hardware result.

## Compiled optimization gate

Verified all 34 actual ELF and archived assembly hashes in optimized-candidates-
first. Eighteen profiles, including both x86 kernels, are byte-identical to the
reviewed pre-optimization candidates. All affected chains fit the existing stack
with the unchanged 640-byte helper allowance: monitor 64,432 bytes, ordinary
runtime 65,376, task matrix 61,552, negative profiles 61,136. Space/process/input
profiles remain bounded by their unchanged 62,480-byte baseline-test chain.
No allowance or existing infeasible-path justification was changed.

Space::audit's frame grows from 48 to 176 bytes; the generated mask helper is a
zero-stack leaf. The additional frame does not alter the worst chains. Inspected
actual generated control flow: 64 outer windows, one finite RX-owner traversal
per window, then all 64 leaf comparisons. The helper's subtract/underflow branch
and offset bitmask reject preceding, unaligned and out-of-window candidates;
only the scalar bitwise result persists. No large bitmap is materialized.

Normalized emitted instruction comparisons for execute_task_turn, dispatch_task
and run_turn retain the previously reviewed counter ordering in the monitor,
ordinary runtime, task and negative kernels. Changed audit computation remains
inside its actual caller interval; no diagnostic or allocation work was moved
across the counter boundary. Approved these exact compiled candidates for the
predeclared fixed-three measurements once the refreshed matching-library layout
and artifact binding gates pass. This is not a timing improvement or full-suite
acceptance claim.

## Fixed optimized trials

Independently replayed all three optimized-first raw serial and complete exception/
GIC traces through the strict ordinary validator: all 50 cases and the 41-check
baseline pass in each run. Verified the actual optimized task ELF hash
cd99abbf969017315e91e9f8b157e456c175fcb8b826ea7abd44a66e99051bdb and the
fresh matching-library layout's current source hashes and exact 88-byte witness;
all owner sizes and metadata charges remain unchanged.

Recomputed the comparison directly from raw CLOCK/CALL/CHARGE records, selecting
the three first-pass copy_inputs root SPAWN turns in each of the three trials
(nine matched observations per binary). Original median is 21.771984 ms, range
19.561488–28.191344; optimized median is 1.883696 ms, range 1.785776–2.354304.
The derived comparison is accurate. These are complete charged SPAWN-turn
intervals, not isolated audit CPU timings. The data support a substantial
reduction for this fixed TCG workload with unchanged budget and expected results;
they do not establish universal kernel speed, native ARM performance or immunity
to scheduling-induced budget exhaustion. Original failures remain preserved.
Full ARM/x86 regression acceptance is still pending.

The first optimized full suite stopped at the strict process artifact gate before
any process-profile guest: the rebuilt kernel's __bss_start moved to 0x40394000,
while the canonical fixtures retained the previous binding. The preserved result
is exit 2 with unchanged captured inputs and the explicit different-kernel error.
Regenerate the affected canonical fixture/address binding and archive, then
review actual byte changes, rebuilt fixed point and stack/order before process
guests. Do not weaken the artifact comparison. Earlier fixed optimized task
measurements remain evidence for their exact binary, not full-suite completion.

## Rebound process artifacts

Independently compared all 33 regenerated process ELFs with committed originals:
file sizes are unchanged, and every byte difference is exactly explained by
replacing the little-endian kernel_data value 0x40393000 with 0x40394000 in the
fixture literal and symbol. No other fixture bytes changed. Decoded the updated
36-entry process archive, verified every ELF member against canonical bytes and
zero tail; SHA256 is
cc6f47bf29a53d9df0501f62d9f681d3a687dcd8e8dd54b3b96f786d7bbefdb7.
It occurs exactly once in the rebound kernel.

Verified rebound ELF cf0af5007d873a88d09e833c440fa303ea5876c1188f41093c4441f70f48984b
and assembly 5b3c924ec5b54d1b0f2dceaf0d593d04a3826d72607526c278b2d2a08c7100bb.
Actual __bss_start is stable at 0x40394000. Every emitted function instruction
stream matches the optimized first process candidate; reviewed stack and counter
ordering bounds therefore remain valid. Approved isolated strict process guests
and the subsequent full suite with the exact rebound artifact binding. Preserve
the earlier artifact-gate failure as an actual failed run.

The second optimized full attempt stopped at small parser-archive reproduction:
initramfs.cpio also embeds process/hello.elf. Independently verified its repair:
all four entries, 20,480-byte total size and every header remain identical; only
two bytes in bin/hello change, exactly matching the reviewed regenerated ELF.
Updated archive SHA256 is
8ce5bbb75f17fd5b952ae7bef964f9188dac2b203c54e2055ff10c40f676ab24.
Dependency inspection finds the small archive only in safe host initramfs and
boot_image integration tests, not a kernel raw include. Negative legacy recovery
uses the separate apps/hello.elf. Thus this regeneration adds no kernel stack or
ordering change. Preserve the second failed attempt; strict reproduction and
host composition tests must pass before the next complete suite.

## Final local regression acceptance

Independently audited task-budget-full-third: make test-all exits zero in
218.922 seconds; all 334 captured source/input hashes still match. Rehashed all
34 actual full-suite ELFs against optimized candidates with only the reviewed
rebound process override; every identity matches. Both raw evidence archive
SHA256 values match archives.json. Their 113 ARM and 17 x86 result records have
no explicit failed gate. The recorded Python batches total 244 tests per target;
the strict actual native host inventory contains all 13 named tests.

Replayed complete archived serial and exception/GIC traces for both ordinary
50-case task matrices and all ten negative profiles. Both matrices pass with
all 41 baseline checks: 64 MiB has 2,027 slices/1,056 SVCs/967 IRQs/4 faults;
128 MiB has 2,009/1,056/949/4. All ten strict negative runs pass (65 slices in
aggregate), with their required individual baselines. Fixture/layout reproduction
and process artifact checks now pass; the first two failed full attempts remain
preserved rather than reclassified.

Accepted locally: the bounded identity-audit optimization preserves checked
mapping semantics, fits the audited stacks, reduces the matched measured charged
SPAWN intervals, and passes the complete configured ARM/x86 regression suite.
This closes the pending local regression acceptance for the ten task injections
as well. It does not establish remote CI for this source revision, general
performance superiority, native ARM/KVM behavior or elimination of timing-budget
failures under arbitrary host scheduling. Source-to-commit identity proof, when
recorded, is separate from this actual execution verdict.
