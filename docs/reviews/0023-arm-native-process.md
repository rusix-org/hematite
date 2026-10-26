# Native ARM process review

Date: 2026-10-26. Independent AI review, not human approval or a complete
soundness audit.

Current verdict: **safe native policy is locally verified** by ten actual host
tests and twelve compiler clients on each architecture. **The core process and
33-case verification designs are approved for implementation**, including the
final PMU and preflight amendments below. The process runtime is not yet
implemented or executed. Final parser approval requires inspection of actual
pinned-QEMU EL0 trace grammar; source review, compiled stack bounds and all
process execution gates remain open.

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
