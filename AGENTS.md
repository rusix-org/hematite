# hematite

A from-scratch Rust-native kernel exploring safer, simpler OS designs, with
best-effort Linux userspace compatibility where useful. This file holds the
objective, requirements, phases, and working rules. The original strict Linux
ABI objective was revised by Aaron on 2026-10-25; see ADR 0005.

The name is a placeholder until a conflict check passes (Phase 0).

## Objective

Build a memory-safe Rust-native operating system for a narrow, declared target
workload. Explore better resource, concurrency, isolation and I/O interfaces
without inheriting Linux's internal design or every historical ABI constraint.
"Better" requires explicit hypotheses and measurements, not a blanket claim.

- Native interfaces are the primary design surface. Define versioned, explicit
  wire layouts at userspace boundaries; do not expose the compiler's Rust ABI.
- Linux userspace compatibility is a useful secondary surface, implemented
  selectively through an adapter. Use a pinned Linux LTS as a behavioral
  reference for supported interfaces, not as a universal conformance mandate.
  Document deliberate differences and missing features. Do not label a native
  or ported program as an unmodified Linux workload.
- Initial deployment target: an **x86_64 virtio guest** (QEMU/KVM microVM
  class): virtio-blk, virtio-net, virtio-console/serial, no legacy hardware.
  arm64 virtio guest is the second target. Nothing else until those work.
- "Useful" means: a declared real service workload runs end to end, with every
  userspace-reachable path in memory-safe Rust and `unsafe` confined to an
  audited core. State whether each workload is native, ported, or an unmodified
  Linux binary. Demonstrate the chosen design improvements with evidence.
- Not goals: running every Linux binary, supporting physical hardware broadly,
  loading Linux kernel modules, or matching Linux internals or performance in
  every case. Gaps are documented, not hidden.

Predecessor: `../rusix` (an incremental Linux fork). Its verification
discipline carries over; its code and Linux-fork architecture do not.

## Phase 0 gate: decide before building

The founding approval gate covered the decisions below. Aaron subsequently
selected scratch and explicitly authorized a runnable local kernel, then
approved the native-first objective (ADRs 0001 and 0005). This authorizes the
local bootstrap; unresolved licensing and public naming remain restrictions
on distribution/public use, not a reason to stop the authorized local work.
Core mechanisms still require written design and independent review.

Decision records:

1. **Evaluate existing projects, especially Asterinas.** Asterinas is a
   Rust, Linux-ABI-compatible "framekernel" aimed at VMs, which is close to
   this objective. Build it, boot it, and record what runs, what is missing,
   its architecture, license, governance, and activity. Also briefly assess
   Maestro, Kerla, and gVisor (as an ABI-reimplementation precedent). Conclude
   with a recommendation: contribute to or fork an existing project, or build
   from scratch. If from scratch, state the concrete design difference that
   justifies it. Aaron makes the final call.
2. **License.** Choose and record it (e.g. MPL-2.0, Apache-2.0/MIT, or
   GPL-2.0). If not GPL-compatible with Linux, the clean-room rule below is
   mandatory, not advisory.
3. **Name.** Check for conflicting projects/trademarks before any public use.
4. **Core architecture** (see Requirements): `unsafe` boundary, memory model,
   concurrency model, syscall dispatch design, boot protocol.

## Requirements

### Safety architecture

- Framekernel structure: a small core crate (working name `hm-core`) owns all
  `unsafe` code: paging, physical memory, context switch, interrupt entry,
  MMIO/port I/O, DMA, user-memory copy. Every other crate declares
  `#![forbid(unsafe_code)]`, enforced in CI.
- Every `unsafe` block in the core has a `// SAFETY:` comment stating the
  invariant and why callers uphold it. Core APIs must be sound: no safe
  function may allow undefined behavior from any input.
- User memory is only accessed through typed core APIs that validate ranges
  and handle faults; never by raw pointer in syscall code.
