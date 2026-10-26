# Protected native ARM process execution

Status: implemented, with source, compiler boundaries, compiled stack and first
three pinned-QEMU EL0 traces independently reviewed, 2026-10-26; see
[review 0023](../reviews/0023-arm-native-process.md). The initial harness failures
are preserved. Exact BTYPE and pinned-emulator DAIF expectations were corrected
after investigation; the fresh full ARM/x86 execution gate now passes. See
[verification evidence](../evidence/native-process.md).
This completes the execution/containment portion of stages C/D in
[0021](0021-arm-first-userspace.md), using the owned address spaces proved in
[0022](0022-arm-owned-address-spaces.md). It is not an initramfs implementation,
a Linux adapter, concurrent userspace, or the complete Phase 2 exit gate.
The target here is a separately linked native Rust hello/write/exit program,
plus real EL0 error, protection-fault and infinite-loop programs.

The [native wire contract](0023-native-wire.md) exclusively defines startup,
register arguments, console grants, statuses and policy budgets. The
[verification contract](0023-process-verification.md) defines the workload
inventory and strict hardware evidence. Changes must keep all three aligned.

## Authority, API and resources

A CPU0-affine `Executor` is privately constructed from the actual admitted
BootPlatform and retains one native Issuer across all runs. A permanent
core initialization latch admits exactly one Executor in this kernel lifetime;
dropping or forgetting it never permits reconstruction. Safe callers therefore
cannot prepare with one hardware issuer and run under another. Issuer retirement
on generation exhaustion is permanent, not a reset/replacement operation. Neither it nor a
Process is Send, Sync or Clone. Safe users cannot supply raw context pointers,
roots, exception vectors, GIC callbacks or executable kernel continuations.

`Executor::prepare(&mut self, space: Space<'pool>, console: bool, extra_frame_budget: usize)`
consumes an inactive validated Space and creates `Process<'pool>`, or returns an
error with the original inactive Space. It issues a fresh policy Session and
fills one newly owned zeroed startup page. Issued generations are never reused,
including failed preparation. Check all fallible inputs before committing the
startup descriptor: context, Space audit, empty startup slot, resource budget,
new frame identity/attributes and encoded startup bytes. Hold the new Frame in
a local owner while these checks run; failure releases it and returns the
unchanged Space. The final commit writes the already-validated fixed leaf,
moves the owner into its empty slot and increments the bounded count, with no
fallible step afterward. Internal corruption after this point is terminal, not
an error that returns a modified Space as the original. A private
Space method installs only the fixed reserved startup slot, with exact user RO,
PXN/UXN descriptor `0x0060_0000_0000_07c3`. No generic safe mapping writer is added.

Startup extends the bound to 285 retained frames (12 tables, 256 image, 16 stack,
one startup), and the current five-page fixture to 34. Stage B's ordinary Space
constructor and its 284-frame tests retain their existing reservation contract.
Space records a private startup-installed flag, initially false. Its audit permits
slot 256 only when that flag is set, requires exactly ReadOnly there, and applies
the matching 284/285 count limit. Commit sets the flag together with ownership;
no public method can set it or recover the prepared Space from Process.
A Process owns the Session, private user-register state and its Space; no mutable
Frame or context reference escapes. Preparing/dropping without execution reclaims
all owners; forgetting an inactive Process leaks them safely.

`Executor::run(&mut self, process: Process<'pool>) -> Result<RunReport, RunError>` consumes
the process and runs a finite state machine to exit, contained user fault,
call limit or deadline. Reports contain bounded scalar observations and counts,
not user references or retained process frames. Admission errors still destroy
only an inactive owned process. Every normal return follows full quiescence,
then drops all Space/startup owners before returning the report.

Run accepts **no caller callback or output writer**. It invokes the fixed safe
`hm_kernel::native::Session` with a private core Host adapter. This matters even
under the boot root: existing timer diagnostics can be called without BUSY, so
an arbitrary callback could change GIC state behind the process runner. The
private adapter copies from retained backing and uses bounded fixed PL011 output;
the verification feature uses the bounded hex record encoder specified in the
verification contract, with kernel-owned run/call IDs and no formatting callback;
no user supplied code can invoke kernel APIs. Output follows complete successful
copying. UART hardware failure is a distinct terminal platform failure, not a
user address fault or a fabricated successful partial write.

