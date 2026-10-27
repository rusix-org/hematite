# Review: bounded writes into inactive owned user memory

Status: **Design approved for the two private Space methods only.** No source,
compiled artifact or runtime acceptance is implied. The interactive coordinator,
UART ownership and idle mechanism remain separately gated.

Reviewed [the proposal](../design/0031-owned-rw-copy.md) against the actual
`Space` owners, `slot`, `ram_index`, `word`, descriptor attributes and `context`
in `crates/hm-core/src/user_space.rs`. The specified user RW and boot RW
attributes match the implementation. The root 0 → table 10 → table 11 path is
the actual user mapping path. Checking retained table owners and their boot
identity leaves before reading their bytes avoids relying on an unchecked
user descriptor to obtain a Rust reference.

The whole-range scalar plan is sufficient: a nonempty range of at most 256
bytes spans at most two 4096-byte pages. Checked endpoint arithmetic, bounded
slots, exact retained-owner/L3 equality and immutable boot RW identity checks
precede every store. A bad second page therefore cannot produce a successful
prefix write. `validate_rw` conveys no retained authority; the writer repeats
validation. No allocation, mutable slice escape, executable backing write or
page-table modification is authorized.

The proposal accurately describes `context()`: it checks the bootstrap CPU and
control registers, but does not acquire BUSY or prove another CPU is OFF.
Soundness additionally relies on the existing Factory/Pool bootstrap contract,
non-transferable owners and sealed synchronous execution. Exclusive `&mut Space`
also excludes obtaining a safe input slice aliasing its own backing. A future
coordinator must retain its execution lease and perform its own lifecycle and
capability checks; these methods cannot replace those checks.

Implementation review must verify that all table-owner identity checks actually
occur before `word` reads; that cursor, offset and source-slice arithmetic follows
the checked plan; and that pass two introduces no error return or input-dependent
panic after its first store. Fixed, proven private indexing is acceptable only
with explicit ownership and bounds invariants. No callback or intervening owner
mutation may be introduced between passes.

The proposed genuine ELF/stack cross-page cases, unchanged-prefix rejection
checks and complete readback are required runtime evidence, not yet executed.
Descriptor corruption rejection remains a source-review claim without a
separately approved injection mechanism. A safe test driver, any adapter,
linked stack/order checks and eventual EL0 visibility tests require their own
review before integration. This approval does not authorize a general Process
writer or an arbitrary-address testing hook.

## Dedicated inactive-copy profile approval

Approved the revised test-profile design for implementation. The five LOADs
cover six image pages: RW slot 0, RX slot 1 with entry B+4096, RW slots 2/3,
RO slot 4 and RW slot 6. This supplies genuine RW→RO, RW→RX and RW→unmapped
boundaries without changing the six-image-page/35-total-frame accounting after
real startup installation. Slot 511→512 separately tests the window end.
The earlier proposal conflated that boundary with an in-window missing page;
the revised geometry resolves the finding.

The 12 ordered cases repeated twice, bounded snapshots, validation followed by
fresh writer validation, complete startup checks, protected-page preservation
and repeated AT batches establish a concrete inactive-copy verification target.
The unchanged 41-case bootstrap baseline follows the copy END and reclamation
checks. Both RAM profiles require bounded successful execution and the sole
privileged final exit. Reuse the existing strict baseline trace sequence in
full, including exact fault counts/order, timer events and GICC configuration
and restoration; checking only membership in the fixup table is insufficient.
No EL0 execution, HVC or additional preflight is permitted in this profile.

This remains design approval. The private methods, safe adapter, canonical ELF,
artifact validator and trace parser require source review; linked stack and
ordering evidence precede the first guest. Actual EL0 observation of delivered
bytes remains an interactive-integration obligation. No mutable Process testing
adapter or descriptor-injection authority is approved.

## Private method source review

Reviewed the initial `user_space.rs` implementation of `WritablePlan`,
`writable_part`, `writable_identity`, `validate_rw` and `copy_into_rw`. Approved
for compilation and subsequent artifact review; no guest result is claimed.

The planner checks context, bounded length, checked endpoint and both endpoint
slots before constructing at most two ascending segments. Each segment checks
owner/permission agreement, boot RW identities for all three retained table
owners before table-byte access, the exact two table descriptors and exact
owned L3 RW leaf. A valid non-RW page or absent mapping returns Bounds;
inconsistent owner/permission presence returns Corrupt.

