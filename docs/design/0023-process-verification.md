# Protected native process verification

Status: approved for implementation, 2026-10-26, by independent
[review 0023](../reviews/0023-arm-native-process.md). The final parser still
requires inspection of actual pinned-QEMU EL0 trace grammar before approval;
no process execution has yet been verified. This is the verification companion
to [process design 0023](0023-arm-native-process.md), whose separate review owns
entry, copy, timer and teardown mechanisms.
[Design 0021](0021-arm-first-userspace.md) owns the milestone requirements;
[design 0022](0022-arm-owned-address-spaces.md) establishes owned loading only.

## Executable inputs and reproducibility

Build separately linked static AArch64 native ELF fixtures with the pinned Rust
compiler/linker, explicit LOAD permissions, no external dependencies and no
runtime dynamic linker. A real Rust hello function reads startup data, verifies
its initialized data and zero BSS, writes the exact bytes `hello from EL0\n`
through its granted console handle, checks the returned byte count, then exits
with a declared status. The old parse-only `native-elf` fixture is not this
program: its entry ends in BRK and it has no native syscall protocol.

Keep ABI/entry shims and deliberate fault instructions in audited fixture files
under hm-core; ordinary fixture logic is safe Rust. Independently record the
source/linker/compiler command hashes, complete ELF SHA256, readelf headers,
program headers and symbol table, and AArch64 disassembly. Exact fixture symbols
identify entry, each SVC instruction and its successor, intentional fault PCs,
and the infinite-loop instruction interval. Retain those symbols independently
of Rust optimizer placement. Require each identified instruction to lie in its
expected file-backed LOAD and check its disassembled instruction. A passing
serial label without corresponding execution evidence is insufficient.

The host rebuild must compare embedded fixture bytes byte-for-byte against the
regenerated inputs. If an initramfs is included, retain its hash and independently
verify the selected member's bytes equal the ELF. If this stage embeds ELFs
directly, label it as a narrower EL0 execution milestone: the initramfs part of
design 0021 remains open. Do not silently count embedding as initramfs loading.

## Fixed inventory

The companion [wire contract](0023-native-wire.md) fixes SVC #0, version 1,
write operation 1, exit operation 2, status values 0..7, a 256-byte write cap,
4096 successful bytes per session and 64 total SVC attempts. Host expectations
are literal contract values, not imported from the implementation under test.
Hello verifies all 64 startup bytes and the remaining zero page, including the
three bounds 256/4096/64 and its actual grant. Required ordered cases:

