# Owned ARM address spaces

Verified locally on 2026-10-26, based on parent commit `85985f4` and the source
identities archived with this milestone. This implements the owned-loading and
installed-translation subset of Phase 2, not EL0 program execution.

[Design 0022](../design/0022-arm-owned-address-spaces.md) and its
[independent AI review](../reviews/0022-arm-owned-address-spaces.md) define the
ownership and switching contract. This is not human approval or a complete
soundness audit. No Linux implementation source was used.

## Implemented behavior

A privately constructed, CPU0-affine Factory loads a validated Image into owned
frames. Each space owns twelve page-table frames, its ELF pages, and sixteen
stack pages. The real five-page Rust ELF therefore requires 33 frames; the
maximum 256-page image requires 284. Loading copies file bytes and zero-fills
BSS, padding and stack storage. Startup and stack-guard pages remain unmapped.

Private page tables preserve privileged kernel/device mappings, omit the owned
kernel arena, and map user pages with R/RW/RX permissions. Executable user
backing has a read-only, nonexecutable identity alias under the private root.
A bounded assembly-only operation installs the root, performs ARM AT queries,
then restores the original root and PAR with full TLB invalidation. Rust never
runs under that temporary root. Actual buffer addresses are checked for common
writable identity mappings; optimizer placement is not assumed.

Ordinary Drop releases all inactive owners. Budget and allocation failures
roll back partially built spaces. Forget deliberately retains frames. Public
inspection returns copies; it cannot expose mutable backing or page-table owners.

## Executed checks

The [first QEMU run](space-first-qemu/result.json) passed three profiles:
one CPU at 64 and 128 MiB, and two CPUs at 128 MiB with CPU1 kept OFF. Each runs
five cases and then all 41 baseline cases in the same guest:

- Real linked ELF loading, complete file/BSS/padding/stack byte comparison,
  exact page descriptors, 21 user backing pages and ten root-switch batches.
- Every failing budget prefix 0–32 returns all frames.
- Real allocator pressure leaves eight frames; partial construction fails and
  returns those eight, then releasing the retained Block restores the baseline.
- Unaligned cross-page input and the maximum 284-frame space load and release.
- Forget retains 33 frames; a following load uses different root/user frames,
  and its Drop leaves exactly the forgotten allocation reserved.

Each fixture run performs 71 queries twice. Every query records PAR before,
during and after installation. Checks compare physical address plus page offset,
EL0 read/write permission, RX identity write denial, kernel/device denial,
guards, holes and the absent owned arena. A live mapped-page borrow survives the
operation and retains its bytes afterward. Oversized query batches are rejected;
a copied read crossing into an absent page leaves its output unchanged.

These AT observations establish data translations and read/write permissions.
They do not establish instruction execution, ordinary-load PAN enforcement,
EL0 exception containment, or cross-CPU shootdown correctness. CPU1 OFF is checked
through PSCI before each installation; traces contain no CPU1 exception events.

Sixteen actual-module compiler clients pass, including rejection of forged
construction, owner extraction, cloning, Send/Sync, use after Drop and escape of
the Pool lifetime. Nine harness mutation tests reject missing/duplicated records,
wrong PAR results, accounting errors and unexpected traces/status. The embedded
ELF reproduces byte-for-byte with the pinned Rust toolchain, independently
checked with readelf and the existing admission suite.

The complete `make test-all` run exited zero. The [final archive](space-final/verification-summary.json)
contains 56 ARM and 10 x86 result-bearing JSON records, all passing, including
90 host harness cases and all prior boot, scheduling, SMP and failure profiles.
[Source hashes](space-final/source-sha256.txt), the exact reviewed ELF and
compressed assembly are preserved beside the raw logs. These are local results;
the formal matching-CI gate remains open.

## Commands and corrections

Builds and guests use `tools/dev.sh`, Rust 1.99.0, QEMU 7.2.22, two build CPUs,
and the existing container limits. Commands used:

```sh
bash tools/dev.sh cargo rustc --locked --offline -p hm-core --release \
  --features space-tests --target aarch64-unknown-none \
  --target-dir target/aarch64/space-test -- --emit=asm
bash tools/dev.sh python3 tools/test_space.py \
  --kernel target/aarch64/space-test/aarch64-unknown-none/release/hematite \
  --output docs/evidence/space-first-qemu
make test-all
```

The initial compiler wrapper rejected the PAN mnemonic on the baseline target;
using the existing encoded register name corrected it. An independent compiled
stack review caught a candidate exceeding 64 KiB before any guest execution.
Splitting test cases into separate non-inlined helpers reduced the largest new
path to 54,720 bytes. The final rebuild exactly matches the reviewed executable
and assembly. See [stack evidence](space-stack-review.md), the retained oversized
assembly, and first/second/final build logs. Clippy's initial precedence and
helper-argument warnings are also retained; the corrected check passes.

Protected EL0 entry/return, instruction-cache synchronization, native calls,
process copy, lifecycle containment and initramfs remain open. No remote push or
new CI result is claimed.
