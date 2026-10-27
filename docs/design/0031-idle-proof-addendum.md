# 0031: timer ordering and translated-block observation

Status: **design-only source investigation; no implementation approval**.
2026-10-26. Supplements [the idle transaction](0031-idle-transaction.md) and
[primary evidence](0031-idle-primary-evidence.md). No code, build, test or VM ran.
Official Arm documents and exact QEMU v7.2.22 files were retrieved read-only;
source was inspected in memory, not copied into an implementation.

## Timer: what the primary sources establish

Arm Generic Timer version 1.4, document 102379_0104_01_en, §§4.1 and 4.5,
pages 10 and 13, establishes ordered counter sampling with ISB, level-sensitive
timer output, and three ways to clear it: mask, disable, or move the comparator
past the current count. Clearing the source must precede GIC deactivation;
otherwise it can signal again. Thus CNTP_CTL=2 is a suitable requested source
state, and expired CVAL need not be rewritten merely to suppress output.
[Official Generic Timer guide](https://documentation-service.arm.com/static/66c4754f32f35b31ceb317ff).

Arm's published AArch64 timer example uses ISB after timer programming,
enabling and disabling. A companion example switches from the secure timer
accessors to CNTP at non-secure EL1. This supports the proposed programming
idiom, but an example is not a normative guarantee of timer-to-GIC signal
propagation latency or proof that one ISB establishes GIC quiescence.
[Arm timer example](https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/retargeting-and-enabling-exceptions-in-an-elf-image),
[non-secure EL1 adaptation](https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/changing-exception-level-and-security-state-with-an-armv8a-fixed-virtual-platform).

Pinned QEMU `gt_cval_write` updates the comparator then calls `gt_recalc_timer`.
`gt_ctl_write` updates writable bits 0..1; ENABLE changes recalculate, while an
IMASK-only change updates the output directly. Recalculation deasserts the
output when disabled, or computes it from comparison and mask when enabled.
These calls occur synchronously in the emulated register-write helper. The
physical timer callback uses the same recalculation path. This establishes the
pinned emulator mechanism, not hardware propagation timing. Its disabled
ISTATUS happens to clear; guest restoration must still compare writable bits
only. [QEMU helper.c, lines 2531–2576 and 2619–2684](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/helper.c).

**Remaining architectural gap:** this investigation did not obtain a primary
normative statement connecting completion of CNTP_CVAL/CTL writes plus ISB to
observability of deassertion at the GIC CPU/distributor interface. Do not replace
that missing statement with the MMIO DSB argument: timer system registers are
not the UART's Device-nGnRnE accesses. Preserve the candidate ISB/readback sequence,
bounded pending/active checks and source-before-EOI order for review, but do not
call the hardware proof closed. In particular, CTL writable-bit readback is not
by itself an observation of the external interrupt wire. Unknown physical-board
latency and errata remain outside the pinned QEMU claim.

## QEMU execution logging: the crucial distinction

In `cpu_tb_exec`, `log_cpu_exec` runs **before** `tcg_qemu_tb_exec` (lines 433–438).
The code later explicitly handles a TB that never started (lines 453–474), resets
its guest PC and can emit `Stopped execution of TB chain before ...`.
Consequently a `Trace` record is a dispatch attempt, not an instruction-retirement
record. The indirect lookup helper also emits an execution record before returning
a TB pointer. `nochain` sets CF_NO_GOTO_TB; it does not set architectural single
step or CF_SINGLE_STEP. [QEMU cpu-exec.c, lines 152–171,278–305,403–412,426–487](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/accel/tcg/cpu-exec.c).

The logger filters on the TB's guest starting PC, not every instruction in its
body. `-dfilter start..end` uses inclusive endpoints; `start+size` ends at
start+size-1. It limits relevant logging, not code execution. `exec` and `in_asm`
serve different purposes: the latter is a translation listing and can be absent
on repeated execution of an already translated block.
[QEMU util/log.c, lines 333–418 and 437–458](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/util/log.c).

The A64 translator treats WFI as a TB terminator, advances PC by four, invokes
the WFI helper, then returns to the main loop if the helper returns normally.
Its single-step branch changes this behavior. Keep GDB stepping, architectural
single-step and the global single-step option out of the accepted command.
[QEMU translate-a64.c, aarch64_tr_tb_stop](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/translate-a64.c).

## Proposed proof strategy, requiring fixture and raw review

Use one immutable linked assembly leaf with an independently inspected entry
instruction `WFI` (0xd503207f), followed immediately by a named resume path.
Enter its first instruction by an explicit branch/call so the WFI starts its
own TB; merely naming a label in the middle of a larger block is insufficient.
The resume path performs the prescribed synchronization and ordered counter
observation before returning. Inspect actual bytes, direct callers, unique
control-flow entry, and absence of alternate branches to the resume label.
The enclosing lease must preserve these code mappings throughout.

For each claimed wait, require CPU0 execution records for the WFI-start TB and
its resume TB, verified TB geometry, a completed private post-wait observation,
and the real cause/ACK/EOI/restoration evidence. Correlate observation ordinals
with this exact bounded sequence; mere address membership is insufficient.
The inference is control-flow based: reaching the unique successor and producing
the completed observation requires traversing WFI, provided no debug/exception/
PC-rewrite path can skip it. Explicitly reject such alternate paths. This is
stronger than a lone dispatch record and does not infer sleep duration.

For the first strict gate, fail closed on any `Stopped execution ...` record in
the relevant interval rather than count its preceding Trace as a completed wait.
If normal pinned runs require retries, design a bounded explicit retry automaton
from the raw sequence before accepting it; do not silently drop records. Repeated
Trace records alone cannot be called repeated completed WFI instructions.
An `in_asm` listing may bind TB instructions once, never replace runtime evidence.
Use source/ELF inspection to prove no bypass, and preserve complete bounded logs.

The first accepted command should use `-d exec,nochain,in_asm,int` with exact
symbol-derived WFI/resume ranges and the existing GIC trace events. Check which
producers apply the address filter; do not assume it filters all exception/GIC
records. No debugger, plugin, guest-memory edits or instruction stepping.
`nochain` affects execution globally even when output is filtered, so measure
its budget/latency overhead and do not present traced timing as uninstrumented
performance. A second uninstrumented functional run can complement, not replace,
the detailed instruction-path proof.

This closes the source-location/filter question and identifies a viable proof
strategy, but **does not yet approve an actual parser or linked leaf**. First raw
logs must establish the exact TB representation, stop behavior and correlation.
WFI may return immediately on pending work; wake-from-idle and quiet-interval
claims additionally need real before/after counts and delayed device/deadline
observations. None of these proves physical power saving.

## Retrieved source fingerprints

All QEMU URLs above name v7.2.22. SHA256 over complete retrieved bytes:

| Source | SHA256 |
|---|---|
| accel/tcg/cpu-exec.c | e62af52deee8abd989f8200d37b398467bff4c1bf8d92e4987bfbacbeb9cdf48 |
| util/log.c | fa85b828596dbbe3e86291bbdf2e3f2c4f948ef67bc05d4c50bee9ddae62567d |
| target/arm/translate-a64.c | 08b543fa9313f8473de4d133ec4243d6b9e93799c07693b9976e2a5015844b27 |
| target/arm/helper.c | e8ec9902ec1be5fe6ff558b0c2bbbb60957a3329123a55f0b40112a43f464fdf |
| Arm Generic Timer 102379_0104_01_en PDF | c8fb557a14970b8302e2f12560fc9038d650da9aaaf97d4bb85327bfa401e82c |

Read-only network retrieval and in-memory PDF text extraction were needed where
browser fetching failed. No Linux implementation was consulted or copied.
