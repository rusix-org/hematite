# Ordinary ARM native launcher

The ordinary `make run` monitor now lists and launches four native programs
from its fully checked embedded archive. `bin/hello` writes from EL0; `bin/info`
reads the native startup contract and reports its console grant and budgets.
`demo/fault` attempts to write its RX code; `demo/loop` runs until its timer
deadline. Each returns control to the monitor with a readable outcome.

The implementation follows [design 0026](../design/0026-native-launcher.md) and
[independent AI review](../reviews/0026-native-launcher.md). One resident issuer
preserves handle generations across launches and interleaved monitor tests.
Output uses a sealed private terminal mode; no caller-selected callback runs
inside process execution. User copying, budgets and teardown retain the existing
process mechanism. Guest userspace semihosting is explicitly disabled.

## Verification

Implementation commit: `d69d87c` (tested source hashes archived below).

The [final summary](launcher-final/verification-summary.json) records the local
`make test-all` result. [Raw build/test output](launcher-final-test-all.log),
[ARM evidence](launcher-final/aarch64/), [x86 evidence](launcher-final/x86_64/),
[source hashes](launcher-final/source-sha256.txt) and the
[reviewed 21-profile inventory](launcher-third-candidates/inputs.json) bind the
claim. The final normal ARM ELF is
`4b8d02898264846846e19772bd0ec7a3159e468d5913f31e448822f3b99df04c`.

Three ordinary-monitor profiles exercise 11 completed launches: five each at
64/128 MiB and one fresh 128 MiB boot without a preceding test command.
The main profiles check all four applications, rejected selections, exact output,
prompt recovery, generations 1–5 and constant free frames after each launch.
Warm-up bootstrap diagnostics deliberately retain three frames once; the fresh
profile independently verifies launching does not require this warm-up.
In each main profile, strict exception traces distinguish six user SVCs, one
RX write fault and one user timer IRQ from the expected kernel diagnostics and timer preflight.

The suite retains the 66-run process matrix at all three configurations,
all ARM scheduling/memory/SMP checks and x86 regressions. There are 142 Python
harness cases per target, including 11 launcher rejection tests, and 21 actual
process compiler clients. GNU cpio extraction checks all four application bytes;
the [artifact gate](launcher-final-artifact/result.json) verifies their unique
read-only archive embedding. Full-suite reproduction uses the final source.

## Failures retained

- [Initial hygiene failure](launcher-first-build.log): missing adjacent SAFETY
  comments, corrected before acceptance.
- [First compiled stack](launcher-stack-review.md): a retained archive temporary
  made the normal launch chain exceed the 64 KiB stack. A separate admission
  function reduced the reviewed maximum to 63,584 bytes, leaving 1,952 bytes.
  No oversized candidate was booted; all 21 affected profiles were rechecked.
- [First full suite](launcher-first-full-suite.log): stale `info.elf` after source
  formatting failed deterministic reproduction before guests in that run.
  [Byte comparison](launcher-reproduction-comparison.json) identifies one
  read-only panic-line byte, 81→108; executable instructions were unchanged.
  Regeneration, renewed independent hash/stack review and a fresh complete suite
  precede acceptance. Earlier successful guests remain bound to their old hashes.

Commands were run through the pinned `tools/dev.sh` Docker environment:
`python3 tools/build_apps.py --update --output docs/evidence/launcher-final-apps`,
`python3 docs/evidence/launcher-build-final-stacks.py`,
`python3 docs/evidence/launcher-check-artifact-reviewed.py`, then `make test-all`
from the host. Expanded compiler and QEMU commands are preserved in the evidence.

This is one-CPU ARM QEMU native execution at 64/128 MiB. x86 launch remains
unsupported. Programs have console output only, with 256-byte writes, a
4,096-byte console budget, 64 calls and a 100 ms user deadline. Arguments,
input, spawn, concurrent userspace, storage, networking and Linux binaries
remain absent. No physical-hardware, performance, human soundness-audit or new
remote-CI claim is made. Formal phase closure remains CI-gated.
