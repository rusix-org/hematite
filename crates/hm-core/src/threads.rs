//! Guarded cooperative contexts; design 0010 owns the lifetime/ABI proof.
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use hm_kernel::thread_api::{MAX_THREADS, RunReport, ThreadContext, ThreadError, ThreadJob};

// SAFETY: Reviewed design 0010 confines SP/context changes to an AAPCS64
// switch and assembly completion after the Rust callback has returned normally.
global_asm!(include_str!("threads-aarch64.S"), entry = sym callback_entry, probe_yield = sym probe_yield);

// SAFETY: Linker owns guarded stack reservations; assembly uses the checked
// Context/Task layouts below and preserves the ordinary AAPCS64 call boundary.
unsafe extern "C" {
    static __thread0_guard: [u8; 0];
    static __thread0_top: [u8; 0];
    static __thread1_guard: [u8; 0];
    static __thread1_top: [u8; 0];
    static __thread2_guard: [u8; 0];
    static __thread2_top: [u8; 0];
    static __thread3_guard: [u8; 0];
    static __thread3_top: [u8; 0];
    fn arm_context_switch(outgoing: *mut Context, incoming: *const Context);
    fn arm_thread_start();
    fn arm_thread_context_probe(bridge: *mut core::ffi::c_void, seed: u64) -> u64;
    #[cfg(all(
        feature = "thread-overflow-test",
        not(any(feature = "nested-fault-test", feature = "unhandled-fault-test"))
    ))]
    fn arm_exception_fail() -> !;
}

const RUNNABLE: u64 = 0;
const RUNNING: u64 = 1;
const DONE: u64 = 2;
pub(super) static BUSY: AtomicBool = AtomicBool::new(false);

#[repr(C, align(16))]
struct Context {
    gpr: [u64; 13], // x18..x30
    sp: u64,
    vector: [[u64; 2]; 8], // q8..q15
    fpcr: u64,
    fpsr: u64,
}
impl Context {
    const fn zero() -> Self {
        Self {
            gpr: [0; 13],
            sp: 0,
            vector: [[0; 2]; 8],
            fpcr: 0,
            fpsr: 0,
        }
    }
}
#[repr(C)]
struct Task {
    context: Context,
    result: u64,
    status: u64,
    yields: u64,
    job: ThreadJob,
}
struct RunState {
    host: Context,
    tasks: [Task; MAX_THREADS],
}
const _: () = {
    assert!(core::mem::size_of::<Context>() == 256);
    assert!(core::mem::align_of::<Context>() == 16);
    assert!(core::mem::offset_of!(Context, gpr) == 0);
    assert!(core::mem::offset_of!(Context, sp) == 104);
    assert!(core::mem::offset_of!(Context, vector) == 112);
    assert!(core::mem::offset_of!(Context, fpcr) == 240);
    assert!(core::mem::offset_of!(Context, fpsr) == 248);
    assert!(core::mem::offset_of!(Task, context) == 0);
    assert!(core::mem::offset_of!(Task, result) == 256);
    assert!(core::mem::offset_of!(Task, status) == 264);
};

/// Boot capability; neither transferable across CPUs nor shareable concurrently.
pub struct Runner {
    cpu_id: Option<u64>,
    _single_cpu: PhantomData<*mut ()>,
}

impl Runner {
    /// Caller proves EL1h, masked DAIF, enabled FP/SIMD, initialized guarded
    /// stacks and immutable mappings. The fixed one-CPU profile remains active.
    /// Multiple capabilities are harmless: the global lease serializes stacks.
    // SAFETY: Caller supplies boot authority, not arbitrary addresses; run
    // additionally acquires exclusive use before touching any worker storage.
    pub(crate) unsafe fn from_boot() -> Self {
        Self {
            cpu_id: super::cpu_local::current_id().filter(|id| *id == 0),
            _single_cpu: PhantomData,
        }
    }

    pub fn run_preemptive(
        &self,
        jobs: &[ThreadJob],
    ) -> Result<hm_kernel::thread_api::PreemptReport, ThreadError> {
        if self.cpu_id.is_none() || super::cpu_local::current_id() != self.cpu_id {
            return Err(ThreadError::Unsupported);
        }
        super::preempt::run(jobs)
    }