- Kernel paths must not panic on userspace input. Errors become errno values.
  Integer overflow behavior is explicit (`checked_`/`wrapping_`/`saturating_`).
- Hardening from early on: SMEP/SMAP, NX, W^X kernel mappings, guard pages on
  kernel stacks. KASLR later.

### Clean-room and provenance

- Implement from specifications and observed behavior: man pages, POSIX,
  the Linux UAPI headers (syscall numbers, struct layouts, constants), virtio
  and ELF specs, filesystem format documentation, and tests run against real
  Linux. Do not copy or translate Linux kernel source. Record sources consulted
  for each subsystem.
- Third-party crates are allowed when pinned, license-compatible, reviewed,
  and recorded (e.g. `smoltcp` for TCP/IP is a reasonable candidate). Crates
  inside the trusted core get the same scrutiny as our own unsafe code.
- AI agents must not add Signed-off-by or claim human review. Disclose
  substantial AI assistance in commit messages.

### Toolchain and build

- Cargo workspace. Pin the exact toolchain in `rust-toolchain.toml`. Prefer
  stable Rust; each nightly feature used is listed with its reason and an exit
  plan.
- Target `x86_64-unknown-none` (custom target JSON only if required).
- Boot protocol: choose and record one (e.g. Limine, Multiboot2, or PVH for
  direct QEMU/Firecracker kernel boot).
- A pinned container image (Dockerfile) defines the build/test environment:
  Rust toolchain, QEMU, test userspace inputs. CI and local runs use the same
  image. One command builds; one command runs the full test suite in QEMU.
- `Cargo.lock` committed; dependencies vendored or hash-pinned.

### Testing and verification

- **CI from the first commit** (GitHub Actions; Linux runners expose KVM).
  Required jobs fail on any failure, skip, missing case, or timeout.
- In-kernel unit/integration tests run inside QEMU and report through serial
  plus `isa-debug-exit`; the host harness parses strict results.
- Userspace compatibility suites, added as phases allow:
  gVisor's syscall tests, LTP, musl `libc-test`, busybox's test suite, and
  selected kselftests that exercise userspace ABI.
- **Differential testing:** Linux compatibility tests also run on a pinned
  Linux LTS in the same VM configuration. Classify results as matching,
  unsupported, intentionally different (with a decision record), or buggy.
  Native interface tests establish their own explicit contracts; Linux does
  not define those contracts.
- Maintain a generated **compatibility matrix** as compatibility interfaces
  are introduced: syscalls and major `/proc`, ioctl and socket-option surfaces,
  with supported scope and covering tests. Track native progress separately
  through invariant/lifetime tests, resource bounds, isolation behavior,
  workload success and performance measurements. Neither line count nor Linux
  syscall coverage alone establishes success.
- Fuzz syscalls (start with a simple argument fuzzer; evaluate syzkaller
  descriptions) once Phase 3 exists. Any kernel crash is a release blocker.
- Record exact commands, toolchain, commit, and raw logs for verification
  claims. Preserve failures. Never mark an unexecuted test as passing.
  A boot or a passing subset is not full compatibility.

## Phases

Each phase ends only when its exit gate passes in CI. Gates may be refined by
a recorded decision; they are never weakened silently.

### Phase 0: decisions and skeleton
Phase 0 gate decisions (above), approved. Workspace, toolchain pin, container
image, CI, `hm-core`/forbid-unsafe split, serial logging, QEMU test harness.
**Exit:** kernel boots in QEMU via the chosen protocol, prints to serial, runs
an in-kernel test suite, and CI reports pass/fail strictly.

### Phase 1: core kernel
Physical frame allocator, paging and kernel address space, kernel heap,
exceptions and interrupts, APIC timer, kernel threads, preemptive scheduler,
spinlocks/mutexes/wait queues, SMP bring-up.
**Exit:** preemptive multitasking on multiple CPUs under stress tests;
allocator and scheduler tests including exhaustion and concurrency cases.

