# ARM cooperative kernel threads

Status: independently AI-reviewed implementation contract; see
[review](../reviews/0010-arm-kernel-threads.md).
Date: 2026-10-26. Extends [owned mappings](0008-arm-owned-mappings.md) and
[scoped IRQ delivery](0009-arm-timer-irq.md).

Run real safe Rust kernel callbacks on separate guarded stacks, with explicit
cooperative yield and normal completion. This is a reusable bounded execution
mechanism, not a synthetic fixed assembly workload. It establishes contexts
and stack ownership before adding timer-driven preemption. IRQ remains masked
throughout Rust execution; this increment neither enables SMP nor satisfies
the complete Phase 1 scheduling gate.

## Safe policy interface

The safe policy crate defines `ThreadJob { entry, input }`, where `entry` is
`for<'a> fn(&'a mut (dyn ThreadContext + 'a), u64) -> u64` and input is a u64 value. The context
trait exposes only `id(&self) -> usize` and `yield_now(&mut self)`. Jobs carry
no borrowed pointers or captured closures. This is an internal Rust interface,
not a proposed stable userspace ABI.

`Platform::run_threads(&self, jobs: &[ThreadJob]) -> Result<RunReport, ThreadError>`
accepts zero through four jobs. More jobs return TooMany before running any
callback. Nested/concurrent execution returns Busy; x86 returns Unsupported.
Empty execution returns an empty report when the execution lease is available.
The report contains completed count, four indexed return values and yield
counts, total dispatches and a bounded dispatch trace of task IDs. Counters
saturate explicitly; the trace records its length and a truncation flag rather
than overrunning or silently reporting a complete history. Use 64 trace slots.

Execution is synchronous: the call returns only after every admitted callback
returns normally. A callback may yield any number of times or fail to return;
cooperative execution provides no latency or termination guarantee. There is
no timeout that discards suspended Rust frames, no cancellation/kill API and
no escaping thread handle. A panic terminates the kernel under its existing
abort policy, rather than unwinding across context-switch assembly.

## Stack reservation and exclusive execution

Reserve four permanent linker-owned stacks, each with 64 KiB usable storage
and a 4 KiB invalid guard below it. Keep their end inside `__kernel_end`, so
physical allocation excludes the complete reservation. Initial page-table
construction and complete BootChecks recognize all four guards. All usable
stack pages retain normal EL1 RW/PXN/UXN attributes. No API exposes their bytes
or lets a caller choose SP, resume PC or register context.

A `Runner` capability with private fields is constructed only by core's
crate-private unsafe boot constructor, after the complete stack/guard mappings
and EL1 state are validated. Its marker makes it non-Send/non-Sync. Safe `run`
requires this capability; an authorityless public free function is not exposed.
BootPlatform retains the capability and implements the safe policy interface.

A permanent core `AtomicBool` execution lease begins with compare_exchange and
returns Busy if another run owns the stacks. The lease is released only after
all admitted contexts complete and control is on the coordinator's stack.
There is no per-task destructor that can free a suspended stack. A callback
cannot obtain ownership of the lease or a context object to forget it. Forgetting
its borrowed context reference merely loses that reference; normal callback
return still completes the task. If a callback never returns, the lease remains
held and stack ownership remains intact.

Run metadata is stored in an `UnsafeCell<RunState>` on the original coordinator
stack, with a stable address for the complete run. This coordinator frame stays
live until all callbacks finish. Workers receive only private raw pointers
into that storage; no shared/exclusive reference to mutable scheduler metadata
or a whole task record remains live across a switch. Field reads/writes use
bounded raw projections; copy immutable function/input metadata before calling
policy. A worker's private context token lives on its own stack and carries
the run pointer and task ID with a non-Send/non-Sync marker. The scheduler
never reads or writes this token while its `&mut` borrow is suspended in yield.

The single active CPU, masked asynchronous exceptions and exclusive lease
serialize all scheduler mutations. Existing timer probes remain independent:
their handler uses only private state and never schedules or touches this run.

## Stack exhaustion is part of soundness

Safe callbacks must not be required to prove that their stack frames fit.
A guard alone does not suffice if a large stack adjustment can jump over it.
The pinned Rust 1.99.0 `aarch64-unknown-none` compiler emits inline page probes:
the actual 128 KiB safe-array fixture generated repeated 4096-byte SP decrements
and a load through each new SP before further allocation. Its LLVM IR carries
`probe-stack=inline-asm`. Preserve this compiler evidence and enforce a host
regression check; no custom target or nightly toolchain is introduced.