    pub fn run(&self, jobs: &[ThreadJob]) -> Result<RunReport, ThreadError> {
        if self.cpu_id.is_none() || super::cpu_local::current_id() != self.cpu_id {
            return Err(ThreadError::Unsupported);
        }
        if jobs.len() > MAX_THREADS {
            return Err(ThreadError::TooMany);
        }
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| ThreadError::Busy)?;
        let mut report = RunReport::empty();
        let state = UnsafeCell::new(RunState {
            host: Context::zero(),
            tasks: core::array::from_fn(|id| Task {
                context: Context::zero(),
                result: 0,
                status: RUNNABLE,
                yields: 0,
                job: jobs.get(id).copied().unwrap_or(ThreadJob {
                    entry: empty_job,
                    input: 0,
                }),
            }),
        });
        let state_ptr = state.get();
        // SAFETY: This call owns the sole stack lease. RunState stays at this
        // address until every Rust callback returns. All metadata is accessed
        // through raw projections inside UnsafeCell, never a live whole-state
        // reference across a switch. Each initial PC/SP is linker/core-owned.
        unsafe {
            for id in 0..jobs.len() {
                clear_stack(id);
                let task = task_ptr(state_ptr, id);
                let mut context = Context::zero();
                context.gpr[1] = task as u64;
                context.gpr[2] = core::ptr::addr_of_mut!((*state_ptr).host) as u64;
                context.gpr[3] = state_ptr as u64;
                context.gpr[4] = id as u64;
                context.gpr[12] = arm_thread_start as *const () as u64;
                context.sp = tops()[id] as u64;
                core::ptr::addr_of_mut!((*task).context).write(context);
            }
            let mut next = 0;
            while report.completed < jobs.len() {
                let id = next;
                next = (next + 1) % jobs.len();
                let task = task_ptr(state_ptr, id);
                if core::ptr::addr_of!((*task).status).read() != RUNNABLE {
                    continue;
                }
                report.dispatches = report.dispatches.saturating_add(1);
                if let Some(entry) = report.trace.get_mut(report.trace_len) {
                    *entry = id;
                    report.trace_len += 1;
                } else {
                    report.truncated = true;
                }
                core::ptr::addr_of_mut!((*task).status).write(RUNNING);
                arm_context_switch(
                    core::ptr::addr_of_mut!((*state_ptr).host),
                    core::ptr::addr_of!((*task).context),
                );
                if core::ptr::addr_of!((*task).status).read() == DONE {
                    report.completed += 1;
                    report.results[id] = core::ptr::addr_of!((*task).result).read();
                    report.yields[id] = core::ptr::addr_of!((*task).yields).read();
                }
            }
            // All callbacks and adapters returned normally before assembly
            // marked Done. No Rust frame/value/borrow remains on these stacks.
            for id in 0..jobs.len() {
                clear_stack(id);
            }
        }
        BUSY.store(false, Ordering::Release);
        Ok(report)
    }

    pub(super) fn checks(&self) -> hm_kernel::ThreadChecks {
        let before = control_state();
        // Private invalid capabilities must reject both entry points before
        // acquiring BUSY; the normal context and nested-run checks follow.
        let affinity = [Some(1), None].into_iter().all(|cpu_id| {
            let foreign = Self {
                cpu_id,
                _single_cpu: PhantomData,
            };
            matches!(foreign.run(&[]), Err(ThreadError::Unsupported))
                && matches!(foreign.run_preemptive(&[]), Err(ThreadError::Unsupported))
        });
        let context = self
            .run(&[
                ThreadJob {
                    entry: context_job,
                    input: 1,
                },
                ThreadJob {
                    entry: context_job,
                    input: 2,
                },
                ThreadJob {
                    entry: context_job,
                    input: 3,
                },
                ThreadJob {
                    entry: context_job,
                    input: 4,
                },
            ])
            .is_ok_and(|r| r.completed == 4 && r.results == [1; 4] && r.yields == [1; 4]);
        let reentrant = self
            .run(&[ThreadJob {
                entry: nested_job,
                input: 0,
            }])
            .is_ok_and(|r| r.completed == 1 && r.results[0] == 1);
        hm_kernel::ThreadChecks {
            context: context && before.0 == 0x3c0 && control_state() == before,
            reentrant: reentrant && affinity,
            guards: guards_valid(),
        }
    }
}

fn empty_job(_: &mut dyn ThreadContext, _: u64) -> u64 {
    0
}

fn control_state() -> (u64, u64) {
    let daif: u64;
    let emergency_sp: u64;
    // SAFETY: Initialized EL1 permits these read-only control observations;
    // no memory or interrupt state changes, and no caller supplies addresses.
    unsafe {
        asm!("mrs {daif}, daif", "mrs {stack}, sp_el0", daif=out(reg) daif, stack=out(reg) emergency_sp, options(nomem, nostack, preserves_flags));
    }
    (daif, emergency_sp)
}