| Case | Required behavior and independent evidence |
| --- | --- |
| hello | Rust startup/data/BSS checks, one exact console message, checked write count, explicit normal exit |
| badptr_null | Console copy returns the specified pointer error; no bytes emitted; process then exits normally |
| badptr_overflow | Checked pointer+length overflow returns pointer error, without console output |
| badptr_crossing | Range begins in a readable mapped page and ends in an unmapped page; reject whole request, with no partial output |
| oversize | Valid pointer but length over the fixed bound returns length error before output |
| badhandle | Wrong slot or generation returns Handle=3; no output |
| badversion | Unsupported native ABI version returns version error; no output |
| badopcode | Unknown operation returns operation error; no output |
| rxwrite | Store to fixture RX page terminates only this process with lower-EL data permission fault |
| rwexecute | Branch into retained RW data terminates with lower-EL instruction permission fault |
| stackexecute | Branch into user stack terminates with lower-EL instruction permission fault |
| kernelread | Load from exported privileged kernel symbol terminates with lower-EL data permission fault |
| kernelwrite | Store to privileged kernel symbol terminates with lower-EL data permission fault |
| uartread | Load from UART mapping terminates with lower-EL data permission fault |
| gicread | Load from GIC mapping terminates with lower-EL data permission fault |
| cntpdisable | Execute exactly `msr cntp_ctl_el0, xzr` with kernel CNTKCTL=0; require its reviewed access-trap syndrome, then kernel restoration |
| daifmask | Execute exactly `msr daifset, #2` with SCTLR.UMA=0; require its reviewed access-trap syndrome, then kernel restoration |
| stackguard | Store into the immediately preceding stack guard terminates with lower-EL translation fault |
| loop | Infinite EL0 branch loop with no SVC is stopped by the kernel physical timer deadline |
| zerowrite | Valid handle, length zero and u64::MAX pointer return OK/0, with no copy/output or byte charge; then normal exit |
| maxwrite | Exactly 256 known bytes return OK/256 and appear once in the sink; then normal exit |
| bytebudget | Sixteen 256-byte writes succeed (4096 bytes); a valid one-byte request returns Budget=6 with zero output, zero-length still succeeds, then normal exit |
| calllimit | Sixty-four zero-length valid writes; the 64th has OK/0 internally but causes CallLimit before any next user instruction; no 65th SVC |
| invalidstorm | Sixty-four invalid-version requests consume attempts, no bytes; CallLimit before any next user instruction |
| exit64 | Sixty-three zero-length valid writes followed by valid exit as attempt 64; normal Exit, not CallLimit |
| invalidexit | Invalid status 256 and nonzero reserved argument return Argument=7 without stopping; a later valid exit succeeds |
| stalehandle | Actual previous run's recorded token fails in the next session despite a new valid console grant; Handle=3, no output |
| nogrant | Startup handle/features are zero; attempted write receives Handle=3 even with a guessed token, then valid exit |
| svcimm | Execute exactly `svc #1`; terminate as process fault with SVC64 EC=0x15, IL=1, immediate=1, no native dispatch |
| registers | Check fresh architectural state, set distinct register/SIMD/FP/NZCV/TLS sentinels, survive three returning zero-length SVCs, verify all preserved fields, then exit 0 |
| badsp | Set SP to u64::MAX, prepare exit 0 without stack access and execute SVC; normal exit proves trap storage does not use user SP |
| startupwrite | Store into startup page; lower-EL data permission fault at that exact address, then full reclamation |
| semihost | Execute `hlt #0xf000` at EL0 with QEMU userspace semihosting explicitly disabled; require Unknown/undefined EC=0, IL=1, with no semihost event |

Run the complete ordered inventory twice with newly loaded, separately owned
processes (66 runs). Every exit/fault/deadline must restore the exact free-frame
baseline; every fresh hello must observe zero BSS and correct initialized data.
The native contract may add malformed startup/copy/handle cases but must not
replace this inventory with a broad aggregate success count. The stale-handle request must equal a preceding RUN's real recorded handle,
while the current handle has a distinct monotonically increasing generation.
The fixture may derive the immediately preceding generation from its startup
handle, but the host must compare it against the earlier run, not merely check
that the request returned an error. The issuer persists across all runs. Each
`nogrant` run still consumes one generation; its zero startup token is not proof
that issuance stopped.

## Fixture instruction and call manifest

The table order above is the execution order, repeated for pass 0 then pass 1.
Each input's checked-in manifest records its exact file hash, entry and exported
instruction symbols, expected SVC calls and exit/fault outcome. Host checks the
manifest against readelf/disassembly; the guest cannot supply its own expected
values. Every successful normal exit is code 0; a fixture assertion failure uses
code 1..255 and fails the gate. Intentional memory/privileged-fault programs have
no syscall prelude; each performs its one disassembled fault operation, with an
unreachable `exit(255)` fallback that must never execute.

