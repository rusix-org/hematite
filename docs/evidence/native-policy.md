# Native call policy verification

2026-10-26, based on parent commit `1359ec3` and the archived
[source identities](native-policy-final/source-sha256.txt). This milestone
implements the safe policy half of the experimental native interface. It does
not execute EL0 or validate the future core user-memory adapter.

The [wire contract](../design/0023-native-wire.md) defines explicit startup
bytes and SVC register arguments. `hm_kernel::native` uses only safe Rust:
private session grants, nonwrapping generation issuance, bounded synchronous
console copying, explicit error results, exit, call limits and byte accounting.
The real executor must retain its issuer and supply its own private Host;
creating an independent policy value confers no hardware authority.

Console output is limited to 256 bytes per call and 4096 successful bytes per
session. Every SVC attempt consumes one of 64 attempts, including invalid calls.
The 64th returning call records its response and immediately stores CallLimit
and revokes its grant. A Reply is not permission to resume hardware execution;
the executor must inspect termination and its absolute deadline before ERET.
A valid 64th exit preserves the requested exit status. Stopped sessions cannot
perform further host operations. User-memory copy completes before output;
a failed copy cannot emit a partial buffer or consume byte budget.

## Actual evidence

`make test-all` exited zero. The [summary](native-policy-final/verification-summary.json)
contains 58 ARM and 11 x86 result-bearing JSON records, all passing:

- [Ten native host tests](native-policy-final/aarch64/native-host/result.json)
  verify exact startup encoding, absent/stale/foreign/wrong-slot handles,
  generation retirement, validation precedence, zero/maximum/oversized writes,
  failing copies and invalid ranges, independent byte/call exhaustion, invalid
  exit, terminal idempotence and the 64th valid exit.
- Twelve actual-library compiler clients pass on each architecture: three
  positive clients and nine expected rejections for field forging, cloning,
  resetting generations/counters/budgets, granting authority or reviving a
  stopped session. The compiler must produce the expected diagnostic class.
- Seven new Python harness cases mutate inventories, duplicates, order,
  summaries, skips, filtering, encoding, truncation, process failures, timeouts
  and artifact limits. All 97 Python harness cases pass per architecture.
- All existing 41-case baselines, owned-space profiles, scheduling, SMP,
  allocator, monitor and negative profiles pass. The space test executable is
  byte-identical to the independently stack-reviewed `cc6357bf...` executable
  archived in the previous milestone. No new core instructions are introduced.

The strict host tool records the Cargo command, source hashes, stdout/stderr,
exit status and timeout result. It rejects missing tests, ignored/filtered tests
and an otherwise successful process whose inventory is incomplete. Host tests
use a bounded mock memory/console adapter; only future EL0 execution can prove
the actual user-pointer and hardware-output path.

Commands:

```sh
bash tools/dev.sh cargo test --locked --offline -p hm-kernel --lib \
  --target x86_64-unknown-linux-gnu --target-dir target/host-native -- --test-threads=1
bash tools/dev.sh cargo clippy -p hm-kernel --tests --locked --offline \
  --target x86_64-unknown-linux-gnu --target-dir target/host-native -- -D warnings
bash tools/dev.sh python3 tools/check_native.py \
  --output docs/evidence/native-policy-first-host
make test-all
```

Initial and refined test logs are retained. Independent AI review caught the
ambiguous Resume name and delayed terminal-state recording at the call limit;
the Reply/termination split and immediate revocation resolve it. Review is not
human approval or a complete soundness audit.

The independently approved process mechanism and verification contracts cover
privilege transitions, dedicated vectors, timer ownership, cache visibility,
register preservation, faults and reclamation. They remain implementation work.
Initramfs and Linux compatibility also remain open; no new remote CI or public
push is claimed.
