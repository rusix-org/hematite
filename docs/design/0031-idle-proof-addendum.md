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

## Refinement: observe pending level before EOI

Follow-up source investigation confirms the requested **pinned GICv2** predicate.
This supports the cleanup refinement without inventing an ISB propagation bound.
It remains conditional on actual admission/readback and reviewed transaction code.

`gic_internal.h::gic_test_pending`, lines 166–178, uses the following logic for
revisions other than 11MPCore: software-pending OR (level-triggered AND input
level asserted). There is no enable, forwarding, priority or active-state term.
The level and active macros access separate state members.
[Exact pinned helper](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/gic_internal.h).

`arm_gic.c::gic_dist_readb`, lines 1028–1047, implements both pending register
views using that predicate. For PPI 30 it selects the current CPU mask; for
SPI 33 it selects all CPUs. `gic_dist_read`, lines 1166–1188, assembles the 32-bit
read from four such byte reads. `gic_set_irq_generic`, lines 367–378, updates
input level independently of enabling/forwarding; active reads independently
inspect the active mask. The pending-read path filters inaccessible Group 0
only when Security Extensions are enabled and the access is Non-secure.
[Exact pinned distributor implementation](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/arm_gic.c).

The pinned virt machine defaults `secure=false` and supplies that value as the
GIC's `has-security-extensions` property for its emulated controller. GICv2 still
supports grouping without those extensions. Therefore the proposed explicit
one-CPU TCG `virt,secure=off,gic-version=2` admission uses the full Group 0 view;
a generic Non-secure-on-Security-enabled view is **not** interchangeable.
Bind the command and actual GIC configuration/readbacks; do not infer the view
from zero pending bits alone.
[virt.c, lines 732–735 and 3030–3038](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/arm/virt.c).

Arm GICv2's pending-status rules separately describe level-following pending
state, including active-and-pending, and clearing software pending cannot hide
a still-asserted level. Disabling forwarding does not turn a pending-register
read into the CPU interface's spurious-IAR observation. This is the relevant
architectural distinction: poll the distributor state, not IAR/HPPIR while
forwarding is disabled.
[Arm IHI0048B.b §§3.2.2, 4.1.4, 4.3.8 and Figure 4-10](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=).

**Derived conclusion:** with level configuration and the accessible view
verified, pending=0 entails that the observed GIC input level is low. This holds
while that interrupt is disabled, while forwarding is disabled, and while its
active bit remains 1. Thus a genuine acknowledged owned interrupt may be checked
as active=1/pending=0 before its matching EOI. Afterwards require active=0 and
pending=0 with the source kept masked. The pending bit is an observation at the
GIC boundary, not a guessed delay since the timer write. This closes the narrow
source-evidence gap for the proposed pinned-profile polling strategy.

Keep the distinctions explicit:

- Require the mask/disable write and its readback before polling. Preserve level
  configuration until the check and EOI finish. Otherwise an edge setting or
  inaccessible view could manufacture a misleading zero.
- Poll only the owned bit, with the fixed 1024-iteration bound. A failed poll is
  failure, never permission to issue EOI and claim quiet. Do not reset an active
  bit manually to make cleanup pass.
- Pending=1 may reflect a software latch rather than a live level. Clear only
  an allowed owned latch at the reviewed cleanup stage, then re-read; the
  predicate prevents such clearing from concealing an asserted level. Never
  clear unrelated pending state or use a blanket register write.
- UART input can remain buffered with IMSC=0 while its GIC input is low. The
  proof is interrupt-output quiescence, not FIFO emptiness, error clearance or
  permission to discard bytes. Later unmasking is a new transaction.
- An unacknowledged owned source needs pending=0/active=0 checks, not an invented
  active=1 state or fabricated EOI. Actual IAR observations determine that branch.
- Separate register reads are not an atomic snapshot. The source remains masked,
  CPU0 serialized and no configuration writer/callback admitted across polling
  and EOI. Keep those invariants in the implementation proof.

Additional complete-source SHA256 fingerprints:

| Source | SHA256 |
|---|---|
| hw/intc/gic_internal.h | 2fe9dbbecd1e94518b24670f05ee86ca20b7af1605f3a84aef56764234be3efe |
| hw/intc/arm_gic.c | 42216ce03268c5dcc561fd2704ae3489d0d59d6cf2d048e2da3a57098fed5859 |
| hw/arm/virt.c | 5b856d28c2cbfda3c1c1bc2efbe8fe60a5481cf452b4a306e460a76eef85594d |

This refinement supersedes the need to assume a fixed timer-to-GIC propagation
delay for cleanup. It does not establish a universal barrier-only guarantee,
physical-device latency, or execution evidence. Source/layout/order and raw
pinned transaction review remain mandatory before acceptance.

## First diagnostic: uniprocessor target registers

Read-only investigation after the first short64 diagnostic returned
Configuration before PMR setup: the requested UART target write/readback is
incompatible with the pinned one-CPU GICv2 model. This finding proposes a
narrow profile correction; it is not evidence of a successful idle transaction.

In official QEMU v7.2.22
[arm_gic.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/arm_gic.c),
`gic_dist_readb`, lines 1078–1095, reads every interrupt target byte as zero
when `num_cpu == 1` and the revision is not the older 11MPCore variant.
`gic_dist_writeb`, lines 1400–1415, ignores writes under the same condition.
Consequently a checked target8 write of `0x100` necessarily reads back zero
in the admitted one-CPU revision-2 profile, before the following PMR write.
The full-word read is assembled from these byte reads; preserving the other
lanes cannot make that requested UART33 lane writable.

This zero does not mean UART33 targets no CPU. In
[arm_gic_common.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/arm_gic_common.c),
reset lines 302–308 initializes every internal `irq_target` cell to 1 on a
one-CPU GIC. [gic_internal.h](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/gic_internal.h)
line 50 defines the target accessor through that internal array;
`arm_gic.c` lines 94–97 selects a pending SPI when that target intersects the
current CPU mask. The ignored target writes leave the internal CPU0 route
unchanged. This is a pinned TCG source derivation, not a physical-board claim.

Proposed correction for independent review: retain the fixed one-CPU admission,
require target8 readback zero, omit the UART target write and its dirty bit
(index 32 in the frozen snapshot order), and retain zero in entry/setup/restored
snapshots. Do not accept both zero and `0x100` as interchangeable observations,
and do not broaden admission to an SMP profile. Actual UART ACK33/wake and
restoration remain required in the next diagnostic.

Complete source SHA256, fetched from the official tag without modification:

- arm_gic.c: `42216ce03268c5dcc561fd2704ae3489d0d59d6cf2d048e2da3a57098fed5859`.
- arm_gic_common.c: `d2bd4a8a109bdd45a00c7aa13a4346b87c13f1b23eb9757e17bdbdf1b3e57865`.
- gic_internal.h: `2fe9dbbecd1e94518b24670f05ee86ca20b7af1605f3a84aef56764234be3efe`.

Temporary complete files were supplied to the independent reviewer for this
finding; they are not incorporated into the kernel. No implementation, build,
or VM was performed by this investigation.