The writer resolves both disjoint owners with `split_at_mut_checked`, both
bounded mutable destination slices and both immutable source portions before
its first copy. Source lengths equal the corresponding planned destination
lengths by construction and the explicit remaining-length check. Therefore
neither `copy_from_slice` can encounter a user-controlled length mismatch,
and no fallible return follows the first store. The single-page branch has the
same explicit length equality before copying. Missing second pages and invalid
second destinations leave the first backing unchanged.

No new unsafe block, allocation, reference escape, raw user-pointer access,
permission mutation or callback was introduced. The private immutable input
borrow cannot safely alias a backing simultaneously borrowed through mutable
Space. Source approval does not replace formatting/static checks, actual linked
stack/order inspection or the pending dedicated runtime tests.

## Initial safe driver and bootstrap source review

Reviewed `rw_copy_checks.rs`, its Cargo feature, ARM wrapper and main dispatch
routing. Approved for formatting/static checks and compilation. The driver
uses genuine loader owners and the real startup installer, checks all initial
fixture bytes, verifies protected pages and the complete startup page after
operations, and checks both methods' exact Bounds results on rejected inputs.
The 257-byte rejection checks its extra destination byte separately; wrapped
addresses are never read as backing addresses. Lexical Result/Space destruction
precedes PASS and exact free-count recovery. No arbitrary memory authority or
new unsafe driver code is introduced.

The wrapper uses the actual BootPlatform Pool/topology and existing private
Factory constructor, requires one CPU, and runs the copy checks before the
unchanged baseline. Combined-feature dispatch precedence and fallback
exclusions preserve existing diagnostic priority. The manual startup header
matches the current Legacy no-console wire layout.

AT source checks cover the expected success/fault partition and successful
physical addresses; the pending host parser must additionally enforce exact
PAR fault/syndrome fields and strict raw trace grammar. Image, owned Space
Result, snapshot buffers and probe results still contribute to full linked
call chains. No compiled stack or runtime acceptance is inferred from this
source approval.

## Fixture and initial host evidence review

Independently decoded the actual 29,672-byte fixture, SHA-256
`3af1b54406bcdd052c5c00669e029ff9877982db7272a05f8f892edfd888f9fa`.
Its five nonoverlapping file-backed LOADs, six deterministic pages and B+4096
entry match the approved geometry and current manifest/source hashes. Builder
verification binds the complete ELF into one readable non-writable kernel LOAD.

The host parser enforces all 24 ordered cases and 48 AT records, exact resource
recovery, stable distinct root/backing addresses, boot FSC 5, guard FSC 7,
permission FSC 15 and rejected stage-2/PTW fault bits. It reuses the complete
strict baseline trace sequence before the sole privileged terminal event.
The actual-module compiler wrapper includes a private positive adapter and
nine clients covering public privacy, owner access, transfer and Pool lifetime.
Synthetic tests exercise complete positive records, removal/duplication of
every copy record, permission/address/accounting mutations, baseline control
flow and exact fixture geometry. These are source observations, not executed
compiler/guest claims.

Two pre-guest host fixes were requested: enforce log size bounds while QEMU is
running rather than only after its 30-second wait, and record hashes of the
parser and reused validators in standalone run metadata. Neither requires a
kernel or fixture behavior change. Final host-source acceptance awaits those
small changes and their verification.

## Compiled gate and host-source acceptance

The runner fixes are present: both files are checked every at most 50 ms against
the size limit while the 30-second deadline remains active, overflow is a
recorded failure, and per-run metadata hashes the actual imported repository
validator modules plus builder dependencies. Approved host source. The recorded
five mutation tests and actual private adapter plus nine compiler clients pass.

Independently verified all 35 actual ELF and emitted assembly hashes in
`docs/evidence/rw-copy/rw-copy-candidates-first/inputs.json`. All 34 previous
profiles are byte-identical to the accepted input-safe candidates, preserving
their existing compiled gates. The new ELF is
`6447c09daf1d348d3058fd9f5cae21e090f81cd03ddfb7966b7bb7d2f69c96ef`;
its emitted assembly hash is
`d503eaf32975e43708cbdb2006ff7e285289bb858d7570481fefdcf66156af3c`.

