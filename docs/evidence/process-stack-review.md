# Native-process compiled stack review

2026-10-26, independent AI inspection; no guest execution by this reviewer.
Input: fourth build, [build log](process-fourth-build.log), emitted assembly
`target/aarch64/process-test/aarch64-unknown-none/release/deps/hematite-36e23ac93b7ecf9f.s`.

- Assembly SHA-256: `a905721f1e1b75e4d36e7c8b23562c09ac27684b804ae61978b7358c269d6939`.
- ELF SHA-256: `d8505cb2973854287b474e8d08175be76af9e9957e3e641d2a67d342c8269d52`.

**The static stack and linked-fixture gates pass for this exact candidate.**
Runtime exception behavior and first-trace parser approval remain separate.

## Method and bounds

Read emitted prologues, including repeated stack-probe loops and CFI offsets,
and follow simultaneously live call frames. Earlier first/second candidates
are preserved with their hashes; review 0023 records their close-to-limit paths
and the subsequent helper/result-borrowing changes. Final frame sizes agree
with the third candidate after lint and fixture-address corrections.

| Function | Bytes |
| --- | ---: |
| boot_main | 7440 |
| process_checks::run / checks | 16 / 752 |
| one / run_loaded | 18992 / 21056 |
| prepare_failure / check_prepare_failure | 18432 / 20848 |
| prepare_drop / drop_loaded | 18416 / 20768 |
| verify | 368 |
| Factory::load / Space::audit | 17280 / 48 |
| Executor::prepare / run / execute | 9056 / 9872 / 976 |

The ordinary execution path through `execute` uses **59,104 bytes**. A
conservative 640-byte allowance covers the inspected fixed console/formatter,
padding/output and terminal-helper chain: **59,744 bytes** total. Preparation
failure uses **56,592 bytes**, inactive prepare/drop **56,496**, and ordinary
loading including audit **44,528**. Formatting uses fixed scalar fields and
does not invoke user-supplied formatters. The allocator/mapping helpers and
cleanup paths fit within these bounds. Linked `memcpy`, `memset`, and `memcmp`
were inspected with `llvm-objdump --disassemble-symbols`; all are stackless
leaves. Panic handling exits without unwinding or recursive formatting.

The existing baseline's deepest inspected path remains boot_main7440 →
run_tests35072 → topology valid8320 → extra_valid4240 → parse6464 → property128
→ terminal slice/panic chain48 = **61,712 bytes**, leaving **3824 bytes** below
65,536. Actual ELF symbols place the coordinator guard at `0x403a0000` and
stack top at `0x403b1000`; the intervening guard is excluded from the 64KiB
usable stack. The boot call adds no separate coordinator frame.

Process entry switches to its distinct trusted 2048-byte frame below exception
stack top `0x403b6000`; it does not call Rust there. Unexpected nested exceptions
take the stack-independent terminal path. Masked coordinator execution does
not add asynchronous callback frames. These are static, scoped call-path
bounds, not stack-watermark measurements or guarantees for future features.

## Linked fixture and compiler evidence

All 33 canonical fixture hashes match the seventh fixture-build evidence.
Their absolute kernel-data symbols and manifest equal the actual kernel
`__bss_start=0x4038f000`; `__text_start=0x40200000`. Process vectors are aligned
at `0x40202800`. The separately decoded LOAD records and instructions remain
host-harness gates before execution.

Inspected all 33 seventh-build disassemblies for stack adjustments. Summing
every downward adjustment in the largest single ELF, even mutually exclusive
register and panic paths, gives **1168 bytes**. The register checker is a leaf;
its snapshot uses 1024 bytes. The intentional bad-SP fixture is excluded from
normal user-stack assumptions and performs no subsequent stack access.
Actual register-fixture instructions include DIT/SSBS reads into snapshot words
101/102 and three writes of the intended bits before SVC; resumed checks require
both zero. The fixed resume policy preserves NZCV and resets these fields.

The actual-module compiler evidence has four accepted and fifteen rejected
clients, with the intended diagnostic classes, and a successfully compiled
wrapper. Its recorded Rust/layout/assembly source hashes match current files.
This establishes the tested type boundaries; it is not runtime evidence.