## Admission and kernel state

The dedicated guest profile first runs the existing timer delivery/context/
quiescence diagnostic, before creating an Executor or process and before its
process-report BEGIN. This both proves IRQ delivery and establishes the required
masked/disabled timer state; firmware reset IMASK is not assumed. Preflight
failure blocks EL0 entry. Its EL1 trace events are distinct from process events.

Acquire the existing global BUSY lease before inspecting or changing execution
state. Require CPU0, CurrentEL=EL1 and SPSel=1, the exact boot TTBR/TCR/MAIR/SCTLR profile, masked DAIF,
PAN enabled, original VBAR `arm_vectors`, and SP_EL0 equal to the reserved CPU0
`__exception_stack_top`. Existing timer/preempt handler-active flags must be
zero. Admitted one-CPU topology needs no PSCI query; admitted two-CPU HVC topology
performs exactly one FEATURES(AFFINITY_INFO64) / AFFINITY_INFO64(CPU1, level 0)
pair per run and must report CPU1 firmware OFF. Unsupported FEATURES or any
non-OFF status rejects admission. No PSCI query occurs during construction,
preparation or individual slices. Hold BUSY until final teardown; no safe activation
guard or suspended process token is returned.

Require the existing GICv2, no-security-extension profile with controller
forwarding disabled, physical timer disabled/masked, PPI30 disabled/not pending/
not active, all non-timer PPI and SPI enables clear, and no active interrupt.
The architectural read-only SGI enable bits are exempt; no SGI may be pending.
Disabled unrelated SPI pending bits may remain set and must be preserved. Reject unexpected live device state before mutation. Snapshot each GIC
field actually changed: distributor/CPU control, PMR/BPR, PPI30 group/configuration
and priority. Do not clear unrelated SPI/SGI state as a shortcut to admission.
The process scope owns PPI30 and its local timer until final cleanup. Hardware
configuration is strictly **per slice**: snapshot original admitted GIC fields,
configure, enter EL0, then fully restore them before every Rust return. The next
slice readmits the quiescent original configuration and takes a fresh snapshot;
it never overwrites the original snapshot while process settings remain active.
BUSY and the unchanged absolute deadline span all slices. A per-slice
controls-restored report includes GIC writable configuration and timer writable
control/comparator, as well as architectural registers; it excludes ISTATUS.

Save exact kernel TTBR0/1, PAR, VBAR, SP_EL0, DAIF, ELR/SPSR/ESR/FAR, CNTKCTL,
CPACR, TPIDR_EL0/TPIDRRO_EL0, FPCR/FPSR and CNTP comparator. Save/restore
only CNTP_CTL writable ENABLE/IMASK bits; read-only ISTATUS is not stable while
disabled and is never an exact-restoration equality claim. TCR, MAIR,
SCTLR and TPIDR_EL1 remain unchanged and are checked afterward. The assembly
call preserves x18..x30 and full q8..q15 in addition to AAPCS64 requirements;
no kernel SIMD/control state leaks into initial userspace.

While EL0 runs, set CNTKCTL_EL1 to zero, denying user physical/virtual timer and
counter access and disabling event stream generation. CNTP registers carrying
an `_EL0` suffix are not permission to let the program reprogram its deadline.
Use CPACR_EL1 `0x0030_0000` (FP/SIMD enabled, SVE/SME not enabled), and require the
admitted SCTLR value with no user permission to modify DAIF or issue privileged
cache operations. Unsupported instruction classes trap to containment. Restore
changed access controls before returning to Rust. Counters and timer setup use
kernel register accesses only; no user clock ABI is introduced. This does not
claim that every architecturally permitted EL0 system-register read is blocked.
The admitted execution profile is the pinned QEMU AArch64 FP/SIMD profile;
unsaved SVE/SME state is disabled, and widening to additional writable extension
state requires an explicit context/admission review. Check
ID_AA64DFR0_EL1.PMUVer: absent PMU requires no PMU-register access; an admitted
implemented version (PMUVer 1, 4, 5 or 6 in this initial admission set) requires
PMUSERENR_EL0 exactly zero before execution and
afterward, denying user PMU writes without changing PMU configuration. Unknown
PMU versions are unsupported. Register preservation claims cover the explicit
saved set, not an unexamined architecture-wide context.

