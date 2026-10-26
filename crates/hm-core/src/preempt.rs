//! Single-CPU timer scheduling; ownership and transition proof: design 0012.
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::Ordering;
use hm_kernel::thread_api::{
    DispatchCause, MAX_THREADS, PreemptReport, ThreadContext, ThreadError, ThreadJob,
};
use hm_kernel::wait_api::{Event, WaitError};

// SAFETY: Private assembly saves full interrupted state in task-owned storage,
// releases the emergency stack, and resumes only live scheduler continuations.
global_asm!(include_str!("preempt-aarch64.S"), entry = sym entry, delivery = const if cfg!(feature = "preemption-no-timer-test") { 0 } else { 1 }, boundary = const if cfg!(feature = "preemption-boundary-test") { 1 } else { 0 },
    cpu_timer = const super::cpu_local::TIMER,
    cpu_preempt = const super::cpu_local::PREEMPT,
    cpu_irq_count = const super::cpu_local::IRQ_COUNT,
    cpu_boundary_flags = const super::cpu_local::BOUNDARY_FLAGS,
    cpu_wait_expiries = const super::cpu_local::WAIT_EXPIRIES);

// SAFETY: The diagnostic owns its stack frame, preserves the caller ABI and
// observes only the IRQ counter while real scheduling changes worker contexts.
global_asm!(include_str!("preempt-probe-aarch64.S"),
    cpu_irq_count = const super::cpu_local::IRQ_COUNT);

#[repr(C, align(16))]
struct Context {
    words: [u64; 32],
}
#[repr(C, align(16))]
struct Image {
    gpr: [u64; 31],
    sp: u64,
    vector: [[u64; 2]; 32],
    pc: u64,
    pstate: u64,
    fpcr: u64,
    fpsr: u64,
}
impl Image {
    const fn zero() -> Self {
        Self {
            gpr: [0; 31],
            sp: 0,
            vector: [[0; 2]; 32],
            pc: 0,
            pstate: 0,
            fpcr: 0,
            fpsr: 0,
        }
    }
}
#[repr(C, align(16))]
struct Task {
    context: Context,
    result: u64,
    status: u64,
    yields: u64,
    kind: u64,
    image: Image,
    preemptions: u64,
    job: ThreadJob,
    wait_event: *const Event,
    wait_observed: u64,
    parks: u64,
}
struct State {
    host: Context,
    tasks: [Task; MAX_THREADS],
}
const _: () = {
    assert!(core::mem::size_of::<Context>() == 256);
    assert!(core::mem::size_of::<Image>() == 800);
    assert!(core::mem::offset_of!(Image, sp) == 248);
    assert!(core::mem::offset_of!(Image, vector) == 256);
    assert!(core::mem::offset_of!(Image, pc) == 768);
    assert!(core::mem::offset_of!(Image, pstate) == 776);
    assert!(core::mem::offset_of!(Image, fpcr) == 784);
    assert!(core::mem::offset_of!(Image, fpsr) == 792);
    assert!(core::mem::offset_of!(Task, result) == 256);
    assert!(core::mem::offset_of!(Task, status) == 264);
    assert!(core::mem::offset_of!(Task, yields) == 272);
    assert!(core::mem::offset_of!(Task, kind) == 280);
    assert!(core::mem::offset_of!(Task, image) == 288);
    assert!(core::mem::offset_of!(Task, preemptions) == 1088);
};
// SAFETY: Only this module calls these fixed assembly entries with private,
// live aligned context records under the global execution lease and masked IRQ.
unsafe extern "C" {
    static __exception_stack_top: [u8; 0];
    fn arm_preempt_context_probe(seed: u64) -> u64;
    fn arm_gic_session_start() -> u64;
    fn arm_gic_session_stop() -> u64;
    fn arm_gic_probe_target() -> u64;
    fn arm_gic_probe_restore_target(target: u64) -> u64;
    fn arm_gic_probe_seed() -> u64;
    fn arm_gic_probe_preserved() -> u64;
    fn arm_preempt_setup() -> u64;
    fn arm_preempt_stop() -> u64;
    fn arm_preempt_dispatch(host: *mut Context, task: *mut core::ffi::c_void, interval: u64);
    fn arm_preempt_yield(task: *mut core::ffi::c_void, host: *const Context);
    fn arm_preempt_park(task: *mut core::ffi::c_void, host: *const Context);
    fn arm_preempt_wait_mask() -> u64;
    fn arm_preempt_start();
    fn arm_exception_fail() -> !;
}

