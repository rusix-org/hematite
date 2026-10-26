# Owned ARM address spaces and installed-translation diagnostics

Status: approved for Stage B implementation by independent
[review 0022](../reviews/0022-arm-owned-address-spaces.md), 2026-10-26.
Implementation, compiled-stack and runtime verification remain pending.
This makes Stage B of
[design 0021](0021-arm-first-userspace.md) concrete. It does not authorize EL0
entry, an ABI, user-memory copying, or the Stage C/D execution mechanisms.
The full isolated native-program objective and its exit tests remain required.

## Authority and ownership

A CPU0-affine `Factory<'pool>` borrows the actual boot `Pool`. Its private unsafe
constructor requires the verified BootPlatform, admitted topology, existing
39-bit boot translation profile, and immutable boot-root/device/RAM tables.
Those are stronger preconditions than `Pool::from_boot` alone. No public safe
constructor accepts roots or physical addresses. Factory and Space are explicitly
non-Send/non-Sync; integer addresses are observations, never mapping authority.

`Factory::load(&Image<'_>, frame_budget: usize) -> Result<Space<'pool>, Error>`
uses the actual validated ELF Image, not caller-built segment metadata. The
returned Space borrows only the Pool: input ELF storage may be released after
copying. Space privately retains every allocated Frame, root, table and backing
page until destruction. It exposes neither Frame owners nor mutable/immutable
byte slices into mappings. Private bounded inspectors may copy bytes into
caller-owned output while inactive; they never return references into backing
storage. No Clone, public activation token, or callback-under-root API exists.

Construction checks the current CPU0/masked-DAIF/expected boot-root context before
observing boot tables. Wrong context is an error before allocations or mutation.
The budget is an ordinary resource cap charged once per allocated frame,
including tables and stack. Budget exhaustion, Pool exhaustion and invalid
context return errors with all acquired frames released. A private partially
built owner records each successful allocation immediately; no root is published
until all copying, descriptors and audits succeed. Forgetting an inactive Space
leaks its owners; it cannot create an unowned mapping or release active pages.

## Layout and exact accounting

The existing image range is `0x1_0000_0000..0x1_0010_0000`. Root entry 4 points
to one user L2 and its entry 0 to one L3, covering the 2 MiB arena. Stack pages
occupy `0x1_001f_0000..0x1_0020_0000` (16 pages); the immediately preceding
page `0x1_001e_f000` is an unmapped guard. Reserve `0x1_0010_0000` for the
future startup page, **unmapped and unallocated in this stage**. All holes remain
invalid. This reservation does not decide startup ABI bytes or grant handles.

Root entry 0 shares the immutable boot device subtree. Entry 1 points to a
private RAM L2 and eight private RAM L3 copies covering the boot identity RAM
window. Entry 2 (CPU0 owned-mapping arena) and all other root entries are zero.
The exact count is 12 table frames (root, RAM L2, eight RAM L3, user L2/L3),
16 stack frames, and `Image::page_count()` image frames: maximum **284** frames.
The current separately linked native fixture has five image pages and therefore
requires **33** frames. No startup-page charge is hidden in these counts.

Use fixed-capacity owner arrays and checked indices/arithmetic, without heap
allocation. Owner representation may use 512 L3-indexed backing slots or a
272-entry compact list; either must retain the same 284-frame allocation bound.
Audit compiled constructor/return stack usage and aggregate temporaries against
the 64 KiB coordinator stack before guest execution; an owner-count bound is
not itself a stack-usage proof.

This explicitly refines design 0008's unchanged-identity-mapping requirement:
physical output, granule, memory type, cacheability and shareability remain
unchanged for the Pool lifetime. Only retained executable owners' identity AP
permissions tighten during the nonescaping assembly interval. Every callable
Frame/Block/KernelBox method and inactive byte inspector runs under the boot
root. No Rust references to those executable bytes survive into that interval.

Allocate zeroed backing frames, copy precisely each segment's file interval at
its virtual page offsets, and leave prefix/suffix padding and BSS zero. Support
unaligned admitted segments and copies crossing pages. Stack pages begin zero.
No mutable borrow of code or table bytes survives sealing or diagnostic entry.
Table frames are ordinary private Pool owners, never executable or EL0-accessible.

## Descriptor contract and alias audit

