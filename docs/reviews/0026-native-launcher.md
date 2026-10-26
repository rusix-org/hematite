# Ordinary native launcher review

2026-10-26. Independent AI review, not human review or a complete soundness
audit. Review of [design0026](../design/0026-native-launcher.md).

**Design approved for its specified implementation.** The final revision fixes
exact command grammar, sealed constructor names, validated high32 generation
diagnostics, LF-to-CRLF terminal transport with original-byte accounting, and
the affected-profile stack audit. Source and compiled-stack approval remain
separate gates before guests. The
ordinary `apps`/`run` surface is a useful, constrained step beyond a test-only
kernel: hello, startup-information utility and explicit fault/deadline demos
exercise actual native applications without implying a shell or useful service.
The information utility must read and format admitted startup fields rather
than print a kernel-generated fixed answer. Dedicated native inputs must not
carry the process-test fixtures' absolute kernel symbols.

The permanent Executor design preserves the single issuer namespace. A valid
selection precedes timer preflight and allocation; missing/type/ELF rejection
cannot consume a grant. Later prepare failure can consume a generation, so the
status diagnostic correctly means last successfully prepared generation, not
last issued generation. Failure must retain the same Executor; neither test
commands nor borrowing workarounds may reconstruct it or reset the latch.
Only the successful fixed interactive sequence requires contiguous generations.

Warm-up bootstrap tests intentionally retain three diagnostic pages once per
boot: forgotten Frame, KernelBox and MappedPage. The baseline therefore belongs
after warm-up; later interleaved tests must retain no further pages. This is not
a relaxation of exact per-launch reclamation. A fresh boot's first launch also
needs independent source/initialization scrutiny because warm-up is a harness
choice, not a user prerequisite.

The forgotten mapping does not conflict with the new process root. The leaked
MappedPage retains its allocator claim and cannot be recycled into an ELF page.
`Space::build_tables` installs root entries0,1,4 only, omitting the owned arena
at root entry2. Boot identity tables remain unchanged; the leaked data's identity
mapping is still privileged data. During the alternate-root interval only the
reviewed process assembly runs, and boot root/TLB restoration precedes any Rust
or mapping operation. No callback, peer CPU or DMA can access the omitted arena
in that interval. The stale Mapper constructor comment mentioning only0022
probes must explicitly include the reviewed0023 process interval.

Terminal output must remain a sealed internal enum choice, not a caller-supplied
Host/callback. The existing trace constructor and exact HMPROC record behavior
remain compatibility requirements for the process verifier. All CALL/RESULT and
OUTPUT sites need consistent mode selection. The final design freezes newline
transport, command grammar and constructor/generation naming; implementation
review must verify those details.

BootPlatform grows in every ARM feature combination, so stack review must cover
all affected profiles, including process/space/preemption/SMP tests as well as
ordinary ARM and x86 monitor `test`. The previous baseline consumes61,712 of
65,536 bytes; an added retained monitor/Platform frame can invalidate it. Keep
archive/image/owner temporaries in separate helpers and inspect actual emitted
call chains before any guest. Normal application embedding needs its own
artifact proof; changed process-test geometry still requires its fixed-point
regeneration and full66-run regression.

The proposed interactive trace gate is appropriately stronger than prompts:
five launches, six SVCs, one RX-write fault and one kernel-deadline IRQ; exact
generation/free-count/status/prompt observations; bounded warm-up16, preflight8
and later test16 EL1 IRQ groups; no extra process slices. Preserve one-CPU
64/128MiB scope and explicit x86 unsupported behavior. No implementation,
compiler or runtime evidence was available at the design-review stage; later
verdicts follow below.

## Initial implementation review

Reviewed `launcher.rs`, the sealed output-mode changes in `process.rs`,
BootPlatform integration, ARM module configuration, Mapper contract amendment,
safe `native_launch.rs`/monitor dispatch, and dedicated `apps.rs`/`apps.S`.
The persistent state borrows Pool only for each call and stores no Factory or
self-reference. Fully checked image selection precedes first preflight and
allocation. Separate load and prepare/run helpers release owners before frame
counts are compared. The issuer remains resident across recoverable resource
failures and terminal user outcomes. No caller-selected output callback or
mutable mapping reference crosses into execution.

One review finding was corrected: exhausted issuer/platform/invariant failures
reported as Unavailable previously left the UI authority Ready. After exact
cleanup accounting, Unavailable results now permanently retire authority;
Busy/Resources retain it. Completed-count overflow also retires before entry.
The status literal is frozen as `Last prepared generation:` to match its actual
meaning. Trace formatting remains guarded as a whole; terminal output converts
LF to CRLF while Session accounting continues to count original user bytes.

The applications validate startup before reading fixed fields, and info
formats those observed values into a bounded256-byte buffer. Dedicated AAPCS64
assembly shims preserve the two-word return convention; RX fault addresses its
own entry through ADR, and the loop has a visible self-branch symbol. They carry
no kernel absolute target dependency. Untrusted user demo faults are contained
by the existing process mechanism, not privileged Rust fault recovery.

This source candidate is approved after the retirement fix. The first compiled
normal candidate exceeded64KiB and was rejected before execution. The separated
admission-helper refactor then passed the independent
[21-profile stack review](../evidence/launcher-stack-review.md), with a
conservative normal-launch bound63,584/65,536. The original failure is retained.

## Verification tools and actual execution

