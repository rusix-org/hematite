# Bounded identity audit and task-budget regression

The optimized kernel passes three predeclared 128 MiB QEMU task-matrix runs,
50 cases and 41 bootstrap checks each. Independent raw-trace review accepts
all three and the [matched timing comparison](task-budget/comparison.json).
First-pass copied-input SPAWN turns (nine samples per binary) have median
21.771984 ms before and 1.883696 ms after. This is charged-turn behavior for
this TCG workload, not isolated audit CPU time or real-ARM performance.

The original [full-suite failure](task-negative-full-first/result.json) is
preserved. Three [unchanged-kernel trials](task-budget/unchanged-first/completed.json)
then produced one Budget failure in allocation_rollback and two complete passes;
the original copy_inputs case completed in all three. No cgroup throttling was
recorded in those trials. That does not identify the cost of host scheduling in
individual guest intervals. Neither the original failure nor the diagnostic
failure is reclassified as a pass.

[Design 0032](../design/0032-bounded-identity-audit.md) and
[review 0032](../reviews/0032-task-budget-reproducibility.md) identify the repeated
4096-by512 executable-owner scan. The revised pure helper builds one scalar
64-bit mask for each 64-page window, reducing owner visits to 64*512 while
retaining all 4096 leaf comparisons and every earlier validation. No buffer,
allocation, mutable cache or unsafe operation is added. Task budgets, scripts,
QEMU clock model and failure expectations are unchanged.

The [exact 13-test host gate](task-budget/native-host-first/result.json) includes
the original ten native-policy cases and three named mask-equivalence cases.
Both-architecture [static checks](task-budget/static-first/result.json) pass.
All [34 candidates](task-budget/optimized-candidates-first/inputs.json) compile
and pass independent emitted stack/order review. Audit itself grows from 48 to
176 stack bytes; the helper uses no stack. Full reviewed worst-case chains stay
within their existing bounds, including the runtime's 65,376/65,536-byte bound
and unchanged allowance. Actual layout/source witness and embedded archives
are bound before the [optimized measurements](task-budget/optimized-first/completed.json).

Commands, source hashes, QEMU/serial logs, CPU usage and cgroup snapshots are
retained. The source analysis is a concrete explanation for repeated avoidable
work; the measured comparison supports improvement for these tested turns.
It does not prove that no possible host delay can exhaust a time budget.

The [first optimized full run](task-budget-full-first/result.json) stops at the
strict process-fixture binding check: code growth moves actual BSS from
0x40393000 to 0x40394000. Regenerating the 33 ELFs changes only their address
literal and symbol bytes, with sizes unchanged. The 36-entry archive is rebuilt;
independent review verifies exact member bytes, stable rebuilt BSS and unchanged
kernel instruction streams/stack bounds. All
[three isolated process profiles](task-budget/process-rebound-guests/result.json)
then pass. No fixture-address check was weakened.

The [second run](task-budget-full-second/result.json) stops during artifact
reproduction: the small parser archive also embeds that hello ELF and needed
regeneration. Independent review verifies only two payload bytes change, with
all headers/other entries identical. Both archives and all process fixtures then
reproduce; the Rust archive-admission tests pass. This host-only archive repair
does not change kernel instructions or stack bounds. Both failed attempts are
preserved; neither is reported as passing.

The [third full ARM/x86 run](task-budget-full-third/result.json) passes in
218.9 seconds with every captured input unchanged, including the ordinary matrix
and all ten strict failure profiles. All
[34 executed kernel images](task-budget-full-third/kernel-comparison.json)
match their independently reviewed candidates exactly. Raw archives retain
113 ARM and 17 x86 result records with no failed verdicts. Earlier full failures
remain preserved. Matching remote CI for this optimization remains pending.
