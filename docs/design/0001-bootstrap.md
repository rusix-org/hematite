# Bootstrap design (2026-10-25)

Authorization: Aaron selected scratch and explicitly instructed "build until we
have something we can run" after reviewing the Phase 0 proposal. This authorizes
local kernel implementation and routine bootstrap choices. No public release,
license selection, host-kernel modification or upstream code reuse is inferred.

## First runnable scope

PVH ELF entry into a 32-bit assembly shim, then x86_64 Rust on QEMU 7.2.22
`microvm`, one CPU, 128 MiB, no disks/network. UART and isa-debug-exit are
explicit test/bring-up instrumentation. Native virtio devices and Linux ABI
are later milestones. Stable Rust 1.99.0; no crates.io dependencies.

`hm-core` is initially the executable owning assembly, linker layout, serial,
page tables, panic and exit. `hm-kernel` is a `forbid(unsafe_code)` library
containing the monitor and test policy. It declares a safe `Platform` trait;
core implements it and supplies an exclusive instance. This avoids a cyclic
crate dependency. Later safe services can depend on a split core library.

## Entry and mappings

PVH note type 18 names a physical 32-bit entry linked at 1 MiB. Interrupts stay
disabled throughout bootstrap; no AP startup, IDT, threads, user memory or DMA.
The shim clears DF, establishes its own stack/GDT, checks CPUID for long mode,
NX, SMEP, SMAP and SSE2, zeroes owned BSS, builds 4 KiB identity mappings for
0..16 MiB, and enters long mode. Boot info EBX is saved but not dereferenced;
no allocator consumes firmware memory until a separate reviewed map parser.
All bootstrap storage lies in linker-owned RAM loaded by QEMU ELF, below
16 MiB. Linker assertions enforce bounds and alignment.

Map supervisor-only: text RX (not writable), rodata R/NX, everything else
RW/NX. Page zero and one page below the 64 KiB stack are non-present. There is
no second writable alias. Bootstrap page tables are in owned BSS, not heap.
CR0.WP, CR4.PAE/OSFXSR/OSXMMEXCPT/SMEP/SMAP, EFER.LME/NXE are set before
Rust. Clear EM/TS; initialize x87/MXCSR before compiled code. BSP stack is
16-byte aligned before the call to Rust; red zone disabled by target config.
Fatal unsupported CPU paths write a failure code to debug-exit, then halt.

## Safe surface and invariants

Platform provides `write(&str)`, `read_byte() -> Option<u8>`, and
`checks() -> BootChecks` (booleans for actual CR register flags and mapped
permissions). No public addresses, pointers, port numbers or mapping mutation.
UART I/O only touches COM1; transmit polling is bounded. An unavailable UART
causes timeout/failure, not arbitrary memory access. Port helpers are private.
Single CPU and permanently disabled interrupts prevent concurrent access to
bootstrap state; no shared mutable references escape. Page-table reads use
raw addresses/volatile reads of owned storage, never user-supplied pointers.
No panics depend on external input; monitor uses a fixed-size input buffer and
rejects oversized lines. Panic exits failure; a halted CPU cannot report pass.

## Verification

Tests must assert actual control registers and PTE flags, including every
mapped page, the stack guard and null page. Safe arithmetic/input checks cover
boundary/overflow handling. Harness expects a fixed named test inventory,
exactly once each, a final summary, and QEMU exit 33 (0x10 debug-exit). Missing,
duplicate, failed, extra, timeout or unexpected exit is failure. Boot alone is
not a test pass. Negative missing-NX CPU run must fail before a success report.
No Linux compatibility tests exist yet; do not label this a Linux ABI kernel.

Sources: Xen PVH public boot ABI and ELF note definition; Intel SDM architecture
register/page-table definitions; QEMU microvm manual. No Linux implementation
or Asterinas kernel code is copied. Upstream tooling was used only for the
prior evaluation and the pinned development environment.


## Implemented profile

Final environment: digest-pinned Rust 1.99.0 slim Bookworm image plus Debian
snapshot `20261025T000000Z`, QEMU package `1:7.2+dfsg-7+deb12u18+b3`.
The earlier QEMU 10.2.1 evaluation environment also booted this bootstrap;
`make build/test/run` use the smaller final environment consistently.
TCG `max` is the tested CPU model; KVM is optional and not claimed verified
for this new kernel. Firmware identity and exact commands are in the evidence.

No PVH memory map is consumed, no frame is allocated, and no AP or virtio
transport is initialized. The 16 MiB mapping is a bootstrap reachability map,
not an allocator's free-memory inventory. Guard checks inspect PTEs; fault
recovery and exception handlers do not exist yet. No Phase 1 claim is made.
