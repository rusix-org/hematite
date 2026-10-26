# hematite (working name)

A from-scratch Rust-native OS exploring explicit ownership, capability-based
access, and better resource lifecycles. Linux compatibility is a selective
adapter, not the definition of the native interfaces. It currently boots on
an ARM64 QEMU `virt` guest into a serial monitor, with x86_64 microvm
retained as a regression target. **There is no Linux userspace support yet.**

With Docker running and Make installed:

```sh
make run    # build and open the ARM64 monitor; Ctrl-C exits
```

Type `help`, `status`, or `test`. `status` reports the active memory protections;
`test` runs 36 bootstrap, capability, memory, topology, event, mutex, thread, timer, exception and mapping checks. The ARM boot banner reports discovered RAM and usable frame capacity.
Memory tests exercise real frame, heap and mapping ownership. Three pages are deliberately
retained once per boot to verify that forgotten owners cannot cause reuse.
This is a kernel monitor, not a shell.

```sh
make build                  # ARM64 image under target/aarch64/monitor/
make test                   # ARM64 lint, host and QEMU checks
make test-all               # both architectures, run sequentially
make run ARCH=x86_64        # optional x86_64 monitor
```

The first command builds the digest-pinned container environment. Subsequent
builds use its cache. QEMU uses software emulation, one CPU and 128 MiB of RAM;
KVM and host boot changes are not required. Builds use two CPUs and at most
4 GiB container RAM. No disks or networking are attached to the guest.

Test runs preserve serial output, input hash, expanded QEMU command and results
under `target/evidence/`. Missing/duplicate/failed cases, unexpected exit status
and timeouts fail the command. GitHub CI runs the same `make test-all` command;
remote results are available in the repository’s GitHub Actions tab.

ARM tests also induce protected-memory faults and verify fixed-site recovery
on a separate exception stack. Unrecognized faults remain fatal.

ARM kernel code can now request fallible page-backed blocks and typed owners;
failed typed allocations return the original value. Allocations currently round
up to whole pages; standard Rust collections and concurrent allocation are
not integrated yet.

ARM also supports 64 owned kernel data-page mappings. Each mapping retains its
frame until unmapping completes translation-cache invalidation. Tests exercise
actual accesses, remapping, exhaustion and reclamation. Table storage is fixed;
user address spaces and concurrent mapping updates remain ahead.

ARM supports both scoped timer diagnostics and timer-driven kernel scheduling.
Tests verify full interrupted contexts, timer shutdown, and blocked delivery.

Up to four kernel callbacks can now run on separate guarded stacks and yield
cooperatively. Tests cover round-robin scheduling, preserved local values,
normal destructor execution, stack reuse and overflow rejection. A separate
preemptive runner schedules safe callbacks that never yield. `make test` also
runs its dedicated guest images, checks forced interrupts at execution
transitions, and reruns the bootstrap tests afterward in the same guest.
SMP and userspace threads remain unimplemented.

Shared kernel values can use an owned mutex whose guard controls access.
Contended cooperative workers yield while waiting. Tests cover real kernel
contention and parallel host threads using the same implementation; interrupt
handlers do not acquire these mutexes.

Preemptive ARM workers can wait on monotonic events. Waiting workers stop being
dispatched until notification changes their observation; notifications wake all
matching waiters. Snapshot before checking the synchronized predicate, then
wait if needed. This does not yet provide device wakeups, cancellation or CPU
idle: an all-blocked run retains its stacks while the coordinator spins.

CPU/PSCI device-tree parsing now describes the supported one- or two-CPU ARM
profile and rejects ambiguous or unsupported topology. Generated QEMU trees
exercise both accepted profiles and excess-CPU rejection. This is preparation
for SMP; the kernel still starts no secondary CPU.

The safe capability experiment checks rights attenuation, bounded accounting,
stale handles, revocation and cancellation/completion lifetimes. It models
metadata only; userspace identities and actual device ownership come later.

See [PROGRESS.md](PROGRESS.md), [the bootstrap design](docs/design/0001-bootstrap.md),
[decision records](docs/adr/), and [AGENTS.md](AGENTS.md). Public repository publication is authorized by [ADR 0007](docs/adr/0007-public-repository.md);
the proposed license has not been adopted. The evaluated
Asterinas code in ignored `.work/` is not part of this kernel.
