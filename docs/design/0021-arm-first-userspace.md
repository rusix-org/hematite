# First protected native ARM userspace

Status: Stage A approved by independent [review 0021](../reviews/0021-arm-first-userspace.md),
2026-10-26. Stage B owned loading and bounded installed-translation diagnostics
are separately specified and approved in [design 0022](0022-arm-owned-address-spaces.md).
[Design 0023](0023-arm-native-process.md) now specifies independently approved
EL0 entry, process copy and teardown mechanisms, now implemented and locally
verified in [native-process evidence](../evidence/native-process.md).
Its safe native wire policy is implemented and host-verified. This is Phase 2 work,
not a claim that the unpublished Phase 1 gate has passed CI.

The target is a separately linked static native AArch64 program loaded from an
initramfs, entered at EL0, able to write through an explicitly granted console
handle and exit. The initial task is pinned to CPU0 with CPU1 confirmed OFF;
concurrent userspace, Linux ABI support and a general process scheduler follow
separate contracts. A parser alone is not execution or isolation evidence.

## Stage A: safe static ELF admission

Implemented and verified locally; see [evidence](../evidence/elf-admission.md).

Add `hm_kernel::elf`, with `#![forbid(unsafe_code)]`, no dependencies or allocation.
`Image::parse(&[u8]) -> Result<Image<'_>, Error>` returns an immutable bounded plan
borrowing its input. Image and Segment fields are private; safe callers cannot
forge or mutate validation results. Segment copies are permitted if their fields
remain private. Permissions can be the exact enum ReadOnly/ReadWrite/ReadExecute.
Getters expose entry, segment slice, total charged page count,
virtual address, memory size, immutable file bytes, permissions and rounded page
range. The core later consumes this metadata and supplies physical ownership;
none of these integer addresses grants memory access by itself.

Concrete bounds for the first profile:

- `USER_IMAGE_START = 0x1_0000_0000`, `USER_IMAGE_END = 0x1_0010_0000` (1 MiB).
  This resides in root entry 4, away from existing device, identity and owned
  mapping arenas. It is not a general address-space layout promise.
- `MAX_FILE_SIZE = 1 MiB`, `MAX_PROGRAM_HEADERS = 16`,
  `MAX_LOAD_SEGMENTS = 8`, `MAX_IMAGE_PAGES = 256`, `PAGE_SIZE = 4096`.
- ELF64, little endian, current identification/header versions, System V
  OSABI 0, ABI version 0, `ET_EXEC`, `EM_AARCH64` and `e_flags == 0` only.
  Reject malformed magic and reserved identification bytes rather than using
  unvalidated header casts. Header size is exactly 64 and program entry size 56.
- Program count is 1..16; reject extended `PN_XNUM` numbering. Check program
  offset/count/size arithmetic and complete containment in input before reads.
  Require e_phoff >= 64 in this strict profile so the table does not overlap
  the ELF header.
  Read fields from checked byte slices with explicit little-endian decoding.
  Section headers are entirely ignored, never consulted for load decisions;
  no section-table validation claim is made.
- Accept `PT_LOAD`; ignore `PT_NULL` payload fields because they are not used.
  Accept at most one `PT_GNU_STACK` only with exact R|W flags and zero file/memory
  size. Its p_align is 0, 1, or a power of two; it never requests a stack layout.
  Reject `PT_INTERP`, `PT_DYNAMIC`, `PT_TLS`, `PT_PHDR` and all other program types
  as unsupported. PT_PHDR rejection is an intentional initial profile limit;
  the separately linked fixture uses explicit LOAD program headers.
- A LOAD has nonzero memory size, file size <= memory size, and checked file
  offset/end entirely inside input (including a zero-byte file interval).
  Its alignment is 0/1 or a power of two, with virtual/file offsets congruent
  modulo that alignment; independently require congruence modulo 4096. Do not
  demand aligned starts: offset-within-page segments are supported.