Hello issues WRITE(15 bytes `hello from EL0\n`) then EXIT(0). Each single-invalid
case issues its named invalid request, checks the exact wire status and zero
written count, then EXIT(0): null/overflow/crossing return Address=4, oversize
(length 257) Length=5, badhandle/stalehandle/nogrant Handle=3, badversion Version=1,
badopcode Operation=2. Zero length uses a valid grant and u64::MAX address;
maxwrite uses a 256-byte array with byte[i]=i. Bytebudget writes that same array
sixteen times, requests one additional byte (Budget=6), makes a zero-length
write (OK/0), then exits: 19 calls, exactly 4096 output bytes. Calllimit and
invalidstorm each have exactly 64 calls and no exit call; an instruction after
the 64th SVC sets a failure sentinel, and must not execute. Exit64 has exactly
63 zero writes followed by EXIT(0). Invalidexit issues status 256, then a valid
status with address=1, then a valid status with length=1 (Argument=7 each), then
EXIT(0). Registers has three zero writes then EXIT(0). Badsp has only EXIT(0).
Use these literal malformed requests: null address 0/length 1; overflowing
address u64::MAX-3/length 8; crossing address 0x1_001f_ffff/length 2;
oversize length 257 at the admitted fixture data symbol; badhandle is the granted
token XOR 1 (slot zero); badversion is 2 with otherwise valid zero write;
badopcode is 255 with version 1. Error fixtures verify the prescribed status
and x1=0 before exit. Nogrant attempts token 1 and verifies startup feature/handle
zero. Stalehandle uses the immediately preceding issued generation's actual
slot-one token. Fault targets are RX entry for rxwrite, the RW fixture data
symbol for rwexecute, stack-top-16 for stackexecute, `__text_start` for kernelread,
`__bss_start` for kernelwrite, UART 0x0900_0000, GIC 0x0800_0000, and guard
0x1_001e_f000. Data operations use one aligned 8-byte load/store. Require FSC=15
for all protected data/instruction accesses and FSC=7 for stackguard; WnR is one
only for stores. All fault PCs are exact independently extracted fixture symbols,
not unchecked runtime suggestions.

The registers fixture's entry shim captures initial x0..x30, q0..q31, FPCR/FPSR,
NZCV, TPIDR_EL0/TPIDRRO_EL0 and SP before Rust can clobber them. Require x0=startup,
x1..x30=0, all vector lanes and FP/TLS/NZCV zero, and SP=0x1_0020_0000. The shim
uses only its own admitted user storage; captures must not assume x18 or any
other register is already scratch. Each pass's first hello also checks fresh
BSS and initialized bytes; the registers fixture repeats on both passes to
catch architectural-state leakage after intervening runs.

Before each returning SVC set distinct per-register x2..x30 sentinels, accounting
explicitly for argument registers x2/x3/x8 and the actual syscall ABI. Set all
q0..q31 lanes to distinct patterns; choose architecturally writable FPCR/FPSR
bits, nonzero NZCV and TPIDR_EL0, keep TPIDRRO_EL0 zero and retain the valid user
SP. After each SVC, assembly captures and checks every preserved value before
Rust code executes; x0/x1 must contain the expected response. Use different
patterns on successive calls so a frozen snapshot cannot pass. For call j=1..3 use x[r]=0x3000_0000_0000_0000 | (j<<8) | r for
r=2..30, except x2=u64::MAX, x3=0 and x8=1 as the zero-write arguments.
Use q[r].low=0x1000_0000_0000_0000 | (j<<8) | r and
q[r].high=0x2000_0000_0000_0000 | (j<<8) | r for r=0..31;
FPCR=j<<22 (rounding mode), FPSR=0x0800_0000 | (1<<(j-1)),
TPIDR_EL0=0x1234_0000_0000_0000 | j, and NZCV respectively
0xa000_0000, 0x5000_0000, 0xf000_0000. TPIDRRO_EL0 stays zero.
Freeze those literal masks/patterns in the fixture manifest and independently
review the capture/check assembly; arithmetic flags must be set last and captured
before comparison instructions change them.
User flags must retain only their permitted NZCV across sanitized returns;
observed control-mode/DAIF/PAN fields are checked against the process contract
in captured SPSR, not settable user sentinels.

The fixture reaches EXIT(0) only after every comparison; a fixed success label
alone is not evidence. Independently inspect the complete disassembly path
including its failure branches. The badsp fixture sets SP to 0xffffffffffffffff
immediately before its exit SVC and performs no later user memory/stack access;
the trace must show that exact SVC successor, normal exit and boot restoration.
Startupwrite targets startup+56, which is RO and initially zero, and must fail
with a write permission fault rather than an arbitrary crash.

## Serial wire contract

Use a dedicated `HMPROC` prefix, canonical unsigned decimal integers and bounded
hexadecimal payload bytes. No omitted/duplicate/unknown record or skipped case
is accepted. All lines must be ASCII, newline terminated and fully matched;
reject unrecognized or blank output inside the report. Proposed order:

The reviewed exact field order is fixed; all numbers are decimal u64 unless the field explicitly admits a fixed
word or lowercase hex. Case/fixture names are drawn only from the inventory.

```text
HMPROC PREFLIGHT checks=15 irqs=8
HMPROC BEGIN 66
HMPROC INFO wire=1 cpus=N peer_off=1 abi=1 max_write=256 byte_budget=4096 call_limit=64 timer_frequency=F timer_ticks=T
HMPROC RUN index=I pass=P case=C fixture=C entry=E root=R sp=S handle=H before=B during=D owned=O
HMPROC CALL index=I ordinal=J pc=PC version=V operation=OP arg1=A1 arg2=A2 arg3=A3
HMPROC OUTPUT index=I ordinal=J length=L hex=HEX
HMPROC RESULT index=I ordinal=J status=STATUS bytes=W
HMPROC STOP index=I reason=REASON code=CODE calls=NC written=NW esr=ESR far=FAR elr=ELR spsr=SPSR irqs=NI
HMPROC RESTORE index=I root=BR vbar=BV controls=1 inactive=1 before=B after=B
HMPROC PASS index=I pass=P case=C
HMPROC END 66
```

RUN..PASS repeat for all 66 runs: P is 0 or 1, and I=P*33 plus the zero-based
case position, yielding exactly 0..65. CALL/OUTPUT/RESULT are repeated only according
to each fixture's frozen call sequence. Ordinals are 1..64. OUTPUT occurs only
for successful nonempty writes, between their CALL and RESULT; HEX has exactly
2*L lowercase hexadecimal digits representing the actual copied bytes. Zero
writes and errors have no OUTPUT. A valid exit has CALL then STOP, with no
RESULT. A returning 64th call has RESULT then CallLimit STOP, with no ERET to
EL0. Other returning calls require exactly one matching ERET. Fault/loop cases
have no CALL, apart from any explicitly fixed prelude in their manifest.

REASON is exactly `exit`, `fault`, `deadline` or `calllimit`. CODE is the user
status only for exit and zero otherwise. Synchronous fault ESR/FAR/ELR/SPSR
are captured architectural state; FAR is zero when not architecturally valid.
For normal exit/calllimit the final SVC's ESR/ELR/SPSR are retained and FAR is
zero. Deadline retains IRQ ELR/SPSR; ESR/FAR are zero, avoiding stale syndromes.
NC is the exact accepted SVC-attempt count, including invalid requests; NW is
exact successful byte count. NI is exactly one for loop and zero for every other positive fixture; any
spurious IRQ or platform-event-limit outcome fails this workload gate. INFO
F must be in the admitted 1000..1_000_000_000 range and T must equal integer F/10,
matching the fixed 100 ms process budget. The guest must additionally check the
single comparator uses initial_count+T and never extends after SVC; a fresh
nonzero deadline value by itself is insufficient.

The test-build private core UART adapter encodes OUTPUT directly from the same
bounded copied buffer that its concrete console-write host receives. It uses
fixed private formatting and bounded UART polling; no caller-supplied closure,
trait object or arbitrary callback is invoked under process/BUSY/GIC authority.
The framed hex representation is the actual test console sink output; the host
decodes and compares it against fixture bytes. Do not print a second guessed
message from the runner or count formatting bytes against the 4096 user-byte
budget. Normal console behavior outside this test sink is a separate scope.
The sink's private run/call identifiers are kernel-owned scalars, not user
supplied format strings. Hardware failure remains terminal and fails the test.

Hello's decoded bytes equal `hello from EL0\n` exactly. Maxwrite's 256 bytes
follow a fixed manifest pattern; bytebudget contains exactly sixteen such
records totaling 4096 bytes. All other cases emit zero payload. No raw fixture
payload can masquerade as HMPROC control records. The full existing 41-case
HMTEST baseline follows END and the sole successful CPU0 semihost exit follows
that baseline. No baseline records may precede END.