## Trusted trap frame and vectors

Reuse the existing guarded 16 KiB CPU0 exception stack exclusively during each
assembly window. It is reserved from the Pool and mapped privileged RW/NX in
both roots. The process trap-frame base is `__exception_stack_top - 2048`, aligned
to 16; SP_EL1 is set to that fixed base before ERET. The frame and all kernel
instructions retain identical physical outputs/attributes across root changes.
The coordinator stack retains ordinary Rust frames but is not used for EL0
exception entry. No handler ever selects or dereferences SP_EL0 while it belongs
to the user; a bad, unaligned or guard-page user SP cannot redirect trap stores.

The 2048-byte frame has this private assembly layout (not a user wire layout):

| Byte offset | Contents |
| --- | --- |
| 0..247 | x0..x30, eight bytes each |
| 248, 256, 264 | user SP, PC, captured SPSR |
| 272..783 | q0..q31, sixteen bytes each |
| 784, 792, 800 | FPCR, FPSR, user TPIDR_EL0 |
| 808, 816, 824, 832, 840 | ESR, FAR, trap kind, complete IAR, FAR-valid flag |
| 848..1023 | reserved zero; no user payload |
| 1024..1167 | 18 kernel words, ordered below |
| 1184..1287 | kernel x18..x30 |
| 1296..1423 | kernel q8..q15 |
| 1424, 1432 | kernel FPCR/FPSR |
| 1440..1559 | 15 private timer/GIC/authority words, ordered below |
| 1560 | private process root argument |
| 1568, 1576, 1584 | private feature mask, original kernel DIT, original kernel SSBS |
| 1592..2047 | reserved zero |

The 18 words beginning at 1024 are, in eight-byte steps: TTBR0, TTBR1, PAR,
VBAR, SP_EL0, DAIF, ELR, SPSR, ESR, FAR, CNTKCTL, CPACR, TPIDR_EL0,
TPIDRRO_EL0, CNTP_CVAL, CNTP_CTL, coordinator SP, and private context pointer.
The 15 words beginning at 1440 are: absolute deadline, active cookie, returned
trap code, GICD_CTLR, GICC_CTLR, GICC_PMR, GICC_BPR, PPI group word, PPI config
word, PPI priority word, original enable bitmap, timer-status observation,
trap counter observation, original TCR, original MAIR. GIC fields occupy the low
32 bits of their word. Saved x30 is the kernel continuation; no user-supplied
resume address can replace it. Save unchanged SCTLR/TPIDR_EL1 observations in
ordinary coordinator state, checked after each slice. Define these offsets in
one layout source and pass constants into assembly; compile-time checks must
verify every store fits the reserved frame. Raw assembly cannot infer Rust struct layout. Kernel state and pointers
are never read from a user-writable page. User state may be copied into this
frame under the boot root; returning state is copied back only after restoration.
This avoids depending on compiler placement of a caller's context/return buffer
in an address arena absent from the process root.

Install a dedicated 2048-aligned, 16-slot process vector table **before** assigning
user SP_EL0. Lower-EL AArch64 synchronous and IRQ slots have dedicated entries.
Their first stores save original x0/x1 through trusted SP_EL1; each other GPR
is saved before it becomes scratch. Separate entry labels record trap kind only
after saving all GPRs. Save all SIMD, FP/TLS and exception state before calling
anything or replacing ELR/SPSR. Lower AArch32, FIQ/SError, and all current-EL
slots lead to a new stack-independent terminal process-kernel failure path.
They must not branch through old handlers that switch to SP_EL0 or consult fixed
EL1 fault fixups. Nested handler faults never attempt ordinary reclamation.

## One assembly slice

A private `enter_until_trap` operation is the only active-root mechanism. It has
one controlled return after restoration, not a closure or public active token.

1. Under the boot root and masked exceptions, save kernel state and copy the
   private user context into the reserved frame. Verify common kernel/trap
   mappings and control profile before mutation. Program PPI30 for the same
   absolute process deadline, with the source disabled during setup.
2. Install process VBAR, select the trusted SP_EL1 trap frame, program restrictive
   user control registers and user SP_EL0. Install the process root using 0022's
   pre/post `DSB SY; TLBI VMALLE1; DSB SY; ISB` protocol around TTBR0 replacement.
   No Rust or changed-address access occurs in the transition interval.