- LOAD permissions must be exactly R, R|W, or R|X. Reject missing R, unknown
  flag bits, and W|X. Checked virtual memory ranges and their page-rounded ranges
  lie wholly inside the image window. Reject any page overlap between LOADs,
  even when their byte ranges do not overlap; no permission merging is implied.
  Reject descending virtual LOAD order, as required by the admitted ELF profile.
- Sum charged pages with checked arithmetic and enforce the page budget. Require
  at least one LOAD. Entry is four-byte aligned and a complete four-byte AArch64
  instruction lies within one executable segment's file-backed virtual bytes;
  an entry in zero-fill BSS or a nonexecutable segment is rejected.

All malformed/unsupported inputs return explicit errors without panic, partial
plans or allocation. Error categories may distinguish Header, Unsupported,
Bounds, Capacity, Layout, Permissions and Entry; callers must not depend on
which invalid property is reported first. No symbol resolution, dynamic linker,
TLS setup, relocation or implicit executable stack is performed.

The future loader allocates each rounded segment page zeroed, then copies exactly
its file interval to the corresponding page offsets. Prefix/suffix padding and
`p_memsz - p_filesz` remain zero. It does not reinterpret absent section tables
or use p_paddr as a physical address. A validated plan retains the immutable
file borrow until copying is complete.

Stage A tests must include a real separately linked no_std AArch64 ELF produced
by the pinned compiler/linker, whose program headers are independently recorded
by readelf. Exercise every header/type/boundary/overflow/alignment/permission/
overlap/entry rejection, valid offset-within-page segments, BSS, read-only/RW/RX
segments and maximum capacities. Truncated inputs and deterministic mutations
must never panic. Verify the accepted plan against the fixture's bytes and ELF
headers. Actual-type clients reject field forging and escaping the input borrow.
A parsing success does not claim the fixture was executed.

## Stage B: owned address spaces

Owned inactive loading and AT diagnostics are implemented in design 0022.
The startup-page and execution-cache extensions are implemented and verified
in design 0023.

Reserve one CPU0-only execution capability and retain the existing joined CPU
lease. A safe construction API takes a validated Image and a Pool borrow, owns
all allocated data/table/stack pages, and returns errors without publishing a
partially constructed root. No caller-supplied physical integers become mappings.
The object and its active-run token are non-Send/non-Sync; no safe method exposes
Frame byte access or mutable table storage while hardware can use the root.

Use the existing 39-bit TTBR0/4 KiB translation regime. A new root retains the
privileged UART/device mapping needed for kernel output and kernel code/stacks.
Its root entry 4 points to private user L2/L3 tables covering a 2 MiB arena.
The first 1 MiB is the ELF image; startup and guarded stack placement occupy a
separately specified part of the remainder. The initial candidate stack is
`0x1_001f_0000..0x1_0020_0000` (64 KiB), with the preceding page unmapped.
Startup-page placement and wire contents must be fixed in the stage's review.
All other user pages are absent. The CPU0 owned-mapping arena is absent too.

Preserve physical W^X for every active alias. User RX code frames cannot retain
an active writable identity alias. The process root therefore owns a private
RAM L2 and copies of the eight existing identity RAM L3 tables; executable-frame
identity aliases become EL1 RO, PXN and UXN while the EL0 RX aliases are active.
User writable data has PXN/UXN protection as appropriate; kernel text remains
privileged RX and inaccessible to EL0. Table storage is never user accessible.
Original boot identity tables are not changed. Boot-root RW aliases are inactive
on CPU0 while code can execute at EL0, and CPU1 is OFF for the entire run.
Changing this rule requires a new shared-address-space/shootdown proof.

Initialize owned bytes before sealing them into the address space. Once sealed,
no identity mutable-byte owner is exposed. Clean newly written code to the
required instruction visibility point and invalidate CPU0 instruction caches
with the prescribed DSB/ISB sequence before first EL0 entry. The exact descriptor
bits, table-copy procedure, cache instructions and full mapping audit belong to
the stage's independent implementation review, not an inference from this plan.

## Stage C: entry, native calls and containment