This field grammar and the exact fixture call sequences are frozen by review. Fixture PCs and symbols are build artifacts rather than numeric design constants.
Process VBAR is the independently extracted 2048-aligned `arm_process_vectors`;
restored VBAR is `arm_vectors`, and restored root is `arm_root`. The private
fixed UART placement is agreed with the process designer. The exact privileged-access syndrome derivation below was independently
reviewed; neither fixture accepts "any fault". Changes need explicit
contract updates rather than parser accommodations. Host expected frame counts
come from independently checked ELF page counts plus the design's explicit
fixed charge of 12 tables + 16 stack + 1 startup, hence O=ELF_pages+29.
RUN before is the common free baseline; require D=B-O before entry and
after=before on every run. Check roots aligned/in the admitted RAM and distinct from
boot root and user backing. The guest must check backing-owner uniqueness and
fresh initialization; integer resource counters alone are not ownership proof.

## Required EL1 timer preflight

Before constructing Executor and before BEGIN, call the existing `timer::checks()`
exactly once. Require all four returned booleans: delivery, context, quiescent,
timeout; encode them respectively as bits 0..3 and emit exactly
`HMPROC PREFLIGHT checks=15 irqs=8`. This record has a fixed position before
BEGIN and cannot be replaced by firmware register assumptions. The actual
`arm_timer_probe` delivers exactly eight IRQs; `arm_timer_timeout_probe` then
withholds routing and observes zero IRQs. Their checked quiescent state includes
CNTP writable control=2, PPI30 disabled/not pending/not active, disabled GIC
forwarding and restored DAIF/SP_EL0. A false bit or skipped call fails before
running a process. The guest must report the returned checks, not a constant.

The trace begins with exactly eight complete CPU0 current-EL IRQ transactions,
validated with the existing `check_timer_trace.validate_trace(..., expected=8)`
contract plus exact `arm_vectors+0x280` vector address and interrupted/return ELR
inside the timer's assembly unmasked window. Add symbol-only aliases
`arm_timer_window_begin/end` at the existing `.Ltimer_window_begin/end` labels
for independent ELF/disassembly range extraction; no timer behavior change is
required. Each transaction is EL1→EL1, physical ID30 acknowledge then complete
ID30 EOI, and EL1→EL1 return. No SVC, data/instruction abort, HVC, semihost or
CPU1 event belongs to this preflight prefix. The timeout half contributes no
IRQ transaction. The exact CPU-interface write inventory (offset,value) is
[(0,0),(4,255),(8,0),(0,1)], then eight (16,30) EOI writes paired to the eight
IRQs, then [(0,0),(0,0),(4,255),(8,0),(0,1),(0,0)]. Thus there are exactly
18 writes, all physical CPU0; no additional acknowledgment or write is allowed.
This matches the current timer assembly and the first checks pair in the
preserved baseline trace. Unknown trace lines/events cannot be ignored as
generic prefix noise. Resolve any logger timestamp prefix
with the existing anchored timestamp grammar, not arbitrary text stripping.

After the eighth completed preflight IRQ, CPU0 PSCI HVC queries are permitted
only for the two-CPU profile's explicit peer-OFF admission checks and must be
parsed as the exact pinned four-line transaction: `Taking exception 11
[Hypervisor Call] on CPU 0`, `...from EL1 to EL2`,
`...with ESR 0x16/0x5a000000`, `...handled as PSCI call`. The pinned emulator
handles PSCI without a vector/ERET record; do not demand a fictitious EL1 ERET. Their count is fixed by reusing design 0022's
PSCI_FEATURES(AFFINITY_INFO) then AFFINITY_INFO admission pair once per run,
under BUSY: exactly 132 HVC transactions for 66 runs; one-CPU profiles have zero.
Neither prepare/construction nor subsequent slices issue additional queries.
The guest checks supported feature result and actual CPU1 OFF from the second
call; identical HVC trace syntax cannot alone distinguish PSCI function IDs. No other EL1 IRQ is admitted until the post-process baseline.
Baseline's existing timer_contract invokes checks twice, yielding exactly
16 EL1 timer IRQ transactions after the last process stop. Those cannot satisfy
loop's separate required EL0 timer transaction.

## Independent QEMU trace gate

