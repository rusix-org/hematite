# ARM cooperative kernel thread review

Date: 2026-10-26. Independent AI review of
[design 0010](../design/0010-arm-kernel-threads.md); not human approval or a
complete soundness certification. Reviewer inspected the existing ARM boot,
guard mappings, timer scope, frame ownership and pinned compiler configuration.

## Design review

Ready for implementation within the declared cooperative single-CPU scope.
The synchronous execution model provides a viable ownership boundary for real
cooperative threads. A Runner capability must originate from the private unsafe
boot constructor and remain non-Send/non-Sync: an execution lease alone does not
prove that guarded stacks, exception entry or CPU state exist. BootPlatform
retains this authority. Public policy receives a borrowed
context through a higher-ranked callback function pointer, so it cannot retain
a usable context borrow after callback completion or supply arbitrary contexts.

The coordinator's UnsafeCell storage remains at one address until all workers
finish. No whole-record mutable borrow or immutable reference to mutable fields
may remain live across a switch. Private raw field projections avoid creating
aliases to suspended callback/token borrows. The thread token lives separately
on its own stack; the scheduler does not mutate that token while yield holds
its exclusive borrow. Context records and stack addresses must never be exposed
for mutation through safe code.

Busy rejection must precede initializing or overwriting shared worker stacks.
Only the coordinator may release the lease after every callback completes.
Forgetting a borrowed context cannot release the lease, and no cancellation or
early-return path may discard suspended Rust frames. A nonreturning callback
can prevent progress but must retain all execution storage. The assembly first
entry must call a Rust adapter that returns normally; only afterward may the
assembly completion path permanently switch away. This runs callback and adapter
local destructors before stack reuse and avoids abandoning live Rust frames.

Cooperative switching obeys the ordinary call ABI rather than an asynchronous
exception ABI. Preserve x19–x29, SP and the return continuation, with the chosen
additional x18, full Q8–Q15 and FP control/status preservation. Caller-clobbered
registers and NZCV remain governed by compiler call semantics. The private
context layout requires exact size/alignment/offset assertions, and assembly
must not expose a transient SP to Rust. Initial and resumed SP values remain
16-byte aligned. IRQ stays masked while scheduling and executing Rust.

Stack exhaustion was treated as a soundness obligation, not a caller limit.
Reviewer independently inspected the pinned compiler's actual 128 KiB safe
array fixture: LLVM IR has `probe-stack=inline-asm`, and AArch64 assembly performs
4096-byte SP decrements with a load at each new SP before entering the function
body. This supports the 4 KiB guard contract for the pinned target/toolchain.
The compiler regression check and an actual overflowing safe callback must
remain required; a bare guard without stack probing would not be sufficient.

Runtime verification must establish actual distinct stacks, repeated yields,
round-robin ordering, no redispatch after completion, normal destructor execution,
resource limits and reuse. The negative overflow image must identify a worker
guard FAR and exit through the separate emergency-stack fatal route. Existing
timer/fault checks after scheduling cover restoration of shared CPU/stack state.
These checks do not establish preemption, SMP or general termination guarantees.

## Source review

Inspected `threads.rs`, `threads-aarch64.S`, safe policy API/tests, linker/boot
guard changes and platform/negative-image integration. No blocking soundness
finding in the reviewed implementation. No build or guest execution was
performed by this reviewer; integration failures and their fixes belong to
the parent's verification record.

- Runner construction is private and unsafe, with a non-Send/non-Sync marker.
  BootPlatform creates it after validating mappings. The global execution lease
  is acquired before clearing any worker stack and released after all callbacks
  complete on the coordinator. The nested diagnostic constructs another valid
  boot capability but is rejected by Busy before touching worker storage.
- RunState is owned by a stable coordinator-stack UnsafeCell. Raw projections
  select bounded Task fields; no whole-state/task reference survives a switch.
  The job's function pointer and scalar input are copied before policy is called.
  Token mutation is independent of scheduler metadata and occurs only in its
  own suspended/resumed continuation. No escaping task handle is exposed.
- Context has asserted 256-byte size, 16-byte alignment and exact assembly
  offsets for GPRs, SP, vectors and FP state. The switch stores x18–x30 and SP,
  full Q8–Q15, FPCR and FPSR, then restores the incoming context and returns
  through its saved continuation. Its extern declaration carries ordinary
  memory effects. All hardware context inputs originate from private records.
- First-entry assembly calls the Rust adapter, which calls the safe callback
  and returns normally. Only afterward does assembly store the result, mark
  Done and switch to the coordinator. Completed tasks cannot be selected again.
  The final stack wipe therefore does not erase live Rust frames or values.
- Zero-job execution never reaches modulo-by-zero dispatch arithmetic. More
  than four jobs reject before executing callbacks. Yield and dispatch counters
  saturate explicitly; trace exhaustion records truncation while execution
  continues to normal completion. Repeated safe-policy runs check completion,
  scheduling order, retained local arrays and destructor execution.
- Linker storage includes four guard/64-KiB pairs before `__kernel_end`;
  bootstrap leaves all four guards invalid. Full mapping checks include those
  guards, and the core also probes their translations. The large-array negative
  callback uses ordinary safe Rust, so its fatal guard result exercises actual
  compiler stack probing rather than a fabricated faulting instruction.

The runtime register probe checks x19–x29, D8–D15, FPCR/FPSR and SP across an
actual yield. Preservation of x18, the full upper Q halves and the return
continuation is established by source review, not those sentinel checks.
A verification improvement was requested and implemented: each probe now checks
its live SP against its assigned worker-stack range, requires 16-byte alignment
and the same SP after yield, and runs in all four slots. Reviewed these added
read-only SP observations and bounded stack-symbol selection. This makes stack
identity an explicit runtime observation as well as a construction proof.

Parent investigation reported a monitor-only normal-stack exhaustion caused by
inlining the full test inventory into the monitor frame. The separate safe-policy
`#[inline(never)]` boundary on `run_tests` changes stack composition without
relaxing guard behavior or the thread lifetime contract. Its effect requires
the parent's final guest rerun; source review does not label that rerun passed.

## Sources checked

- [AAPCS64 2025Q4](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst):
  sections on general/SIMD registers, stack alignment and subroutine calls.
- [Rust Reference undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html):
  reference aliasing, correct call ABI, and the runtime prohibition on dropping
  stack frames without executing owned-local destructors.
- Local pinned compiler evidence in `target/threads-investigation/large-frame.s`
  and `.ll`, generated from the safe fixture under
  `docs/evidence/threads-investigation/`.

No Linux implementation source was consulted or copied.