Source-approved the dedicated app builder, exact interactive transcript and
trace harness, old-monitor expectations, and two additional actual compiler
clients for private terminal construction/output selection. Added review-requested
binary fixup-table mutations cover invalid bounds/permissions, wrong ESR/FAR,
duplicate PCs/resumes and nonexecutable returns. The six allowed baseline
fixups are decoded from read-only file-backed kernel bytes and checked against
fixed fault classes, not accepted as arbitrary guest-supplied exceptions.
Final app evidence checks ELF symbols/opcodes, PC-relative own-entry fault,
absence of kernel absolute symbols, canonical archive bytes, GNU extraction and
unique read-only embedding without individual ELF fallback.

**The ordinary launcher runtime gate passes for the three recorded profiles.**
Independently inspected [first QEMU evidence](../evidence/launcher-first-qemu/result.json),
raw transcripts and full traces, and reran the reviewed strict validators over
those preserved bytes without starting a guest. All recorded serial/trace/stderr
hashes and harness source hashes match. Main64/128MiB profiles each execute27
commands with five completed launches and generations1..5. After the deliberate
warm-up retention, every run and the interleaved41-case bootstrap test retain
the same3129 free frames. Invalid paths, directory selection, malformed command
syntax and overflow recovery leave execution uninitialized before first launch.
No HMPROC diagnostics appear in normal program output.

Actual info output reports ABI1, granted console,256-byte writes,4096-byte budget
and64-call limit; hello prints its native payload. Fault and loop report their
distinct outcomes, restore the prompt and permit later execution. Each main
trace has exactly eight EL0 slices: six SVCs, one own-RX write abort with
ESR0x9200004f/FAR0x100000000, and one lower-EL timer IRQ at the named self-branch.
The40 EL1 timer IRQs and48 registered synchronous fault/return transactions
match the two bootstrap tests and one preflight, with exact GIC ordering and
no arbitrary extra transitions. An IRQ's printed ESR may retain an earlier
synchronous syndrome; the IRQ classification/vector/ELR/acknowledgment supply
its evidence, not that stale ESR.

Fresh128MiB evidence additionally launches hello without a preceding test:
generation1,3132 free frames before/after, two EL0 SVCs and only the eight
preflight EL1 IRQs. Warm-up is therefore not an operator prerequisite. All
monitors remain alive after the final prompt until deliberate host termination;
none relies on timeout or a killed guest to claim completion.

Independently recomputed normal archive bytes and checked embedding against
ELF`0ad998d20161cf406323da919304579ed2340f7fc10c5ebcb14895c8f43a82b3`.
The729,088-byte archive at file offset371336 has SHA-256
`2dfbad7d66809562d6a163c297d78e54147260709198cae5cf3f5dddba7f17f6`;
all four preserved GNU extracted payloads equal their canonical ELFs. Binary
fixup decoding independently matches the hardware trace metadata. Ordinary
`tools/run.sh` and the shared test command now explicitly disable EL0
semihosting (`userspace=off`), matching the launcher harness containment policy.

This establishes usable ordinary one-CPU ARM native launch, including a real
startup-information utility and recoverable demos. Full regression/CI publication
is tracked separately; no Linux compatibility, general init service, concurrent
userspace or useful network/storage workload is claimed. Existing pinned-QEMU
architectural qualifications remain applicable.

## Reproducibility follow-up

The subsequent full regression run stopped at normal-app reproduction:
`info.elf` differed after the application source was formatted between the
initial fixture build and final-source verification. The failure is preserved;
the successful first runtime evidence above remains valid only for its exact
old canonical ELF/archive/kernel hashes. It is not proof that those inputs
reproduce from the final source. Regenerated artifacts and rebuilt kernels need
fresh byte/stack binding before another guest and final evidence acceptance.

That byte/stack check now passes: independently found exactly the one-byte
read-only panic-line metadata change81→108 in info.elf; executable/RW LOADs are
identical. All21 third-candidate emitted function bodies equal the reviewed
second candidates, and20 kernel ELFs are unchanged. The corrected normal ARM
kernel is`4b8d02898264846846e19772bd0ec7a3159e468d5913f31e448822f3b99df04c`.
The [stack review](../evidence/launcher-stack-review.md) binds the final inventory.
The preserved failed suite is`launcher-first-full-suite.log` with its raw evidence
directory; final full-suite/guest outcomes still require their own exact hashes.

## Final-source runtime acceptance

Independently inspected the final ARM full-suite launcher outputs under
`target/evidence/20261026T195828-447588634-aarch64/launcher`, to be preserved at
[launcher-final/aarch64/launcher](../evidence/launcher-final/aarch64/launcher/result.json).
All three input records bind the corrected normal ELF`4b8d0289…`; all raw
serial/exception/stderr hashes and harness source hashes match. Fresh strict
validation of all three preserved transcripts/traces passes without relaxed
expectations. The main profiles again have27 commands, five launches,
generations1..5 and3129 free frames; the fresh profile has generation1 and3132
free frames. Main6-SVC/one-fault/one-deadline and40-EL1-IRQ trace inventories,
and fresh2-SVC/eight-preflight-IRQ inventory, remain exact.

Independently recomputed the corrected archive and verified canonical bytes,
all four GNU-extracted payloads and the unique read-only embedding without raw
fallback. The final archive SHA-256 is
`bd37dbaa482b0d20bb70f3f38970cf9d96c1701eb5aa3a9295ef9e6e1ffed31c`;
length729,088 and file offset371336 are unchanged. **Final-source ordinary
launcher runtime acceptance passes for these exact artifacts.** This supersedes
the old-hash runtime acceptance for release/reproduction purposes without
discarding its historical evidence. Complete cross-architecture suite status
and remote-CI phase closure remain separate from this launcher-specific review.
