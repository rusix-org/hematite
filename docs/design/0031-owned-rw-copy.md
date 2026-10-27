# Bounded writes into inactive owned user memory

Status: **implemented and locally verified for inactive owned memory**; see
[review](../reviews/0031-owned-rw-copy.md) and
[evidence](../evidence/rw-copy.md). Interactive integration remains separate.
This is the copy mechanism needed by interactive READ, not approval of the
interactive coordinator, UART ownership or idle mechanism. It extends the owned
Space contract in [0022](0022-arm-owned-address-spaces.md) and protected execution
in [0023](0023-arm-native-process.md); no new address translation or raw user
pointer dereference is introduced.

## Private API and result contract

In `user_space.rs`, propose these crate-private operations:

```rust
pub(super) fn validate_rw(&self, address: u64, length: usize) -> Result<(), Error>;
pub(super) fn copy_into_rw(&mut self, address: u64, input: &[u8]) -> Result<(), Error>;
```

Both first call the existing `context()`, then reject lengths outside 1..=256
and checked `address + length` overflow. Context errors remain `Context`;
invalid range, absent mapping or non-RW permission returns `Bounds`; inconsistent
owner/table/identity metadata returns `Corrupt`. Validate pages in ascending VA
order, with permission/range rejection before descriptor consistency checks on
each page. All returned errors leave every owned byte and descriptor unchanged.
There is no allocation, issuance, scheduling, UART, callback or public write API.

`validate_rw` returns no capability, reference or cached validity token. A later
copy always validates again; a pending READ retains only its address/capacity,
not a page borrow. The eventual coordinator must separately validate input
capability, lifecycle, cancellation and budgets. These methods confer none of
that authority.

## Concrete two-pass implementation

A private helper constructs a bounded local plan of at most two scalar segments
(slot, offset, length), never a Frame reference. The plan remains within the
method invocation; do not expose it to callers or retain it in Process.

Pass one validates the complete range. The existing 4 KiB pages and maximum
256-byte length imply at most two pages, including unaligned requests. For each:

- Resolve `slot(va)` within the existing 512-slot user window. Require both a
  retained `pages[slot]` owner and `permissions[slot] == ReadWrite`.
- Verify the actual owned user-table path: root table 0 entry 4 equals
  `table_address(10)|3`, and table 10 entry 0 equals `table_address(11)|3`.
  Missing path/table ownership or descriptor mismatch is `Corrupt`.
- Read table 11's relevant L3 entry with the existing `word` helper. Require
  exact equality to the retained Frame physical address OR
  `attributes(ReadWrite)` (currently `0x0060_0000_0000_0743`). Do not accept
  merely a writable AP bit or an independently supplied physical address.
- Require the Frame physical address to pass `ram_index` and its immutable boot
  identity leaf to equal `physical | RW` (currently
  `RW = 0x0060_0000_0000_0703`). Validate the table Frames used above against
  the same boot identity invariant before reading their bytes. This is a local
  path check, not an assertion that the entire Space was freshly audited.
- Record bounded offsets/counts and verify their total equals the input length.
  No writes occur while constructing the plan, including on its second page.

Pass two copies from the caller's immutable bytes into each retained owner's
`Frame::bytes_mut()` subrange. It follows only the just-validated slots and
bounds. It has no new fallible admission/lookup after its first store: resolve
needed ownership/bounds during pass one, then use the established private
invariants for bounded indexing in pass two. An implementation must show these
invariants concretely rather than returning an error after a partial copy.
No raw user VA is dereferenced; no descriptor changes, TLB operations or
instruction-cache maintenance are needed for these existing non-executable RW
backings. No page-sized temporary or additional owned Frame is introduced.

## Why validation remains valid until the stores

Actual `context()` verifies CPU0, boot TTBR0, zero TTBR1, exact admitted TCR,
MAIR and SCTLR, masked DAIF and PAN. It does **not** acquire BUSY or establish
peer-OFF; this proposal adds no invented claim that it does. Factory's existing
unsafe bootstrap proof binds the real Pool, immutable boot mappings and absence
of external CPU starts/DMA. Space and Process retain their private ownership and
`!Send`/`!Sync` markers. Executing a Process is sealed and synchronous: Rust
receives it back only after boot-root restoration and TLBI. A mutable borrow of
that inactive owner cannot overlap its execution/probe or a second safe writer.

The eventual input coordinator calls these operations only with its inactive
Process and execution lease retained. Existing context/boot authority plus
exclusive ownership, masked interrupts and no callback during either pass
exclude permission/owner changes between validation and copy. User mappings
remain present but inactive; no mutable Frame or backing slice escapes the
method. This is a narrow internal modification of owned RW data, not a public
safe mapping writer. Kernel failure/power loss is not promised transactional;
all ordinary returned errors are byte-atomic. Callers must not interpret a
successful copy as commitment of a READ reply or FIFO removal.

## Required verification before integration

Add a separately reviewed safe QEMU test driver using the real loader, boot
Factory and owned pages; no arbitrary descriptor-mutation hook. Exercise lengths
1, 255 and 256, page-end exact fit, and two adjacent RW pages (including stack
pages) with a 256-byte crossing. Read back complete expected data and surrounding
sentinels via `read_into`, checking exact frame counts before/after reclamation.

