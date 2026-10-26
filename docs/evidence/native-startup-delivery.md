# Owned startup input delivered to ARM programs

Implementation: `fb62076`.

Stage B of [design 0027](../design/0027-native-startup-input.md) connects the
verified input codec to protected native execution. `make run` now accepts
`run bin/echo hello from ARM`; the application reads its own startup mapping and
writes the copied payload. The first delimiter separates the archive path from
opaque text. Later spaces are preserved; a trailing delimiter supplies explicitly
empty input. Existing no-input launches retain their original zero-tail contract.

Core preparation copies directly into an owned startup frame before publishing
its read-only, nonexecutable mapping. No source or Input borrow survives in the
returned Process. The persistent issuer, frame budgets, console grant, deadline,
call limits and teardown rules remain in force. No new page or ambient authority
is granted. [Independent AI review](../reviews/0027-native-startup-input.md)
covers the source, wire layout, artifact checks, compiled stacks and actual traces.

## Actual verification

The [full local summary](startup-input-stage-b/final/verification-summary.json),
[raw suite log](startup-input-stage-b/final-test-all.log),
[source identities](startup-input-stage-b/final/source-sha256.txt) and
[22 reviewed kernel identities](startup-input-stage-b/third-candidates/inputs.json)
bind these results. The [design snapshot](startup-input-stage-b/design-at-full-test.md)
preserves the specification hash used during testing, before its status was
updated to verified:

- Two hardware-input profiles, one CPU at 64/128 MiB, each complete 24 processes.
  They distinguish absent/empty input, verify one-byte/255-byte/all-256-byte
  payloads, and check every startup-page byte independently in EL0 and through a
  bounded inactive observer. Original source mutation and ended Input scopes
  precede execution. Repeated absent launches catch stale payload bytes.
- Each profile verifies actual RO-write and NX-execution faults at startup+88,
  exact faulting instructions and saved state, two deadline terminations,
  54 SVCs and 60 user slices. Generations progress 1–24; budget/exhaustion failures
  consume 25/26 and a prepared/dropped process proves generation 27. Free frames
  return exactly to the common baseline after each run and rollback.
- Four ordinary monitor profiles complete 22 launches: ten each at 64/128 MiB,
  one fresh hello, and one fresh input-first echo. Tests cover empty input,
  preserved spaces, a maximum-length interactive command, legacy hello/info,
  fault/deadline recovery and interleaved kernel diagnostics. Main-profile
  generations reach ten, with 21 SVCs and constant free frames after each launch.
- Eight host policy cases exercise the actual command module through an effect
  recorder; actual Platform wiring is separately compiled and tested in guests.
  Twenty-six process compiler clients verify source/Input independence, Pool
  lifetimes, typed input admission and the private observer boundary.
- The complete ARM/x86 suite retains the existing 198 process runs and all
  memory/scheduling/SMP gates. There are 164 Python harness cases per target.
  x86 explicitly reports input launch unsupported and recovers its prompt.

GNU cpio independently extracts all payloads. Exact read-only archive embedding
checks reject individual ELF fallbacks. The normal archive contains five programs;
the separate test archive has eight probes and uses preserved named symbols with
debug metadata stripped. Probe artifact checks bind SVCs, NZCV clearing and the
three instructions constructing the fixed startup target. Actual full traces
supply separate hardware evidence.

## Failures and corrections

The [stack review](startup-input-stage-b/stack-review.md) preserves the rejected
76,176-byte test chain. It was never booted. Merging redundant owner-return
helpers retained all lifetime/readback checks and reduced the new profile below
the 64 KiB bound. The accepted larger baseline is 62,448 bytes including helper
allowance; normal launch is 64,128 bytes, leaving 1,408 bytes of headroom.

Other preserved failures are a standalone-feature missing `arm_vectors`
declaration (masked by all-features compilation), a formatting-separated SAFETY
comment, and two copied host mutation cases referring to an obsolete test name.
The cfg, adjacent comment and test names were corrected; the gates were not
relaxed. Review also corrected the proposed NX branch-state expectation to
BTYPE 1 before implementation, matching the existing pinned-QEMU evidence.

Builds and execution use the pinned `tools/dev.sh` container. The recorded
commands include fixture generation/reproduction, all-profile `cargo rustc
--emit=link,asm`, `check_process_types.py`, `test_startup_delivery.py`,
`test_launcher.py`, and finally `make test-all`. Expanded commands and raw logs
are retained alongside the results. The final artifact hashes are recorded in
the summary and compressed kernel ELFs in the final archive.

This verifies one-time native startup data delivery on pinned ARM QEMU. The
interactive line limit remains 64 printable ASCII bytes; typed callers can
supply all 256 opaque bytes. Console reads, argv/environment, spawn, IPC,
concurrent userspace, storage/networking and Linux binaries remain unimplemented.
The earlier QEMU architectural coverage qualifications remain applicable. No
performance advantage, physical-hardware result, human audit or new remote CI
is claimed; formal phase closure remains CI-gated.
