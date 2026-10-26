# Archive execution composition review

2026-10-26. Independent AI review, not human review or a complete soundness
audit. This review covers [design 0025](../design/0025-archive-execution.md).

The proposed composition preserves existing authority boundaries: complete
archive admission precedes exact regular-file selection and ELF validation;
only a borrowed Image escapes. The path lifetime is independent of the payload
borrow. Factory load copies admitted bytes into owned frames before execution.
No archive metadata grants execution authority, and no new unsafe mechanism or
Executor change is needed. Calling the helper for each run avoids retaining the
large archive plan across execution, but actual compiled stack inspection is
still required.

The malformed-suffix test is essential: a valid executable prefix must not
escape whole-archive rejection, including when the requested name is missing.
Guest malformed input may be small because precedence, not ELF execution, is
being tested there; host tests must use a genuinely valid early ELF. The other
three guest error cases use the actual embedded archive. Require all four
admission resource counts to equal the existing run baseline, not just each
other, so a leak between checks and the first run cannot disappear in reporting.

The complete 36-entry archive contains all 33 existing native fixtures. The
full 66-run matrix, prepare failures, three VM profiles, strict hardware trace
correlation and baseline tests remain mandatory. No individual ELF fallback or
hello-only substitution is acceptable. Source review must establish that every
load and preparation case uses the embedded archive and the checked helper.

Artifact verification must establish one complete archive occurrence across
all file-backed load contents, wholly contained in one readable, nonwritable
PT_LOAD. Bounds must be checked before slicing; partial overlap, a writable
match, duplicate embedding or a standalone ELF outside the archive span fails.
GNU listing and byte-exact stdout extraction independently connect the archive
payloads to canonical fixture inputs. Presence alone does not establish use;
source review supplies that distinct check.

The bounded four-iteration regeneration procedure correctly treats kernel BSS
addresses, standalone fixture bytes and both archives as one fixed-point
dependency. Each failed candidate must remain evidence. A stable result still
requires non-updating reproduction, fresh stack analysis and artifact checks
before QEMU. Previous ELF-only execution evidence is insufficient.

**Design approved for the specified implementation.** Both requested
clarifications are now explicit: resource counts are tied to the run baseline,
and the archive span and uniqueness rules cover all file-backed load contents.
Source review now approves `boot_image.rs`, its eight integration tests,
`process_inputs.rs`, the archive changes in `process_checks.rs`, and the
builder's `--process` mode. All regular execution and preparation paths use
the same archive helper; no individual ELF fallback remains in that input
module. The helper contains no allocation or unsafe operation. Tests cover
all errors, exact lookup, independent path lifetime, and corrupt suffix
precedence with a real early ELF. Independently decoded the shared 126-byte
malformed fixture: a valid ordinary `first` entry with one-byte payload and
zero padding precedes the six-byte incomplete suffix `broken`.

The updated host tools are source-approved, including strict admission counts
tied to the run baseline and cross-LOAD duplicate/split/raw-fallback rejection
tests. The eight-case host inventory and six actual-library composition clients
preserve failure-sensitive exit, deadline, diagnostics and source evidence.
The [stable compiled stack review](../evidence/archive-process-stack-review.md)
passes for its exact ELF/assembly hashes: new admission frames do not overlap
execution frames. The independent complete host artifact gate remains required
before guests.
This milestone does not change default monitor
startup, add an init service or establish Linux compatibility.

## Independent execution verdict

The [first archive-backed QEMU evidence](../evidence/archive-process-first-qemu/result.json)
passes independent inspection for64MiB/one CPU,128MiB/one CPU and128MiB/two CPUs.
Revalidated the preserved raw serial and full exception traces with the reviewed
strict parser; all three pass, and raw artifact hashes match their recorded
values. No new guest was run by this reviewer. Each profile has exactly66 runs
and66 passes:34 exits,26 contained faults,4 call-limit stops and2 kernel-deadline
stops. Preflight has8 timer IRQs; the process matrix has2 timer IRQs and the
post-process baseline has16 IRQs. Two-CPU tracing has exactly132 expected PSCI
transactions with CPU1 remaining OFF; one-CPU traces have none. The existing
full syscall/exception/register correlation and41 baseline checks pass.

The five new records occur in the required position and order. Archive length
is1,326,592 with36 entries/34 files. All four admission errors preserve the2984
free-frame baseline, and every subsequent run returns to that same baseline.
PREPARE failure/drop checks also pass. The lower baseline compared with the
earlier ELF-only kernel follows the changed linked reserved geometry; it is
not used to excuse any within-run resource loss.

Independently recomputed canonical archive bytes and checked the complete
embedding/no-fallback rule against stable ELF97426148…: the unique read-only
archive starts at file offset380896 with SHA-256
`f02b5d17cadcc700adcdddd879ce51c4aba68121e198a484018c932f5524c75f`.
All34 preserved GNU stdout extractions equal the canonical payloads, and the
verbose listing matches the exact inventory and metadata. These artifact
checks plus the reviewed call sites establish archive-backed execution, rather
than merely the presence of unused archive bytes in a kernel.

For the declared local CPU0-only native ARM profile, the Phase2 functional
requirements now have evidence: owned address spaces, static ELF admission and
copying, protected EL0 entry/return, typed user copies, checked initramfs input,
versioned startup/console/exit handles, pointer/handle rejection, containment and
exact repeated reclamation. This satisfies the native first-program experiment
specified by0021 within its bounds. It does not establish a published CI exit
gate: the project requires phase gates in CI, and the current full regression
run remains separately tracked by the parent. Linux static musl hello-world is
still unsupported and requires its own adapter/differential evidence. Default
monitor boot still has no init service; concurrent userspace, general process
scheduling and useful service workloads are later contracts. None of these
local results completes the overall OS objective or retroactively publishes
the Phase1 CI gate. Existing pinned-QEMU DAIF/DIT qualifications remain in force.