Also run a negative guest image whose ordinary safe callback uses a 128 KiB
stack array on a 64 KiB worker stack. It must fault in that worker's guard and
terminate through the separate emergency-stack fatal route, with no successful
test report. The host verifies actual fault FAR against the worker guard symbol.
The stable target/compiler, inline stack probing and invalid guard jointly
underpin the arbitrary-safe-callback claim. Changing target/compiler requires
revalidating this property before using the thread API.

## Context switch and lifetime transitions

Each worker and the coordinator have private 16-byte-aligned context records.
Use a `repr(C)` layout with compile-time size/alignment/offset checks matching
assembly. Cooperative switch is an ordinary AAPCS64 function call: preserve
x18–x30, SP, Q8–Q15 and FPCR/FPSR. Saving full Q8–Q15 is stronger than the ABI's
low-64-bit requirement. Caller-clobbered x0–x17, other SIMD registers and NZCV
are handled by normal compiler call semantics; do not claim asynchronous full
context preservation. DAIF remains masked and is verified at run boundaries.

The assembly switch writes the outgoing record, reads the incoming one, sets
SP and resumes through the incoming saved LR. It makes no Rust call while SP
is transient. It is not marked nomem/readonly/pure and does not unwind. Its
private callers supply distinct initialized records whose stacks are live and
exclusively owned. No safe caller can fabricate either record or resume it twice.

Initial worker context points to its stack top and a fixed assembly first-entry
trampoline, with run pointer/task ID in preserved registers. The trampoline
CALLs an `extern "C"` Rust adapter. That adapter constructs the local token,
calls the selected safe job and returns its u64 result normally. All Rust
callback/adapter frames and their local values therefore finish normally.
Only after that return does assembly store the result and mark the task Done,
then switch permanently to the saved coordinator context. This completion
path contains no live Rust frame and must never resume.

Yield performs Running -> Runnable and switches to the coordinator. The
coordinator uses a rotating next index to select the next Runnable task,
records dispatch, marks it Running and switches. A resumed yield returns to
the same token/callback frame; no other task can access that frame. A completed
task is never dispatched again. The coordinator may reset/wipe stack storage
only after every task is Done; no Rust values or references into those stacks
survive. Returned reports contain only copied integer metadata/results.

## Verification

Safe callback tests must exercise two or more workers with distinct inputs,
different yield counts and stack-local arrays whose contents survive many
switches. Verify exact round-robin dispatch order while runnable, returned
values, completion counts and no further dispatch of Done tasks. Exercise zero
jobs, all four slots, five-job rejection without running callbacks, repeated
runs and trace truncation. A core-only nested-run callback checks Busy without
changing current run state. A destructor-bearing local held across yields
must run its destructor once before callback completion; do not infer this
from byte patterns alone.

A separate assembly probe runs the actual switch with distinct callee-register
and FP control sentinels, then verifies that coordinator and worker contexts
retain their own values. Preserve the normal AAPCS contract for its caller.
The probe checks x19–x29, low 64 bits of v8–v15, FPCR/FPSR and SP while a
different worker uses distinct sentinels. x18, LR and upper vector halves are
preserved by the reviewed switch but are not claimed as runtime sentinels
through an ordinary Rust call, whose ABI permits different clobber behavior.
Hardware/protection diagnostics verify all worker guards and unchanged normal/
emergency stack state, allocation counts and mappings around execution.
Post-run timer/fault/memory checks continue to pass. x86 reports Unsupported.

Compiler probes use actual implementation types and positive clients. Reject
private context construction, arbitrary SP/PC/register mutation, escaping a
context borrow, Clone and Send/Sync where exposed. Check the function-pointer
job boundary rejects capturing/borrowed closures. Source review separately
checks reentrancy and forgetting, because negative compiler tests alone cannot
prove those lifecycle properties.

Keep exact commands, source/ELF identities, serial logs and guard-fault traces.
The existing full dual-architecture suite remains required. Do not claim
preemption, interrupt-safe scheduler locks, dynamic stack allocation, SMP,
user threads or broad arbitrary callback termination from this increment.

## Primary sources

Consulted 2026-10-26; no Linux implementation source used or copied.

- [AAPCS64, 2025Q4 issue](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst):
  callee-preserved registers, stack alignment and ordinary function-call ABI.
- [Rust Reference: undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html):
  alias validity and the runtime assumption about not deallocating a live Rust
  frame while it still owns local values needing destruction. This is distinct
  from safe `mem::forget`, which consumes ownership without running Drop.
  Our stronger lifecycle contract requires normal callback return before
  completion abandons its assembly-only continuation; suspension resumes normally.
- Compiler-probe evidence under `docs/evidence/threads-investigation/` records
  the exact pinned target's stack probing rather than assuming generic ARM behavior.
