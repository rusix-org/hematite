# Review: fixed idle artifact and host gates

Status: **Source accepted; subsequent raw/full-suite acceptance is recorded in
[the transaction review](0031-idle-transaction.md). No VM/build/test was run by
this source reviewer.** Scope: `build_idle.py`, `test_idle.py`,
`test_test_idle.py`, compared with approved0031 profile and actual `idle.rs` /
`idle_checks.rs`. This document records source review. The review flag remained
false during initial diagnostics and was enabled only after independent review
of all nine actual cases; earlier findings below retain their original scope.

## Findings corrected and independently re-read

1. `trace_events` clears an unfinished disassembly block when an execution
   Trace line arrives. This can bypass the required exact single-WFI versus
   three-instruction-resume TB geometry check, which currently runs only at
   the blank block terminator. Reject a Trace within an unfinished block, or
   validate/finalize the full exact block before accepting it. Add mutations
   for missing terminators and merged/partial translated blocks.
2. Artifact source binding captures idle files and selected core sources but
   omits main.rs, Cargo.toml and actual launcher/Space/CPU/BUSY/native-policy
   dependencies. Bind the complete workspace source/build-input closure and
   actual imported tool closure, excluding output directories. Kernel hashes
   alone do not satisfy the requested source-reproduction evidence.
3. The runner checks live stderr size only at loop entry. A final write and
   process exit can break the loop without another size check; stderr is then
   hashed without a final size gate and does not participate in validation.
   Recheck all log sizes after wait and before acceptance; preserve failure
   metadata. Add a focused final-size policy mutation/check.
4. Coordinated entry/restored snapshot mutations can describe impossible
   admission (TCR/MAIR/SCTLR zero, nonzero TTBR1, invalid PMR/BPR high bits) and
   pass. Bind the actual fixed context constants, linked VBAR/SP0 and admitted
   PMR/BPR widths, while leaving genuinely variable preserved registers variable.
   Existing synthetic entries should model that actual admitted context.

All four corrections are now on disk and were independently re-read. The
Trace branch requires a completed translation block; source_paths includes
workspace Rust/assembly/linker/Cargo inputs and root lock/config/dev wrapper
plus imported tools; post-exit log_sizes marks any oversized log failed before
validation; exact admitted CPU constants, linked VBAR/SP0 and PMR/BPR widths
are checked. Corresponding mutation cases cover malformed block termination,
late stderr size, source closure and coherent impossible snapshots. No remaining
source blocker was identified. This reviewer did not modify the tools.

## Confirmed design alignment

The parser consumes ordered case records and typed scalar fields with u64/range
bounds, a constant session wall origin and wall_end=start+frequency*300. The
production case requires limit=wall_end, wake>=limit and a real enabled timer
cause. Short cases distinguish pending/no-WFI and actual WFI UART/timer outcomes;
no shell/READ or halted-residency claim is made. Copy-only snapshots, bounded
poll counts and actual DR error-free bytes match the core emitter. Two real
preflights, subsequent canonical hello's two SVCs and the complete existing
baseline/privileged exit are consumed with the strict shared trace reader.

The host sends P only at pending_byte READY and U only after uart_wait READY
plus a matching WFI execution-log attempt. Earlier cases have no WFI, so the
first eligible attempt is not confused with an earlier completed case. Input
writes/timestamps are preserved; absence, duplication or a READY-based U trigger
fails the declared input gate. A logged attempt alone is not called retirement:
entry/resume ordering, ELF opcodes, post counter/cause and no alternate exception
path jointly establish the intended control-flow inference. Stopped attempts
are rejected rather than silently omitted. Actual pinned raw formatting still
needs independent review before enabling the grammar flag.

The direct-branch artifact scan rejects branches into the continuation and
requires a caller to the WFI entry. It is not a proof against arbitrary indirect
control flow; source and linked disassembly review of the sealed leaf/call site
remain necessary. Likewise GIC CPU writes/ACKs independently check only exposed
trace events; Distributor/system-register snapshots still rely on the audited
core implementation and compiler ordering. No fictitious hardware trace source
is introduced.

Nine synthetic test methods exercise positive short/wall transcripts, required
records, clock/cause/restoration mutations, stopped/wrong execution traces,
execution/input failure gates and leaf-byte/direct-control-flow mutations.
These are host-validator checks, never evidence of a real300-second wait or
hardware wake. Their execution and all actual guests remain parent-serialized.


Approval does not authorize enabling TRACE_GRAMMAR_REVIEWED from synthetic
results. Actual linked stack/ordering, direct call-site inspection, fresh
artifact inputs and first complete raw QEMU logs remain separate gates. Preserve
all rejected first-run evidence and fix demonstrated parser/source mismatches
explicitly, rather than accepting extra event classes or optional preflights.

## Final small source and integration update

Independently re-read the shared live/final `log_sizes` helper: it rejects both
individual logs over their limits and the combined trace.log+stderr.log total
over16MiB. A combined-only overflow mutation covers the distinction. Frequency
admission now matches the actual core range1000..1,000,000,000, with valid and
invalid endpoint mutations. The synthetic inventory is now ten methods. Parent
reports host-second passed all ten; this reviewer did not execute them.

Makefile integration is consistent: test entry clears HM_TASK_NEGATIVE and
HM_IDLE_PROFILE; only the two ARM idle cargo invocations receive command-local
short/wall values and distinct target directories. Synthetic idle checks run
in the ordinary fail-fast host sequence. The ARM-only full test path invokes
test_idle with both built kernels and the canonical apps manifest/archive;
the runner requires all three profiles. No optional skipped idle gate or
unbounded alternate command was introduced. Source approval remains in force;
compiled review and actual guest/trace approval are still pending.

## Approved one-CPU target-register amendment

Independently reviewed the host delta against the approved first-diagnostic
amendment in the transaction review and source addendum. MASKS now excludes
only target8; entry requires its entire word zero, setup retains that exact
zero, and restored equality still applies. The expected changed bitmap excludes
only index32. All eight remaining configuration fields retain their exact mask,
requested value and readback requirements. The eleventh synthetic method rejects
coordinated target0x100/0x1 substitutions, reintroduced dirty bit32, and separate
mutations of every other programmed field. No zero-or-0x100 fallback was added.

The artifact builder still hashes the real revised idle source and checks the
same immutable WFI leaf/callers. Makefile profile isolation and full three-run
gate remain unchanged from the prior integration review. Source approval stands
for fresh host checks and diagnostics after renewed compiled/core review. The
failed first guest remains failure evidence; this amendment supplies no WFI,
UART-wake or restoration execution claim. No tests, builds or VMs were run by
this reviewer, and the raw grammar flag remains a separate acceptance gate.