Use the existing 4 KiB, 39-bit, MAIR `0xff`, TCR `0x80993519` profile. Output
addresses use the admitted physical address width and must be aligned. Table
descriptors contain only the checked output address and `0b11`; no block leaves,
contiguous hint, hardware dirty-management or unreviewed extension is introduced.
Hardware access-flag and dirty updates remain disabled in TCR; AF is preset,
so table walks cannot mutate copied or shared tables.
L3 leaves have type `0b11`, AttrIndx 0 (Normal WB), SH `0b11` (inner shareable),
AF bit 10 set, and nG bit 11 clear. AP occupies bits 7:6, PXN bit 53 and UXN
bit 54. All unspecified attribute bits are zero. All spaces initially use ASID
zero and full local EL1 invalidation on each switch; this is not an ASID design.

| Mapping | AP | PXN | UXN | Attribute bits excluding output address |
| --- | --- | --- | --- | --- |
| EL0 image RX | `11` | 1 | 0 | `0x0020_0000_0000_07c3` |
| EL0 image R | `11` | 1 | 1 | `0x0060_0000_0000_07c3` |
| EL0 image RW / stack | `01` | 1 | 1 | `0x0060_0000_0000_0743` |
| RX frame's identity alias | `10` | 1 | 1 | `0x0060_0000_0000_0783` |

Copy boot RAM descriptors verbatim except each user RX frame's identity alias,
which becomes EL1 read-only and execute-never. Boot kernel RX/RO, stack guards,
reserved regions and device attributes remain unchanged. Validate each replaced
alias was the expected normal EL1 RW, PXN/UXN identity page and belongs to its
retained Frame; reject unexpected descriptors. Shared boot tables are never
modified. Read-only user data may retain an EL1 writable identity alias; the
W^X claim concerns executable physical pages, not immutable kernel internals.

Audit all root/table entries against the retained ownership and expected boot
layout before publication: no user access to kernel/device/table pages, no user
W+X page, no writable active alias of an executable user frame, no foreign
physical output, no extra valid user page, and no descriptor outside the fixed
layout. Inspect complete descriptor words, not just selected permission bits.
Boot-root writable aliases are inactive on CPU0 while the private root is active;
CPU1 is OFF throughout. This argument cannot be extended to concurrent spaces,
DMA, peer execution or shared mutable boot tables without a separate review.

## Scoped installed-translation diagnostic

The safe diagnostic acquires the existing global `threads::BUSY` lease, verifies
CPU0, exact masked DAIF, expected boot root and admitted boot controls. A topology
capability proves either the admitted one-CPU profile or the admitted two-CPU
HVC profile; the latter checks firmware AFFINITY_INFO reports CPU1 OFF. BUSY
excludes existing CPU_ON/thread runners until restoration. Unknown topology,
live peer, wrong controls or a busy lease return an error before switching.
There is no fallback that assumes a failed firmware query means OFF.

`Space::probe(&mut self, queries: &[Probe]) -> Result<ProbeBatch, Error>` accepts
at most 16 queries; Probe contains a scalar VA and one of four AT access kinds.
ProbeBatch owns a fixed array of before/active/after raw PAR triples and its
initialized count, plus installed/restored root observations and control equality.
The same bounded query set runs under the boot root before installation, the
private root after maintenance, and the restored boot root after maintenance.
This makes disappearing user translations and restored identity permissions
observable without exposing a separate arbitrary activation operation. Copy
all queries into fixed coordinator-stack storage before switching and collect
results there, then construct the returned value after restoration. Never pass
caller storage directly to assembly: it could reside in the omitted owned arena.
Validate the actual query/result/root-observation buffer ranges immediately
before assembly: compiler return-slot promotion can otherwise place a local
result in caller storage. Every covered page must have identical privileged
RW identity mappings in both roots; reject other placement before switching.
This runtime check makes the proof independent of compiler stack placement.
Arbitrary scalar query addresses are allowed, but never dereferenced.

A private assembly routine switches roots and performs only bounded scalar
`AT S1E0R/W` and `AT S1E1R/W` observations, with `ISB` before reading PAR_EL1
for each query. Inputs/results reside in retained buffers with checked common identity mappings;
all code, stacks, vectors and buffers retain identical privileged translations.
No Rust callback, allocator, logging, owned-arena access, arbitrary dereference
of user VA, or EL0 entry occurs under the alternate root. Existing SP_EL0 remains
the kernel emergency stack and VBAR remains unchanged. Unexpected exceptions
are terminal, never a path that releases possibly active owners.

