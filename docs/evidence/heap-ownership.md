# Fallible heap verification

Executed 2026-10-26 UTC (2026-10-25 local), parent `6cbfac2` plus this change.
Command: `make test-all`. The pinned environment remains Rust 1.99.0 and QEMU
7.2.22, with TCG and one guest CPU. Final log:
[heap-suite-final.log](heap-suite-final.log). Exact tested source hashes,
commands, ELF hashes, raw serial output and compiler diagnostics are in
[heap-final/](heap-final/). Earlier passing runs are retained in heap-suite.log
and heap-suite-2.log; subsequent runs add review-requested coverage.

Both architectures passed formatting, Clippy with warnings denied, repository
and safety checks, all 12 host harness tests, all 21 guest cases, five capability
compiler probes and 22 frame/heap compiler probes. ARM 64 MiB and 128 MiB
monitor profiles and CPU-rejection profiles passed. All expected results are
required; missing cases, unexpected diagnostic categories or timeouts fail.

The ARM heap cases exercise actual RAM through safe policy interfaces:

- 8193-byte/8192-aligned and 4097-byte allocations coexist with a live Frame,
  charge exactly their whole-page sizes, preserve neighbouring contents and
  return zeroed storage after dirty reuse.
- Invalid geometry and valid-but-unavailable size/alignment requests preserve
  accounting. The latter include a 17 MiB request and alignment of 1 TiB.
- A compact private ledger exhausts free pages without large stack arrays.
  Releasing only even ledger indices creates free capacity without a two-page
  run; a 4097-byte request fails with an unchanged bitmap. Cleanup releases
  only ledger-owned indices. A live typed value remains readable/mutable.
- Typed tests cover mutation, borrowed payloads, an over-aligned zero-sized
  value with observable destruction, exact destructor counts and reentrant
  allocation while the original owner's page remains charged.
- Actual exhaustion returns a non-Copy value intact without early destruction;
  explicitly dropping the returned value runs its destructor once.
- One typed owner is forgotten once per pool, suppressing its destructor and
  retaining its page. Together with the earlier frame leak diagnostic this
  permanently charges two pages per boot. Repeated diagnostics preserve all
  remaining capacity. The forgotten payload is never read or destroyed again.

The expanded compiler probes compile the actual core module and require valid
frame and heap clients. Negative clients cannot clone/forge owners, inspect a
typed owner's backing bytes, escape pool/payload lifetimes, overlap mutable
borrows, drop borrowed owners, or transfer typed owners through Send/Sync.
They establish the safe-client type boundary, not physical memory validity.

The x86 cases explicitly require Unsupported and preserve returned typed
values. They do not establish a physical x86 heap. Shared planning/parser and
compiler tests still execute on both targets.

Independent AI design/source review found no remaining defect; this is not a
human audit or complete soundness proof. The heap uses whole-page charges and
contiguous runs; it is not efficient small-object allocation. No GlobalAlloc,
standard alloc collections, concurrent allocation, new mappings, DMA or user
address spaces are implemented. Remote CI has not run this change, and no
service-workload or performance-improvement claim is made.
