# Ordinary native task creation

Scope: a fixed-catalog ARM supervisor starts and manages native children. This
is not arbitrary executable launch, a shell, Linux compatibility, or acceptance
of the detailed negative-authority matrix.

[Review 0029](../reviews/0029-native-task-creation.md) records source, compiled
ordering, layout, stack and independent raw-trace review. All 23 affected kernel
profiles compile. Use the [second inventory](task-creation/task-second-candidates/inputs.json)
for 22 profiles and the [third inventory](task-creation/task-third-candidates/inputs.json)
for runtime-tests. The latter extracts test-driver polling into a noinline helper;
its conservative stack bound is 65,376 of 65,536 bytes. The unchanged helper
allowance remains included.

The [expanded compiler witness](task-creation/task-expanded-layout/result.json)
measures Process 9,832 bytes (three pages), Image 472 bytes, Authority 928 bytes,
and ordinary Group 1,960 bytes. Catalog images and authority consume three
metadata pages. The witness records an exactly reversible module-copy
transformation and original/generated hashes; it is not a runtime allocation
proof. Allocation recovery is separately observed in the guest profiles.

[Ordinary first](task-creation/ordinary-first/result.json) preserves three passing
diagnostic profiles with the aggregate trace-review gate deliberately disabled.
After independent raw inspection, the [strict rerun](task-creation/ordinary-second/result.json)
passes at 64 MiB, 128 MiB and fresh 128 MiB. Each profile launches the supervisor
twice: nine parent SVCs and six finite-worker SVCs per run, a genuinely parked
WAIT, timer-preempted loop cancellation, and no loop entry after cancellation.
Completion/generation counts advance 3, 6, then 7 after a legacy hello launch.
Free frames return to the same baseline and the monitor remains responsive.
Missing-path and malformed-command checks preserve launch counts.

The [normal archive](task-normal-archive/result.json) has seven programs,
including bin/supervisor and bin/worker. It is rebuilt from pinned Rust inputs
and independently inspected using GNU ELF/archive tools. Existing demo/loop
remains the normal catalog's looping child.

Preserved failures include the initial layout dependency-search error and six
initial Clippy findings. The corrected [static gate](task-creation/static-third/result.json)
passes on both architectures. The earlier runtime stack candidate was rejected
before any guest execution. No stack limit or allowance was relaxed.

The [first full regression run](task-full-first/result.json) stopped before guest
execution at the process-forgery compiler probe: its stale Session field type
produced E0308 instead of the required privacy error. Supplying Some(session)
restores the intended test without relaxing E0451; all 41 probes now pass in
[the isolated rerun](task-creation/process-types-corrected/result.json).
The [second full run](task-full-second/result.json) passed ordinary task guests,
then stopped before the detailed runtime guests because their validator expected
only the old 40-byte witness. The corrected validator now requires all eleven
values, exact bytes/hashes, task-page arithmetic and task source identities.
Its mutation checks and [both runtime profiles](task-creation/runtime-corrected/result.json)
pass against the unchanged reviewed kernel.

The [complete third ARM/x86 run](task-full-third/result.json) passes in 189.8
seconds. All 264 captured source inputs match implementation commit
`c59c6e07`; see the [committed-blob verification](task-full-third/verified-commit.json). Raw
archives for all three full attempts are retained alongside their command/input
records; no failure was overwritten.
The subsequent [ordinary authority/fault/rollback matrix](task-matrix.md) now
passes its strict guests after independent trace review and its full ARM/x86
regression. Ten dedicated failure-injection profiles remain outstanding.
