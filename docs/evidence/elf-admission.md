# Safe ELF admission verification

Executed 2026-10-26 UTC on parent `ac10326` plus this change. Final
`make test-all` exited 0. [Complete output](elf-full-suite-attempt2.log),
[source identities](elf-final/source-sha256.txt), exact commands, ELF/library
hashes, compiler diagnostics and raw guest traces are preserved in
[final evidence](elf-final/). Existing pinned Rust 1.99.0/QEMU 7.2.22 build
container and resource limits apply. This stage validates a load plan; it does
not allocate user pages, enter EL0 or establish Linux compatibility.

## Admission and actual ELF evidence

The dependency-free safe parser accepts the explicit static AArch64 profile in
[design 0021, stage A](../design/0021-arm-first-userspace.md). The immutable plan
borrows its source bytes and has private fields. Checked byte decoding validates
headers, table containment, segment file/memory bounds, alignment, page-disjoint
permissions, capacity and a complete aligned file-backed entry instruction.
Unsupported dynamic/TLS/interpreter/other program headers fail explicitly.
Section tables and physical-address fields never supply load authority.

The [separately linked fixture](elf-final/aarch64/elf-host/native-elf) is built
from original Rust with a small core-owned assembly entry and an explicit linker
script. Its entry calls a Rust function then traps; it is inspected, not executed.
Independent `readelf` output matches the actual parser's plan and file-byte
fingerprints:

| Segment | Virtual address | File bytes | Memory bytes | Permissions |
| --- | ---: | ---: | ---: | --- |
| Code | 4294967296 | 56 | 56 | RX |
| Read-only data | 4294971392 | 16 | 16 | R |
| Initialized data and BSS | 4294975488 | 16 | 8208 | RW |

Entry is 4294967296; rounded segment storage totals five pages. The fixture is
13648 bytes. The [host result](elf-final/aarch64/elf-host/result.json) requires
all five stages: fixture compilation, readelf execution, independent reference
validation, actual-parser host compilation and strict successful execution.

The host client rejects 32 targeted malformed mutations, exercises all 13649
prefixes, and runs 4096 deterministic random/mutated inputs without a panic.
Every prefix lacking a required header/LOAD byte must fail; every prefix that
retains those bytes must produce the same plan, even if ignored section data
is absent. A valid fixture padded to the 1 MiB limit succeeds; one additional
byte fails. Accepted mutations must satisfy independent range, permission,
borrow and accounting invariants. A host buffer simulation checks proposed
file/BSS contents only; it is not evidence of a kernel ELF loader.

## Guest, compiler and regression coverage

Two new safe guest cases cover valid offset-within-page segments, BSS,
read-only/RW/RX permissions, ignored section fields, zero/one/large alignments,
maximum program/load counts and full page budget. Malformed fixtures cover
headers, types, truncated and overflowing ranges, count limits, zero memory,
permission bits/W^X, descending/overlapping loads, stack declarations and
entry alignment, nonexecutable/BSS entries and partial instruction bounds.

The [eleven actual-library compiler clients](elf-final/aarch64/elf-types/result.json)
also pass on x86. Three positive clients use the plan, copy a validated segment
and retain file bytes within the input lifetime. Negative clients reject field
forging/mutation, source lifetime escape, mutation while an image remains live,
and mutation through immutable file bytes. These compile the actual public API.

Final ARM and x86 suites pass 41 baseline cases, all prior scheduling, allocator,
mapping, monitor and negative profiles, and 81 host harness unit tests. Six new
harness unit tests reject corrupted reference/report metadata, bytes, BSS,
permissions and inventory. All 58 result-bearing JSON files in the final
archive report success (48 ARM, 10 x86). ARM still exercises both RAM sizes and
two-CPU scheduling/allocation followed by the same-guest baseline.

## Review and observed corrections

[Independent AI review](../reviews/0021-arm-first-userspace.md) approved stage A
before implementation and found no remaining source blocker. A proposed
alignment-negative fixture was corrected before execution because its original
128 KiB alignment was valid at the selected address; the actual test uses a
mismatched 8 GiB alignment. Review also strengthened the host's truncated-input
expectation so it cannot silently reject complete LOAD data with missing
section-table tails.

The [first build](elf-first-build.log), [first guest](elf-first-boot/result.json)
and [preliminary real-ELF check](elf-first-host/result.json) passed. The
[first full suite](elf-full-suite-attempt1.log) stopped at two redundant-reference
Clippy warnings in test comparisons. Removing those references preserved the
assertions; the complete second suite passed. Failures and preliminary evidence
are retained. This is AI review, not human approval or a full soundness audit.

The [Phase 1 audit](phase1-audit.md) records scoped local evidence and the open
matching-CI gate. Protected address spaces, EL0 stack/trap entry, safe copy,
versioned native calls/handles, reclamation and initramfs remain required.
Design 0021 runtime stages B–D remain proposed and unapproved. No public push
or new remote CI result is claimed.
