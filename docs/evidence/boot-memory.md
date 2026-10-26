# ARM boot-memory verification

Executed 2026-10-26 UTC (2026-10-25 local) on parent `224571b` plus this change.
Tested source hashes are in [boot-memory-final/source-sha256.txt](boot-memory-final/source-sha256.txt).
Command: `make test-all`, using the pinned Docker environment, Rust 1.99.0,
QEMU 7.2.22, one guest CPU and TCG. The complete final log is
[boot-memory-suite-final.log](boot-memory-suite-final.log).

Both architectures passed formatting, Clippy with warnings denied, repository
and safety checks, all 12 host harness tests, all 16 required guest cases, five
capability compiler probes, CPU rejection and interactive monitor checks.
ARM monitor boots at both 128 MiB and 64 MiB reported exactly the configured RAM
amount from the loaded DTB. Raw commands, ELF hashes, serial logs and result
JSON are in [boot-memory-final/](boot-memory-final/).

The two new guest cases exercise valid RAM/reservations, multiple reg tuples,
exact 16/32 range capacities, every truncation of a valid blob, deterministic
single-byte mutations, and explicit malformed header/tree/range/reservation
fixtures. Mutation acceptance is checked for map invariants; this is a bounded
fixture corpus, not exhaustive fuzzing or proof of parser correctness.

## Failures and corrections

1. QEMU's generated DTB is padded to 1 MiB. The original ELF began 512 KiB above
   RAM and the loader omitted the blob. A stopped guest read showed zeros at
   RAM origin. Moving the ARM ELF to `0x40200000` reserves 2 MiB without changing
   the 16 MiB mapped window or 64 KiB stack. Loaded header reads then showed FDT
   magic and the expected size.
2. The first relocated guest explicitly rejected the DTB. Inspection found 15
   properties with nonzero alignment padding in QEMU's output. After independent
   review, the parser now ignores only bounded property-padding bytes. Node
   padding, semantic lengths and every other check remain intact. This deviation
   from the format specification is recorded in the design/review.
3. The next guest parsed RAM, then faulted inside the stack guard during the
   malformed test prologue. QEMU reported FAR `0x4021fc10`, inside guard page
   `0x4021f000..0x40220000`, and ELR `0x4020f28c`. Disassembly showed a large
   fixture frame. Splitting fixtures into non-inlined helpers preserved all
   cases and the existing stack size. The final malformed dispatcher reserves
   only 16 bytes before calling its bounded helpers. This is observed release
   code, not a general future stack-depth proof.
4. Initial Clippy failures were corrected before runtime verification.

Failures and investigation commands are retained in
[boot-memory-investigation/](boot-memory-investigation/), including the original
compressed DTB and hash, loaded-memory observations, exception trace and
before/after disassembly. Earlier build/test logs remain beside this record.

Independent AI design and source review identified no remaining defect;
this is not human approval or a complete unsafe audit. Discovery returns
metadata only: no frame allocator, heap, new mappings, userspace or SMP exists.
No x86 memory-discovery or Linux compatibility claim is made. Remote CI has
not executed this change.