struct Token {
    state: *mut RunState,
    id: usize,
    _single_cpu: PhantomData<*mut ()>,
}
impl ThreadContext for Token {
    fn id(&self) -> usize {
        self.id
    }
    fn yield_now(&mut self) {
        // SAFETY: Only callback_entry creates a token for its running worker.
        // Its private pointer/id stay valid for the entire synchronous run.
        // Scheduler touches only RunState, never this stack-local token while
        // its exclusive borrow is suspended. This exact continuation resumes.
        unsafe {
            let task = task_ptr(self.state, self.id);
            let yields = core::ptr::addr_of!((*task).yields).read();
            core::ptr::addr_of_mut!((*task).yields).write(yields.saturating_add(1));
            core::ptr::addr_of_mut!((*task).status).write(RUNNABLE);
            arm_context_switch(
                core::ptr::addr_of_mut!((*task).context),
                core::ptr::addr_of!((*self.state).host),
            );
        }
    }
}

// SAFETY: Only the first-entry trampoline calls this with its initialized
// run pointer and bounded task ID, while the synchronous coordinator stays live.
unsafe extern "C" fn callback_entry(state: *mut core::ffi::c_void, id: usize) -> u64 {
    let state = state.cast::<RunState>();
    // SAFETY: Initial assembly contexts carry the stable live RunState and
    // their bounded unique task ID. Copy job metadata before calling policy;
    // no reference into scheduler-mutated records spans a yield.
    let job = unsafe { core::ptr::addr_of!((*task_ptr(state, id)).job).read() };
    let mut token = Token {
        state,
        id,
        _single_cpu: PhantomData,
    };
    (job.entry)(&mut token, job.input)
}

// SAFETY: Caller proves live RunState and id<MAX_THREADS; raw field/element
// projection does not create an aliasing reference to scheduler metadata.
unsafe fn task_ptr(state: *mut RunState, id: usize) -> *mut Task {
    // SAFETY: The caller's bounds and live storage satisfy array projection.
    unsafe {
        core::ptr::addr_of_mut!((*state).tasks)
            .cast::<Task>()
            .add(id)
    }
}

// SAFETY: Caller owns the global stack lease, id is bounded, and no live Rust
// frame/reference exists on this worker stack (before start or after completion).
pub(super) unsafe fn clear_stack(id: usize) {
    // SAFETY: Guard+4096 begins the complete64KiB reserved normal-RAM stack.
    unsafe { core::ptr::write_bytes((guards()[id] + 4096) as *mut u8, 0, 65536) }
}

pub(super) fn guards() -> [usize; MAX_THREADS] {
    [
        core::ptr::addr_of!(__thread0_guard) as usize,
        core::ptr::addr_of!(__thread1_guard) as usize,
        core::ptr::addr_of!(__thread2_guard) as usize,
        core::ptr::addr_of!(__thread3_guard) as usize,
    ]
}
pub(super) fn tops() -> [usize; MAX_THREADS] {
    [
        core::ptr::addr_of!(__thread0_top) as usize,
        core::ptr::addr_of!(__thread1_top) as usize,
        core::ptr::addr_of!(__thread2_top) as usize,
        core::ptr::addr_of!(__thread3_top) as usize,
    ]
}
// SAFETY: Fixed secondary worker reservations are established and validated by
// coldboot; these address-only markers grant no runtime ownership by themselves.
unsafe extern "C" {
    static __secondary_thread0_guard: [u8; 0];
    static __secondary_thread0_top: [u8; 0];
    static __secondary_thread1_guard: [u8; 0];
    static __secondary_thread1_top: [u8; 0];
    static __secondary_thread2_guard: [u8; 0];
    static __secondary_thread2_top: [u8; 0];
    static __secondary_thread3_guard: [u8; 0];
    static __secondary_thread3_top: [u8; 0];
}
pub(super) fn local_guards(cpu: usize) -> Option<[usize; MAX_THREADS]> {
    match cpu {
        0 => Some(guards()),
        1 => Some([
            core::ptr::addr_of!(__secondary_thread0_guard) as usize,
            core::ptr::addr_of!(__secondary_thread1_guard) as usize,
            core::ptr::addr_of!(__secondary_thread2_guard) as usize,
            core::ptr::addr_of!(__secondary_thread3_guard) as usize,
        ]),
        _ => None,
    }
}
pub(super) fn local_tops(cpu: usize) -> Option<[usize; MAX_THREADS]> {
    match cpu {
        0 => Some(tops()),
        1 => Some([
            core::ptr::addr_of!(__secondary_thread0_top) as usize,
            core::ptr::addr_of!(__secondary_thread1_top) as usize,
            core::ptr::addr_of!(__secondary_thread2_top) as usize,
            core::ptr::addr_of!(__secondary_thread3_top) as usize,
        ]),
        _ => None,
    }
}
// SAFETY: Caller exclusively owns this CPU's worker stack and proves no live
// callback/frame remains there; CPU affinity and global session lease are held.
pub(super) unsafe fn clear_local_stack(cpu: usize, id: usize) {
    let (Some(guards), Some(tops)) = (local_guards(cpu), local_tops(cpu)) else {
        return;
    };
    let (Some(&guard), Some(&top)) = (guards.get(id), tops.get(id)) else {
        return;
    };
    // SAFETY: Exact linker bounds exclude guards; caller supplies ownership.
    unsafe {
        core::ptr::write_bytes((guard + 4096) as *mut u8, 0, top - guard - 4096);
    }
}
fn guards_valid() -> bool {
    guards().iter().all(|address| {
        let par: u64;
        // SAFETY: EL1 probe of a fixed linker-owned guard VA, without memory
        // dereference; IRQ masked so PAR_EL1 cannot race another observation.
        unsafe { asm!("at s1e1r, {address}", "isb", "mrs {par}, par_el1", address=in(reg) *address, par=out(reg) par, options(nostack, preserves_flags)); }
        par & 1 != 0
    })
}

