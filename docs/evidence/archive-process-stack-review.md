# Archive process stack review

2026-10-26, independent AI inspection. No builds or guests run by this reviewer.
Initial inspected emitted assembly:
`target/aarch64/process-test/aarch64-unknown-none/release/deps/hematite-97951d16c02ba655.s`.
SHA-256 `87ed64560360c361217be3e7abf8e32a8e8389a80c018e0a63a26d3f1de0253f`.
Stable iteration1 ELF SHA-256:
`9742614856107fe6772c0d882a82ae39a6e6f7e1217530a99764463d96af1d3b`.
The archived gzip assembly decompresses to the same inspected hash.
**The compiled stack gate passes for this exact stable candidate.** Guest
execution still requires the independent host artifact gate; execution results
remain separate evidence.

Read emitted prologues, stack probe loops, CFI offsets and direct call edges.
The execution frames remain as in the [previous review](process-stack-review.md):
boot_main7440, process run16/checks752, one18992/run_loaded21056,
Executor run9872/execute976. The simultaneous execution sum is 59,104;
the previously inspected fixed formatter/console allowance of640 gives59,744.
Preparation failure/drop and Factory load frames also remain unchanged.

New frames are admission_checks3056, archive_inventory6304,
boot_image::admit9280, Archive::parse3424; ELF Image::parse is1056.
The UTF-8 validator called by Archive::parse is a stackless leaf. The largest
new parsing chain is boot7440 → run16 → checks752 → one18992 → admit9280 →
Archive::parse3424 =39,904 bytes, with at most the previously bounded terminal
slice/panic helpers beyond that. Inventory and preallocation admission checks
use less. The archive plan and admission return slots are absent from the
load/prepare/execute call chain: these are sequential calls in `one`, not
simultaneously retained parser stack frames. The large archive itself is
embedded data, never copied onto the coordinator stack.

The baseline frames remain boot7440 → run_tests35072 → topology valid8320 →
extra_valid4240 → parse6464 → property128 → terminal48 =61,712 bytes.
This remains below the65,536-byte coordinator stack by3,824 bytes. Dedicated
process exception-frame behavior is unchanged and adds no Rust call chain to
the coordinator stack. These are static scoped bounds, not measured stack
watermarks or guarantees for later compiler/source changes.

Independently checked final ELF hash, archived assembly hash, actual symbol log
and process manifest: `__bss_start=0x40393000` matches the stable fixture target.
The coordinator guard is0x403a4000 and stack top0x403b5000, leaving64KiB usable
after the guard; process exception stack top is0x403ba000. The preserved
fixed-point log records the rejected stale0x4038f000 candidate before iteration1.
Independent ELF program-header decoding locates the1,326,592-byte archive in
the PF_R-only load at file offset380896. Its SHA-256 is
`f02b5d17cadcc700adcdddd879ce51c4aba68121e198a484018c932f5524c75f`.
The strict host gate separately checks complete uniqueness, absence of raw ELF
fallback bytes and all payload/symbol/GNU bindings before guests.
