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

At the monitor prompt, try:

```text
apps
run bin/hello
run bin/info
run bin/echo hello from ARM
run demo/fault
run demo/loop
run-group bin/echo bin/echo
run-group demo/loop bin/hello
run-task bin/supervisor
status
```

The ARM launcher runs isolated native programs from the checked application
archive. `hello` prints from userspace; `info` reports its startup capabilities
and budgets. `echo` receives a copied startup payload and writes it back.
Spaces after the path are preserved; there is no quoting or argument expansion.
The fault and loop demos return to the prompt after a contained
fault or timer deadline. `status` reports launch counts, the last prepared
generation, free frames and active memory protections. Native launch currently
supports one ARM CPU at 64/128 MiB; the x86 monitor reports it as unsupported.
Programs can receive up to 256 opaque startup bytes through the typed native
interface. Interactive commands retain the 64-byte line limit. Console reads,
argument vectors and arbitrary executable spawning remain unimplemented.

`run-group` admits two programs before running either and schedules them in
turns. The echo pair receives `worker-a` and `worker-b`; their text and newline
writes can interleave. In the loop/hello pair, hello finishes while the loop is
preempted, then the loop exhausts its budget. These demos and the broader cancellation, exhaustion and reclamation suite
pass locally, including the full ARM64 and x86_64 regression run.

`run-task bin/supervisor` demonstrates native userspace task creation. The
supervisor starts a finite worker and a looping child, waits for the worker,
cancels the loop and exits after both children are reclaimed. It prints
`worker complete` and `supervisor complete`, then returns to the monitor.
Repeated runs preserve free-frame counts, and ordinary programs still launch
afterward. The three ordinary QEMU profiles pass with independently reviewed
traces; the broader negative authority matrix is still pending.

This first task interface uses a fixed two-program catalog and at most two
outstanding children. Only the supervisor receives launch authority. A child
completion handle grants explicit wait/cancel rights; completion consumption
releases its slot. This is a bounded native design experiment, not a Unix
fork/exec implementation.

Type `help` for commands;
`test` runs 41 bootstrap, capability, memory, topology, event, mutex, thread, timer, exception and mapping checks. The ARM boot banner reports discovered RAM and usable frame capacity.
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
builds use its cache. The default QEMU run uses software emulation, one CPU and 128 MiB of RAM;
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
up to whole pages. The same pool now supports concurrent allocation on both
ARM CPUs, with zeroing before publication, exact exhaustion/reclamation checks,
and reentrant destructors. Standard Rust collections and small-object size
classes are not integrated yet. See [allocator verification](docs/evidence/concurrent-allocation.md).

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
Dedicated two-CPU tests now run up to four preemptive callbacks per CPU, with
shared mutex contention and cross-CPU Event wakeups. Userspace threads remain
unimplemented.

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
exercise both accepted profiles and excess-CPU rejection. Dedicated lifecycle
tests start and stop CPU1 twice, confirming
firmware OFF before reusing its state. Timer and scheduler state
now live in CPU-local records, with affinity validation and inactive-record
canary checks. Shared interrupt-controller setup is separate from local timer
setup and teardown, with preservation checks across local sessions. Secondary
CPU stacks and a root excluding the mutable mapping arena are now reserved
and checked against allocator exclusions. CPU1 verifies its installed root,
stack and translation permissions. `make test-all` exercises this at both RAM
sizes and checks failure paths. Concurrent scheduling tests verify each CPU's
interrupts, worker stacks and register preservation, then confirm CPU1 is OFF
before reuse. See [SMP verification](docs/evidence/arm-smp-scheduling.md).

A safe ELF parser now admits a bounded static AArch64 profile and produces a
validated, borrowed load plan. Tests compare a separately linked ELF with
`readelf` and reject malformed images, unsafe permissions and lifetime escapes.
See
[ELF admission evidence](docs/evidence/elf-admission.md).

ARM now also loads that ELF into owned address spaces with protected code,
read-only data, writable data and guarded stacks. Three QEMU profiles verify
allocation rollback, maximum-size loading, ownership retention and repeated
page-table restoration using ARM translation probes; see
[owned-space evidence](docs/evidence/owned-address-spaces.md).
The native console/exit policy now has versioned startup bytes, scoped grants,
checked copying and resource limits, verified on the host and by compiler
boundary tests; see [native policy evidence](docs/evidence/native-policy.md).

Protected native ARM userspace now executes separately linked Rust programs.
The process tests cover console/exit calls, invalid arguments, capability limits,
register restoration, protection faults, timer termination and memory reclamation.
`make test` runs 33 programs twice in each of three ARM guest profiles and checks
the actual exception trace. These archive-backed programs are a development milestone;
normal userspace startup, a shell and Linux compatibility remain future work. See
[native process evidence](docs/evidence/native-process.md), including the pinned
emulator limitations.

A safe bounded newc initramfs parser now validates whole archives before file
lookup. GNU cpio reference checks and malformed-input/lifetime tests cover
admission. The process test kernel now loads all 33 programs from that checked
archive, with exact embedded-payload verification and no individual ELF fallback.
See [archive execution evidence](docs/evidence/archive-process-execution.md).

The safe capability experiment checks rights attenuation, bounded accounting,
stale handles, revocation and cancellation/completion lifetimes. It models
metadata only; userspace identities and actual device ownership come later.

See [PROGRESS.md](PROGRESS.md), [the bootstrap design](docs/design/0001-bootstrap.md),
[decision records](docs/adr/), and [AGENTS.md](AGENTS.md). Public repository publication is authorized by [ADR 0007](docs/adr/0007-public-repository.md);
the proposed license has not been adopted. The evaluated
Asterinas code in ignored `.work/` is not part of this kernel.