Reject lengths 0 and 257, overflow near `u64::MAX`, below/above the user window,
unmapped slots, startup RO, ELF RO/RX, and the stack guard. Crucially test a
mapped RW prefix followed by an absent/RO/RX page: require failure with the
entire original prefix unchanged, not merely a request starting in a bad page.
Use genuine ELF segment arrangements for these boundary cases. Keep startup
and executable bytes unchanged. Repeat copies and validation-before-copy to
show no retained token is necessary; verify a later real EL0 program observes
its copied RW bytes and still faults when attempting startup/RX writes.

Compiler clients must reject external access to the methods and Frame owners,
mutable-byte escape, cross-thread Space/Process transfer and owner lifetime
escape. Production artifacts must omit test-only adapters; the two private
methods may remain as the actual READ implementation, but no public/general
Process writer or arbitrary-address test hook is added. Descriptor corruption
rejection requires source review of exact equality checks; do not claim a
hardware corruption test without a separately approved fault-injection seam.
Measure actual linked stack chains and inspect that whole-range validation
precedes every backing store. Preserve existing process/space/ownership tests.
No present document claims these tests have run or that interactive READ exists.

## Dedicated inactive-copy profile (proposed exact adapter scope)

Use a new `rw-copy-tests = ["boot-tests"]` feature and safe
`rw_copy_checks.rs` with `#![forbid(unsafe_code)]`, rather than modifying
`space-tests` or its existing strict inventory. Its bootstrap wrapper follows
that profile's actual BootPlatform proof: CPU0, admitted one-CPU topology,
masked exceptions, immutable boot tables and real Pool/Factory. No Executor,
Session, new unsafe constructor or EL0 entry is needed. A private safe
`run(pool: &Pool, factory: &Factory<'_>, output: &mut dyn Write) -> bool`
receives that authority. Calls to the two private methods remain within core;
no production Process forwarding mutator is added for testing.

Create a separately linked, parse-only `fixtures/rw-copy.elf` using pinned
Rust/assembler/linker tooling and five PT_LOAD segments. With B=0x100000000
and P=4096, freeze these six complete file-backed pages:

| Slots | Permission | Purpose |
|---|---|---|
| 0 | RW | writable prefix followed by RX |
| 1 | RX | entry at B+P, retained instruction bytes |
| 2,3 | RW | adjacent writable pages; slot3 followed by RO |
| 4 | RO | read-only bytes |
| 6 | RW | writable prefix followed by unmapped slot7 |

Slots5 and7 are absent. `Image::parse` requires an aligned entry with four
file-backed bytes in an executable segment, not entry==B; entry B+P satisfies
that actual contract. Each mapped page has deterministic builder-verified initial bytes. The ELF has exactly
six image pages; the existing loader adds 12 tables and 16 stack pages: 34
Frames. Before each case, call the existing private
`install_startup(&header, 1, None)` with a fixed 64-byte canonical v1 startup
header (zero handle, flags zero, legacy limits), then require 35 Frames and
startup slot256 RO. This uses the real installer and allocator, not fabricated
Process or hardware authority. Confirm its header and zero tail using bounded
`read_into` chunks; no 4 KiB local scratch buffer. Keep no Image/Space return
copies on nested scheduling stacks; helpers must receive borrowed owners.

Freeze twelve cases, in this order, repeated twice. Each uses a fresh Space and
checks full reclamation to the same baseline after lexical owner drop:

1. `lengths`: successful lengths1/255/256 at slot2 offset32.
2. `page_end`: successful length256 at slot2 offset3840, ending exactly at P.
3. `cross_rw`: successful length256 at slot2 offset3968, splitting128/128.
4. `cross_stack`: same split between stack slots496/497.
5. `tail_ro`: reject length256 at slot3 offset3968; both prefix and RO unchanged.
6. `tail_rx`: reject length256 at slot0 offset3968; both prefix and RX unchanged.
7. `tail_absent`: reject length256 at slot6 offset3968 into absent slot7;
   all128 mapped prefix bytes unchanged.
8. `protected`: reject length1 at slots1,4,256; backing bytes unchanged.
9. `holes`: reject length1 at slots5,7,255,257,495 (including guard).
10. `bounds`: reject lengths0/257 at slot2; length2 at u64::MAX;
    length1 at B-1 and B+512P; length2 at slot511 offset4095 crossing the
    user-window end, with its mapped stack prefix unchanged.
11. `startup`: verify all4096 startup bytes against header plus zero tail,
    reject length256 at startup+64, verify the entire page unchanged.
12. `translations`: perform the fixed AT batch below, successful crossing copy
    as in `cross_rw`, then repeat the same batch and require identical results.

Every copy operation first calls `validate_rw` independently and checks its
result, then calls `copy_into_rw` and checks the same expected result. Successful
cases use all byte values0..255 as payload (truncated for smaller lengths),
compare exact copied bytes, and compare immediate mapped neighboring sentinels.
Rejected cases compare all mapped destination bytes against their pre-operation
values, plus complete protected pages in chunks where applicable. Snapshot
buffers are bounded at256 bytes; helpers may reuse them after each comparison.
Every operation checks unchanged Frame count and free count. `audit()` must pass
before and after each case. No write case changes table permissions.

