# Native ARM process review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit.

Current verdict: **safe native policy is locally verified** by ten actual host
tests and twelve compiler clients on each architecture. **The core process and
33-case verification designs are approved for implementation**, including the
final PMU and preflight amendments below. **The initial runtime and fixture
sources are approved for compilation** after the source corrections recorded
below, including the subsequently corrected DIT/SSBS restoration finding.
Integration, nineteen actual compiler clients and the fourth candidate's
compiled-stack/linked-fixture gates are now reviewed. **That exact candidate is
approved for first QEMU execution**, as identified in the
[stack review](../evidence/process-stack-review.md). The first three actual
traces have now been inspected and the parser approved with the exact
architectural/emulator corrections below. Initial aggregate failures are
preserved; a fresh complete execution gate remains pending.

## Staged scope

The safe native-wire policy and the unsafe execution mechanism have separate
entry gates. Review of [native wire](../design/0023-native-wire.md) does not
authorize EL0 transition code. The forthcoming
[process design](../design/0023-arm-native-process.md) must establish dedicated
vectors, trusted trap storage, cache visibility, timer control, user-memory
ownership and exact restoration before runtime implementation.

## Native-wire policy review

The explicit byte-encoded startup layout and register call contract avoid
compiler ABI exposure. Session-local grant lookup, private issuance state,
generation retirement and no Clone prevent policy mutation through safe
clients. The actual executor must retain one issuer across runs; arbitrary
safe model instances confer no hardware authority. This is a synchronous
console table, not evidence of the earlier asynchronous capability model.

Validation precedence, all-or-nothing copied reads, zero-length semantics and
effect-free malformed requests are explicit. Every request consumes an attempt;
the 64th valid exit remains permitted, while a returning 64th call cannot
resume. The independent absolute timer deadline remains mandatory. The revised
4,096-byte limit makes byte-budget exhaustion reachable before the attempt
limit, unlike the initial redundant 16,384-byte proposal.

The clarified console contract requires a returning host write to have emitted
all bytes. Bounded UART failure is terminal platform failure, may have emitted
a prefix, and cannot return an OK response. This resolves the initially
ambiguous interaction between a void host method and bounded hardware polling.

## Runtime constraints under review

Existing interrupt entry selects SP_EL0 as its emergency stack. That cannot
be used after SP_EL0 becomes user-controlled. Dedicated process vectors must
save all user state through trusted SP_EL1 storage before using scratch
registers, with stack-independent fatal paths for current-EL/nested exceptions.
Reuse of the reserved guarded emergency stack requires exact initial stack
state, inactive old timer/preemption records, BUSY exclusion and masked entry.

Every trap must restore the original root and complete invalidation before
Rust accesses owners or copies user bytes. Restore the old emergency SP_EL0
before restoring the old VBAR. Initial user GPR/SIMD/FP/TLS state cannot expose
kernel state. Reentry cannot trust user mode/DAIF bits; it must sanitize PSTATE
and preserve only the explicitly permitted user state.

The deadline must remain absolute across SVC returns. CNTKCTL_EL1 must deny
EL0 control of physical timer registers, independently of firmware defaults;
otherwise userspace could disable its own deadline. Unsupported extended
register facilities must trap rather than expose unsaved kernel state.
No arbitrary callback may run while a process timer/controller session is
active: existing timer diagnostics do not all acquire BUSY themselves.

