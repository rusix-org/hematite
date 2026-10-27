# Interactive input: safe codec and owned READ policy

Scope: safe metadata and policy only. No UART receive access, writable-user copy,
interactive coordinator, hardware idle or userspace READ handler is enabled.
The existing monitor still has no interactive userspace shell.

[Safe API0031](../design/0031-input-safe-api.md) is independently approved in
[the contract review](../reviews/0031-interactive-input-contract.md). The codec
encodes one canonical64-byte input grant with an input-slot generation and fixed
limits; it does not mint hardware authority. A private fixed Interactive Session
profile shares the original nonwrapping issuer and retains counters through
ReadAdmission/ParkedRead ownership. Existing Admission is unchanged. Interactive
entrypoints cannot mint task guards, and legacy input admission rejects READ.
Final recognized READ4096 stops before detaching; READ4095 can park/finish first.

Strict actual host inventories pass:
[codec13](input-safe/codec-second/result.json),
[policy18](input-safe/policy-second/result.json),
[existing native13](input-safe/legacy-native/result.json) and
[existing task16](input-safe/legacy-task/result.json). The initial codec compile
failure is preserved: new tests called nonexistent View::parse; they now use the
existing startup_input::decode and immutable View::bytes API. Production codec
logic was unchanged by that correction.

Actual-library compiler clients pass on
[ARM](input-safe/types-aarch64/result.json) and
[x86](input-safe/types-x86_64/result.json):22 each, including five positive clients
and rejection of cloning, forging, consuming twice, private-field access and
invalid reference escape. Host tests establish original-session ownership,
validation/accounting policy and canonical bytes, not real input or user-memory
fault handling.

Both integration Clippy and both-architecture
[static checks](input-safe/static-first/result.json) pass. Actual ARM layout
remains Process9832 bytes, ordinary Authority928 bytes and three metadata pages.
The feature-specific negative witness is required before those guest profiles.
All34 candidates compile and pass independent emitted stack/order review.
Nineteen ELFs are byte-identical to accepted candidates. Task maximum grows by
16 bytes to61,568/65,536 including the unchanged allowance; other maximum chains
and final counter ordering remain accepted. Process-test BSS remains0x40394000,
matching canonical fixtures. Artifact checks precede any regression guest.

The [first full run](input-safe-full-first/result.json) stops before guests at
the legacy Session-forgery probe: its explicit field list became stale after
adding the private profile. The corrected probe uses external record update
and still requires E0451. All twelve legacy probes pass on both targets; no
expected diagnostic or production check was relaxed.

The [second full ARM/x86 run](input-safe-full-second/result.json) passes in
198.9 seconds with all captured inputs unchanged. Every one of the
[34 executed kernels](input-safe-full-second/kernel-comparison.json) matches
its reviewed candidate. Raw archives retain116 ARM and18 x86 result records,
with no false verdicts. Independent review replays both ordinary task matrices
and all ten failure profiles. Interactive kernel execution and hardware contracts
remain separately gated; a passing metadata stage does not establish a shell.
All 342 captured inputs match committed source `0d7b3224` exactly; see the
[committed-blob verification](input-safe-full-second/verified-commit.json).