#[repr(C)]
struct ProbeBridge<'a> {
    context: &'a mut (dyn ThreadContext + 'a),
}
// SAFETY: The probe calls this only with the live stack-local bridge supplied
// by context_job; assembly never dereferences or retains that opaque pointer.
unsafe extern "C" fn probe_yield(bridge: *mut core::ffi::c_void) {
    let bridge = bridge.cast::<ProbeBridge<'_>>();
    // SAFETY: The context probe passes its caller's live stack-local bridge;
    // its only access is this temporary reborrow, which resumes before return.
    unsafe {
        (*bridge).context.yield_now();
    }
}
fn context_job(context: &mut dyn ThreadContext, seed: u64) -> u64 {
    let id = context.id();
    let (Some(guard), Some(top)) = (guards().get(id).copied(), tops().get(id).copied()) else {
        return 0;
    };
    let before = stack_pointer();
    if before < guard + 4096 || before >= top || !before.is_multiple_of(16) {
        return 0;
    }
    let mut bridge = ProbeBridge { context };
    // SAFETY: Probe preserves its caller ABI and passes this live bridge only
    // to probe_yield; its yields resume before either function returns.
    let result = unsafe { arm_thread_context_probe(core::ptr::addr_of_mut!(bridge).cast(), seed) };
    u64::from(result == 1 && stack_pointer() == before)
}

#[inline(always)]
fn stack_pointer() -> usize {
    let sp: usize;
    // SAFETY: Read-only observation of the current SP; no memory is accessed
    // or stack/control state changed. Result conveys no dereference authority.
    unsafe {
        asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags));
    }
    sp
}
fn nested_job(_: &mut dyn ThreadContext, _: u64) -> u64 {
    // SAFETY: This callback can run only through an initialized Runner. The
    // boot mappings remain valid; a second capability still uses the same
    // global lease and must reject before touching any worker stack.
    let runner = unsafe { Runner::from_boot() };
    u64::from(matches!(runner.run(&[]), Err(ThreadError::Busy)))
}

#[cfg(all(
    feature = "thread-overflow-test",
    not(any(feature = "nested-fault-test", feature = "unhandled-fault-test"))
))]
pub(super) fn overflow(runner: &Runner) -> ! {
    #[inline(never)]
    fn large_frame(_: &mut dyn ThreadContext, value: u64) -> u64 {
        let mut bytes = [0u8; 131072];
        core::hint::black_box(&mut bytes);
        bytes[0] = value as u8;
        u64::from(core::hint::black_box(bytes[131071]))
    }
    let _ = runner.run(&[ThreadJob {
        entry: large_frame,
        input: 17,
    }]);
    // SAFETY: This negative image must terminate even if the expected guard
    // fault failed. This exit lacks the fatal marker, so the host cannot pass it.
    unsafe { arm_exception_fail() }
}