#[inline(never)]
pub(super) fn run(jobs: &[ThreadJob]) -> Result<PreemptReport, ThreadError> {
    if super::cpu_local::current_id() != Some(0) {
        return Err(ThreadError::Unsupported);
    }
    if jobs.len() > MAX_THREADS {
        return Err(ThreadError::TooMany);
    }
    super::threads::BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| ThreadError::Busy)?;
    let mut report = PreemptReport::empty();
    if jobs.is_empty() {
        super::threads::BUSY.store(false, Ordering::Release);
        return Ok(report);
    }
    // SAFETY: Runner's boot authority and global lease ensure this masked
    // coordinator exclusively owns the one-CPU GIC/timer and guarded stacks.
    let shared = unsafe { arm_gic_session_start() };
    if shared == 0 {
        super::threads::BUSY.store(false, Ordering::Release);
        return Err(ThreadError::TimerUnavailable);
    }
    // SAFETY: Shared forwarding is initialized under the lease; CPU1 remains
    // off. Local setup owns only this CPU's GICC, PPI30 and timer record.
    let interval = unsafe { arm_preempt_setup() };
    if interval == 0 {
        // SAFETY: No worker has been published; local cleanup is idempotent.
        // Shared shutdown is valid because CPU1 is still off, not just because
        // the local setup failed. No armed controller escapes this lease.
        unsafe {
            if arm_preempt_stop() != 1 || arm_gic_session_stop() != 1 {
                arm_exception_fail();
            }
        }
        super::threads::BUSY.store(false, Ordering::Release);
        return Err(ThreadError::TimerUnavailable);
    }
    let state = UnsafeCell::new(State {
        host: Context { words: [0; 32] },
        tasks: core::array::from_fn(|id| Task {
            context: Context { words: [0; 32] },
            result: 0,
            status: 0,
            yields: 0,
            kind: 0,
            image: Image::zero(),
            preemptions: 0,
            job: jobs.get(id).copied().unwrap_or(ThreadJob {
                entry: empty,
                input: 0,
            }),
            wait_event: core::ptr::null(),
            wait_observed: 0,
            parks: 0,
        }),
    });
    let state = state.get();
    // SAFETY: State stays at this stable address until every callback returns.
    // Only raw projections cross dispatch; no references to IRQ-mutated task
    // records are retained. Handler accesses only the published running task.
    unsafe {
        for id in 0..jobs.len() {
            super::threads::clear_stack(id);
            let task = task(state, id);
            let image = core::ptr::addr_of_mut!((*task).image);
            (*image).sp = super::threads::tops()[id] as u64;
            (*image).pc = arm_preempt_start as *const () as u64;
            // EL1h, D/A/F masked and IRQ enabled, PAN retained from coordinator.
            let pan: u64;
            asm!("mrs {}, S3_0_C4_C2_3",out(reg) pan,options(nomem,nostack,preserves_flags));
            (*image).pstate = 0x345 | pan;
            (*image).gpr[19] = task as u64;
            (*image).gpr[20] = core::ptr::addr_of_mut!((*state).host) as u64;
            (*image).gpr[21] = state as u64;
            (*image).gpr[22] = id as u64;
        }
        let mut next = 0;
        #[cfg(feature = "wait-all-blocked-test")]
        let mut announced_blocked = false;
        while report.run.completed < jobs.len() {
            let mut blocked = 0;
            for waiting in 0..jobs.len() {
                let waiting = task(state, waiting);
                if core::ptr::addr_of!((*waiting).status).read() != 3 {
                    continue;
                }
                let event = core::ptr::addr_of!((*waiting).wait_event).read();
                if event.is_null() {
                    arm_exception_fail();
                }
                // The BLOCKED worker's live wait call retains this immutable
                // Event borrow. No reference to Task survives dispatch; Event
                // reads touch only its atomic sequence. Clear the pointer
                // before RUNNABLE so the borrow may end after resumption.
                let observed = core::ptr::addr_of!((*waiting).wait_observed).read();
                if (&*event).snapshot() != observed {
                    core::ptr::addr_of_mut!((*waiting).wait_event).write(core::ptr::null());
                    core::ptr::addr_of_mut!((*waiting).wait_observed).write(0);
                    core::ptr::addr_of_mut!((*waiting).kind).write(3);
                    core::ptr::addr_of_mut!((*waiting).status).write(0);
                } else {
                    blocked += 1;
                }
            }
            if blocked == jobs.len() - report.run.completed {
                #[cfg(feature = "wait-all-blocked-test")]
                if !announced_blocked {
                    super::arch::wait_blocked_marker();
                    announced_blocked = true;
                }
                // No current external producer exists. Keep every suspended
                // stack/borrow and the lease live; do not return or fake a wake.
                // This spins the coordinator CPU rather than claiming idle.
                core::hint::spin_loop();
                continue;
            }
            let id = next;
            next = (next + 1) % jobs.len();
            let task = task(state, id);
            if core::ptr::addr_of!((*task).status).read() != 0 {
                continue;
            }
            let cause = match core::ptr::addr_of!((*task).kind).read() {
                0 => DispatchCause::Initial,
                1 => DispatchCause::Yield,
                2 => DispatchCause::Timer,
                3 => DispatchCause::Wake,
                _ => arm_exception_fail(),
            };
            report.run.dispatches = report.run.dispatches.saturating_add(1);
            if report.run.trace_len < report.run.trace.len() {
                let slot = report.run.trace_len;
                report.run.trace[slot] = id;
                report.causes[slot] = cause;
                report.run.trace_len += 1;
            } else {
                report.run.truncated = true;
            }
            arm_preempt_dispatch(
                core::ptr::addr_of_mut!((*state).host),
                task.cast(),
                interval,
            );
            if core::ptr::addr_of!((*task).status).read() == 2 {
                if !core::ptr::addr_of!((*task).wait_event).read().is_null() {
                    arm_exception_fail();
                }
                report.run.completed += 1;
                report.run.results[id] = core::ptr::addr_of!((*task).result).read();
                report.run.yields[id] = core::ptr::addr_of!((*task).yields).read();
                report.preemptions[id] = core::ptr::addr_of!((*task).preemptions).read();
                report.parks[id] = core::ptr::addr_of!((*task).parks).read();
            }
        }
        // Quiesce source/controller before publication withdrawal and wiping
        // completed stacks. Failure terminates rather than reclaiming early.
        if arm_preempt_stop() != 1 || arm_gic_session_stop() != 1 {
            arm_exception_fail();
        }
        for id in 0..jobs.len() {
            super::threads::clear_stack(id);
        }
    }
    super::threads::BUSY.store(false, Ordering::Release);
    Ok(report)
}
fn empty(_: &mut dyn ThreadContext, _: u64) -> u64 {
    0
}