The new profile's largest direct chain remains the bootstrap baseline:
7,600-byte boot + 35,072 run_tests + 8,320 topology valid + 4,240 extra_valid +
6,464 parse + 128 property + 16 compatible = 61,840 bytes. Including the
unchanged 640-byte helper allowance gives 62,480 within the 65,536-byte stack.
The copy-profile admission chain is 34,848 bytes before that allowance:
boot 7,600, wrapper 16, run 32, checks 192, one 9,568, Factory::load 17,264,
audit 176. Probe/buffer paths do not exceed the baseline chain.

Emitted writer inspection confirms that writable_plan completes before either
memcpy; writable_part validates all table identities before reading table
bytes and exact owner L3 equality before returning. The two-page writer resolves
both owners, both bounds and both source lengths before its first memcpy. From
that call onward it only computes the second already-validated pointer, copies,
and returns success; there is no rejection branch or interrupt-control change.
Its own frame is 128 bytes, plan 176 and part 80. No page-sized copy temporary
or new runtime owner is present.

Approved bounded first diagnostic guests for this exact candidate after normal
kernel/fixture artifact binding. The trace-review flag must remain false until
actual raw evidence has been independently checked. This gate authorizes no
interactive process execution or UART input behavior.

The first diagnostic invocation failed before QEMU: the builder passed a Path
to the shared byte-oriented `embedded_archive` validator. Reviewed and accepted
the correction to `kernel.read_bytes()`, which enables the intended independent
LOAD-span check without weakening it. Regenerating the source-bound manifest
is required; the fixture SHA and linked kernel remain unchanged. Preserve that
attempt as an artifact-binding failure, not an executed guest failure or pass.

## First actual raw trace acceptance

Independently replayed both `docs/evidence/rw-copy/guest-second` profiles against
the actual kernel fixup table and current strict serial/trace validators.
Verified the recorded kernel hash equals the compiled candidate, validator
source hashes are current, and the sole exact fixture is contained in its
read-only kernel LOAD at file offset 354,036. Commands use one CPU, 64/128 MiB,
TCG, no user semihosting and the required exception/GIC tracing.

Each profile returns zero within one second, with no timeout or log overflow:
24 copy cases, 48 AT observations and the complete 41-case baseline. Every copy
case returns from 35 charged frames to the common 3,307-frame free baseline.
Actual boot PAR faults are 0x80b (FSC 5); protected writes 0x81f (FSC 15);
guard accesses 0x80f (FSC 7). Successful translations match the reported owned
physical pages and are unchanged across the copy. Private root 0x40315000
restores to actual boot root 0x40250000. Both raw traces contain exactly the
reviewed 16 baseline timer IRQs and 24 fault/return pairs followed by privileged
exit, with no EL0 event, HVC or unknown exception.

Approved enabling the host trace-review flag for this exact grammar. Existing
first diagnostic results correctly remain aggregate-false because that flag
was deliberately unset; preserve them. A fresh strict invocation and the full
regression suite remain outstanding. These results establish inactive owned
copy and AT permission behavior, not interactive READ delivery or execution.

Replayed the fresh `guest-third-strict` serial and full hardware traces with
the enabled review gate: both profiles pass unchanged 24/48/41 inventories and
baseline-only trace counts. Kernel identity and per-run validator source hashes
match. Reviewed Makefile integration retains the old gates while adding fixture
reproduction, the standalone profile build, five host mutation tests, nine
actual privacy clients/private adapter and both strict RAM profiles. Full-suite
acceptance remains pending its final artifact/source audit.

## Final local regression acceptance

Independently audited `docs/evidence/rw-copy-full-first`: full `make test-all`
returned zero in 364.407 seconds with unchanged inputs. All 351 captured source
hashes match current files, and all 35 actual full-suite kernel hashes match
the reviewed candidates. Both raw archive hashes match their recorded SHA-256;
reading all result records found 121 ARM and 18 x86 records with no false pass
status or reported errors.

Replayed both full-suite RW-copy serial and exception/GIC traces against actual
linked symbols/fixups. Each passes all 24 copy cases, 48 AT observations and
41 bootstrap cases with free baseline 3,307, 16 baseline IRQs, 24 exact fault
pairs, and no EL0/HVC events. Per-run validator sources and kernel identities
match. Canonical fixture reproduction passes with the reviewed 3af1b544… hash;
the actual private adapter and all nine ownership/privacy clients pass.

Accepted for the local inactive owned-RW-copy milestone, preserving the earlier
pre-VM artifact-binding failure and initial diagnostic review-flag failures.
This is not interactive READ delivery, active userspace copy, UART reception or
masked-WFI acceptance. No remote-CI result for the new revision is claimed.
