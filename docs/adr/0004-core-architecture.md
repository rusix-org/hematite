# ADR 0004: core architecture and boot contract

Status: **Proposed; requires Aaron's approval and independent design review.**
Date: 2026-10-25
Scope: a from-scratch implementation, conditional on the project-selection ADR.
No implementation, boot, or soundness claim is made here.

## Context and proposed decision

Use the framekernel boundary required by [AGENTS.md](../../AGENTS.md), with
an explicit capability model for physical memory, address spaces, devices,
and CPU execution. Keep Linux ABI policy outside the trusted core. This
document proposes mechanisms; it does not authorize kernel implementation.

### Crates and trust boundary

- `hm-core`: boot entry, architecture instructions, page tables, physical
  frames, fault recovery, CPU contexts, synchronization primitives, MMIO,
  DMA, user-copy, and the allocation boundary. Public fields cannot expose
  authority; raw addresses alone cannot create capabilities.
- `hm-kernel`: safe scheduling policy, process and VM policy, dispatch,
  filesystem and network services. `#![forbid(unsafe_code)]`.
- `hm-abi`: safe, explicitly sized ABI constants and byte codecs, with no
  pointer dereferences or casts from arbitrary bytes to Rust values.
  `#![forbid(unsafe_code)]`.

The core supplies the entry adapter and calls a safe kernel entry function.
Linker metadata, assembly, special exported symbols and interrupt stubs stay
in the core. A separate test image uses the same boundary. Third-party unsafe
code reachable through otherwise-safe crates remains part of the trusted
computing base: audit the transitive dependency graph, not just source lint
results. Toolchain runtime/compiler builtins are recorded separately.

### Memory ownership and user access

`Frame` owns allocated RAM; `AddressSpace` owns mapping authority. A mapping
retains its backing frames. No safe API maps arbitrary physical addresses,
retypes live page-table frames as data, creates a writable alias of kernel
code, or releases frames still accessible by another CPU or device. Reclaim
after unmap requires completed TLB invalidation on every relevant CPU.
Boot data, firmware regions, MMIO, kernel image and allocator metadata are
reserved before frames become available. Allocation failure is an ordinary
error; user-directed allocations use fallible operations and explicit limits.

Use 4 KiB pages and four-level x86_64 paging initially. Keep kernel mappings
supervisor-only with NX and W^X; guard kernel stacks. Require CPU support for
NX, SMEP and SMAP in the declared guest profile and fail boot clearly if
missing. The direct map must respect executable-page permissions as well:
a writable alias would defeat W^X. Fixed virtual layout initially, KASLR later.

`UserRange` contains an address-space handle, checked address and length, never
a Rust reference into user memory. Each operation checks canonicality, overflow,
user bounds and current mapping permissions. Validation alone is insufficient:
mapping locks or page pins cover each bounded copy; user-copy assembly handles
faults and brackets SMAP access. No locks needed by fault handling may be held
across a faultable copy. The core returns copied length plus fault status so
the ABI layer can implement partial transfers. Copy structured input into
kernel-owned bytes and validate it there; re-reading user memory does not
produce a trusted snapshot. An eventual futex primitive needs its own reviewed
atomic user-access API; ordinary copying cannot implement it.

CPU, DMA and user aliases must not be presented as exclusive Rust references.
DMA buffers use states for CPU ownership, device ownership and completion;
CPU access is unavailable while device writes are possible. Dropping an
in-flight request cannot free its storage. Completion/reset must quiesce the
device before reclamation. MMIO permits only checked register widths/offsets
inside an acquired device region. Virtio metadata stays untrusted even after
device completion; validate indices, lengths and descriptor chains.

Register bounds alone do not make MMIO safe: a DMA-address register can grant
the device access to unrelated memory. The core must own DMA-capable register
programming and descriptor publication, accepting owned buffer capabilities
instead of arbitrary physical integers. Safe device policy cannot bypass this
through PCI configuration, queue setup, indirect descriptors or reset. The
per-device core interface and any required isolation assumptions need their
own review before exposing a safe API.

A frame pin preserves allocation lifetime, not current user permissions.
Specify the synchronization/linearization of copying against permission
changes, unmapping and address-space replacement in the user-copy API design;
do not treat pinning as a substitute for that contract.

### Concurrency and execution

Use preemptive kernel threads, initially one global run queue with a stated
lock order, then per-CPU queues only after measurements justify them. Timer
interrupts request rescheduling; context switches occur only at core-defined
safe boundaries. Core context objects own stacks, register and extended-state
storage. Saving/restoring SIMD state must precede enabling its use across tasks.