// SAFETY: Caller holds the execution lease, State is live and id<MAX_THREADS.
unsafe fn task(state: *mut State, id: usize) -> *mut Task {
    // SAFETY: Bounded raw projection into live task storage forms no reference.
    unsafe {
        core::ptr::addr_of_mut!((*state).tasks)
            .cast::<Task>()
            .add(id)
    }
}
struct Token {
    state: *mut State,
    id: usize,
    _local: PhantomData<*mut ()>,
}
impl ThreadContext for Token {
    fn id(&self) -> usize {
        self.id
    }
    fn yield_now(&mut self) {
        // SAFETY: Private token identifies this live running task. Assembly
        // masks before scheduler mutation, and resumes this same continuation.
        unsafe {
            arm_preempt_yield(
                task(self.state, self.id).cast(),
                core::ptr::addr_of!((*self.state).host),
            );
        }
    }
    fn wait(&mut self, event: &Event, observed: u64) -> Result<(), WaitError> {
        // SAFETY: This private token is a running worker on the sole CPU.
        // The compiler memory barrier and IRQ mask serialize the last check
        // with registration. No public mask token or arbitrary callback escapes.
        let saved_daif = unsafe { arm_preempt_wait_mask() };
        let current = event.snapshot();
        let outcome = if current != observed {
            Ok(())
        } else if current == u64::MAX {
            Err(WaitError::Exhausted)
        } else {
            // SAFETY: The masked worker alone owns this RUNNING task. The
            // event reference remains borrowed through the resumed snapshot
            // below; a blocked callback never drops or loses its stack. Only
            // the masked coordinator reads this pointer and clears it before
            // making this continuation runnable. Raw projections avoid whole-
            // task references across the assembly context switch.
            unsafe {
                let task = task(self.state, self.id);
                if core::ptr::addr_of!((*task).status).read() != 1
                    || !core::ptr::addr_of!((*task).wait_event).read().is_null()
                {
                    arm_exception_fail();
                }
                core::ptr::addr_of_mut!((*task).wait_event).write(event as *const Event);
                core::ptr::addr_of_mut!((*task).wait_observed).write(observed);
                let parks = core::ptr::addr_of!((*task).parks).read();
                core::ptr::addr_of_mut!((*task).parks).write(parks.saturating_add(1));
                core::ptr::addr_of_mut!((*task).kind).write(3);
                core::ptr::addr_of_mut!((*task).status).write(3);
                arm_preempt_park(task.cast(), core::ptr::addr_of!((*self.state).host));
                if !core::ptr::addr_of!((*task).wait_event).read().is_null() {
                    arm_exception_fail();
                }
            }
            // This Acquire is the returning worker's publication observation,
            // and keeps its original Event borrow live through suspension.
            if event.snapshot() == observed {
                // SAFETY: The sequence cannot reset or wrap, so a resumed wait
                // with its original observation is an internal scheduler fault.
                unsafe { arm_exception_fail() }
            }
            Ok(())
        };
        // SAFETY: All task metadata is stable and any parked registration has
        // been withdrawn. Restore the exact saved mask for every return path;
        // this local value cannot be forgotten by arbitrary safe callers.
        unsafe {
            asm!("msr daif, {saved}", "isb", saved=in(reg) saved_daif,
                options(nostack, preserves_flags));
        }
        outcome
    }
}
// SAFETY: Only first-entry assembly supplies this private stable state pointer
// and admitted task ID. All scheduler metadata reads finish before policy runs.
unsafe extern "C" fn entry(state: *mut core::ffi::c_void, id: usize) -> u64 {
    let state = state.cast::<State>();
    // SAFETY: Entry receives the initialized live task; its job is immutable.
    let job = unsafe { core::ptr::addr_of!((*task(state, id)).job).read() };
    let mut token = Token {
        state,
        id,
        _local: PhantomData,
    };
    (job.entry)(&mut token, job.input)
}