### Phase 2: first userspace
User address spaces, static ELF loader, protected entry/return, safe user-memory
copy, initramfs, and a minimal versioned native ABI for process startup, owned
handles, console I/O and exit. Add a small Linux adapter for unmodified static
musl hello-world when it fits the design; do not embed Linux semantics in core
ownership APIs.
**Exit:** an isolated native program runs; bad pointers/handles and process
termination cannot violate ownership. Resource reclamation tests pass. Report
Linux hello-world compatibility separately, including any remaining gaps.

### Phase 3: processes, services, and selected compatibility
Capability-scoped spawn, resource budgets, threads, wait/cancellation semantics,
service communication, clocks and a VFS with explicit ownership. Give each
experiment a written hypothesis and exit tests. Add Linux process, pipe, futex,
TTY, procfs and linking features as selected workloads need them; `fork`, global
namespaces and POSIX signals are not mandatory native mechanisms.
**Exit:** multi-process native workloads under resource exhaustion/cancellation
and isolation tests; chosen Linux tests have documented results, and syscall
fuzzing runs crash-free for an agreed duration. Unsupported compatibility is
visible and does not silently weaken native contracts.

### Phase 4: devices and storage
PCI and virtio (virtio-pci and/or virtio-mmio), virtio-blk, block layer, page
cache, ext2 then ext4 (read, then write, then journaling), `fsync`
semantics, filesystem images created and checked by Linux's `e2fsprogs`.
**Exit:** create/mount/use/remount ext4 on virtio-blk; `e2fsck` reports clean
after workloads; filesystem LTP subsets pass differentially.

### Phase 5: networking
virtio-net, TCP/IP (own or a vetted crate), sockets API including
`AF_INET`/`AF_INET6`/`AF_UNIX`, socket options used by target workloads.
**Exit:** the declared native or compatibility service serves host traffic;
resource-lifetime and cancellation stress tests pass. Run differential network
tests for the Linux socket behavior actually claimed.

### Phase 6: useful system
Run the declared target workloads end to end. Compare equivalent workloads
against pinned Linux on the same VM (throughput, latency, memory), with a
recorded budget and explicit native/ported/compatible implementation labels.
Measure the chosen architecture hypotheses, including failed hypotheses.
`unsafe` audit of `hm-core`. Published native contracts, compatibility matrix
and known-gaps list.
**Exit:** the "useful" definition in Objective is met and independently
reproducible from a fresh clone.

### Phase 7: arm64 and widening
arm64 virtio guest at parity with x86_64 test results. Then widen the
workload/ABI target by recorded decision.

## Working rules for agents

- Read this file, then `PROGRESS.md` and `docs/adr/` (once they exist). Inspect
  actual Git state and test results; ledger claims point to evidence, they are
  not evidence.
- Create and maintain `PROGRESS.md`: current phase, what is implemented,
  verification evidence with scope, blockers, next actions, short dated
  session entries. Mark items planned, implemented/unverified, verified
  (scope), or blocked.
- Record decisions as numbered ADRs in `docs/adr/`. Changes to the
  objective, target, license, or safety architecture require Aaron's approval.
- One home per rule: link to this file or an ADR instead of restating it.
  Keep docs short; let code, tests, and CI carry the argument.
- Small reviewable commits. Separate refactors from behavior changes.
- Design before code in the core: memory, concurrency, and `unsafe` APIs get
  a short written design and independent review before implementation.
- Aaron authorizes parallel sub-agents for independent work with clear file
  ownership; serialize builds per output directory and integration edits.
  Use independent review for core/unsafe changes.
- Resource limits on this laptop: build on disk, never in `/tmp`; modest
  parallelism (`-j2` / `CARGO_BUILD_JOBS=2` initially); bound VM memory and
  run time. Never install or boot hematite on physical hardware, modify the
  host bootloader, or touch the host kernel without asking.
- Do not create a public remote or push without Aaron's explicit go-ahead.