3. Load all user GPR/SIMD/TLS/FP state. Initial state is zero except x0 startup VA,
   SP `0x1_0020_0000` and validated ELF entry. Program ELR to the private saved PC.
   Construct SPSR as `0x0040_0340 | (saved_nzcv & 0xf000_0000)`: EL0t, PAN set,
   D/A/F masked, IRQ unmasked. Never copy saved mode, DAIF or other user-controlled
   state into the exception-return control word. Enable the timer and ERET.
4. A lower-EL trap saves complete user state into the fixed frame. Validate the
   originating mode was AArch64 EL0t. For SVC64 EC `0x15`, require IL=1 and
   immediate zero; use the captured architectural return PC without adding four
   again. Other synchronous exceptions become a process Fault; preserve ESR/PC
   and FAR only for lower-EL instruction/data aborts (EC 0x20/0x24) with
   FnV clear. Other reports set FAR-valid=false and FAR=0 rather than exposing
   stale state. Do not dereference FAR.
5. For IRQ, first read the complete GICC_IAR and capture CNTP_CTL.ISTATUS and
   the physical count **before disabling CNTP**. ID30 requires that captured
   timer status and an expired absolute deadline. Then mask/disable CNTP,
   disable PPI30 routing, and quiesce the source; write the complete acknowledged
   IAR to EOIR only after deasserting the level (combined deactivate mode).
   Synchronous traps also disable/mask the source without inventing an IAR/EOI.
   After source deassertion and any required EOI complete with DSB SY, clear only
   PPI30 pending, DSB SY again, and verify enable/pending/active all clear.
   Restore original PPI group/config/priority and CPU/distributor configuration
   while forwarding remains disabled. Restore original masked/disabled timer
   writable control and comparator; verify writable-field equality. Unrelated
   pending state is untouched. This complete quiescence/restoration happens on
   every slice, including SVC and fault, not only final process termination. Special IDs 1020..1023 get no EOI and return
   a bounded spurious-event result; unexpected real IDs are platform failure.
6. Restore boot TTBR0 and complete full TLBI before copying captured state to
   any ordinary Rust-owned buffer or touching the owned arena. Copy the captured user result into its private context under the boot root
   while process vectors still protect the transition. Restore original SP_EL0
   and coordinator SP **before** old VBAR, restore kernel control/FP/TLS state
   and load every saved kernel register needed from the trap frame. Install old
   VBAR last, followed by ISB and the saved AAPCS return; perform no more trap
   frame accesses after old vectors are active. Verify
   restoration before the next policy action. No Rust executes under process
   vectors or with user SP_EL0 installed.

All normal traps stop the task. IRQs never dispatch into current kernel-worker
or timer-probe handlers, and no handler resumes userspace internally. Safe Rust
interprets the captured result under the restored boot root. It handles a valid
SVC through the fixed native policy; only x0/x1 response fields change. A return
PC that is not four-byte aligned or does not cover a complete four-byte
instruction in executable user backing terminates the task before reentry. User SP
may be arbitrary; entry never uses it as a kernel pointer, and subsequent user
accesses fault normally. TPIDRRO_EL0 is zero throughout user execution; TPIDR_EL0 starts zero and is
saved/restored as private user state. General registers, SIMD, FP and TLS persist between
returning native calls; a new process starts with freshly zeroed state.

## Deadline and finite bounds

Use a fixed 100 ms kernel deadline for this first profile. Admit CNTFRQ in
1000..1_000_000_000 and counter values no greater than `0x00ff_ffff_ffff_ffff - frequency`, leaving
one second before the architectural minimum 56-bit wrap. Compute one checked absolute comparator from the initial
physical count; never extend it after SVC. Before every slice, re-read the
counter with ISB and terminate if expired. Reject backward counter movement
as platform failure; a wrapped counter never authorizes another entry. Check
the deadline again before dispatching a stopped SVC. A synchronous UART call
has its own finite polling bound, so this is a user-execution deadline plus
bounded kernel-service latency, not a hard 100 ms total wall-clock guarantee. An enabled PPI30 must regain control
from an EL0 program that executes no syscall, including a tight infinite loop.
User attempts to modify CNTP or mask IRQs must trap, not suppress that bound.

