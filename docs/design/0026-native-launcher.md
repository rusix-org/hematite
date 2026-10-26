# Ordinary native program launcher

Status: implemented and verified locally, 2026-10-26. Independent
[review 0026](../reviews/0026-native-launcher.md) and all affected compiled-stack
gates preceded guest execution; [evidence](../evidence/native-launcher.md) records
the full local suite. Matching remote CI remains open.
This adds user-facing native program launch to the ordinary ARM64 serial monitor.
It composes [0024 archive admission](0024-initramfs.md),
[0025 archive execution](0025-archive-execution.md) and the protected process
mechanism in [0023](0023-arm-native-process.md). It does not add spawn, concurrent
processes, IPC, a filesystem, command arguments, a Linux adapter or a service
workload. Existing local evidence and unpublished CI gates remain distinguished.

## User behavior and normal applications

`make run` on ARM64 boots the existing monitor with `apps` and `run <path>` in
addition to help/status/test. `apps` lists actual regular-file paths from the
fully admitted normal application archive, in archive order. It does not invent
an executable namespace independently of the archive. `run` accepts one exact
archive path: no aliases, whitespace splitting into arguments, `./` removal,
search path or fallback. The existing64-byte input-line bound remains; therefore
interactive paths are at most60 bytes including any typed suffix after `run `.
Bare `run`, `run `, multiple separator spaces, or any whitespace inside the
path prints `Usage: run <archive-path>.` and returns the prompt. Commands and
paths are case-sensitive. Invalid/missing/directory/non-ELF selection prints an
error and returns the prompt. `apps` never initializes execution authority.

Normal output is the native program's console bytes, followed by one human
termination line: `Program exited (status N).`, `Program faulted.`,
`Program timed out.`, or `Program exceeded its call limit.`. The prompt returns
only after the existing root/control restoration and owned-frame reclamation.
Bootstrap tests remain available between launches. Status must no longer claim
there is no userspace on an ARM launcher build; it reports native availability
and diagnostics without claiming networking, storage or a general process OS.

Ship a separate small normal archive with this exact order:
`bin`, `bin/hello`, `bin/info`, `demo`, `demo/fault`, `demo/loop`.
Both directories precede their children. All four programs are separately linked
native Rust inputs with the reviewed assembly entry/syscall shims, no external
crates, no kernel absolute symbols and no process-test fixture fallback.

- hello validates startup and emits `hello from EL0\n`, then exits0.
- info validates startup, reads its version/features/resource limits, formats
  these values in safe native Rust, emits one bounded console write, and exits0.
  Exact output is `native ABI 1\nconsole: granted\nwrite limit: 256 bytes\nconsole budget: 4096 bytes\ncall limit: 64\n` for the admitted wire.
  It is a small real interface-inspection utility, not a hardcoded kernel message.
- fault deliberately writes its own RX entry using a PC-relative address, with
  an independently visible fault instruction symbol; it never targets kernel BSS.
- loop executes a named single-instruction self-branch until the kernel deadline.

The demos make containment observable to an operator. This is modest usability,
not the later declared useful-service workload. Argument-driven utilities require
an explicit bounded, versioned startup extension rather than an undocumented
new register convention.

## Safe policy/platform boundary

Add a small `hm_kernel` native-launch API containing typed errors, reports and
status. Extend Platform with default-unsupported operations:

```rust
fn native_archive(&self) -> Option<&'static [u8]>;
fn run_native(&mut self, path: &str) -> Result<LaunchReport, LaunchError>;
fn native_status(&self) -> NativeStatus;
```

The immutable archive getter grants no execution or memory authority. Safe monitor
policy can fully parse/list it using the existing parser, then release the plan.
The ARM implementation of run_native uses its own fixed normal archive and
`boot_image::admit`; callers cannot substitute an arbitrary image/root/callback.
Return a successful completion report only after validating that all launch-owned
frames have been dropped and the measured free count equals its pre-load value.
Reports contain an outcome enum (Exited(u8), Fault, Deadline, CallLimit) and
copied diagnostic scalars: completed launches, last prepared generation and free
frames before/after. Errors distinguish Unsupported, Archive, Missing, Directory,
Elf, Unavailable, Busy and Resources; core admission/corruption errors are never
misreported as successful user exits. Preserve the underlying archive/ELF errors
where useful for tests without exposing pointers or mutable owners.

NativeStatus contains availability (Unsupported/NotInitialized/Ready/Unavailable),
completed launches, most recently successfully prepared generation (optional until one is
prepared), and current free frame count where supported. Status text is diagnostic:
`Native launcher: ...`, `Native launches: N`, `Last prepared generation: N|none`,
`Free frames: N`. The generation scalar is not a grant token. Default x86 methods
return Unsupported/None, and x86 help/status must accurately identify that scope;
attempting run there is a recoverable unsupported command, not a panic.