Interrupt/preemption guards are CPU-local and non-transferable. Spinlock
guards disable local preemption; interrupt-shared locks also mask the relevant
interrupt context. Lock acquisition uses acquire ordering and release uses
release ordering; device DMA needs its own specified barriers. Sleeping locks
are forbidden in interrupt context or while a spinlock/interrupt guard is held.
Wait queues atomically register a waiter and recheck its condition under the
condition lock before sleeping, preventing lost wakeups. Safe callers cannot
switch away while retaining CPU-local mutable access. SMP requires reviewed
TLB shootdown, AP startup, lock ordering and frame-reclamation designs before
being enabled. Do not infer safety from a single-CPU boot.

Safety cannot depend on `Drop` running: callers may forget guards, contexts or
DMA requests. Forgotten objects may leak resources or prevent progress, but
must not permit early reclamation or overlapping mutable CPU-local access.
Review nesting, cancellation and forgotten-guard paths explicitly; making a
guard non-transferable is only one part of that contract.

### Syscall boundary

Core entry assembly switches to a guarded kernel stack before handling user
state, saves registers and masks unsafe flag states. A safe dispatcher accepts
six integer arguments and the syscall number; x86_64 Linux is the only initial
ABI (no x32/i386). Unsupported numbers return `ENOSYS`. ABI conversion checks
sizes, flags and arithmetic; user pointers become `UserRange` values. Return
state is validated by the core, including canonical addresses and allowed
flags; use a conservative return path until optimized return is independently
reviewed. Per-syscall results and errno behavior come from the pinned Linux
oracle, not from assuming all failures are interchangeable.

### Boot protocol and guest profile

Propose **PVH direct ELF boot** for QEMU x86_64, with a small 32-bit entry shim
inside `hm-core` that establishes owned tables/stack and enters 64-bit Rust.
The [PVH ABI](https://xenbits.xen.org/docs/unstable/misc/pvh.html) specifies a
32-bit entry note and initial register state; it does not by itself prove a
particular QEMU/firmware combination works. Pin and verify that combination
before treating this choice as implemented. Firecracker support is not implied.

Target QEMU `microvm` with virtio-mmio. Its [documented device model](https://www.qemu.org/docs/master/system/i386/microvm.html)
has no PCI and requires host-side kernel loading. Device discovery, RAM map,
AP enumeration and interrupt routing must be specified for the pinned version;
do not hard-code a demonstration's addresses as a general discovery mechanism.
Use modern virtio transport; virtio-console is the deployment console.

The founding plan explicitly requires serial plus `isa-debug-exit` for tests.
Use those only as test instrumentation, with PIC/PIT/RTC unused; deployment
uses virtio-console. The selected machine must support the required test
instrumentation; verify that combination before pinning it. An ISA-only test
boot does not establish the deployment device gate. This implements the stated
test requirement without adding legacy hardware to the deployment target.

Limine would simplify the initial long-mode transition, but introduces a
bootloader/image path to reconcile with direct microVM loading. Multiboot2
likewise needs a compatible loader. PVH costs extra entry/boot-information
review; its direct-loader fit is the reason for the proposal, pending a pinned
QEMU feasibility check. No loader code has been written or executed.

## Verification and remaining decisions

Before implementing this design: resolve project selection, approve this ADR,
pin the boot toolchain/profile, and obtain independent review of the specific
core APIs. Each later core design must include invariants, failure paths and
reclamation rules; this outline is not that review.

An [independent AI review](../reviews/0004-core-architecture.md) identified the
DMA authority, forgotten-guard and pin-versus-permission constraints above.
They are now explicit design requirements, not verified API implementations.
Pinned PVH/microvm boot and device discovery remain unresolved feasibility
items. The review is neither human approval nor a soundness certification.

Phase 0 evidence must include a bounded VM run, exact image/toolchain/source
identities, raw serial logs, a complete expected test inventory, and matching
debug-exit status. Missing, duplicate, skipped or timed-out cases fail. Check
page permissions and CPU feature enablement in the guest, not just in build
configuration. Phase 1 adds allocation exhaustion, mapping alias/reclamation,
interrupt/preemption and multi-CPU stress cases. Phase 2 adds faulting user
ranges, permission changes, cross-page partial copies and ABI differential tests.

Open approval items: PVH versus Limine; exact Linux
LTS oracle and initial workload; boot metadata/discovery contract. No versions
are selected implicitly by links to moving documentation.

## Sources consulted

Accessed 2026-10-25 for architectural constraints; no Linux implementation
source was consulted for this ADR. Pin source revisions during implementation.

- [Rust Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html)
  — safe-client soundness, aliasing and data races.
- [PVH boot ABI](https://xenbits.xen.org/docs/unstable/misc/pvh.html)
  — entry state and boot-information contract.
- [QEMU microvm](https://www.qemu.org/docs/master/system/i386/microvm.html)
  and [fw_cfg](https://www.qemu.org/docs/master/specs/fw_cfg.html)
  — machine constraints and firmware information interface.
- [Limine protocol](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md)
  — alternative boot contract.
- [Virtio 1.3 draft specification](https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html)
  — device/transport design reference only, not an adopted dependency or version.
