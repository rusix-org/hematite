# Native execution from a checked initramfs

2026-10-26, implementation `f3ca143`, based on parent commit `f355478`. This connects the checked newc
archive to the already verified native process runtime under
[design 0025](../design/0025-archive-execution.md). Independent
[source and verification review](../reviews/0025-archive-execution.md) and
[compiled-stack review](archive-process-stack-review.md) preceded guest execution.
The [complete ARM/x86 suite](archive-process-final/verification-summary.json)
passes, with [raw log](archive-process-final-test-all.log) and
[implementation identities](archive-process-final/source-sha256.txt).

Every ordinary run and preparation check now uses `boot_image::admit`: validate
the whole archive, look up the exact regular-file path, then admit its ELF.
Only the borrowed ELF plan escapes the temporary archive plan. Owned loading
copies its bytes before execution. The process kernel embeds one archive;
standalone ELF files remain build/reference inputs and are not fallback paths.
No Executor, page-table, trap, timer, syscall or teardown mechanism changed.

## Actual evidence

`make test-all` exited zero. The archive contains 190 passing result-bearing ARM
records and thirteen x86 records, including subprocess checks. All 131 Python
harness tests pass per target. The eight composition host tests and six compiler
clients per target pass, as do the existing native-policy/archive/ELF suites,
198 process runs, baseline, allocator, scheduling/SMP, negative and monitor gates.
The final process ELF is byte-identical to the independently reviewed candidate.
See [final process profiles](archive-process-final/aarch64/process/result.json),
[host tests](archive-process-final/aarch64/boot-image-host/result.json),
[ARM clients](archive-process-final/aarch64/boot-image-types/result.json) and
[x86 clients](archive-process-final/x86_64/boot-image-types/result.json).


- [First three QEMU profiles](archive-process-first-qemu/result.json) pass all
  33 programs twice each: 64MiB/one CPU, 128MiB/one CPU and 128MiB/two CPUs.
  Existing console/exit, malformed calls, capabilities, register state,
  real protection faults, call limits, deadline and reclamation checks remain.
  Every guest also passes the 41-case baseline after process teardown.
- Four admission records precede process entry: malformed suffix, missing path,
  directory selection and invalid ELF. Each has its distinct error class and
  identical before/after frame count of 2984, also matching the execution
  baseline. No additional exception or PSCI transaction is admitted.
- [Artifact binding](archive-process-first-artifact/result.json) independently
  checks all ELF symbols/opcodes/LOAD metadata and actual kernel fault targets.
  GNU cpio lists and extracts all 33 ELF payloads and the banner exactly. The
  1,326,592-byte archive occurs once in a readable, nonwritable file-backed
  kernel LOAD at file offset 380896. No full standalone ELF appears outside
  that span. The archive hash is
  `f02b5d17cadcc700adcdddd879ce51c4aba68121e198a484018c932f5524c75f`.
- [Eight composition tests](archive-process-first-host/result.json) require
  complete admission before exact lookup, all error variants, real native ELF
  bytes, malformed suffix rejection despite an early valid ELF, and temporary
  path lifetimes. [Six ARM compiler clients](archive-process-first-types/result.json)
  test the actual library's input lifetime and private Image boundary.
- Twenty-one process-harness tests include wrong admission counts/order/errors,
  changed frame counts, missing records, archive splits/duplicates across LOADs,
  writable placement, overlapping LOAD intervals and raw ELF fallback copies.
  Five new host-harness tests enforce the exact eight-test inventory and reject
  execution failures, skips, truncation and oversized evidence.

## Exact binary and fixed point

Embedding the archive moved kernel BSS from `0x4038f000` to `0x40393000`.
The seed candidate was compiled but never booted with stale fault targets.
The [fixed-point ledger](archive-process-fixed-point.json) records seed and
iteration1 hashes. Regenerating all 33 ELF inputs with `0x40393000`, then the
small parser archive and full process archive, produced a stable kernel whose
actual BSS matches every reference. No address check was loosened.

The independently reviewed and first-tested executable SHA256 is
`9742614856107fe6772c0d882a82ae39a6e6f7e1217530a99764463d96af1d3b`.
Its [preserved compiler assembly](archive-process-stable-stack.s.gz) hashes to
`87ed64560360c361217be3e7abf8e32a8e8389a80c018e0a63a26d3f1de0253f`.
The largest inspected kernel stack path remains 61,712 of 65,536 bytes;
archive parsing returns before the deeper loading/execution paths. This is a
static bound for the reviewed binary, not a future-feature guarantee.

Exact update/build commands used for the stable iteration:

```sh
python3 tools/build_process_fixtures.py --update --kernel-data 0x40393000 \
  --output docs/evidence/archive-process-iteration1-elf
python3 tools/build_initramfs.py --update \
  --output docs/evidence/archive-process-iteration1-small
python3 tools/build_initramfs.py --process --update \
  --output docs/evidence/archive-process-iteration1-archive
cargo rustc --locked --offline -p hm-core --release --features process-tests \
  --target aarch64-unknown-none --target-dir target/aarch64/process-test -- --emit=link,asm
```

These ran through `tools/dev.sh` in the pinned container. Routine verification
uses non-updating fixture reproduction and `make test-all`; expanded QEMU,
GNU cpio and compiler commands and raw outputs are archived. The seed mismatch
is preserved as evidence, not a passing runtime result.

This establishes archive-backed native execution on pinned ARM QEMU. The normal
boot still enters the kernel monitor; it is not a default userspace init service
or shell. CPU1 stays OFF during native execution. No Linux ABI, external
bootloader-supplied archive, physical hardware or performance claim is made.
The previous QEMU DAIF-syndrome and DIT-coverage qualifications still apply.
No new remote CI or publication is claimed.

## Phase 2 requirement audit

This is the current native scope in AGENTS.md and ADRs 0005/0006, not the
superseded requirement to match arbitrary Linux binaries.

| Requirement | Authoritative local evidence and scope |
| --- | --- |
| User spaces and static ELF loader | Owned-space profiles, independent ELF metadata checks and actual 33-program execution; CPU0-only static AArch64 profile |
| Protected entry/return and safe user copying | Strict lower-EL traces, invalid-pointer/crossing tests, RX/RW/kernel/device faults, register checks and independently reviewed core mechanisms |
| Initramfs | Complete newc admission, independent GNU payload checks, exact embedded archive binding, all execution/preparation paths using archive lookup |
| Versioned native startup, handles, console and exit | Existing wire-version tests, startup/register fixture, scoped grant/error/limit tests, actual output and exit results; console output only, no input syscall |
| Isolation and reclamation | Four preallocation failures, contained process faults and deadlines, preparation rollback/drop and exact repeated free counts in all three profiles |
| Linux hello-world | Unsupported and reported separately; no adapter or differential Linux claim |
| Phase gate in CI | Not established for this commit; all evidence here is local, with no new publication |

The local native functional experiment is verified within those scopes. Formal
phase closure remains CI-gated. Default userspace init, an interactive launcher,
concurrent userspace, service workloads, storage/networking and measured design
comparisons remain further work; the overall project objective is not complete.