Existing pinned QEMU 7.2.22 evidence in
`docs/evidence/elf-final/aarch64/boot/exceptions.log` includes complete blocks:

```text
Taking exception 4 [Data Abort] on CPU 0
...from EL1 to EL1
...with ESR 0x25/0x9600004f
...with FAR 0x4023f0fc
...with ELR 0x40201184
...to EL1 PC 0x40200a00 PSTATE 0x4003c5
Exception return from AArch64 EL1 to AArch64 EL1 PC 0x40201190
```

Those existing EL1 diagnostics cannot prove EL0 protection. The process parser
must retain complete exception and ERET transactions, not count `Taking
exception` lines. At initial entry require an `Exception return from AArch64
EL1 to AArch64 EL0 PC ...` record matching the fixture entry. Subsequent process
exceptions must explicitly say `...from EL0 to EL1`; all target vector addresses
must equal the process VBAR plus the reviewed lower-AArch64 slot (sync 0x400,
IRQ 0x480). Check recorded entry PSTATE's EL1h/asynchronous-mask bits, and check
saved user SPSR in guest evidence separately. Require each successful nonterminal
call to ERET back to the exact SVC successor. No return to EL0 follows a terminal
exit/fault/deadline. Do not accept an EL1 fault at a similarly named symbol.

Require lower-EL instruction-abort EC 0x20 or data-abort EC 0x24 as applicable,
never same-EL EC 0x21/0x25. For data aborts verify WnR, expected FSC, exact FAR
when valid, and the fixed fault instruction ELR. Instruction faults identify the
attempted target PC/FAR and permission status. Reject external aborts, alignment
faults and different translation levels in place of the prescribed fault. For
SVC require EC 0x15, the reviewed immediate and exact successor ELR. CNTP-disable and DAIF-mask both require EC=0x18 and IL=1, FAR invalid/zero,
and ELR at the trapping instruction (not its successor). Decode the system-access
ISS against the independently disassembled instruction: direction bit 0 is 0
(write), Rt=31, with fields (op0,op1,op2,CRn,CRm) respectively
(3,3,1,14,2) for `msr cntp_ctl_el0,xzr` and (0,3,6,4,2) for `msr daifset,#2`.
The ISS packing is op0[21:20], op2[19:17], op1[16:14], CRn[13:10],
Rt[9:5], CRm[4:1], direction[0]. The independently reviewed exact raw ESR values are
0x6232fbe4 and 0x620cd3e4 respectively. Neither fixture may be replaced by
an undefined instruction or BRK.
SVC #1 requires the valid SVC64 syndrome with immediate 1 and successor ELR,
but there is no CALL/RESULT because native dispatch is forbidden.

For loop require an actual CPU0 lower-EL IRQ whose interrupted ELR lies in the
fixture loop interval, followed by physical timer ID30 acknowledgment and EOI in
order, a guest deadline stop and boot-root recovery. Enable the same
`gic_acknowledge_irq`/`gic_cpu_write` events used by `check_timer_trace.py`.
No host timeout counts as a guest deadline. Declare a finite IRQ budget and
kernel tick bound; host wall time is a separate failure cutoff, not a scheduling
accuracy claim. The first physical timer IRQ ends loop: require exactly one such transaction. Do not require stale ESR/FAR fields on
IRQ to resemble synchronous-fault syndromes.

Parse CPU0 process transactions in run order and match their complete ordered
inventory to fixture CALL/STOP expectations; reject missing/orphan/duplicate
ELR/ESR/FAR/vector/return fields and nested kernel exceptions. Because serial
and trace are separate files, do not assume cross-file byte ordering: correlate
by exact counts, ordered run entry/terminal transitions and independently decoded
fixture PCs. Multiple fixtures sharing virtual PCs still require distinct ordered
run entry and completion transactions. The eight explicitly specified EL1 timer preflight transactions are admitted
only before process entry. Existing EL1 baseline faults/IRQs are admitted only
after the last terminal process transaction, through the existing baseline
contract; neither set can satisfy an EL0 fixture observation. Any CPU1 exception/event is a failure while CPU1 is OFF. The semihost fixture
must yield an EL0 Unknown/undefined instruction exception with EC=0, IL=1 and
ELR at its independently decoded `hlt #0xf000`. Reject any semihost exception
or host termination attributed to that instruction. The sole allowed semihost
transaction is the final EL1 kernel exit after all 66 processes and baseline.