The fixed 12-query AT batch is read/write for slot2, slot3, slot4, slot1,
slot256 and slot495. RW reads/writes succeed; RO/RX/startup reads succeed and
writes fault; guard reads/writes fault. Boot-before and boot-after user VAs fault.
Use existing `Space::probe` admission/restoration checks and its three PAR
observations; never invoke the output writer while alternate tables are active.
This proves translation permissions, not instruction execution or an EL0 READ.
The preceding eventual EL0-consumer requirement is deferred to interactive
integration: this dedicated profile deliberately has **no active EL0**.

Host artifacts: independently inspect ELF headers/segments/entry/page count,
reproduce bytes/hash, bind the sole embedded ELF span to a readable non-writable
file-backed kernel LOAD, and preserve linker/manifest/source hashes. Run the
profile on QEMU virt oneCPU64/128MiB only. Strict output uses `HMRWCOPY BEGIN
passes=2 cases=12`, then `CASE pass=<0|1> name=<above> before=<free>`, one matching
`PASS pass=... name=... frames=35 during=<before-35> after=<before>`, and a final
`HMRWCOPY END cases=24`. Print PASS only after every internal comparison and
owner drop succeeds; any failure prints FAIL and fails the guest. No missing,
duplicate, reordered, unknown or trailing success record is accepted. Inside `translations`, before its PASS, require stages0/1 in order. Each emits
`HMRWCOPY ROOT pass=<p> stage=<s> during=<hex> restored=<hex> controls=1`,
then exactly12 ordered records `HMRWCOPY PROBE pass=<p> stage=<s> index=<0..11>
va=<hex> access=<r|w> before=<hex> active=<hex> after=<hex>`. Hex fields use
`0x` plus lowercase digits; query order is the six slots listed above with read
then write. Check restored root against the linked boot symbol, private root
distinct/aligned, PAR fault bit and successful physical page against the
loader mapping evidence. Add `physical=<hex>` to each PROBE, copied from
`page(va)` or zero for guard; require repeated stages agree and successful PAR
PA equals it. This scalar evidence does not independently establish physical
ownership; retained-owner source checks and artifact binding remain necessary.
No bare PASS record substitutes for these comparisons or raw evidence.

Require compiler privacy clients using the actual core module to reject external
calls to both methods and owner/backing access, plus a positive safe adapter
client. Check default production modules contain no test adapter/general writer.
Fresh source review and emitted stack review precede guests; retained buffers,
Image, Space and probe result slots all count. Existing space/profile trace
inventories remain unchanged. This profile adds no UART IRQ, active-process
copy, generic corruption seam or synthetic hardware permission claim.


### Bootstrap, baseline and raw exit acceptance

The new feature uses the existing safe boot-tests route: run this copy profile
first; on failure exit failed immediately; only after its complete END run the
unchanged 41-case `hm_kernel::run_tests` baseline and exit with that result.
No timer preflight, process execution, SMP start or PSCI query is introduced by
the copy profile. All HMTEST records must follow the final HMRWCOPY END, with
exact existing baseline names/count/status accepted by `test_kernel.validate_run`.
The baseline's deliberate retained test allocations occur after copy reclamation
checks and do not change their common pre-baseline free count.

Freeze a 30-second deadline per oneCPU64/128MiB guest, bounded complete serial
and exception/GIC logs (the existing host MAX_LOG_BYTES bound), no launch error,
no timeout, successful QEMU return code and the baseline's successful exit
record. A valid prefix or a host-killed guest can never pass. Use the pinned
QEMU exception/GIC grammar already used by process tests, with privileged
semihosting and `userspace=off`. Require exactly one final CPU0 semihost event,
`Taking exception 16 [Semihosting call] on CPU 0`, whose final line is
`...handling as semihosting call 0x20`; no events may follow it.

Before that exit, admit only existing baseline CPU0 EL1→EL1 data/prefetch
aborts and timer IRQs, at `arm_vectors+0x200` and `arm_vectors+0x280`
respectively. Reuse the existing strict baseline trace sequence in full: exact fault counts
and order, GICC setup/restoration writes, and handler return pairing, not merely
event membership. Fault ELR/FAR/ESR and resume PC must match the actual linked
baseline `arm_fault_fixups` table; require matching EL1 returns. Timer evidence
must pass the existing strict timer validator for exactly16 baseline IRQs,
with GICC ACK/EOI ID30 and existing bounded timer-window PCs. No HVC, SVC,
undefined instruction, unexpected abort, other IRQ ID, other CPU, EL0 source,
EL0 target or return-to-EL0 is admitted. Ordinary EL1 exception returns from
baseline handlers remain required; this does not ban those legitimate ERETs.
AT probes generate no synchronous abort event: an unmapped AT yields PAR fault
status instead. UART strings alone cannot prove trace ordering across separate
log streams, so combine exact baseline event identities with serial ordering;
do not claim timestamp correlation that the captured logs do not provide.
