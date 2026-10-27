# 0031: primary evidence for masked-WFI idle

Status: **source investigation, not implementation or guest approval**.
Reviewed 2026-10-26 against the [candidate contract](0031-interactive-input-contract.md).
Only this document changed. No kernel code, build or VM was run. Findings support
masked-WFI as a viable smaller design; exact transaction and failure cleanup
still need independent review and pinned execution evidence.

## Wake is distinct from taking an exception

Arm explicitly documents that an interrupt masked by PSTATE can wake WFI, while
GIC priority masking can prevent that wake. Thus retaining PSTATE.I while waiting
is architecturally supported; it does not establish that every interrupt route
is masked by I or that WFI must actually sleep. The candidate must exclude
higher-EL routing and retain its bounded spurious-return handling.
[Arm Critical Interrupt Prioritization, p7](https://documentation-service.arm.com/static/5ed6547eca06a95ce53f9321?token=).

The pinned QEMU source chain is now inspected:

- [target/arm/op_helper.c, HELPER(wfi)](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/op_helper.c): returns immediately when cpu_has_work is true; otherwise checks WFx trapping, marks the CPU halted and exits translated execution.
- [target/arm/cpu.c, arm_cpu_has_work](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/cpu.c), lines 120–128: powered-on CPU interrupt-request bits determine work without consulting DAIF. arm_cpu_exec_interrupt and arm_excp_unmasked separately decide exception delivery; same-EL IRQ delivery tests PSTATE.I. Higher-EL routing has exceptions to masking.
- [accel/tcg/cpu-exec.c, cpu_handle_halt](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/accel/tcg/cpu-exec.c), lines 624–645: halted state is cleared when cpu_has_work becomes true.

**Inference from that complete chain:** an asserted admitted IRQ can resume the
TCG CPU after WFI while same-EL exception delivery remains masked. Already-pending
work avoids halting. The guest should execute the instruction after WFI without
an IRQ exception in this profile. This is source evidence, not a measured idle
interval. Internal QEMU wake requests can also cause non-device returns; do not
infer UART delivery merely from WFI returning.

## GIC fields and acknowledgement constraints

The following offsets/masks are calculated for IDs 30/33 from the GICv2 register
indexing rules (32-bit accesses; preserve unrelated fields):

| Field | Timer 30 | UART 33 |
|---|---|---|
| IGROUPR | 0x080, bit 30 | 0x084, bit 1 |
| ISENABLER / ICENABLER | 0x100 / 0x180, bit 30 | 0x104 / 0x184, bit 1 |
| ISPENDR / ICPENDR | 0x200 / 0x280, bit 30 | 0x204 / 0x284, bit 1 |
| ISACTIVER | 0x300, bit 30 | 0x304, bit 1 |
| IPRIORITYR word/lane mask | 0x41c / 0x00ff0000 | 0x420 / 0x0000ff00 |
| ITARGETSR word/lane | banked/read-only PPI | 0x820 / 0x0000ff00 |
| ICFGR edge bit | 0xc04, bit 29 | 0xc08, bit 3 |

Level configuration clears only the edge bit. PPI programmability varies.
CPU-interface CTLR/PMR/BPR/IAR/EOIR offsets are 0/4/8/12/16; IAR ID is bits 9:0.
EOIs match acknowledgements in reverse nesting order. Never EOI 1022/1023.
EOImode must select combined priority-drop/deactivation. Group/security register
views differ: inspect actual configuration rather than assume Group0 access.
[Arm GICv2 IHI0048B.b §§3.2,3.4,4.1,4.3,4.4](https://documentation-service.arm.com/static/5f8ff196f86e16515cdbf969?token=).

Pinned [arm_gic.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/intc/arm_gic.c)
confirms enabled/targeted/nonactive candidates, group forwarding and priority
filtering. Acknowledgement activates the selected interrupt; an interrupt not
higher priority than the running interrupt produces 1023. BPR writes are clamped
to the implementation minimum. The emulator tolerates some invalid EOI writes;
that is not permission for guest code to emit them.

Design consequence: do not read two IARs at the same priority before completing
the first and expect both IDs. Specify a bounded per-ID acknowledge, mask its
source, complete device ordering, EOI transaction, followed by the next IAR.
Alternatively a deliberate nested protocol needs its own priority/order proof.
Sampling both peripheral causes before service preserves simultaneous-event
policy, but does not promise both subsequently produce an IAR ID after masking.
Freeze how that case is represented. Unexpected 1022 is a profile failure, not
an owned interrupt or a reason to write an EOI. An unrelated valid IAR has already
changed state: failure cleanup must handle that fact explicitly rather than
pretend no acknowledgement happened.

## PL011 facts and pinned limitations

DR@0, RSR/ECR@4, FR@0x18, IMSC@0x38, RIS@0x3c, MIS@0x40 and ICR@0x44
match DDI0183F. DR carries receive-error bits 11:8; inspect them before byte
truncation. The proposed mask 0x7d0 selects RX, receive-timeout and four error
interrupts, excluding TX/modem. Error clearing and interrupt clearing are
different operations; neither is a general FIFO flush.
[Arm PL011 DDI0183F §§3.2–3.3](https://documentation-service.arm.com/static/5e8e3655fd977155116a9042?token=).

Pinned [pl011.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/char/pl011.c)
uses a one-byte RX trigger, forwards masked status, resets receive storage on
FIFO-enable changes, and does not implement enable/loopback controls fully.
Receive backpressure limits accepted bytes; a host paste is not proof of physical
UART overrun behavior. ICR clears stored interrupt bits; RX is set when enqueue
reaches the trigger. Therefore clearing RX while unread data remains must not
be assumed to reassert a level immediately. Break injection enters an error-tagged
FIFO datum; this is not evidence that all hardware error/timeout sources work.
Use FR/data readiness independently of interrupt status and do not toggle FIFO
configuration to simplify ownership transfer.

Pinned [virt.c](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/hw/arm/virt.c)
assigns UART base 0x09000000 and SPI index 1, hence INTID 33. Physical-board UART
configuration, hardware overrun, flow control and ARM/KVM behavior remain outside
this QEMU-only source claim.

## Barrier evidence and remaining proof

Arm's memory-ordering guide explains that DSB completes preceding accesses for
its access types/domain and prevents subsequent instructions performing their
function until completion. The A64 instruction guide defines SY as full-system,
reads and writes. These are current applicable memory-ordering facts, not the
removed generic WFI/barrier explanatory passage.
[Arm ordering guide §6](https://documentation-service.arm.com/static/62a304f231ea212bb662321d),
[A64 DSB reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

A full-system completion barrier before WFI and between peripheral masking and
EOI is a defensible candidate, **not a completed platform proof**. The final
implementation review must tie it to actual UART/GIC Device memory attributes,
write acknowledgement/readbacks and compiler ordering. A volatile access or a
compiler memory clobber alone is not hardware completion. Separately establish
CNTP system-register synchronization; the MMIO argument does not prove that.
A completed MMIO transaction also does not automatically prove every asynchronous
peripheral side effect has settled. Require bounded readback/active/pending checks
and preserve only owned state; never clear unrelated pending bits.

Before code approval, freeze: all admitted register values/security views;
per-ID service order; simultaneous-cause representation; no-wake and failed-restore
cleanup; timer synchronization; and the compiler-visible private WFI boundary.
Before acceptance, real traces must bind the linked WFI PC, pre/post counters,
DAIF, admitted configuration, actual UART/deadline facts, matching GIC reads/EOIs
and restoration. Absence of an exception trace alone proves neither WFI execution
nor hardware wake. Test arrival before recheck, between recheck/WFI, during wait,
simultaneous expiry and quarantine. No claim of exhaustive race coverage follows
from a few timed writes.

## Retrieval provenance

Browser retrieval supplied the Arm PDFs and some pinned QEMU files. Exact CPU,
TCG and GIC URLs returned cache misses there; sandbox DNS also failed. Authorized
read-only HTTPS retrieval outside that network restriction succeeded. Source was
inspected in memory, not copied into kernel implementation. No source translation
or build occurred. Exact retrieved SHA256 values:

| QEMU v7.2.22 file | SHA256 |
|---|---|
| target/arm/cpu.c | 28ad3ac7fe7684ae1972220fca8d82c7a484aa76d0dfb362fcda270c06b9f218 |
| target/arm/op_helper.c | 685010acb6247146a8ebba7ee513a8d4734663d94b9336a060f88134c7cde9b1 |
| accel/tcg/cpu-exec.c | e62af52deee8abd989f8200d37b398467bff4c1bf8d92e4987bfbacbeb9cdf48 |
| hw/intc/arm_gic.c | 42216ce03268c5dcc561fd2704ae3489d0d59d6cf2d048e2da3a57098fed5859 |
| hw/char/pl011.c | 91a296f590d3825bd065b00a7cdbf8c18f86a255dec8676dbbfa0136d6abae44 |
| hw/arm/virt.c | 5b856d28c2cbfda3c1c1bc2efbe8fe60a5481cf452b4a306e460a76eef85594d |