Save the exact original TTBR0_EL1 and PAR_EL1. Publish completed table writes
with `DSB SY`; `TLBI VMALLE1`; `DSB SY`; `ISB`; write TTBR0_EL1;
`ISB`; `TLBI VMALLE1`; `DSB SY`; `ISB` before
any AT probe. On return use the same sequence to reinstall the saved boot root
and finish full local invalidation before any Rust code or backing-frame access.
After the final boot-root observation pass, restore PAR_EL1 exactly. TTBR1, TCR, MAIR, SCTLR, VBAR, SP_EL0, DAIF and PAN are
unchanged. Preserve the assembly calling convention. The common kernel mappings
must be valid across the switch and invalidation window; no newly mapped user
address is accessed during that window. Active tables are immutable, so no
live valid-to-valid descriptor replacement or break-before-make edit occurs.
This is a restricted root replacement, not a general permission/remap primitive:
every VA valid in both roots retains its physical output, granule and memory
attributes; only executable-owner identity AP changes. User VAs are added and
the owned arena removed. No access to changed or added mappings occurs before
the post-switch flush; all assembly fetches and data accesses use common
unchanged mappings. The pre-switch flush drains old accesses; the post-switch
flush also removes translations refilled in the intervening instructions. Full
VMALLE1 is required despite ASID zero and global leaves. No BBM exemption for
changed physical output, granule or cacheability is claimed.

The method returns only after restoration; release BUSY last. No safe caller can
forget an active guard. Drop is legal only in the inactive state and releases
all frames exactly once; an impossible internal active-state teardown is fatal,
not reclamation. Assembly has no recoverable early return while installed.

AT results prove installed data-translation permissions and output addresses,
including absence of guard/startup/hole/kernel access from EL0. They do **not**
prove instruction execution, NX enforcement, lower-EL exception containment or
PAN behavior for ordinary loads. PXN/UXN and alias W^X here have descriptor-audit
evidence only. Stage C must add instruction-cache visibility maintenance and
actual EL0 execution/fault tests before claiming those runtime protections.

## Required implementation evidence

Use the existing separately linked `fixtures/native-elf.rs` and linker script as
an embedded actual ELF input, parse it with Image, load it through Factory and
verify its bytes, zero-fill, entry, pages and permissions against the independently
recorded readelf layout. Fixture integration must not be replaced by toy metadata.
Test an admitted offset-within-page ELF as well, including page-crossing copies.

Verify all descriptor/owner invariants, all mapped backing physical addresses,
EL0 read/write results for each user page, EL0 denial for privileged code/data,
stack guard, startup reservation and holes, and denial of the absent owned arena.
AT physical-address comparison includes the requested page offset and checks
PAR fault/status before interpreting the output. Audit exact control-register
restoration, repeat diagnostics and show the ordinary boot mapping tests still
work afterward. Do not call descriptor inspection an execute-permission test.

For each allocation prefix 0..required-1, set the frame budget to that prefix,
require construction failure and exact baseline free count. Independently exhaust
the real Pool below the required count using retained owners and verify allocation
failure rolls back. Success consumes exactly the expected count, inactive Drop
restores the baseline, repeated load/drop reuses zeroed pages, and deliberate
forget retains allocation rather than exposing a live owner. Test maximum image
capacity and all admitted permission variants. Actual-type compile-fail clients
must reject Send/Sync, Frame extraction, forged construction, and escape of the
Pool lifetime; releasing ELF bytes after successful construction must compile.

Preserve existing ARM, x86 and SMP regression gates. Record build commands,
stack-usage checks, strict guest results and raw logs. This milestone reports
owned ELF loading and scoped installed-translation evidence only; the complete
native program, initramfs, ABI, containment and reclamation gate in 0021 remains
open.

## Sources and review boundary

The [Arm memory model guide, ARM062-948681440-3279](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Learn%20the%20Architecture/Armv8-A%20memory%20model%20guide.pdf)
section 3 diagram and sections 10–11 establish descriptor bit positions, AP
permissions, PXN/UXN and preset AF; section 13 addresses compatible aliases.
The [Arm memory management guide, 101811 v1.4](https://documentation-service.arm.com/static/68e680a2587fd957f5efd742)
sections 6–9 establish descriptor types, translation geometry, maintenance and
AT/PAR semantics. The conservative two-flush restricted root-switch protocol is
our derived implementation choice, requiring independent review; the guide does
not prescribe this exact routine. The root switch does not rely on FEAT_BBM
extensions or change cacheability/output addresses of common translations.
Existing source-of-truth boot constraints are `boot-aarch64.S`, `frames.rs`,
`mappings.rs`, `threads.rs` and `secondary.rs`; this design does not infer a
stronger hardware profile from a generic Pool borrow. No Linux implementation
source is used or copied.