The wire's finite SVC budget includes invalid requests. At most 64 returning
calls can reach the policy; no 65th resume is allowed after a returning 64th.
Count spurious IRQ returns separately, cap them at eight, and terminate with a
reported platform-event limit rather than spinning on repeated special IDs.
Thus every Rust/assembly transition has a bounded cause. Console loops and
copies are independently bounded; the host timeout is an outer test failure
bound, not the mechanism that stops the program. A broken/missing physical timer
cannot be repaired by an EL1 polling loop while arbitrary EL0 executes; timer
admission/delivery evidence is a prerequisite, and missing-delivery negative
images are expected to fail the host gate rather than pretend containment.

## Instruction visibility and alias lifetime

Before first EL0 execution, under the boot root, clean every owned executable
frame to PoU using `DC CVAU` over the full frame's identity alias. Derive D-cache
line size as `4 << CTR_EL0.DminLine`, require 4..4096 bytes and
iterate aligned full-page ranges without overflow. Then `DSB SY; IC IALLU;
DSB SY; ISB` on CPU0. Whole local I-cache invalidation covers virtual instruction
aliases; do not assume invalidating only the identity VA evicts the EL0 alias.
CPU1 is OFF, so no remote instruction visibility protocol is claimed.

User RX bytes cannot change after initialization. Its private identity alias is
EL1 RO/PXN/UXN and no other active alias is writable. Boot identity RW aliases
become active only after EL0 has trapped, root restoration and full TLBI complete;
retained owners are still inaccessible to callers. Between slices the core may
read copied backing, but never modifies executable frames. Kernel copies touch
only retained pages in the selected process, enforce checked complete readable
user ranges, and never create a Rust reference to a user VA. An invalid crossing
produces an Address error before any output. Writable data/stack remain NX;
startup is read-only/NX; user access to kernel, tables, UART and GIC is denied.

## Quiescent completion and evidence

Exit, user fault, deadline and call-limit share one teardown. Each completed
slice has already restored GIC/timer state; final teardown verifies that state
rather than taking a new snapshot or undoing an earlier snapshot twice. Confirm source
masked/disabled, PPI disabled/not pending/not active, and no acknowledgement
requiring EOI remains. Restore changed GIC fields and original forwarding state;
restore the exact saved timer comparator and disabled/masked writable control
bits; do not compare disabled ISTATUS. Restore all
saved architectural controls and clear private process-active authority only
while the boot root, stacks and original vectors are installed. Release BUSY
last. Then destroy Session and every retained Space/startup owner. Any uncertain
root, stack, active IRQ or restoration failure is terminal; it must never free
pages that hardware may still use.

The QEMU verification command explicitly sets semihosting `userspace=off`;
EL0 HLT must fault inside the process rather than invoke host semihosting.
Only privileged test instrumentation may terminate the emulator.

The required gate executes a real linked native program through multiple SVC
returns, checks output/exit, rejects invalid pointers/handles/version/opcodes,
and contains actual RX write, NX execution, kernel/device access, privileged
instruction and guard faults. An infinite loop ends by a lower-EL timer IRQ.
Use independently inspected ELF symbols/disassembly and source-EL0 trace records;
Stage B AT results cannot substitute. Test register/SIMD/NZCV/TLS preservation,
fresh-process zero state, exact controls/restoration, stale grants, no-grant
execution, repeated runs and exact frame reclamation after every outcome.
Reinspect compiled full call-chain stack bounds before guests; the earlier
0022 oversized-stack candidate demonstrates why source estimates are inadequate.

Successful execution of an embedded native ELF is a meaningful runnable result.
Initramfs parsing/loading remains an explicit separate input-source gate from
0021, not silently removed. The same Process path must later accept an Image
from that checked archive without changing ownership or the native wire API.

## Primary architectural sources