Launch admission and archive listing each use separate non-inlined helpers.
Do not retain Archive/Image/Space/Process storage in the long-lived monitor loop
or enlarge InputLine. The static input borrow can end after owned ELF copying.
The safe Platform trait never acquires callbacks into user policy while a process
lease is held. All messages through platform.write occur before entering or
after returning from run_native; process console output uses the fixed sink below.

## Permanent authority and initialization

BootPlatform retains exactly one private native execution state for its lifetime:
NotInitialized, Ready(Executor), or Unavailable. It never stores a Factory borrowing
another BootPlatform field. Construct a scoped Factory from the retained Pool and
verified immutable boot roots/topology for each load. Executor/Issuer remains in
Ready across commands, tests, user failures and cleanup. Never reconstruct an
Executor per command or temporarily discard it to work around Rust borrowing.
Compile `user_space` and `process` on all ARM64 builds using the architecture
cfg alone; keep process_checks/process_inputs behind process-tests and space_checks
behind space-tests. Add the normal launcher/app-input adapter on ARM64 only.
BootPlatform native state and diagnostic counters are present on ARM64 in every
feature combination, initialized without claiming Executor authority. The
process-test boot path keeps its existing trace Executor; its unused normal
launcher state remains NotInitialized. Exclusive boot paths and the common latch
prevent two issuers; no new giant feature-negation predicate chooses authority.
x86 gains only default safe Platform methods, with no ARM process module.

Existing non-Send/non-Sync affinity and the permanent CLAIMED latch remain intact;
BootPlatform containing Executor must not gain Send/Sync through a new wrapper.

On the first valid ELF launch, and before constructing Executor, run the existing
bounded timer delivery/context/quiescence/timeout preflight once. Require its
complete successful result, then use the existing private unsafe constructor
under the BootPlatform proof. Failed preflight/constructor sets Unavailable for
this boot, reports a recoverable launcher-unavailable error, and never retries
or resets the permanent latch. Invalid archive/path/type/ELF selection occurs
before this initialization and before allocating frames or issuing a generation.
Ordinary launch remains CPU0-only, with the existing peer-OFF/BUSY protocol.
The supported normal profile is one CPU at64/128MiB. Do not advertise ordinary
two-CPU support merely because the process-test executable admits that topology;
two-CPU launcher support requires an explicit later profile and integration gate.

After initialization, load/prepare/run follows existing ownership rules. Allocation
or prepare failure releases all owned frames and preserves the resident Executor.
Prepare already consumes an issuer generation before some later failures; gaps
are intentional and generations are never reused. Record the generation from Process::handle high32 immediately after successful
prepare, before run. Require the normal console grant nonzero, low32 slot equal1
and high32 generation nonzero before recording that high32 scalar; do not derive
a generation from a guessed UI launch count or expose the complete token.
The diagnostic means last successfully prepared process;
a failed prepare can consume a generation without changing that diagnostic. Completed launches count only terminal RunReport results,
using checked arithmetic; exhaustion retires launch authority rather than wrapping.
The required success-only interactive sequence has contiguous generations1..5.

Monitor `test` may call the existing timer and thread diagnostics between launches.
It must run with no live Space/Process and no BUSY process lease. Each later launch
still applies all current admission/readback checks; successful earlier preflight
never substitutes for present quiescence. A test command must not reset the issuer
or retain an alternate root. The existing forgotten Mapped owner remains charged and cannot be reallocated.
Space::build_tables installs only device, identity RAM and user roots; it omits
the boot owned-mapping arena. Thus this live boot alias is absent while EL0 runs,
and full root restoration restores its visibility before Rust resumes. Existing
Stage B tests already retain a live mapped arena across private-root probes;
source review must confirm the launcher keeps the same invariant, including
unchanged identity mappings for retained leaked data, the allocator claim
preventing frame reuse, no callback/peer, and no Rust/Mapper access until boot
root restoration and full TLBI. Amend the existing mappings::from_boot safety
contract, currently naming only the0022 probe window, to also name this reviewed
0023 assembly-only process window; do not infer an arbitrary permission to
switch roots around Mapper callers.
The core transition and complete failure teardown remain unchanged; unknown
restoration failure remains terminal.

## Sealed normal output versus test diagnostics

Preserve `Executor::from_boot(topology)` and its trace behavior/signature for
existing actual-type compiler probes. Add a separate private terminal constructor
named `pub(crate) unsafe fn from_boot_terminal(topology: &Topology)` returning
the same Result type, sharing checked initialization and the permanent latch
through private `from_boot_mode`. The private enum is OutputMode{Trace,Terminal}. Introduce a
private fixed output selection at construction, immutable
for its lifetime: Trace for the existing process-test profile, Terminal for the
ordinary launcher. This is an enum selecting audited internal behavior, not a
function pointer, generic Host, dyn Write, closure or public runtime sink setter.
The same private Host validates/copies owned user bytes while stopped under the
boot root. Terminal sends the successful copy through the bounded existing UART
writer; Trace preserves byte-for-byte HMPROC CALL/OUTPUT/RESULT behavior.

