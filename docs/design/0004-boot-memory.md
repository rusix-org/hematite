# ARM boot-memory discovery

Status: implementation contract independently reviewed by a separate AI agent
on 2026-10-25 before core changes. Review is not human approval or a soundness
certification; execution evidence remains separate. Date: 2026-10-25. This increment discovers metadata only. It
does not allocate frames, map newly discovered memory or claim Phase 1 complete.

## Boot boundary

Keep the pinned direct-ELF QEMU `virt` profile from
[design 0002](0002-arm64-bootstrap.md). Its DTB starts at `0x40000000`; the
incoming `x0` convention belongs to Linux-image boot and is not used here.
Move the kernel start from `0x40080000` to `0x40200000`, leaving a 2 MiB
DTB window. The current 512 KiB gap is insufficient for QEMU's padded generated
blob: the parent observed a 1 MiB `dumpdtb` output, and inspected QEMU 7.2
loader behavior that declines a blob larger than the address limit. Boot
verification must establish that the relocated ELF receives a valid DTB.
A stopped guest with the old ELF layout had zero words at `0x40000000`
(parent evidence: `target/boot-memory/old-layout-monitor.log`).
The entire interval is already mapped as normal RAM by the bootstrap.

Only `hm-core` may turn the fixed address into a reference. First read the
40-byte header from that known initialized RAM window, decode totalsize, and
reject sizes below 40 or above 2 MiB before constructing the full slice.
Never derive the initial readable bound from the untrusted header. The
QEMU loader/profile, not the tree's claimed RAM ranges, establishes this first
read's validity. Linker assertions must retain the image boundary. Use one
short-lived shared byte slice during parsing, return owned metadata, and let
no reference escape. One CPU, masked interrupts, no DMA and no concurrent
writer establish immutability during that borrow. Future allocator activation
must preserve the blob until parsing ends and reserve its pages explicitly.
Do not expose a public safe arbitrary-address slice constructor.

The parser belongs in safe `hm-kernel` code and accepts arbitrary `&[u8]`.
Its result is descriptive data, never a capability to dereference physical
addresses. All decoding uses byte reads, checked offsets and checked `u64`
range ends; no casts to packed structs, heap, recursion or panic paths.

## Accepted subset and bounds

Use fixed arrays: 16 RAM ranges, 32 combined reservations, nesting depth 32.
Overflowing a capacity is an error, never truncation. Require at least one RAM
range. Represent ranges as half-open `u64` intervals; reject zero length and
overflow. Reject overlapping RAM ranges rather than double-counting them.
Reservations may overlap between sources and remain exclusions; do not sum
them as usable bytes. An error returns no partially usable map.

Validate big-endian magic and the 40-byte v17 header; accept version 17 and
last-compatible version 16 only for this initial profile. Require totalsize
within the supplied slice and 2 MiB cap. Check block alignment, bounded
ends and ordered nonoverlap: header, reservation block, structure, strings.
Scan 16-byte reservation entries only before the structure block, requiring
the zero/zero terminator. Reject nonterminal zero-size entries and range
overflow. Reject overlapping entries within the header reservation list.

Walk every structure token, including unrelated subtrees. Require one empty
root name, balanced nodes, properties before children, bounded terminated
names, valid property-name offsets, and exact final END at structure end.
Reject unknown tokens, nonzero node-name alignment padding, excess depth and
duplicate interpreted properties. Property padding is handled as described below. NOP is permitted. Strings referenced by properties
must terminate within the strings block; unknown properties are structurally
validated but do not authorize memory.

### Pinned-QEMU property-padding accommodation

Devicetree section 5.4.1 specifies zero alignment padding. The pinned QEMU
7.2.22 dump contains nonzero padding after 15 properties, including root
`model` (`00 00 02`) and memory `device_type` (`73`); its node-name padding
is zero. The first relocated guest rejected this tree. Preserve the original
blob and hash under `docs/evidence/boot-memory-investigation/`.

