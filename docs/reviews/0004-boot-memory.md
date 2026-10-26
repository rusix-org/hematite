# ARM boot-memory discovery review

Date: 2026-10-25. Independent AI review of
[design 0004](../design/0004-boot-memory.md); not human approval or a soundness
certification. Execution evidence is maintained separately by integration.

The bounded discovery contract is suitable for implementation. QEMU's
[v7.2 virt documentation](https://github.com/qemu/qemu/blob/v7.2.0/docs/system/arm/virt.rst)
places the bare-metal ELF DTB at RAM origin, unlike the non-ELF register
handoff. Parent investigation found the existing 512 KiB gap insufficient for
the loader's padded blob. Relocating the kernel to `0x40200000` keeps a 2 MiB
metadata prefix inside the existing 16 MiB mapping. Guest evidence must confirm
actual loading; a host dump alone cannot establish it.

The core may borrow only a fixed, initialized loader RAM window, after the
single-CPU bootstrap has excluded concurrent writers. Header values can narrow
that window but never enlarge it. Only owned metadata may leave parsing; no
arbitrary-address safe API or long-lived DTB reference is introduced. Linker
assertions must enforce the separation, and the eventual allocator must exclude
the full prefix independently of reservations advertised by the tree.

The safe parser's specified whole-tree validation, checked arithmetic,
bounded capacities and rejection of unsupported memory/reservation semantics
prevent partial or ambiguous discovery from being used as an allocation map.
Discovery remains descriptive: even accepted integer addresses cannot confer
memory ownership or access authority.

Review requested explicit rejection of duplicate reserved-memory containers,
unsupported root/container status and child nodes beneath accepted memory
nodes. These close interpretation ambiguities without broadening the supported
binding subset. Implementation review must check these requirements plus
truncation, block overlap, name termination, reservation termination, range
overflow, property ordering and exact final structure termination.

No builds or guest tests were performed by the reviewer. Final integration
must retain all protection tests, exercise actual ARM DTBs at two RAM sizes and
run malformed-input fixtures on both architectures.

## Core implementation review

Reviewed the linker relocation, `aarch64.rs::discover_memory`, owned
`BootPlatform` metadata, shared reporting and 128/64 MiB monitor profiles.
No defect identified. The private core reader has no caller-controlled pointer;
its first 40-byte borrow is independently bounded and the full borrow cannot
cross the asserted kernel origin. Both return only owned parser results.
Initialization rejects a parse failure before reporting success. The monitor
profiles require one exact parsed-RAM line matching their QEMU memory setting.
This review does not itself establish that the loader supplies a valid tree.

## Parser and fixture review

Read `boot_memory.rs` after implementation. No defect identified: bounded byte
helpers and a fixed node stack enforce the specified block/tree contract;
checked regions and private map counters preserve capacity and accounting.
The requested duplicate-container, parent-status and memory-child restrictions
are implemented. No pointer or allocation authority escapes through the result.

Fixtures cover valid multi-range metadata, static exclusions, unsupported
bindings, truncations, per-byte mutations and malformed headers/trees. Review
requested stronger isolated missing-reservation-terminator and unterminated-node
fixtures: the initial versions could fail on earlier overlap or root-name
checks without reaching the intended boundary. Explicit early/trailing END
fixtures were also requested. Passing aggregate tests alone must not be
described as proof of a path that its fixture did not actually reach.

## Property-padding compatibility review

The first relocated guest rejected the loader tree. Independently decoded
`target/boot-memory/virt-128m.dtb` (SHA-256
`cd2599f39b8ca06840c3fcde4c03288442be5c0444adf0e86614ae108d86b21b`)
and confirmed 15 properties with nonzero padding; all node-name padding was
zero. Examples are root `model` padding `000002` and memory `device_type`
padding `73`. This contradicts the zero-padding requirement in
[Devicetree section 5.4.1](https://devicetree-specification.readthedocs.io/en/stable/flattened-format.html).

Accepted the narrow compatibility proposal before implementation: skip only
the independently bounded zero to three property-alignment bytes, without
interpreting their contents. The declared property length still bounds the
value and the next token still begins at its computed alignment. Padding cannot
become a range, name or token, so the change cannot grant authority or alter
semantic traversal. Node-name padding remains zero-required. The design now
records this pinned-loader accommodation and requires a positive fixture;
integration must preserve the rejected run and retest both guest targets.

Post-change source review confirmed `align(false)` is used only after bounded
property values, while names retain `align(true)`. The positive fixture places
`ff 03 09` in all three status-property padding bytes and still checks exact
metadata. The requested isolated malformed-name/reservation and early/trailing
END fixtures are now present. No new defect identified; execution remains the
parent integrator's responsibility.
