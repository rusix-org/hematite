//! Protected CPU0 native execution; designs 0023 and 0023-native-wire.
use super::user_space::{self, Space};
#[path = "process_layout.rs"]
mod layout;
use core::arch::{asm, global_asm};
use core::fmt::{self, Write};
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use hm_kernel::cpu_topology::{Conduit, Topology};
use hm_kernel::native::{self, Host, Issuer, Session};

pub const STARTUP: u64 = 0x1_0010_0000;
pub const STACK_TOP: u64 = 0x1_0020_0000;
const MAX_COUNT: u64 = 0x00ff_ffff_ffff_ffff;
static CLAIMED: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Context,
    Topology,
    AlreadyInitialized,
    Generation,
    Budget,
    Exhausted,
    Corrupt,
    Busy,
    Peer,
    Timer,
    Platform,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    Exit,
    Fault,
    Deadline,
    CallLimit,
}
#[derive(Clone, Copy, Debug)]
pub struct RunReport {
    pub reason: Reason,
    pub code: u8,
    pub calls: usize,
    pub written: usize,
    pub esr: u64,
    pub far: u64,
    pub elr: u64,
    pub spsr: u64,
    pub irqs: u64,
    pub root: u64,
    pub vbar: u64,
    pub controls: bool,
}
pub struct Executor {
    issuer: Issuer,
    cpus: usize,
    index: u64,
    frequency: u64,
    _local: PhantomData<*mut ()>,
}
pub struct Process<'pool> {
    space: Space<'pool>,
    session: Session,
    context: [u64; layout::USER_WORDS],
    handle: u64,
}
// SAFETY: Fixed assembly uses the reserved CPU0 exception stack, no user stack.
// It retains roots/context until boot controls and GIC are fully restored.
global_asm!(include_str!("process-aarch64.S"),
    ACTIVE = const layout::ACTIVE,
    DEADLINE = const layout::DEADLINE,
    FRAME_SIZE = const layout::FRAME_SIZE,
    GICC_CTL = const layout::GICC_CTL,
    GICD_CTL = const layout::GICD_CTL,
    GIC_BPR = const layout::GIC_BPR,
    GIC_CONFIG = const layout::GIC_CONFIG,
    GIC_ENABLE = const layout::GIC_ENABLE,
    GIC_GROUP = const layout::GIC_GROUP,
    GIC_PMR = const layout::GIC_PMR,
    GIC_PRIORITY = const layout::GIC_PRIORITY,
    K_CNTKCTL = const layout::K_CNTKCTL,
    K_CONTEXT = const layout::K_CONTEXT,
    K_CPACR = const layout::K_CPACR,
    K_CTL = const layout::K_CTL,
    K_CVAL = const layout::K_CVAL,
    K_DAIF = const layout::K_DAIF,
    K_ELR = const layout::K_ELR,
    K_ESR = const layout::K_ESR,
    K_FAR = const layout::K_FAR,
    K_FPCR = const layout::K_FPCR,
    K_FPSR = const layout::K_FPSR,
    K_GPRS = const layout::K_GPRS,
    K_MAIR = const layout::K_MAIR,
    K_PAR = const layout::K_PAR,
    K_Q = const layout::K_Q,
    K_SP = const layout::K_SP,
    K_SP0 = const layout::K_SP0,
    K_SPSR = const layout::K_SPSR,
    K_TCR = const layout::K_TCR,
    K_TLS = const layout::K_TLS,
    K_TLSRO = const layout::K_TLSRO,
    K_TTBR0 = const layout::K_TTBR0,
    K_TTBR1 = const layout::K_TTBR1,
    K_VBAR = const layout::K_VBAR,
    RETURNED_KIND = const layout::RETURNED_KIND,
    ROOT = const layout::ROOT,
    FEATURES = const layout::FEATURES,
    K_DIT = const layout::K_DIT,
    K_SSBS = const layout::K_SSBS,
    TIMER_STATUS = const layout::TIMER_STATUS,
    TRAP_COUNT = const layout::TRAP_COUNT,
    USER_ESR = const layout::USER_ESR,
    USER_FAR = const layout::USER_FAR,
    USER_FAR_VALID = const layout::USER_FAR_VALID,
    USER_FPCR = const layout::USER_FPCR,
    USER_FPSR = const layout::USER_FPSR,
    USER_GPRS = const layout::USER_GPRS,
    USER_IAR = const layout::USER_IAR,
    USER_KIND = const layout::USER_KIND,
    USER_PC = const layout::USER_PC,
    USER_PSTATE = const layout::USER_PSTATE,
    USER_Q = const layout::USER_Q,
    USER_SIZE = const layout::USER_SIZE,
    USER_SP = const layout::USER_SP,
    USER_TLS = const layout::USER_TLS,
);
// SAFETY: Fixed private symbols belong to the reviewed kernel image; the
// context is copied under boot mappings and no pointer comes from EL0.
unsafe extern "C" {
    static arm_vectors: u8;
    static __exception_stack_top: u8;
    fn arm_process_slice(root: u64, context: *mut u64, deadline: u64) -> u64;
    fn arm_process_kernel_fatal() -> !;
}
impl Executor {
    /// Caller proves actual BootPlatform topology, completed timer preflight,
    /// no external CPU starts/DMA, and permanent reviewed boot mappings.
    // SAFETY: Permanent latch prevents another hardware grant namespace.
    pub(crate) unsafe fn from_boot(topology: &Topology) -> Result<Self, Error> {
        admission()?;
        let cpus = match topology.affinities() {
            [0] => 1,
            [0, 1] if topology.psci().is_some_and(|p| p.conduit == Conduit::Hvc) => 2,
            _ => return Err(Error::Topology),
        };
        let frequency = frequency();
        if !(1000..=1_000_000_000).contains(&frequency) {
            return Err(Error::Timer);
        }
        CLAIMED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::AlreadyInitialized)?;
        Ok(Self {
            issuer: Issuer::new(),
            cpus,
            index: 0,
            frequency,
            _local: PhantomData,
        })
    }
    pub fn frequency(&self) -> u64 {
        self.frequency
    }
    pub fn ticks(&self) -> u64 {
        self.frequency / 10
    }
    // Returning the original fixed-size owner avoids allocation on failure;
    // caller/constructor stack bounds are independently checked before guests.
    #[allow(clippy::result_large_err)]
    #[inline(never)]
    pub fn prepare<'p>(
        &mut self,
        mut space: Space<'p>,
        console: bool,
        budget: usize,
    ) -> Result<Process<'p>, (Error, Space<'p>)> {
        if admission().is_err() || space.cpus() != self.cpus {
            return Err((Error::Context, space));
        }
        let session = match self.issuer.issue(console) {
            Ok(s) => s,
            Err(_) => return Err((Error::Generation, space)),
        };
        let startup = session.startup();
        if let Err(e) = space.install_startup(&startup, budget) {
            let error = match e {
                user_space::Error::Budget => Error::Budget,
                user_space::Error::Exhausted => Error::Exhausted,
                _ => Error::Corrupt,
            };
            return Err((error, space));
        }
        let handle = u64::from_le_bytes(startup[24..32].try_into().unwrap_or([0; 8]));
        let mut context = [0; layout::USER_WORDS];
        context[0] = STARTUP;
        context[layout::USER_SP / 8] = STACK_TOP;
        context[layout::USER_PC / 8] = space.entry();
        context[layout::USER_PSTATE / 8] = 0x0040_0340;
        Ok(Process {
            space,
            session,
            context,
            handle,
        })
    }
    #[inline(never)]
    pub fn run(&mut self, mut process: Process<'_>) -> Result<RunReport, Error> {
        user_space::context().map_err(|_| Error::Context)?;
        if super::threads::BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::Busy);
        }
        let lease = Lease;
        let result = self.execute(&mut process);
        drop(process);
        drop(lease);
        result
    }
    #[inline(never)]
    fn execute(&mut self, process: &mut Process<'_>) -> Result<RunReport, Error> {
        admission()?;
        if self.cpus == 2
            && ((hvc(0x8400_000a, 0xc400_0004, 0) as i64) < 0 || hvc(0xc400_0004, 1, 0) != 1)
        {
            return Err(Error::Peer);
        }
        let index = self.index;
        self.index = self.index.checked_add(1).ok_or(Error::Platform)?;
        process
            .space
            .prepare_instructions()
            .map_err(|_| Error::Corrupt)?;
        let initial = counter();
        if initial > MAX_COUNT - self.frequency {
            return Err(Error::Timer);
        }
        let deadline = initial.checked_add(self.ticks()).ok_or(Error::Timer)?;
        let saved = observe();
        let mut previous = initial;
        let mut irqs = 0;
        let mut spurious = 0;
        let (reason, code) = loop {
            let now = counter();
            if now < previous {
                return Err(Error::Timer);
            }
            previous = now;
            if now >= deadline {
                break (Reason::Deadline, 0);
            }
            let pc = process.context[layout::USER_PC / 8];
            if pc & 3 != 0
                || !process
                    .space
                    .page(pc)
                    .is_some_and(|p| p.permissions.executable())
                || !process
                    .space
                    .page(pc + 3)
                    .is_some_and(|p| p.permissions.executable())
            {
                break (Reason::Fault, 0);
            }
            admission()?;
            // SAFETY: BUSY and actual peer OFF retained; private context and
            // audited owned root stay alive. Assembly copies only under boot
            // mappings, uses trusted exception stack, restores before return.
            let kind = unsafe {
                arm_process_slice(
                    process.space.root_address(),
                    process.context.as_mut_ptr(),
                    deadline,
                )
            };
            if observe() != saved || admission().is_err() {
                fatal();
            }
            let spsr = process.context[layout::USER_PSTATE / 8];
            if spsr & 0x1f != 0 {
                fatal();
            }
            match kind {
                3 => {
                    irqs += 1;
                    break (Reason::Deadline, 0);
                }
                4 => {
                    spurious += 1;
                    if spurious > 8 {
                        return Err(Error::Platform);
                    }
                    continue;
                }
                2 => break (Reason::Fault, 0),
                1 => {}
                _ => fatal(),
            }
            let now = counter();
            if now < previous {
                return Err(Error::Timer);
            }
            previous = now;
            if now >= deadline {
                break (Reason::Deadline, 0);
            }
            let request = native::Request {
                version: process.context[0],
                operation: process.context[8],
                argument: process.context[1],
                address: process.context[2],
                length: process.context[3],
            };
            let ordinal = process.session.calls() + 1;
            let _ = writeln!(
                Uart,
                "HMPROC CALL index={index} ordinal={ordinal} pc={} version={} operation={} arg1={} arg2={} arg3={}",
                process.context[layout::USER_PC / 8],
                request.version,
                request.operation,
                request.argument,
                request.address,
                request.length
            );
            let mut host = Console {
                space: &process.space,
                index,
                ordinal,
            };
            match process.session.dispatch(request, &mut host) {
                native::Outcome::Stop(native::Stop::Exit(code)) => break (Reason::Exit, code),
                native::Outcome::Stop(native::Stop::CallLimit) => break (Reason::CallLimit, 0),
                native::Outcome::Reply(reply) => {
                    let _ = writeln!(
                        Uart,
                        "HMPROC RESULT index={index} ordinal={ordinal} status={} bytes={}",
                        reply.status as u64, reply.written
                    );
                    process.context[0] = reply.status as u64;
                    process.context[1] = reply.written;
                    if !process.session.may_resume() {
                        break (Reason::CallLimit, 0);
                    }
                }
            }
        };
        let report = RunReport {
            reason,
            code,
            calls: process.session.calls(),
            written: process.session.written(),
            esr: if reason == Reason::Deadline {
                0
            } else {
                process.context[layout::USER_ESR / 8]
            },
            far: if reason == Reason::Deadline || process.context[layout::USER_FAR_VALID / 8] == 0 {
                0
            } else {
                process.context[layout::USER_FAR / 8]
            },
            elr: process.context[layout::USER_PC / 8],
            spsr: process.context[layout::USER_PSTATE / 8],
            irqs,
            root: saved[0],
            vbar: saved[1],
            controls: true,
        };
        Ok(report)
    }
}
impl Process<'_> {
    pub fn entry(&self) -> u64 {
        self.space.entry()
    }
    pub fn root_address(&self) -> u64 {
        self.space.root_address()
    }
    pub fn frame_count(&self) -> usize {
        self.space.frame_count()
    }
    pub fn handle(&self) -> u64 {
        self.handle
    }
}
struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        super::threads::BUSY.store(false, Ordering::Release);
    }
}
struct Console<'a, 'p> {
    space: &'a Space<'p>,
    index: u64,
    ordinal: usize,
}
impl Host for Console<'_, '_> {
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), native::CopyError> {
        self.space
            .read_into(address, output)
            .map_err(|_| native::CopyError)
    }
    fn write_console(&mut self, bytes: &[u8]) {
        let _ = write!(
            Uart,
            "HMPROC OUTPUT index={} ordinal={} length={} hex=",
            self.index,
            self.ordinal,
            bytes.len()
        );
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in bytes {
            uart(HEX[(byte >> 4) as usize]);
            uart(HEX[(byte & 15) as usize]);
        }
        uart(b'\n');
    }
}
struct Uart;
impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            uart(b);
        }
        Ok(())
    }
}
fn uart(byte: u8) {
    for _ in 0..100_000 {
        // SAFETY: Admitted fixed PL011 device page, no DMA or caller address.
        unsafe {
            if (0x0900_0018 as *const u32).read_volatile() & (1 << 5) == 0 {
                (0x0900_0000 as *mut u32).write_volatile(byte as u32);
                return;
            }
        }
        core::hint::spin_loop();
    }
    fatal();
}
fn fatal() -> ! {
    // SAFETY: Nonreturning stack-independent failure never reclaims active state.
    unsafe { arm_process_kernel_fatal() }
}
fn counter() -> u64 {
    let value;
    // SAFETY: Kernel physical counter observation.
    unsafe {
        asm!("isb","mrs {},cntpct_el0",out(reg)value,options(nomem,nostack,preserves_flags));
    }
    value
}
fn frequency() -> u64 {
    let value;
    // SAFETY: Read-only kernel timer frequency.
    unsafe {
        asm!("mrs {},cntfrq_el0",out(reg)value,options(nomem,nostack,preserves_flags));
    }
    value
}
fn hvc(function: u64, a: u64, b: u64) -> u64 {
    let result;
    // SAFETY: Only admitted PSCI feature/status operations under BUSY.
    unsafe {
        asm!("hvc #0",inlateout("x0")function=>result,inlateout("x1")a=>_,inlateout("x2")b=>_,inlateout("x3")0u64=>_,clobber_abi("C"),options(nostack));
    }
    result
}
fn observe() -> [u64; 26] {
    macro_rules! read { ($register:literal) => {{
        let value:u64;
        // SAFETY: Read-only listed EL1 system register, no memory access.
        unsafe {asm!(concat!("mrs {},",$register),out(reg)value,options(nomem,nostack,preserves_flags));}
        value
    }}; }
    [
        read!("ttbr0_el1"),
        read!("vbar_el1"),
        read!("sp_el0"),
        read!("daif"),
        read!("cntkctl_el1"),
        read!("cpacr_el1"),
        read!("tpidr_el0"),
        read!("tpidrro_el0"),
        read!("par_el1"),
        read!("sctlr_el1"),
        read!("tcr_el1"),
        read!("mair_el1"),
        read!("ttbr1_el1"),
        read!("elr_el1"),
        read!("spsr_el1"),
        read!("esr_el1"),
        read!("far_el1"),
        read!("fpcr"),
        read!("fpsr"),
        read!("tpidr_el1"),
        read!("cntp_cval_el0"),
        read!("cntp_ctl_el0") & 3,
        read!("currentel"),
        read!("spsel"),
        if (read!("id_aa64pfr0_el1") >> 48) & 15 == 1 {
            read!("S3_3_C4_C2_5")
        } else {
            0
        },
        if (read!("id_aa64pfr1_el1") >> 4) & 15 == 2 {
            read!("S3_3_C4_C2_6")
        } else {
            0
        },
    ]
}