Primary sources consulted include the official
[Arm exception model, 102412 version 1.3](https://documentation-service.arm.com/static/67ac57fb091bfc3e0a9479cc),
[Arm cache-maintenance explanation](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/caches-self-modifying-code-implementing-clear-cache),
and [Generic Timer guide](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Generic%20Timer.pdf?revision=c710e7a7-9f52-4901-8c9d-91b19f44f9c7).
Existing timer/GIC/exception ownership contracts and source were inspected;
Linux implementation source was not used.

## Verdict

**Native-wire design approved for safe policy implementation only.** This
includes the revised 4,096-byte budget and explicit console failure contract.
Policy source, tests and compiler probes remain pending. **Runtime design
approval remains pending**; no EL0 transition or other core mechanism is
authorized by the safe-policy verdict.

## Safe policy source inspection

Initial `native.rs`/`native_tests.rs` inspection found one contract mismatch:
the 64th returning dispatch produced Resume, leaving CallLimit until another
dispatch, although `may_resume()` was already false. The approved contract
requires that call itself to produce the terminal outcome, while still
allowing a valid 64th Exit. Requested a final outcome check after validation
and effects, plus corresponding terminal revocation and tests. Resolution
and policy execution are pending. Other inspected generation, range-length,
budget, byte-encoding and effect-ordering paths match the safe policy design.

Resolved through an explicitly reviewed contract refinement: `Outcome::Reply`
records a response but never grants resume authority. The 64th returning
dispatch immediately stores CallLimit and clears its console entry while
returning that response for reporting; `termination()` exposes the stop reason.
A valid 64th Exit retains its Exit result. The runtime must inspect termination
and deadline before every ERET. Updated source/tests implement this distinction
and immediate revocation. **No blocking safe-policy source finding remains.**
Actual test and compiler execution evidence remain pending.

The safe-policy harness and Makefile integration were inspected separately.
`check_native.py` requires exactly ten named successes with no ignored,
filtered or missing cases, complete output and successful bounded execution.
Its persisted mutation tests exercise skipped/altered inventory and otherwise
perfect output accompanied by timeout, failure or missing artifacts.
`check_native_types.py` checks the actual library with three positive and nine
negative clients for private issuer/session fields and cloning. No blocking
harness-source finding remains; parser fixtures are not execution evidence.

## Safe policy execution evidence

Inspected the archived
[host result](../evidence/native-policy-final/aarch64/native-host/result.json)
and its raw stdout: all ten named policy tests pass, with zero failed, ignored
or filtered cases, exit zero and no timeout. The actual
[ARM compiler clients](../evidence/native-policy-final/aarch64/native-types/result.json)
and [x86 compiler clients](../evidence/native-policy-final/x86_64/native-types/result.json)
each pass all twelve expected cases. **Safe policy is locally verified within
those tests and type boundaries.** This evidence does not exercise the future
user-memory adapter or EL0 execution. The reviewer inspected the integrating
agent's artifacts rather than launching these runs independently.

## Process mechanism design verdict

**Design 0023 is approved for core implementation**, after the concrete
revisions described here. This is preimplementation approval only: source,
fixture disassembly, aggregate compiled stack use and hardware execution need
their own review and evidence. It does not complete Phase 2 or its initramfs
input-source gate.

The singleton Executor latch retains one hardware issuer namespace for the
kernel lifetime. Process preparation consumes actual Space ownership, adds one
charged startup frame, and performs all fallible work before its fixed-slot
commit. Failure returns the unchanged inactive Space; normal run completion
retains every owner through root restoration and quiescence, then reclaims it.
No active token, arbitrary callback, Frame or mutable context escapes.

Dedicated all-slot vectors and the fixed, reserved 2048-byte trap frame avoid
the old SP_EL0 emergency-stack assumption. The explicit frame offsets separate
user register values from trusted kernel controls and continuation pointers.
Current-EL/nested failures are stack-independent terminal paths. Every normal
trap restores boot mappings before touching ordinary context buffers; old
SP_EL0 and coordinator SP are restored before old VBAR, and no trap-frame
access occurs after old vectors become active. An invalid user SP therefore
cannot choose kernel trap-store addresses.

User state starts zero apart from declared entry/startup/stack values.
Returning state preserves the listed GPR/SIMD/FP/TLS values while constructing
EL0t PSTATE with fixed masks and only permitted NZCV. PC admission covers an
aligned complete executable instruction. Restrictive CNTKCTL prevents the user
from disabling its own timer; explicit CPACR/SCTLR and PMU admission bound the
supported writable register state. Preservation claims are limited to the
declared set, not all possible architectural extension state.

The timer comparator is one absolute checked deadline, never renewed by SVC.
Count/deadline checks precede every entry and dispatch. Review corrected IRQ
ordering to capture ISTATUS/count before disabling the source, and corrected
restoration to compare only CNTP_CTL writable bits. GIC admission excludes
non-timer enables and active interrupts while preserving unrelated disabled
pending SPI state. Each slice fully restores its original GIC/timer state
before Rust, so a later slice cannot overwrite the original snapshot with
process configuration. Acknowledged ID30 is deasserted before matching EOI;
spurious IDs never receive EOI, and their retry count is bounded.

Code visibility uses owned identity aliases for the PoU clean followed by whole
local instruction-cache invalidation and barriers. Cache-line size is bounded,
CPU1 remains OFF, and executable owners cannot be mutated after sealing.
Private copies use stopped backing owners under the boot root; they never
form references to arbitrary user VAs. Exact source proof of these transitions,
compiler memory effects and cleanup ordering remains mandatory.

## Process verification design verdict

**The 33-case, 66-run verification contract is approved for implementation.**
It now explicitly includes fresh/register/SIMD/FP/NZCV/TLS state, hostile timer
and IRQ-mask writes, bad-SP SVC entry, startup RO protection and EL0 semihosting
denial in addition to normal calls, faults, bounds and the actual timer-stopped
loop. Literal call manifests and output bytes, independent ELF symbols and
complete lower-EL trace transactions prevent an EL1 diagnostic or aggregate
PASS count from substituting for user execution. A returning 64th Reply is
recorded but cannot produce another ERET.

Independently recomputed the system-access syndrome packing: CNTP_CTL write
with fields (3,3,1,14,2), Rt31 and write direction gives ISS `0x32fbe4`;
DAIFSet immediate two with fields (0,3,6,4,2), Rt31 gives `0xcd3e4`.
With EC `0x18` and IL one, the respective expected ESR words are
**`0x6232fbe4` and `0x620cd3e4`**. The designers located the official
[Arm ISS field reference](https://documentation-service.arm.com/static/649ae5b238511951cb799288)
and instruction/access-trap specifications; no arbitrary-exception fallback
is approved. Actual pinned-emulator disagreement must be preserved and
investigated rather than silently widening these expectations.

Pinned [QEMU 7.2.22 configuration](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/semihosting/config.c)
defaults userspace semihosting off. Its
[AArch64 HLT translation](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/translate-a64.c)
checks that permission before intercepting HLT `0xf000`; otherwise it produces
an undefined instruction. The process harness explicitly sets `userspace=off`
and requires the EL0 HLT fixture to fault, with only the final EL1 test exit
using semihosting. This is reviewed emulator behavior, not copied kernel code.

The exact first EL0 trace text remains to be inspected from the pinned binary
before final parser approval. Fixture source/disassembly, parser mutation
tests, actual control/register restoration, compiled stack bounds and all
three guest profiles remain unexecuted for this new process milestone.

The latest mechanism design also requires the existing timer delivery/quiescence
preflight before Executor construction. Its EL1 IRQ trace must have a separate
fixed inventory before the first process transaction. It must not be accepted
as arbitrary prelude noise or confused with the 41-case post-process baseline;
the test author was asked to freeze that preflight grammar before parser review.

Final freeze check confirms the preflight amendment: one existing
`timer::checks()` call, all four observed booleans true (`checks=15`), exactly
eight EL1 timer IRQs, paired ID30 acknowledgments/EOIs and the eighteen stated
CPU-interface writes. The timeout half adds no IRQ. This agrees with the
existing timer source's setup, eight EOI writes, cleanup and timeout sequence.
The old-vector preflight is distinct from both EL0 timer containment and the
sixteen post-process baseline timer IRQs. The two-CPU profile admits exactly
one FEATURES/AFFINITY_INFO pair per run: 132 PSCI HVC transactions for 66 runs;
the one-CPU profile has none. **These concrete amendments are approved.**

The final PMU admission includes encoding 6. Independently checked the official
[Neoverse N2 PMU guide, section 3.1.2](https://documentation-service.arm.com/static/66ace844882fec713ef48199):
PMUVer `0b0110` denotes PMUv3p5. Requiring PMUSERENR zero for this version is
consistent with denying user PMU mutation without changing counter state.
The designer checked the pinned QEMU initialization and reset definitions;
the implementation must still observe actual PMUVer/PMUSERENR at runtime.
The admitted set is absent 0, or implemented 1/4/5/6 with PMUSERENR zero;
unknown versions remain unsupported. This is an explicit target-profile
correction, not removal of the register-access requirement.

## Initial runtime source review

Reviewed `process.rs`, `process-aarch64.S`, `process_layout.rs` and the startup
and cache-maintenance additions to `user_space.rs`. Four findings were fixed
before compilation approval: one named Rust/assembly frame layout with geometry
assertions, readback of all 24 listed saved/unchanged controls, verification of
GIC setup writes before entry, and a borrowing `execute` helper that lets `run`
reclaim Process before releasing BUSY on every returning path after acquisition.
The private ROOT argument uses offset 1560 within the existing 2048-byte frame;
this is an approved private-layout amendment. TCR/MAIR and returned-kind slots
are populated consistently with their names.

The source retains owned backing and tables throughout entry and restoration.
It copies the private context only under the boot root, uses the reserved
exception stack while the process root is active, and captures all user GPRs
before using scratch registers. No Rust callback runs with that root installed.
Timer status and count precede source disable; ID30 EOI follows deassertion,
and pending/active checks precede restoring the original GIC/timer configuration.
The boot root and translations are restored before copying out context. The
coordinator SP and original SP_EL0 are restored before installing the old VBAR
last, with no subsequent fixed-frame access. Control mismatches enter the
stack-independent terminal failure path rather than reclaiming active owners.

Startup installation completes every fallible check before committing the leaf
and owner. Its added page is user RO/NX; executable cache maintenance uses
retained identity aliases and explicit compiler memory effects/barriers. Safe
callers cannot supply context registers, another Host, a callback, or a second
hardware issuer. These are source arguments, not proof of actual timer delivery
or hardware register behavior.

Reviewed the three fixture sources. The register entry branch changes no
register or flags before capture; all GPR/SIMD state is saved before scratch
use, and the entire 1024-byte snapshot is initialized before Rust borrows it.
Syscall shims preserve AAPCS64 and return the two-word C-layout reply. The bad-SP
case reaches SVC without further user-stack access. Deliberate faults have
separate instruction aliases and failure continuations. Linked permissions,
symbols, code generation and aggregate kernel/user stack bounds still require
compiled-artifact inspection.

Initial host-parser review found `controls()` accepted missing, repeated or
reordered GICC control writes and could consume baseline writes after the final
process. Exact per-slice sequences were requested before parser approval.
Compiler-boundary evidence must also hash the included layout and assembly
inputs. The deliberate unreviewed-trace gate remains closed pending actual
pinned-QEMU output.

Both harness findings are fixed in the reviewed source: each slice now requires
the exact three setup and four restoration CPU-interface writes, with timer
EOI before restoration, and compiler evidence includes layout/assembly hashes.
Missing, duplicate and reordered setup/cleanup mutation cases were added.
The strict serial inventory, actual-output hex, per-case calls and faults,
resource accounting, preflight/process/baseline trace separation and nineteen
actual-module compiler clients are approved for execution. Reported mutation
test success is the author's execution claim at this stage; compiler clients
and real guest traces have not yet been inspected for this implementation.

Integration source review approves the feature dispatch, private BootPlatform
constructor callsite after timer preflight, and safe 66-run runner. The runner
checks all loaded bytes/padding/BSS, zero stack pages, distinct mapped frame
addresses and resource recovery. Exported timer-window aliases add no machine
instructions. Fixture snapshot tail clearing now uses encodable single-register
stores at offsets 808 through 1016, preserving flags and initializing every
byte. The manifest producer/consumer schema mismatch was corrected: independent
ELF LOAD decoding checks all segment fields, and fixture absolute kernel symbols
must match the tested kernel's actual text/BSS symbols before launch.

The new prepare Budget/Exhausted rollback branches currently have source review,
not direct process runtime coverage; the 66-run suite exercises successful
preparation and all declared terminal outcomes. Earlier Space allocation tests
do not establish execution coverage for these new preparation branches.

Approved an additive verification amendment after all 66 runs: exercise zero
startup budget, actual Pool exhaustion held by a Block, and successful prepare
followed by inactive drop. Failed preparation must return an audited Space with
unchanged root/entry/frame count and absent startup mapping; successful prepare
must own exactly one additional frame. Every path must recover the baseline.
The required PREPARE record occurs before END and after run generations are
finished, preserving the existing token and PSCI inventory. Implementation and
execution of this amendment remain to be checked.

Read-only inspection of all 33 `process-fixtures-third` disassemblies found
bounded user stacks: register capture uses 1024 bytes with a leaf checker;
ordinary fixture frames use 16 or 32 bytes. Even summing all stack decrements
within the largest ELF, including its panic chain, gives 1168 bytes without
recursion, below the 64KiB mapping. The intentional bad-SP case is separate.
Hello has two RX, one RO and two RW pages, with an additional 4096 zero-initialized
memory bytes in its RW LOAD and actual compiled data/BSS reads. The final
kernel-address fixed-point fixture rebuild still needs hash confirmation.

A subsequent architectural review found an additional state-domain issue before
guest execution: EL0 can write DIT when implemented, and it remains unchanged
on exception entry. This is confirmed by Arm's
[A-profile known-issues clarification, D1.3.2 IX0001](https://documentation-service.arm.com/static/686bf0c446dc1e49e34b8121).
The first source candidate did not restore or observe that field. The integrating
agent is holding the next build for a concrete DIT admission/restoration
amendment and hostile-register check. Earlier compilation approval does not
constitute execution approval with this finding open.

The concrete amendment is approved: conditionally save/restore/read back DIT
for feature encoding 1 and SSBS for encoding 2, admitting their absent encoding
0 without accessing the register; reject other encodings and require MTE0.
This explicitly excludes TCO from the admitted profile. New named metadata
slots 1568/1576/1584 remain within the reserved frame. The existing register
fixture will set DIT1 before its three SVCs and check DIT0 on each sanitized
resume. Arm semantics and the independently inspected pinned QEMU encoding
definitions are distinguished in the design's provenance. Source implementation
review remains pending.

The PSTATE source amendment has now been independently reviewed and approved.
Admission validates the feature nibbles before optional accesses; observe also
guards each access independently by its exact supported encoding. Assembly
stores feature bits and zero/actual snapshots in the named in-bounds slots,
then restores supported fields with an ISB before Rust resumes. The control
comparison now covers 26 registers/fields. This resolves the source finding;
compiled artifacts and the hostile-DIT fixture remain separate execution gates.

## Preliminary compiled-stack findings

The first emitted assembly, preserved in `process-first-stack.s.gz` with SHA
`de7fbf067bd0ef55e642afbc4db2f3d625114e12c5af8fee510e3c3f0e5c6187`,
had a 64,320-byte normal loading chain. In particular `one` used 38,816 bytes:
its 4KiB stack-probe decrement executes nine times. Counting individual emitted
SUB instructions once would have understated the frame. This candidate was
within 64KiB but had little margin; loading and execution helpers were split.

The second assembly, preserved in `process-second-stack.s.gz` with SHA
`2208b979be7a7a7436b63441336b27bd5a05e0d875de2efa339721c61277976a`,
reduced the ordinary path but its new failure-preparation path used 64,624 bytes:
boot 7440 + wrapper 16 + checks 752 + prepare_failure 18432 +
check_prepare_failure 28880 + prepare 9056 + audit 48. This still fits, with
912 bytes remaining. The integrating agent changed the failure checker to
borrow the Err owner inside its Result instead of moving another large Space
onto the stack. Linked memcpy/memset/memcmp were independently confirmed leaf
functions without stack adjustment. Neither preliminary calculation approves
the forthcoming revised binary: final prologues and simultaneous call chains
must be checked after the PSTATE amendment and fixture fixed point.

The third emitted assembly SHA
`4351374a8b6f54026b5aad0cd1906b48996ddba4a7d100d98b5d1b4568ba0b7c`
contains the PSTATE fix and borrowed-result checker. Its compile succeeded;
the subsequent clippy gate failed and is separate from this stack calculation.
The failure helper now uses 20,848 bytes. Its aggregate path is **56,592 bytes**;
the successful inactive-drop path is **56,496 bytes**. The ordinary execution
chain is boot7440 + wrapper16 + checks752 + one18992 + run_loaded21056 +
Executor::run9872 + execute976 = **59,104 bytes**. A conservative further 640
bytes covers the observed console/formatter/padding/output/terminal helpers,
giving **59,744 bytes**. Formatting accepts fixed scalar fields, without recursive
user formatters. Direct loading is shallower at **44,528 bytes** including audit.

The post-process baseline's inspected topology path remains **61,712 bytes**
(7440 + 35072 + 8320 + 4240 + 6464 + 128 + 48), the largest reviewed aggregate,
leaving 3824 below the 65,536-byte coordinator mapping. Linked memory builtins
were rechecked as stackless leaves. The process transition instead uses its
separate fixed 2048-byte exception frame, without calling Rust there, and
unexpected nested exceptions use the stack-independent fatal path.
**This emitted candidate passes the static stack gate.** A final lint-clean
rebuild must preserve these prologues/call paths or undergo another inspection;
this is neither a runtime watermark measurement nor approval of future code.

## First actual trace review

Independently inspected the raw serial and complete trace grammar in
`docs/evidence/process-first-qemu` for 64MiB/one CPU, 128MiB/one CPU and
128MiB/two CPUs. All guests exited successfully and ran the existing 41-case
baseline. Each serial stream contains 66 runs, 488 dispatched calls, 34 normal
exits, 26 contained faults, four call-limit stops and two deadline stops;
the additive PREPARE checks restore the same 2988-frame baseline. Exact output
bytes and bad-pointer/handle/resource outcomes are part of the strict parser.

Each trace contains 516 EL0 entries and 516 lower-EL exceptions: 488 dispatch
SVCs, two deliberately invalid-immediate SVCs, fourteen data aborts, four
instruction aborts, six undefined/system-access exceptions and two timer IRQs.
The preflight has eight EL1 IRQs and the later baseline sixteen; only the two
loop cases have EL0 timer IRQs, each acknowledged and EOIed as ID30. The two-CPU
trace adds exactly 132 PSCI transactions and no CPU1 execution. Only the final
EL1 exit uses semihosting. Setup/teardown, vector addresses, SVC successor PCs,
terminal no-resume behavior and complete line inventory match the reviewed
grammar. The first aggregate results remain failures because of these two
overly narrow test expectations, investigated before any acceptance change:

- The two BR-to-NX fixtures capture BTYPE01, adding `0x400` to saved PSTATE.
  [Arm's BR specification](https://documentation-service.arm.com/static/68da52dfbd7cab51328c0622)
  defines that update for an unguarded source page. Approve exact saved base
  `0x00400740` for rwexecute/stackexecute only, and `0x00400340` elsewhere,
  allowing NZCV in both. Do not mask arbitrary BTYPE values or other fields.
- The pinned emulator reports DAIF ESR `0x620793e4`, whereas instruction-field
  packing gives architectural `0x620cd3e4`. Independently fetched the exact
  [QEMU v7.2.22 DAIF helper](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/helper-a64.c)
  (lines 69–84) and its
  [syndrome constructor](https://raw.githubusercontent.com/qemu/qemu/v7.2.22/target/arm/syndrome.h)
  (155–161). The helper supplies low three bits of operation `0x1e` as op1 and
  high three bits as op2, reversing the instruction fields. The constructor
  therefore produces the observed value exactly. Approve only that literal for
  this pinned QEMU profile; preserve the architectural value and discrepancy
  in the contract. CNTP's architectural ESR matches without exception.

**Actual trace grammar is approved after these exact corrections and mutation
coverage; the deliberate parser-review gate may be opened.** The architecture
rules are not weakened, and no core/emulator code was changed to accommodate
the observations. Fresh gate results and the full regression suite are separate.

DIT evidence has a specific limitation. All lower-EL target PSTATE trace words
are `0x4003c5`, even following the fixture's DIT1 writes. This is not merely
omitted logging: pinned
[QEMU exception entry](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/helper.c#L10211)
initializes a new mode, carries PAN and selects TCO/SSBS, but does not carry DIT
(lines 10359–10391); it then logs the full PSTATE read at 10398.
[PSTATE helpers](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/cpu.h#L1509)
read the stored field and replace it on write. The observed fixture verifies
actual user DIT/SSBS writes and sanitized zero on resume, plus kernel zero
readback. It does **not** dynamically prove recovery from architecturally
retained DIT1; the explicit save/restore remains required by Arm semantics.

Two fixture-description inaccuracies are corrected without changing acceptance:
the deliberate-fault fallback exits 205 (a required nonzero failure), not 255.
The post-64th SVC instruction is RET into the checked helper, with eventual
exit202 if execution continued; it is not an immediate sentinel store. The host
already forbids every ERET after the terminal 64th result and any 65th SVC, so
neither fallback is accepted as a substitute for correct termination.