All current unconditional CALL/RESULT diagnostic sites must obey that selection,
not just OUTPUT. Terminal must not print process-internal HMPROC records. Output
permission, call/byte budgets, deadline, native return values and copy failure
semantics stay unchanged. Console bytes are native console-grant output; Terminal transport converts each LF byte to CR LF and passes other bytes
unchanged, matching ordinary monitor output. Wire budgets and successful byte
counts charge the original user bytes before transport conversion. Trace mode
is unchanged. Exact interactive expectations include this transport policy.
No external callback runs while BUSY or process GIC authority is held. Formatting
a human completion line belongs to safe monitor policy after core teardown.

## Build, resource and verification gates

Build a dedicated deterministic normal application archive from dedicated inputs,
with independent GNU listing/extraction and manifest hashes. Do not reuse existing
process.S merely by cfg selection: its retained absolute kernel_text/kernel_data
references couple even nominally ordinary fixtures to a test kernel layout.
The normal builder uses no kernel --defsym values. Verify no such symbols exist,
record own ELF entry/segments/write/exit/fault/loop symbols and inspect opcodes.
Require the exact unique read-only PT_LOAD archive embedding and absence of raw
fallback ELF embeddings established by0025. This archive requires no kernel-BSS
fixed-point regeneration; normal app code uses only its own mappings and startup.
Existing process-test fixed-point bindings and strict tests remain separate.

Before guests, independently review changes to constructor output selection,
private sink, BootPlatform storage/lifetimes and safe interface. Inspect compiled
full call chains for ARM normal launch and ARM/x86 monitor `test`, and every
other affected ARM feature profile: the unconditional BootPlatform state changes
boot_main live storage in process, space, preemption, secondary and SMP builds
including negative profiles. Recheck their prior maximum paths with the new
compiled caller sizes rather than assuming only launcher stacks changed. The current
baseline path leaves little64KiB-stack headroom; adding Platform fields or a
retained monitor frame can break a previously passing baseline. Include timer
preflight, archive parsing, owned loading, prepare/run and cleanup branches.
Do not treat small source structs or the process-test stack result as proof.

Interactive tests must boot the ordinary monitor in64/128MiB one-CPU ARM profiles
and drive: test (warm-up), apps, status, run bin/info, status, run bin/hello, status,
run demo/fault, status, run demo/loop, status, test, run bin/hello, status.
Require exact visible app bytes/termination/prompt behavior; five completed runs,
contiguous successful generations1..5, and the same free-frame baseline after
every run and the interleaved bootstrap tests. The first warm-up test deliberately
retains three existing diagnostic frames once per boot (forgotten Frame,
KernelBox and Mapped owners); record the baseline only after that test. The later
test must retain no additional frames. This does not weaken per-launch reclamation
or hide the existing diagnostic retention. Warm-up is only a test-baseline
choice, never a user prerequisite. A separate fresh128MiB boot must run
status → run bin/hello → status without any preceding test command: require
lazy initialization, generation1, exit0, unchanged initial free count, two
source-EL0 SVCs and exactly the8-IRQ launcher preflight. Add missing path, directory,
unsupported path syntax and input overflow/recovery cases; rejected selections
must not initialize/reset the issuer, consume a generation or allocate frames.
All four shipped programs must execute, not merely be listed.

Independently correlate actual source-EL0 traces to the normal ELF symbols:
info and each hello produce one write SVC plus exit SVC (six SVCs total), fault
produces the exact RX-write permission abort, and loop ends in one lower-EL timer
IRQ. Preserve per-slice restoration/GIC ordering checks. Distinguish the one
initial timer preflight and interleaved test-command EL1 interrupts with strict
bounded grammar: the warm-up and interleaved test each produce16 EL1 timer IRQs,
and the one launcher preflight produces8. Do not allow arbitrary EL1 noise to make
the launcher test pass.
The eight resulting process slices must have no unexpected extra transitions.
Host prompt success alone is not hardware isolation evidence. Host parser tests
must reject wrong output, sequence/generation/free-count drift, missing prompts,
wrong fault addresses and missing deadline IRQs.

Keep the complete existing33-case66-run archive process matrix, compiler lifetime
checks, all unsafe/safety gates and full ARM/x86 regressions. Existing monitor
harness expectations must be updated explicitly for new commands/status, with
x86 unsupported behavior tested. Publish concrete local evidence and remaining
CI scope; this milestone does not silently mark Phase3 or a useful OS complete.
