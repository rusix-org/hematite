# Protected native ARM process execution

2026-10-26, implementation commit `290c64e`, based on `4392dfa` and the archived
[source identities](process-final/source-sha256.txt). `make test-all` passed:
[summary](process-final/verification-summary.json),
[full log](process-final-test-all.log). Original failed first-run results remain
unchanged; fresh results use the independently reviewed exact trace grammar.

This milestone implements [design 0023](../design/0023-arm-native-process.md):
CPU0-only protected native ELF execution, versioned console/exit calls, scoped
console authority, typed checked user copying, synchronous fault containment,
an absolute timer deadline, and owner reclamation. CPU1 stays OFF. Embedded
ELFs are separately compiled native Rust programs, not Linux binaries.

The [independent review](../reviews/0023-arm-native-process.md) covers the source,
fixtures and verification contract. The [compiled-stack review](process-stack-review.md)
binds the exact executable and inspected assembly. Maximum inspected aggregate
kernel stack use is 61,712 of 65,536 bytes; this is a static bound for the
reviewed paths, not a runtime watermark or a future-code guarantee.

## Evidence and failures

The final archive contains 64 passing result-bearing ARM records and eleven
x86 records. All 115 Python harness tests pass per target, including eighteen
process-parser mutation tests. The ten native policy host tests, all earlier
scheduling/allocator/address-space profiles, monitor and negative tests pass.
The [three final process profiles](process-final/aarch64/process/result.json)
pass 66 runs each, followed by the 41-case baseline in each guest. Nineteen
[process compiler clients](process-final/aarch64/process-types/result.json)
and [fixture reproduction](process-final/aarch64/process-fixtures/manifest.json)
also pass. The executed ELF is byte-identical to the independently reviewed
`d8505cb2973854287b474e8d08175be76af9e9957e3e641d2a67d342c8269d52` binary.


- [First QEMU results](process-first-qemu/result.json) preserve all three raw
  guest traces and failed host results. Every guest reported 66 process runs
  and the 41-case baseline, then exited zero. The first validator rejected
  saved branch-type state on the two intentional indirect execute faults.
  Architectural review identifies BTYPE01 from BR; the parser must require
  that exact value only for those cases, preserving all other PSTATE checks.
  Further review found pinned QEMU swaps op1/op2 in the trapped DAIF-write
  syndrome: it emits `0x620793e4`, versus architectural `0x620cd3e4`. The
  validator requires the exact pinned value; it does not accept both or mask
  the difference. Primary-source provenance is recorded in review 0023.
- [Compiler boundaries](process-first-types/result.json): nineteen actual-module
  clients, including fifteen expected rejections. These validate ownership and
  authority types; they do not establish runtime isolation.
- [Fourth build log](process-fourth-build.log): clean release build and Clippy.
  Earlier logs retain the invalid fixture STP immediate, missing ARM disassembler,
  and lint failures. Earlier stack candidates were caught before guest execution;
  splitting ownership-heavy helpers and borrowing returned owners reduced peaks.
- [Seventh fixture build](process-fixtures-seventh/manifest.json) records the
  final canonical ELF identities. Disassembly and readelf outputs bind LOAD
  permissions, actual fault instructions, syscall sites and kernel addresses.
  The pinned build image adds the ARM binutils disassembler from its existing
  Debian snapshot; Rust and QEMU pins are unchanged.

The 33-case inventory runs twice per guest at 64MiB/one CPU, 128MiB/one CPU and
128MiB/two CPUs. It covers hello output, malformed arguments and handles,
write/byte/call limits, stale or absent authority, register restoration,
invalid SP at exit, RX write and RW/stack execution faults, privileged memory
and device access, timer/interrupt tampering, a preempted infinite loop,
startup-page writes and forbidden EL0 semihosting. Preparation budget failure,
real frame exhaustion and inactive process drop must preserve/reclaim owners.
All runs must restore the kernel root and controls and exactly recover the
free-frame baseline before another process starts.

The pinned QEMU trace does not demonstrate retention of user DIT into EL1:
its exception implementation clears that bit before logging the full state. The fixture executes DIT/SSBS
writes and checks sanitized resume, and the kernel reads back restored controls;
these observations do not prove recovery from a retained DIT1 state on hardware.

No initramfs, concurrent userspace, interactive userspace shell, Linux ABI,
physical ARM hardware or performance advantage is established here. The full
Phase 2 gate remains open. No new remote CI or public push is claimed.

Reproduce with `make test-all` from the repository root. This builds the pinned
container, rebuilds every fixture, checks all type/host gates, and executes the
ARM and x86 profiles serially. Expanded commands, tool versions, binary hashes,
raw serial/exception logs, timeouts and result records are in the final archive.
The all-feature Clippy gate is clean. The standalone space-only release image
emits two dead-code warnings for process-only helpers/retained pool access;
these do not affect compilation or test outcomes.