Accept arbitrary contents only in the zero to three bytes between a property's
declared value end and the next four-byte boundary. Bounds-check those bytes
before advancing. They are never part of the value, a name, a token or a
memory range. Keep node-name padding zero-required and retain every other
structure/bounds check. This deliberate loader accommodation grants no memory
authority and is covered by a positive nonzero-property-padding fixture; it is
not a relaxation of property length or token alignment.

For memory interpretation, require root `#address-cells = 2` and
`#size-cells = 2`. Accept direct root children named `memory` or `memory@...`
with exact `device_type = "memory"` and a nonempty `reg` of 16-byte tuples.
Reject nested nodes claiming `device_type = "memory"` or a root memory node
without the required type, rather than silently guessing bus translation.
Absent status or exact `okay`/`ok` is accepted; other memory-node statuses
are unsupported errors. Reject children under accepted memory nodes. Reject
duplicate root cell properties and non-okay root status.

Accept at most one `/reserved-memory` container; reject duplicates or
non-okay container status. If it exists, require its own explicit 2/2 cells and empty
`ranges`; only its direct children with static, nonempty `reg` tuples are
supported. Reject dynamic allocation requests (`size`, `alignment`,
`alloc-ranges`), nested reservation children, `no-map`, `reusable`, and
non-okay status. These are deliberate narrow-profile restrictions; notably
the current bootstrap cannot honor `no-map` after eagerly mapping 16 MiB.
Require interpreted reservation properties exactly once. Preserve all static
reserved intervals even when outside advertised RAM; exclusion must never
be silently lost through an intersection during parsing.

## Allocation follow-on

Discovery does not yet produce free memory. A separately reviewed frame
allocator must intersect discovered RAM with accessible mappings, subtract
the complete kernel image (including stack, guard, tables and allocator
metadata), the entire pre-kernel 2 MiB DTB window and all reservations, round usable ranges inward to 4 KiB and
exclusions outward, and reject arithmetic/capacity failures. Address integers
must never mint ownership. The allocator must issue unique non-cloneable frame
owners, preserve mappings/DMA lifetimes, and permit forgotten handles to leak
rather than enable duplicate ownership. Mapping expansion, reclamation, heap,
SMP and x86 PVH discovery each need their own implementation/review evidence.

## Verification required

Run safe parser fixtures on both guest architectures: valid RAM/reservations,
multiple ranges, unknown device nodes, NOP; then every truncation of a valid
fixture, malformed header/block boundaries, unterminated names/reservations,
bad tokens/tree balance, overflow, duplicate interpreted properties, capacity
limits and every unsupported memory/reservation form above. Deterministic
mutation tests must not panic. Boot ARM with at least two RAM sizes and match
reported RAM to the configured guest; preserve exact commands and serial
results. Confirm DTB parsing does not mutate input and boot protection tests
still pass. x86 continues regression boot without claiming PVH memory discovery.

## Provenance

Consulted 2026-10-25; no Linux implementation source consulted or copied.
The moving documentation describes formats; runtime evidence must identify
the pinned QEMU version actually tested.

- [QEMU virt bare-metal boot](https://www.qemu.org/docs/master/system/arm/virt.html#hardware-configuration-information-for-bare-metal-programming):
  RAM origin and ELF DTB placement.
- [QEMU v7.2 ARM loader](https://github.com/qemu/qemu/blob/v7.2.0/hw/arm/boot.c):
  parent inspection of DTB address-limit handling; no code copied.
- [Devicetree flattened format](https://devicetree-specification.readthedocs.io/en/stable/flattened-format.html):
  v17 header, reservation entries, structure tokens and strings.
- [Devicetree memory nodes](https://devicetree-specification.readthedocs.io/en/latest/chapter3-devicenodes.html):
  memory and static/dynamic reserved-memory encodings. Restrictions above are
  project choices, not claims of full Devicetree conformance.
