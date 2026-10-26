//! Fixed secondary lifecycle diagnostic; ownership and admission: design 0018.
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU64, Ordering};
use hm_kernel::cpu_topology::{Conduit, Topology};

const IDLE: u64 = 0;
const PREPARED: u64 = 1;
const ONLINE: u64 = 2;
const GO: u64 = 3;
const RETURNED: u64 = 4;
const VERSION: u64 = 0x8400_0000;
const FEATURES: u64 = 0x8400_000a;
const CPU_ON: u64 = 0xc400_0003;
const CPU_OFF: u64 = 0x8400_0002;
const AFFINITY_INFO: u64 = 0xc400_0004;

#[derive(Clone, Copy)]
#[repr(C)]
struct PeerReport {
    cpu: u64,
    root: u64,
    sp: u64,
    checks: u64,
}
impl PeerReport {
    const ZERO: Self = Self {
        cpu: 0,
        root: 0,
        sp: 0,
        checks: 0,
    };
}
#[repr(C, align(64))]
struct Mailbox {
    phase: AtomicU64,
    generation: AtomicU64,
    report: UnsafeCell<PeerReport>,
}
// SAFETY: CPU0 owns report preparation/reuse only while CPU1 is confirmed OFF
// under BUSY. CPU1 writes only after Acquire Prepared/Go. CPU0 reads only after
// Acquire Returned plus PSCI OFF. No payload references escape either owner.
unsafe impl Sync for Mailbox {}
static MAILBOX: Mailbox = Mailbox {
    phase: AtomicU64::new(IDLE),
    generation: AtomicU64::new(0),
    report: UnsafeCell::new(PeerReport::ZERO),
};
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
const _: () = {
    assert!(core::mem::offset_of!(Mailbox, phase) == 0);
    assert!(core::mem::offset_of!(Mailbox, generation) == 8);
    assert!(core::mem::offset_of!(Mailbox, report) == 16);
    assert!(core::mem::size_of::<AtomicU64>() == 8);
    assert!(core::mem::size_of::<PeerReport>() == 32);
};

// SAFETY: Fixed CPU1 entry uses only reserved stacks/root, establishes EL1 ABI,
// returns through every Rust frame before STLR Returned and nonreturning OFF.
global_asm!(include_str!("secondary-aarch64.S"),
    adapter = sym adapter, mailbox = sym MAILBOX,
    affinity_mask = const super::cpu_local::AFFINITY_MASK,
    cpu_stride = const super::cpu_local::STRIDE,
    off_return = const if cfg!(feature = "secondary-cpu-off-return-test") { 1 } else { 0 });
// SAFETY: Private fixed entries/markers belong to the reviewed image. No caller
// supplies arbitrary stack/root addresses; CPU_ON always uses this entry.
unsafe extern "C" {
    fn arm_secondary_start();
    fn arm_secondary_failure() -> !;
    #[cfg(feature = "secondary-stale-generation-test")]
    fn arm_secondary_stale() -> !;
    static mut arm_secondary_root: [u64; 512];
    static __secondary_stack_guard: [u8; 0];
    static __secondary_stack_top: [u8; 0];
    static __secondary_exception_stack_guard: [u8; 0];
    static __secondary_exception_stack_top: [u8; 0];
}

#[derive(Clone, Copy)]
pub(super) struct Info {
    pub version: u32,
    pub cpu_on: i64,
    pub cpu_off: i64,
    pub affinity_info: i64,
}
pub(super) struct Session {
    pub generation: u64,
    pub cpu: u64,
    pub on: i64,
    pub root: u64,
    pub sp: u64,
    pub checks: u64,
}
#[derive(Debug)]
pub(super) enum Error {
    Authority,
    Busy,
    Generation,
}

/// Boot capability; no transfer, sharing, cloning or caller-provided jobs.
pub struct Runner {
    info: Info,
    _local: PhantomData<*mut ()>,
}
impl Runner {
    /// Caller proves actual boot topology, initialized immutable roots/stacks,
    /// coherent QEMU PSCI profile and exclusive boot authority on CPU0.
    // SAFETY: This constructor cannot be reached through a safe topology value
    // alone; the caller supplies machine authority, not merely parsed bytes.
    pub(crate) unsafe fn from_boot(topology: &Topology) -> Option<Self> {
        if !admitted(topology) || !authority() {
            return None;
        }
        let version = hvc(VERSION, 0, 0, 0) as u32;
        if version >> 16 != 1 {
            return None;
        }
        let info = Info {
            version,
            cpu_on: hvc(FEATURES, CPU_ON, 0, 0) as u32 as i32 as i64,
            cpu_off: hvc(FEATURES, CPU_OFF, 0, 0) as u32 as i32 as i64,
            affinity_info: hvc(FEATURES, AFFINITY_INFO, 0, 0) as u32 as i32 as i64,
        };
        if info.cpu_on != 0 || info.cpu_off != 0 || info.affinity_info != 0 {
            return None;
        }
        Some(Self {
            info,
            _local: PhantomData,
        })
    }
    pub(super) fn info(&self) -> Info {
        self.info
    }