/// Observation for the masked platform test coordinator; not worker authority.
pub(super) fn irq_count() -> u64 {
    super::cpu_local::irq_count()
}

/// Require that the boundary image actually executed its masked-expiry path.
/// Ordinary builds have no such instrumentation or claim.
pub(super) fn wait_boundary_check() -> bool {
    #[cfg(feature = "preemption-boundary-test")]
    {
        super::cpu_local::wait_expiries() != 0
    }
    #[cfg(not(feature = "preemption-boundary-test"))]
    true
}

#[cfg(all(
    feature = "wait-all-blocked-test",
    not(any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test"
    ))
))]
pub(super) fn all_blocked(runner: &super::threads::Runner) -> ! {
    struct Witness;
    impl Drop for Witness {
        fn drop(&mut self) {
            // SAFETY: A suspended negative worker must never drop its frame.
            // Any unexpected destruction takes a fatal route rejected by host.
            unsafe { arm_exception_fail() }
        }
    }
    fn park_local(context: &mut dyn ThreadContext, _: u64) -> u64 {
        let _witness = Witness;
        let event = Event::new();
        let observed = event.snapshot();
        let _ = context.wait(&event, observed);
        0
    }
    let _ = runner.run_preemptive(&[ThreadJob {
        entry: park_local,
        input: 0,
    }]);
    // SAFETY: There is no notifier for this callback-local Event; normal return
    // would violate this diagnostic's required retained blocked continuation.
    unsafe { arm_exception_fail() }
}