Initial EL0 trace grammar must be inspected in the pinned binary's actual output
before approval of the parser; currently recorded examples are EL1-only. Preserve
first failures, do not silently widen patterns to accept the implementation.

## Profiles and harness failure tests

Use ARM QEMU virt/GICv2/TCG with explicit
`-semihosting-config enable=on,target=native,userspace=off`, bounded 64 and
128 MiB one-CPU runs, then 128 MiB
two-CPU run with CPU1 admitted and confirmed OFF throughout. Every profile must
finish the full inventory and baseline with exit 0. Capture exact command,
image/input hashes, tool versions, serial, exception/GIC trace, readelf/disassembly
and parsed results. Use fresh evidence directories, bounded logs and a bounded
host deadline. Missing tools/artifacts, timeout, nonzero exit or skipped profile
fail the gate. Existing ARM and x86/SMP gates remain required independently.

Mutation tests must remove/duplicate/reorder every report class and complete
trace transaction; substitute EL1 origin for EL0; change ESR class/FSC/WnR,
FAR/ELR/vector, SVC successor, CPU, timer ID/EOI order, frame counts, call status,
payload, terminal reason and fixture hash. Replace loop's timer completion with
host kill, synthetic PASS, SVC exit or an EL1 IRQ and require failure. Test partial
records, non-ASCII, integer overflow, unknown records, extra output and baseline
before teardown. A positive host fixture is a parser test, not guest evidence.

## Sources and limits

Consulted existing repository design 0021, test_kernel.py, test_space.py,
check_timer_trace.py and preserved QEMU trace above. The
[QEMU invocation documentation](https://qemu-project.gitlab.io/qemu/system/invocation.html)
describes logging/trace controls; exact text is pinned-binary evidence, not an
assumed stable QEMU API. The
[Arm ESR_EL1 register reference](https://support.arm.com/documentation/111107/2026-09/AArch64-Registers/ESR-EL1--Exception-Syndrome-Register--EL1-)
and [Arm architecture manual](https://documentation-service.arm.com/static/67e40f3398aa3c3b6eea6a85)
are the specification sources for exception syndrome classes. Exact instruction,
vector and timer details must be cross-referenced by the mechanism review.
Arm's [register pseudocode, 2021 edition](https://documentation-service.arm.com/static/6166bf63e4f35d248467c9c0)
shows CNTP_CTL_EL0 access with EL0PTEN=0 trapping to EL1 with EC=0x18 in this
non-EL2 profile. [DDI0616 B.a, base MSR instruction, page 761](https://documentation-service.arm.com/static/6526e1bd9e189a266cef8412)
likewise routes EL0 DAIFSet/DAIFClr with UMA=0 to EL1 EC=0x18. These primary
pseudocode extracts resolve the class. The [DDI0615 A.d MSR/MRS/System ISS
encoding](https://documentation-service.arm.com/static/649ae5b238511951cb799288)
defines the field packing from the issued instruction. Combining that rule
with the separately checked opcodes yields the exact ESR calculations above;
the reviewer independently obtained the same values.
The independent reviewer checked the pinned QEMU v7.2.22
[semihost configuration](https://github.com/qemu/qemu/blob/v7.2.22/semihosting/config.c)
and [AArch64 translation](https://github.com/qemu/qemu/blob/v7.2.22/target/arm/translate-a64.c):
user semihosting is disabled by default and A64 HLT #0xf000 requires the
current-EL-aware semihost enable check. The process command nevertheless makes
`userspace=off` explicit and independently executes the hostile instruction.
No Linux implementation source was opened or copied.

This proves a declared native process's execution, containment and reclamation
on the pinned emulator once actually run. It does not establish Linux ABI
compatibility, native ARM cache/ordering behavior, concurrent processes, SMP
shootdown correctness, physical hardware support or the unfinished initramfs
requirement.
