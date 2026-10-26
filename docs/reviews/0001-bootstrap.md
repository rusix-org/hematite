# Independent bootstrap design review

Date: 2026-10-25. Reviewer: independent AI agent; not human review.
Scope: [bootstrap design](../design/0001-bootstrap.md), before implementation.
No code, ELF image, VM boot, or compiled safe API was audited in this review.

The single-CPU, permanently interrupt-disabled bootstrap is a workable first
scope. Its safe surface does not expose arbitrary pointers or device authority.
Proceed with implementation subject to these concrete requirements; this is
not a soundness certification or an approval of later memory/concurrency APIs.

## Requirements for the implementation

1. **Read-only GDT:** set the Accessed bit in code/data descriptors before
   making the GDT page read-only. Hardware can otherwise attempt to update
   those descriptors during segment loading, faulting once CR0.WP is enabled.
   Long-mode CS must have L=1 and D=0; data descriptors must not set L.
2. **Entry ordering:** preserve PVH EBX before CPUID clobbers it. Check supported
   CPUID leaf ranges before querying feature leaves. Clear BSS before relying
   on any pushed value, return address or saved object inside the BSS stack;
   a register-only clearing loop can establish ESP beforehand but cannot
   retain live stack data across the clear. Set the full RSP in 64-bit mode,
   reload data segments, and maintain SysV call alignment before Rust.
3. **Mapping boundary:** page-align text/rodata/data transitions and ensure
   the 32-to-64-bit transition instructions are in the executable range.
   Give the guard its own page, with no overlap with stack, page tables or
   metadata. Keep parent paging entries supervisor-only and executable enough
   for text; leaf NX implements per-page restrictions. Enable EFER.NXE before
   paging can consume NX-marked entries. Linker assertions must cover all
   bootstrap allocations, including `NOBITS` storage, not only file bytes.
4. **Page-table observation:** hardware may update Accessed/Dirty bits. Read
   paging structures through raw volatile loads and check the permission/address
   bits deliberately; do not compare whole entries against pristine constants
   or form Rust references implying that hardware cannot mutate the entries.
5. **State isolation:** keep the actual platform constructor and port helpers
   private. A safe caller cannot obtain a second platform instance that relies
   on exclusive mutable global state. No return from entry and no interrupt
   enablement is part of this bootstrap's safety contract.
6. **Failure evidence:** with no IDT, an unexpected fault may triple-fault
   instead of producing a diagnostic. Never interpret normal QEMU shutdown as
   test success: require the complete named inventory and exact debug-exit 33.
   Keep missing-feature boot as a negative case. This phase does not provide
   recovery from exceptions or a usable stack-overflow handler.

The memory-map parser, allocator, SMP, IDT/fault handling, user-copy and DMA
remain outside this review. Mapping addresses is not permission to allocate
or access firmware/device memory; those operations need their later designs.
The pinned microvm/PVH pair must still be proven by execution.

## References

- [Xen PVH public boot ABI](https://xenbits.xen.org/docs/unstable/misc/pvh.html):
  inherited entry-state contract (provided in the parent research context).
- [Intel SDM Volume 3A](https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-3a-part-1-manual.pdf):
  segment descriptor type bits, paging permissions and control registers;
  official manual search result consulted 2026-10-25. Review requirements above
  are applied architectural reasoning, not a claim to have audited machine code.

No Linux kernel implementation source or Asterinas kernel code was consulted.

## Pre-boot implementation review

Read `crates/hm-core/src/main.rs`, `src/boot.S`, `linker.ld`, Cargo settings,
and the safe `hm-kernel` policy on 2026-10-25. No builds or VM execution were
performed by this reviewer. Findings sent to the implementing agent:

1. **Must fix — immutable foreign page tables.** The initial Rust declarations
   used immutable `extern static` arrays for all four paging structures. CPU
   Accessed/Dirty writes after entering Rust violate the immutable-static
   contract even when Rust only uses raw volatile reads. Declare the arrays
   `static mut` and continue using raw pointers without references, or use
   appropriate interior-mutable storage. The
   [Rust Reference's extern-static rule](https://doc.rust-lang.org/reference/items/external-blocks.html#statics)
   explicitly applies to mutations originating outside Rust. The matching
   assembly reserves full arrays, so the raw pointer bounds/provenance approach
   itself is appropriate after correcting the mutability declaration.
2. **Verification gap — unused paging entries.** The initial `checks().wx`
   checked only the first PML4/PDPT link, eight directory links and 4096 leaves.
   It did not check that unused PML4/PDPT/directory entries remain zero. Add
   those checks to substantiate the absence of other aliases. The assembly's
   full BSS clear appears to establish that invariant; this finding is missing
   runtime coverage, not evidence of a currently present writable alias.

The observed transition addresses, GDT Accessed bits and L/D bits, CR0/CR4/EFER
masks, NX leaf encoding, preserved PVH register, BSS clearing order, full RSP
initialization and 16-byte pre-call alignment are internally consistent with
the design. Linker-owned text/rodata/guard boundaries are page-aligned and the
allocation end is asserted below the identity-map limit. The core's safe
surface does not expose mapping mutation or arbitrary ports/pointers.

These observations remain source-level review. Actual ELF program headers,
compiler-generated instructions and pinned firmware/loader behavior require
the build and boot evidence; a successful boot does not resolve a Rust UB
finding by itself. Recheck the two findings after integration.

Follow-up inspection on 2026-10-25 confirmed both findings were addressed:
all four table arrays are now mutable extern statics accessed only through
raw pointers, and `checks().wx` verifies unused PML4/PDPT entries 1..512 and
directory entries 8..512 are zero. No remaining blocker was identified in
this limited source review. Build/boot results are recorded by the main agent.
