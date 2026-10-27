# Fixed UART/timer idle transaction

Status: **verified for the fixed boot-only profile, including full ARM/x86 regression**. This exercises real
PL011 input and masked-WFI timer waiting on pinned one-CPU ARM64 QEMU. It does
not implement userspace READ, an interactive coordinator or a shell.

The [transaction review](../reviews/0031-idle-transaction.md) and
[harness review](../reviews/0031-idle-harness.md) record independent agent review
of source, actual linked ordering/stack, artifact binding and raw diagnostics.
The [profile contract](../design/0031-idle-test-profile.md) requires nine cases
across three fresh boots, including an actual quiet 300-second expiry.

[Static third](idle/idle-static-third/result.json) passes both architectures.
The first static compile failure (missing frequency type and an overly long
lexical terminal borrow) remains in [static first](idle/idle-static-first/result.json).
The [latest host log](idle/host-fifth.log) passes 12 synthetic/mutation methods;
earlier host logs preserve the successive reviewed parser/bounds refinements.
These checks do not establish device behavior.

All 37 [first candidates](idle/idle-candidates-first/inputs.json) compile, with
all 35 regression ELFs byte-identical to the accepted RW-copy baseline; see
[comparison](idle/first-candidate-comparison.json). They were rejected before
boot because the idle profile's later hello call chain needed 67,808 bytes before
the 640-byte helper allowance. An idle-only initialization boundary reduces
that maximum to 63,440 including allowance. The first 37 and subsequent two-profile
candidate sets remain preserved. The actual
[third short/wall images](idle/idle-candidates-third/inputs.json) pass renewed
stack and ordering review with 2,096 bytes spare. Full-suite execution confirms
all 37 actual kernel hashes; see the final audit below.

Both final candidate artifact inspections pass:
[short](idle/artifact-short-second/result.json),
[wall](idle/artifact-wall-second/result.json). They bind the immutable four-word
WFI/ISB/CNTPCT/RET leaf, direct callers and absence of alternate direct branches
into its continuation, native hello/archive, sources and disassembly.

The [first short64 diagnostic](idle/guest-first-short64/short64/result.json)
fails safely before WFI: the proposed UART target write cannot read back on
QEMU's uniprocessor GICv2. Independent pinned-source investigation establishes
RAZ/WI targets with implicit CPU0 routing. The corrected profile explicitly
requires one CPU interface and a zero target word, omits its write/dirty bit32,
and retains every other exact register check.

The [corrected short64 diagnostic](idle/guest-second-short64/short64/result.json)
completes all four guest cases, native hello and 41 baseline tests. Its initial
parser rejects the actual ISB block boundary; the rejected result is unchanged.
Independent raw replay approves exactly three blocks per entered wait: WFI,
ISB, then MRS/RET. The amended parser rejects missing, duplicate, swapped or
merged continuations. The
[short128 diagnostic](idle/guest-third-short128/short128/result.json) passes
every substantive gate and remains deliberately unaccepted while the global
raw-review flag is false. Each short profile has 2 completed waits, 2 later EL0
SVCs, 32 exact timer IRQs and 24 registered baseline faults. Actual P/U bytes are
consumed only after verified hardware restoration. UART timing proves input
across the WFI boundary, without claiming residence in QEMU's halted state.

The [quiet wall diagnostic](idle/guest-fourth-wall128/wall128/result.json)
returns zero after302.421 seconds with no input or watchdog action. At62.5MHz,
wall_end-start is exactly18,750,000,000 ticks (300 seconds), limit equals that
immutable wall_end, and the genuine wake counter exceeds it. Raw evidence has
one WFI/ISB/counter sequence, ACK/EOI30, spurious1023 without EOI, exact restored
controls, native hello and the complete baseline. Independent review accepts all
nine diagnostic cases. Their original false aggregate verdicts remain unchanged:
the review flag was deliberately disabled during capture, and short64 also
preserves its original parser-shape rejection. The final trace gate is now
enabled. A short timer or host watchdog cannot substitute for this evidence.

The [fresh full ARM/x86 run](idle-full-first/result.json) passes in668.740 seconds
with all359 captured inputs unchanged and matching committed `dc8c8cfd` blobs.
The [independent final audit](idle/independent-final-audit.json) verifies both
archive hashes,125 ARM/18 x86 result verdicts with no failures, all37 executed
kernel paths/hashes, and direct replay of the fresh strict nine-case inventory.
The fresh quiet wall case takes302.283 seconds with its exact300-second guest
counter bound, genuine timer cause, no input and complete recovery.
[Matching GitHub Actions](https://github.com/rusix-org/hematite/actions/runs/40457340082)
also passes for `dc8c8cfd756bccb567ca18dc6a81502ea22b79ff`.

Earlier failures and unaccepted diagnostic verdicts remain intact. Acceptance is
for pinned TCG, one CPU, the explicit UP GICv2/PL011 profile and tested bounds;
physical ARM/KVM, active-process input delivery, RX error races, simultaneous
causes and quarantine are not established by this milestone.