fn admission() -> Result<(), Error> {
    user_space::context().map_err(|_| Error::Context)?;
    let (pfr0, pfr1): (u64, u64);
    // SAFETY: Feature IDs are always accessible at admitted EL1; no optional
    // PSTATE register is accessed until its exact supported feature is known.
    unsafe {
        asm!("mrs {},id_aa64pfr0_el1", "mrs {},id_aa64pfr1_el1",
            out(reg)pfr0,out(reg)pfr1,options(nomem,nostack,preserves_flags));
    }
    if !matches!((pfr0 >> 48) & 15, 0 | 1)
        || !matches!((pfr1 >> 4) & 15, 0 | 2)
        || (pfr1 >> 8) & 15 != 0
    {
        return Err(Error::Context);
    }
    let state = observe();
    if state[5] != 0x0030_0000
        || state[1] != core::ptr::addr_of!(arm_vectors) as u64
        || state[2] != core::ptr::addr_of!(__exception_stack_top) as u64
    {
        return Err(Error::Context);
    }
    let (el, spsel, pmu, timer, local): (u64, u64, u64, u64, usize);
    // SAFETY: Read-only privileged CPU identity/feature/timer observations.
    unsafe {
        asm!("mrs {},currentel","mrs {},spsel","mrs {},id_aa64dfr0_el1","mrs {},cntp_ctl_el0","mrs {},tpidr_el1",out(reg)el,out(reg)spsel,out(reg)pmu,out(reg)timer,out(reg)local,options(nomem,nostack,preserves_flags));
    }
    if el != 4 || spsel != 1 || timer & 3 != 2 {
        return Err(Error::Context);
    }
    match (pmu >> 8) & 15 {
        0 => {}
        1 | 4 | 5 | 6 => {
            let access: u64;
            // SAFETY: PMU presence/version checked before accessing its user gate.
            unsafe {
                asm!("mrs {},pmuserenr_el0",out(reg)access,options(nomem,nostack,preserves_flags));
            }
            if access != 0 {
                return Err(Error::Context);
            }
        }
        _ => return Err(Error::Context),
    }
    // SAFETY: cpu_local::current_id in context authenticated TPIDR_EL1. These
    // private flags are stable under masked CPU0/BUSY; GIC pages are boot mapped.
    unsafe {
        if ((local + super::cpu_local::TIMER + 16) as *const u64).read_volatile() != 0
            || ((local + super::cpu_local::PREEMPT + 16) as *const u64).read_volatile() != 0
        {
            return Err(Error::Busy);
        }
        let d = 0x0800_0000 as *const u32;
        let c = 0x0801_0000 as *const u32;
        if d.read_volatile() != 0
            || c.read_volatile() != 0
            || d.add(1).read_volatile() & (1 << 10) != 0
            || (c.add(0xfc / 4).read_volatile() >> 16) & 15 != 2
        {
            return Err(Error::Platform);
        }
        let banks = (d.add(1).read_volatile() & 31) + 1;
        for bank in 0..banks as usize {
            let enable = d.add(0x100 / 4 + bank).read_volatile();
            if enable & if bank == 0 { 0xffff_0000 } else { u32::MAX } != 0
                || d.add(0x300 / 4 + bank).read_volatile() != 0
            {
                return Err(Error::Platform);
            }
        }
        if d.add(0x200 / 4).read_volatile() != 0 {
            return Err(Error::Platform);
        }
    }
    Ok(())
}