#[cfg(all(
    feature = "preemption-overflow-test",
    not(any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test"
    ))
))]
pub(super) fn overflow(runner: &super::threads::Runner) -> ! {
    #[inline(never)]
    fn large_frame(_: &mut dyn ThreadContext, value: u64) -> u64 {
        let mut bytes = [0u8; 131072];
        core::hint::black_box(&mut bytes);
        bytes[0] = value as u8;
        u64::from(core::hint::black_box(bytes[131071]))
    }
    let _ = runner.run_preemptive(&[ThreadJob {
        entry: large_frame,
        input: 17,
    }]);
    // SAFETY: A negative fixture that unexpectedly returns must terminate with
    // no expected fatal marker; the host then rejects the negative test.
    unsafe { arm_exception_fail() }
}

pub(super) fn context_checks(runner: &super::threads::Runner) -> bool {
    fn probe(_: &mut dyn ThreadContext, seed: u64) -> u64 {
        // SAFETY: Fixed assembly probe accepts a small integer seed, preserves
        // AAPCS and returns normally after checking task-private snapshots.
        unsafe { arm_preempt_context_probe(seed) }
    }
    runner
        .run_preemptive(&core::array::from_fn::<_, MAX_THREADS, _>(|id| ThreadJob {
            entry: probe,
            input: id as u64,
        }))
        .is_ok_and(|report| {
            report.run.completed == MAX_THREADS
                && report.run.results == [1; MAX_THREADS]
                && report.run.yields == [0; MAX_THREADS]
                && report.preemptions.iter().all(|count| *count > 0)
        })
}

pub(super) fn control_checks() -> bool {
    let (daif, stack, timer): (u64, u64, u64);
    // SAFETY: Read-only control/MMIO observations on the boot-proven soleCPU;
    // caller is the platform coordinator, outside any running worker lease.
    unsafe {
        asm!("mrs {daif}, daif", "mrs {stack}, sp_el0", "mrs {timer}, cntp_ctl_el0",
            daif=out(reg) daif,stack=out(reg) stack,timer=out(reg) timer,
            options(nomem,nostack,preserves_flags));
        let dist = 0x08000000 as *const u32;
        let cpu = 0x08010000 as *const u32;
        daif == 0x3c0
            && stack == core::ptr::addr_of!(__exception_stack_top) as u64
            && timer & 3 == 2
            && dist.read_volatile() == 0
            && cpu.read_volatile() == 0
            && dist.add(0x100 / 4).read_volatile() & (1 << 30) == 0
            && dist.add(0x200 / 4).read_volatile() & (1 << 30) == 0
            && dist.add(0x300 / 4).read_volatile() & (1 << 30) == 0
    }
}

/// Exercise local lifecycle while a disabled shared interrupt remains pending.
/// No worker runs and IRQ stays masked throughout this diagnostic.
pub(super) fn gic_checks() -> bool {
    let daif: u64;
    // SAFETY: Read-only EL1 mask observation; the private caller has boot
    // authority. Reject enabled IRQ before any controller or lease mutation.
    unsafe {
        asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack, preserves_flags));
    }
    if daif != 0x3c0 || super::cpu_local::current_id() != Some(0) {
        return false;
    }
    if super::threads::BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    // SAFETY: CPU0 owns BUSY; CPU1 is off. The session disables all SPIs before
    // the private SPI32 pending fixture. Every path after start stops both local
    // and shared state before releasing the lease. No active bit is fabricated.
    let passed = unsafe {
        if arm_gic_session_start() == 0 {
            false
        } else {
            let original_target = arm_gic_probe_target();
            let mut passed = arm_gic_probe_seed() == 1;
            for _ in 0..2 {
                let interval = arm_preempt_setup();
                // A second local owner must be rejected without changing the
                // first setup; normal stop must still preserve shared state.
                let duplicate_rejected = arm_preempt_setup() == 0;
                if arm_preempt_stop() != 1 {
                    arm_exception_fail();
                }
                // Do not short-circuit cleanup or either repeated cycle.
                let preserved = arm_gic_probe_preserved() == 1;
                passed = passed && interval != 0 && duplicate_rejected && preserved;
            }
            if arm_gic_session_stop() != 1 {
                arm_exception_fail();
            }
            // Restore the diagnostic target even when a seed/cycle failed.
            if arm_gic_probe_restore_target(original_target) != 1 {
                arm_exception_fail();
            }
            passed && control_checks()
        }
    };
    super::threads::BUSY.store(false, Ordering::Release);
    passed
}