The [Arm exception-model guide, ARM062-1010708621-27](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Exception%20model.pdf),
sections 5–6, establishes exception state, SP_ELx selection and vector layout.
The [Arm generic timer guide 102379 v1.3](https://documentation-service.arm.com/static/65fac6957bcc0c1c661b36c0),
sections 4–5, covers comparator/control registers, CNTKCTL access and timer IRQs.
The [Arm cache-maintenance explanation](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/caches-self-modifying-code-implementing-clear-cache)
identifies the PoU clean/invalidate/barrier ordering and CTR line sizes; the
[Arm AArch64 cache-operation table](https://documentation-service.arm.com/static/60b871f615ea424a9cf05728),
B1.6, identifies IC IALLU as whole-cache invalidation to PoU. The existing GICv2
specification/provenance in designs 0009/0012 supplies the controller protocol;
new process code must independently review its actual readback/EOI ordering.
The [Arm PMUSERENR architectural clarification](https://documentation-service.arm.com/static/62f4d062e95b0a633aff7ad7),
section 2.60, identifies the register-access authority controlled by PMUSERENR.
The [Arm PMU version table](https://documentation-service.arm.com/static/62c410c8b334256d9ea8dffb),
G1.3, supplies encodings 0/1/4/5; the
[Arm Neoverse N2 PMU guide](https://documentation-service.arm.com/static/66ace844882fec713ef48199),
section 3.1.2, identifies encoding 6 as PMUv3p5. Unknown versions are rejected.
The pinned [QEMU v7.2.22 CPU initialization](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/cpu64.c#L1208)
sets the AArch64 `max` PMUVer to 6; its
[PMUSERENR definition](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/helper.c#L2103)
has reset value zero. Those emulator definitions were inspected only to check
the declared target profile, not copied into kernel code; actual runtime
PMUVer/PMUSERENR observations remain required.
These sources establish instruction semantics; the scoped root/trap/lifetime
protocol is our design argument. No Linux implementation source is used.

## PSTATE restoration amendment (2026-10-26)

Independent review requires conditional kernel DIT and SSBS snapshot, restoration
and readback on every slice. Admit ID_AA64PFR0_EL1.DIT [51:48] only 0/1,
ID_AA64PFR1_EL1.SSBS [7:4] only 0/2, and MTE [11:8] only 0. Unknown
encodings, SSBS1 (without direct register access), and MTE-enabled guests are
rejected before entry. Absent registers are never accessed. The frame feature
mask has bit 0 for DIT and bit 1 for SSBS2; absent snapshots are zero. Restore
both supported fields before returning to Rust and include them in control
readbacks. User ERET still preserves only NZCV: DIT/SSBS are initially zero and
reset to zero on every resume. The register fixture must set DIT before SVC
and verify its reset after resume, while kernel control readbacks prove isolation.

DIT survives exception entry; SSBS instead takes SCTLR_EL1.DSSBS, which need
not equal the original kernel SSBS. Therefore both require explicit restoration.
TCO is not silently ignored: this profile requires MTE absent and does not access
TCO. This supplements the listed register domain, not a claim that all future
architectural extensions have been isolated.

The first pinned-QEMU traces revealed that version 7.2.22 clears DIT during
exception entry, unlike Arm's retention rule. The fixture therefore verifies
user writes and sanitized resumes plus the observed kernel-zero state; it does
not dynamically exercise restoration from retained DIT1. Keep the explicit
architectural save/restore. The exact emulator-source investigation and tested
scope are recorded in [review 0023](../reviews/0023-arm-native-process.md).

Primary semantics: [Arm L.b known issues, D1.3.2 IX0001](https://documentation-service.arm.com/static/686bf0c446dc1e49e34b8121)
identifies unchanged DIT on exception entry; [Arm I.a correction D19696](https://documentation-service.arm.com/static/644c18cbaa78c007af74fa2c)
identifies EL0 DIT/SSBS/TCO access; [Arm RME register supplement](https://documentation-service.arm.com/static/649ae5b238511951cb799288)
defines DIT feature encodings; [Arm SME register supplement, E3.2](https://documentation-service.arm.com/static/62015c6c965f7d118e3f5f4c)
defines SSBS2 direct access and MTE encodings. [Arm A65AE SCTLR description,
3.1.70](https://documentation-service.arm.com/static/624ff3decaabfd7b3c13e9c7)
defines exception-entry SSBS selection. Pinned QEMU v7.2.22
[CPU initialization](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/cpu64.c)
sets DIT1/SSBS2 and downgrades MTE when the board lacks tag memory; actual ID
readbacks remain mandatory. Its [register definitions](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/helper.c)
corroborate encoded aliases S3_3_C4_C2_5 (DIT) and S3_3_C4_C2_6 (SSBS).
No implementation code was copied; this target-source check supplements Arm's
architectural register semantics.