    pub(super) fn run(&mut self) -> Result<Session, Error> {
        if !authority() {
            return Err(Error::Authority);
        }
        super::threads::BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        if hvc(AFFINITY_INFO, 1, 0, 0) != 1 || MAILBOX.phase.load(Ordering::Acquire) != IDLE {
            fail();
        }
        let generation = NEXT_GENERATION.load(Ordering::Relaxed);
        let Some(next) = generation.checked_add(1) else {
            super::threads::BUSY.store(false, Ordering::Release);
            return Err(Error::Generation);
        };
        NEXT_GENERATION.store(next, Ordering::Relaxed);
        // SAFETY: BUSY and PSCI OFF prove there is no peer reader/writer or live
        // Rust frame. Prepare only permanent payload and private CPU1 record.
        unsafe {
            MAILBOX.report.get().write(PeerReport::ZERO);
            super::cpu_local::prepare_secondary();
        }
        MAILBOX.generation.store(generation, Ordering::Relaxed);
        MAILBOX.phase.store(PREPARED, Ordering::Release);
        // SAFETY: Publish cached mailbox/record writes before firmware starts
        // the peer; its first mutable reads happen only after coherent MMU setup.
        unsafe {
            asm!("dsb sy", options(nostack, preserves_flags));
        }
        let context = if cfg!(feature = "secondary-stale-generation-test") {
            next
        } else {
            generation
        };
        let on = hvc(CPU_ON, 1, arm_secondary_start as *const () as u64, context) as i64;
        if on != 0 {
            fail();
        }
        wait_phase(PREPARED, ONLINE);
        MAILBOX.phase.store(GO, Ordering::Release);
        wait_phase(GO, RETURNED);
        loop {
            match hvc(AFFINITY_INFO, 1, 0, 0) {
                1 => break,
                0 | 2 => core::hint::spin_loop(),
                _ => fail(),
            }
        }
        // SAFETY: Acquire Returned and firmware OFF jointly prove all peer Rust
        // frames ended and no CPU executes on either stack. No borrowed payload
        // exists; copy report before clearing only usable stack bytes.
        let report = unsafe {
            let report = MAILBOX.report.get().read();
            clear_stacks();
            super::cpu_local::restore_secondary();
            report
        };
        MAILBOX.phase.store(IDLE, Ordering::Release);
        super::threads::BUSY.store(false, Ordering::Release);
        Ok(Session {
            generation,
            cpu: report.cpu,
            on,
            root: report.root,
            sp: report.sp,
            checks: report.checks,
        })
    }
}

pub(super) fn admitted(topology: &Topology) -> bool {
    topology.affinities() == [0, 1] && topology.psci().is_some_and(|p| p.conduit == Conduit::Hvc)
}
fn authority() -> bool {
    let daif: u64;
    // SAFETY: Private EL1 observer; does not modify control state.
    unsafe {
        asm!("mrs {}, daif", out(reg) daif, options(nomem, nostack, preserves_flags));
    }
    daif == 0x3c0 && super::cpu_local::current_id() == Some(0)
}
fn hvc(function: u64, a: u64, b: u64, c: u64) -> u64 {
    let result: u64;
    // SAFETY: Private callers use only the listed PSCI IDs under boot authority.
    // Explicit inputs/outputs plus C clobbers cover SMCCC caller state; default
    // memory effects prevent compiler motion across firmware admission/status.
    unsafe {
        asm!("hvc #0", inlateout("x0") function => result,
             inlateout("x1") a => _, inlateout("x2") b => _, inlateout("x3") c => _,
             clobber_abi("C"), options(nostack));
    }
    result
}
fn wait_phase(waiting: u64, next: u64) {
    loop {
        let phase = MAILBOX.phase.load(Ordering::Acquire);
        if phase == next {
            return;
        }
        if phase != waiting {
            fail();
        }
        core::hint::spin_loop();
    }
}
fn fail() -> ! {
    // SAFETY: Stack-independent terminal guest exit; never returns authority.
    unsafe { arm_secondary_failure() }
}

// SAFETY: CPU0 holds BUSY and has observed Returned+OFF; each exact linker range
// is reserved, mapped RW and excludes its guard. No peer frame or borrow lives.
unsafe fn clear_stacks() {
    let ranges = [
        (
            core::ptr::addr_of!(__secondary_stack_guard) as usize,
            core::ptr::addr_of!(__secondary_stack_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_exception_stack_guard) as usize,
            core::ptr::addr_of!(__secondary_exception_stack_top) as usize,
        ),
    ];
    for (guard, top) in ranges {
        // SAFETY: Linker and full boot checks prove exact usable bounds.
        unsafe {
            core::ptr::write_bytes((guard + 4096) as *mut u8, 0, top - guard - 4096);
        }
    }
}