Design 0023 now owns the approved concrete implementation contract. Its safe
call policy and core runtime are locally verified; see native-process evidence.

The current kernel uses SP_EL0 as a kernel emergency stack. It cannot be reused
unchanged when EL0 owns SP_EL0. Provide dedicated lower-EL AArch64 vector paths
that switch to an explicitly retained kernel exception stack without trusting
user SP, then save all user register/control state before scratch use. Use a dedicated process-active vector table for all vector slots, including
stack-independent fatal nested EL1 paths: a handler fault must not redirect into
the old SP_EL0 emergency path while SP_EL0 names user memory. Restore the original
vector table only after leaving the process context. Existing EL1 diagnostics
retain their contract outside process execution. Lower-EL IRQ handling must not dispatch
through the current worker handler, which assumes SP_EL0 is an emergency stack.
A bounded timer must regain control from an infinite EL0 loop, not rely solely
on the host to terminate the emulator.

A minimal versioned native wire ABI uses fixed-width integers, never Rust struct
layout or Rust calling convention. Specify SVC convention, request version,
opcodes, status values and startup-page byte offsets before implementing entry.
Startup grants an explicit console handle. Calls initially include bounded
console write and exit; unsupported versions/opcodes/handles report errors.
No raw integer handle alone bypasses per-process ownership or generation checks.
The interface is native and must never be labeled a Linux syscall adapter.

Typed core copy APIs validate checked user ranges, permissions, page coverage and
length caps. They return copied bytes/status into kernel-owned buffers, never
Rust references into arbitrary user addresses. Since the one task is stopped
and no peer/user/DMA accesses its pages, owned backing pages may be copied while
retained; all temporary borrows end before resumption. PAN remains enabled.
A bad pointer, unmapped crossing, write to RX, execution from RW/stack, kernel
address, privileged instruction or unrecognized EL0 fault terminates or returns
a declared process error. It must not enter kernel fixed-site fault recovery or
kill the whole kernel. Define copy fault handling and partial-copy semantics
explicitly before exposing the API.

## Stage D: teardown and first-program gate

Execution, containment and reclamation are locally verified under design 0023.
Archive-backed input and execution are now locally verified under
[design 0025](0025-archive-execution.md). The [requirement audit](../evidence/archive-process-execution.md)
records the native functional result, missing matching CI and Linux gaps.

Normal exit, user fault and timeout converge on one quiescent teardown path.
No active user frame or root is freed while EL0 or a translation can access it.
Return through all Rust frames, restore the boot root, complete required TLB
invalidation and barriers, clear active authority, then release process-owned
pages. Do not restore a writable identity alias before user execution and stale
user translations are impossible. Forgotten inactive objects may leak resources;
forgetting a public token must never release a live address-space borrow.
Any uncertain transition is terminal rather than permitting unsafe reuse.

Load the native ELF from a bounded, checked initramfs (cpio newc is a candidate,
requiring its own parser contract). Tests execute a normal hello/exit program,
bad-pointer/handle/version programs, protected-memory and privileged-instruction
faults, stack overflow, and an infinite loop stopped by the kernel deadline.
After each exit/fault/deadline, verify exact resource reclamation and repeat
execution from fresh zeroed pages. Preserve all existing core/SMP regression
gates and independently verify EL0 exception/vector/exit evidence. Only this
full gate supports an isolated native-program claim. Linux hello-world remains
separately unsupported until its adapter is implemented and differentially tested.

## Provenance

No Linux implementation source is needed or copied. ELF admission uses the
[gABI ELF header](https://gabi.xinuos.com/elf/02-eheader.html) and
[program header](https://gabi.xinuos.com/elf/07-pheader.html) specifications;
AArch64-specific interpretation follows
[Arm AAELF64](https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst).
The current architecture and ownership constraints are established by designs
0002, 0005, 0007, 0008, 0012, 0019 and 0020. Exact Arm entry/cache/TLB references
must accompany the later runtime-stage review; this document does not approve
those unsafe mechanisms in advance.