// SAFETY: Only the fixed assembly CPU1 entry invokes this after complete EL1
// and coherent-memory setup, carrying the PSCI context in x0. Every frame returns.
unsafe extern "C" fn adapter(context: u64) {
    if MAILBOX.phase.load(Ordering::Acquire) != PREPARED {
        fail();
    }
    if MAILBOX.generation.load(Ordering::Acquire) != context {
        #[cfg(feature = "secondary-stale-generation-test")]
        // SAFETY: Dedicated negative exits only upon detected admission mismatch.
        unsafe {
            arm_secondary_stale();
        }
        #[cfg(not(feature = "secondary-stale-generation-test"))]
        fail();
    }
    MAILBOX.phase.store(ONLINE, Ordering::Release);
    wait_phase(ONLINE, GO);
    let mut report = observe();
    report.checks |= 1 << 7; // Fixed diagnostic callback returned normally.
    // SAFETY: Acquire Go grants CPU1 sole report ownership until it returns to
    // assembly. CPU0 cannot read until Returned and OFF; no reference escapes.
    unsafe {
        MAILBOX.report.get().write(report);
    }
}

fn translation(address: u64) -> u64 {
    let result;
    // SAFETY: AT performs an architectural translation query, no Rust pointer
    // dereference or faulting load. PAR is local to this CPU; IRQ stays masked.
    unsafe {
        asm!("at s1e1r, {address}", "isb", "mrs {result}, par_el1",
        address = in(reg) address, result = out(reg) result, options(nostack, preserves_flags));
    }
    result
}
fn observe() -> PeerReport {
    let (mpidr, root, sp, emergency, pan, daif, sctlr, tcr, mair, el, cpacr, timer): (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    );
    // SAFETY: Read-only observations on CPU1 after its assembly entry; only
    // CPU1's own registers are accessed, with IRQ masked and a private stack.
    unsafe {
        asm!("mrs {mpidr}, mpidr_el1", "mrs {root}, ttbr0_el1", "mov {sp}, sp",
            "mrs {emergency}, sp_el0", "mrs {pan}, S3_0_C4_C2_3", "mrs {daif}, daif",
            "mrs {sctlr}, sctlr_el1", "mrs {tcr}, tcr_el1", "mrs {mair}, mair_el1",
            "mrs {el}, CurrentEL", "mrs {cpacr}, cpacr_el1", "mrs {timer}, cntp_ctl_el0",
            mpidr=out(reg) mpidr,root=out(reg) root,sp=out(reg) sp,emergency=out(reg) emergency,
            pan=out(reg) pan,daif=out(reg) daif,sctlr=out(reg) sctlr,tcr=out(reg) tcr,
            mair=out(reg) mair,el=out(reg) el,cpacr=out(reg) cpacr,timer=out(reg) timer,
            options(nomem,nostack,preserves_flags));
    }
    let mut checks = 0;
    let cpu = mpidr & super::cpu_local::AFFINITY_MASK;
    if cpu == 1 {
        checks |= 1;
    }
    if super::cpu_local::secondary_identity() {
        checks |= 1 << 1;
    }
    if root == core::ptr::addr_of!(arm_secondary_root) as u64 {
        checks |= 1 << 2;
    }
    let guard = core::ptr::addr_of!(__secondary_stack_guard) as u64;
    let top = core::ptr::addr_of!(__secondary_stack_top) as u64;
    let translated = translation(sp);
    if sp >= guard + 4096
        && sp < top
        && sp.is_multiple_of(16)
        && emergency == core::ptr::addr_of!(__secondary_exception_stack_top) as u64
        && translated & 1 == 0
        && translated & 0x0000_ffff_ffff_f000 == sp & !4095
    {
        checks |= 1 << 3;
    }
    if el == 4
        && daif == 0x3c0
        && pan & (1 << 22) != 0
        && sctlr & 0x100d == 0x100d
        && sctlr & (1 << 19) != 0
        && sctlr & (1 << 23) == 0
        && tcr == 0x8099_3519
        && mair == 0xff
        && cpacr & 0x300000 == 0x300000
        && timer & 3 == 2
    {
        checks |= 1 << 4;
    }
    if translation(guard) & 0x7f == 15 {
        checks |= 1 << 5;
    }
    if translation(0x8000_0000) & 0x7f == 11 {
        checks |= 1 << 6;
    }
    PeerReport {
        cpu,
        root,
        sp,
        checks,
    }
}
