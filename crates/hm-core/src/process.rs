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
pub(super) const MAX_COUNT: u64 = 0x00ff_ffff_ffff_ffff;
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
#[derive(Clone, Copy, Eq, PartialEq)]
enum OutputMode {
    Trace,
    Terminal,
}
pub struct Executor {
    issuer: Issuer,
    output: OutputMode,
    cpus: usize,
    index: u64,
    frequency: u64,
    retired: bool,
    next_group: u64,
    next_task_cap: u32,
    last_prepared: Option<u32>,
    _local: PhantomData<*mut ()>,
}
pub struct Process<'pool> {
    space: Space<'pool>,
    session: Option<Session>,
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
        // SAFETY: The caller supplies the same permanent BootPlatform proof.
        unsafe { Self::from_boot_mode(topology, OutputMode::Trace) }
    }
    /// Same permanent BootPlatform and completed timer-preflight proof as
    /// from_boot; only the sealed private console representation differs.
    // SAFETY: Caller proves actual boot authority and completed timer preflight.
    pub(crate) unsafe fn from_boot_terminal(topology: &Topology) -> Result<Self, Error> {
        // SAFETY: The caller supplies the same permanent BootPlatform proof.
        unsafe { Self::from_boot_mode(topology, OutputMode::Terminal) }
    }
    // SAFETY: Private constructor callers pass their complete BootPlatform proof.
    unsafe fn from_boot_mode(topology: &Topology, output: OutputMode) -> Result<Self, Error> {
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
            output,
            cpus,
            index: 0,
            frequency,
            retired: false,
            next_group: 1,
            next_task_cap: 1,
            last_prepared: None,
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
        space: Space<'p>,
        console: bool,
        budget: usize,
    ) -> Result<Process<'p>, (Error, Space<'p>)> {
        self.prepare_inner(space, console, budget, None)
    }
    /// Copies validated owned input before returning; Process retains no input borrow.
    #[allow(clippy::result_large_err)]
    #[inline(never)]
    pub fn prepare_with_input<'p>(
        &mut self,
        space: Space<'p>,
        console: bool,
        budget: usize,
        input: &hm_kernel::startup_input::Input,
    ) -> Result<Process<'p>, (Error, Space<'p>)> {
        self.prepare_inner(space, console, budget, Some(input))
    }
    // Inline into the two entry points to avoid another large by-value owner
    // frame; compiler stack review remains mandatory for both paths.
    #[allow(clippy::result_large_err)]
    #[inline(always)]
    fn prepare_inner<'p>(
        &mut self,
        mut space: Space<'p>,
        console: bool,
        budget: usize,
        input: Option<&hm_kernel::startup_input::Input>,
    ) -> Result<Process<'p>, (Error, Space<'p>)> {
        if self.retired {
            return Err((Error::Platform, space));
        }
        if admission().is_err() || space.cpus() != self.cpus {
            return Err((Error::Context, space));
        }
        let session = match self.issuer.issue(console) {
            Ok(s) => s,
            Err(_) => return Err((Error::Generation, space)),
        };
        let startup = session.startup();
        if let Err(e) = space.install_startup(&startup, budget, input) {
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
        if handle != 0 {
            self.last_prepared = Some((handle >> 32) as u32);
        }
        Ok(Process {
            space,
            session: Some(session),
            context,
            handle,
        })
    }
    #[inline(never)]
    pub fn run(&mut self, mut process: Process<'_>) -> Result<RunReport, Error> {
        let lease = Lease::acquire()?;
        let result = if self.retired {
            Err(Error::Platform)
        } else {
            self.execute(&mut process, &lease)
        };
        drop(process);
        drop(lease);
        result
    }
    #[inline(never)]
    fn execute(&mut self, process: &mut Process<'_>, lease: &Lease) -> Result<RunReport, Error> {
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
            let kind = self.slice(process, deadline, &saved, lease);
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
            if let Some(stop) = self.dispatch(process, index) {
                break stop;
            }
        };
        let report = RunReport {
            reason,
            code,
            calls: process.calls(),
            written: process.written(),
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

    fn slice(
        &mut self,
        process: &mut Process<'_>,
        deadline: u64,
        saved: &[u64; 26],
        _lease: &Lease,
    ) -> u64 {
        // SAFETY: The retained private BUSY lease and caller's admitted CPU0/
        // peer-OFF proof exclude other execution. Process owns the root and
        // context throughout the assembly-only window; boot mappings and all
        // listed controls are restored before any Rust resumes.
        let kind = unsafe {
            arm_process_slice(
                process.space.root_address(),
                process.context.as_mut_ptr(),
                deadline,
            )
        };
        if observe() != *saved
            || admission().is_err()
            || process.context[layout::USER_PSTATE / 8] & 0x1f != 0
        {
            fatal();
        }
        kind
    }
    pub(super) fn group_slice(
        &mut self,
        process: &mut Process<'_>,
        deadline: u64,
        saved: &[u64; 26],
        lease: &Lease,
    ) -> u64 {
        if self.cpus != 1
            || self.retired
            || !process.session.as_ref().is_some_and(Session::may_resume)
        {
            fatal();
        }
        self.slice(process, deadline, saved, lease)
    }
    pub(super) fn retire(&mut self) {
        self.retired = true;
    }
    pub(super) fn retired(&self) -> bool {
        self.retired
    }
    pub(super) fn cpus(&self) -> usize {
        self.cpus
    }
    pub(super) fn last_prepared_generation(&self) -> Option<u32> {
        self.last_prepared
    }
    pub(super) fn issue_group(&mut self) -> Result<u64, Error> {
        if self.retired || self.next_group == 0 {
            return Err(Error::Platform);
        }
        let group = self.next_group;
        self.next_group = group.checked_add(1).unwrap_or(0);
        Ok(group)
    }
    pub(super) fn issue_task_cap(&mut self) -> Option<u32> {
        if self.retired || self.next_task_cap == 0 {
            return None;
        }
        let generation = self.next_task_cap;
        self.next_task_cap = generation.checked_add(1).unwrap_or(0);
        Some(generation)
    }
    pub(super) fn group_namespace_available(&self) -> bool {
        self.next_group != 0
    }
    pub(super) fn admit_task_call(
        &mut self,
        process: &mut Process<'_>,
        capture: &mut GroupCapture,
    ) -> native::Admission {
        if capture.request.is_some() {
            fatal();
        }
        let session = process.session.take().unwrap_or_else(|| fatal());
        let request = native::Request {
            version: process.context[0],
            operation: process.context[8],
            argument: process.context[1],
            address: process.context[2],
            length: process.context[3],
        };
        capture.ordinal = session.calls() + 1;
        capture.pc = process.pc();
        capture.request = Some(request);
        let mut host = GroupConsole {
            output: self.output,
            space: &process.space,
            capture,
        };
        session.admit_task(request, &mut host)
    }
    pub(super) fn tracing(&self) -> bool {
        self.output == OutputMode::Trace
    }
    // Legacy execution retains synchronous HMPROC serialization and timing.
    fn dispatch(&mut self, process: &mut Process<'_>, index: u64) -> Option<(Reason, u8)> {
        let request = native::Request {
            version: process.context[0],
            operation: process.context[8],
            argument: process.context[1],
            address: process.context[2],
            length: process.context[3],
        };
        let ordinal = process.calls() + 1;
        if self.tracing() {
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
        }
        let mut host = Console {
            output: self.output,
            space: &process.space,
            index,
            ordinal,
        };
        match process
            .session
            .as_mut()
            .unwrap_or_else(|| fatal())
            .dispatch(request, &mut host)
        {
            native::Outcome::Stop(native::Stop::Exit(code)) => Some((Reason::Exit, code)),
            native::Outcome::Stop(native::Stop::CallLimit) => Some((Reason::CallLimit, 0)),
            native::Outcome::Reply(reply) => {
                if self.tracing() {
                    let _ = writeln!(
                        Uart,
                        "HMPROC RESULT index={index} ordinal={ordinal} status={} bytes={}",
                        reply.status as u64, reply.written
                    );
                }
                process.context[0] = reply.status as u64;
                process.context[1] = reply.written;
                if process.session.as_ref().is_some_and(Session::may_resume) {
                    None
                } else {
                    Some((Reason::CallLimit, 0))
                }
            }
        }
    }
    pub(super) fn dispatch_group(
        &mut self,
        process: &mut Process<'_>,
        capture: &mut GroupCapture,
    ) -> Option<(Reason, u8)> {
        if capture.request.is_some() {
            fatal();
        }
        let request = native::Request {
            version: process.context[0],
            operation: process.context[8],
            argument: process.context[1],
            address: process.context[2],
            length: process.context[3],
        };
        capture.request = Some(request);
        capture.ordinal = process.calls() + 1;
        capture.pc = process.pc();
        let mut host = GroupConsole {
            output: self.output,
            space: &process.space,
            capture,
        };
        match process
            .session
            .as_mut()
            .unwrap_or_else(|| fatal())
            .dispatch(request, &mut host)
        {
            native::Outcome::Stop(native::Stop::Exit(code)) => Some((Reason::Exit, code)),
            native::Outcome::Stop(native::Stop::CallLimit) => Some((Reason::CallLimit, 0)),
            native::Outcome::Reply(reply) => {
                capture.reply = Some(reply);
                process.context[0] = reply.status as u64;
                process.context[1] = reply.written;
                if process.session.as_ref().is_some_and(Session::may_resume) {
                    None
                } else {
                    Some((Reason::CallLimit, 0))
                }
            }
        }
    }
}
impl Process<'_> {
    pub(super) fn put_session(&mut self, session: Session) -> Result<(), Session> {
        if self.session.is_some() {
            return Err(session);
        }
        self.session = Some(session);
        Ok(())
    }
    pub(super) fn has_session(&self) -> bool {
        self.session.is_some()
    }
    pub(super) fn copy_task_bytes(
        &self,
        address: u64,
        output: &mut [u8],
    ) -> Result<(), user_space::Error> {
        self.space.read_into(address, output)
    }
    pub(super) fn task_reply(&mut self, reply: native::Response) {
        self.context[0] = reply.status as u64;
        self.context[1] = reply.written;
    }
    pub(super) fn pc(&self) -> u64 {
        self.context[layout::USER_PC / 8]
    }
    pub(super) fn executable_pc(&self) -> bool {
        let pc = self.pc();
        pc & 3 == 0
            && self
                .space
                .page(pc)
                .is_some_and(|p| p.permissions.executable())
            && pc.checked_add(3).is_some_and(|end| {
                self.space
                    .page(end)
                    .is_some_and(|p| p.permissions.executable())
            })
    }
    pub(super) fn prepare_instructions(&self) -> Result<(), user_space::Error> {
        self.space.prepare_instructions()
    }
    pub(super) fn calls(&self) -> usize {
        self.session.as_ref().unwrap_or_else(|| fatal()).calls()
    }
    pub(super) fn written(&self) -> usize {
        self.session.as_ref().unwrap_or_else(|| fatal()).written()
    }
    pub(super) fn trap(&self) -> (u64, u64, u64, u64) {
        (
            self.context[layout::USER_ESR / 8],
            if self.context[layout::USER_FAR_VALID / 8] == 0 {
                0
            } else {
                self.context[layout::USER_FAR / 8]
            },
            self.pc(),
            self.context[layout::USER_PSTATE / 8],
        )
    }
    #[cfg(feature = "runtime-tests")]
    pub(super) fn runtime_release_word(&self) -> Result<u64, user_space::Error> {
        self.space.runtime_release_word()
    }
    #[cfg(feature = "runtime-tests")]
    pub(super) fn release_runtime_probe(&mut self) -> Result<(), user_space::Error> {
        self.space.release_runtime_probe()
    }
    /// Fixed-page, inactive-only observer for the startup-input profile.
    #[cfg(feature = "startup-input-tests")]
    pub(super) fn copy_startup(
        &self,
        offset: usize,
        output: &mut [u8],
    ) -> Result<(), user_space::Error> {
        if offset
            .checked_add(output.len())
            .is_none_or(|end| end > 4096)
        {
            return Err(user_space::Error::Bounds);
        }
        self.space.read_into(STARTUP + offset as u64, output)
    }

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
pub(super) struct Lease {
    _local: PhantomData<*mut ()>,
}
impl Lease {
    pub(super) fn acquire() -> Result<Self, Error> {
        user_space::context().map_err(|_| Error::Context)?;
        super::threads::BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        Ok(Self {
            _local: PhantomData,
        })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        super::threads::BUSY.store(false, Ordering::Release);
    }
}
struct Console<'a, 'p> {
    output: OutputMode,
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
        if self.output == OutputMode::Terminal {
            for byte in bytes {
                if *byte == b'\n' {
                    uart(b'\r');
                }
                uart(*byte);
            }
            return;
        }
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
/// One owned diagnostic sink, confined to a single inactive group turn.
pub(super) struct GroupCapture {
    request: Option<native::Request>,
    reply: Option<native::Response>,
    ordinal: usize,
    pc: u64,
    output: [u8; native::MAX_WRITE],
    length: usize,
    output_recorded: bool,
}
const _: () = assert!(core::mem::size_of::<GroupCapture>() <= 512);
impl GroupCapture {
    pub(super) fn new() -> Self {
        Self {
            request: None,
            reply: None,
            ordinal: 0,
            pc: 0,
            output: [0; native::MAX_WRITE],
            length: 0,
            output_recorded: false,
        }
    }
    pub(super) fn emit_task_call(&self, group: u64, turn: u64) {
        if let Some(r) = self.request {
            let _ = writeln!(
                Uart,
                "HMTASK CALL group={group} turn={turn} ordinal={} pc={} version={} operation={} arg1={} arg2={} arg3={}",
                self.ordinal, self.pc, r.version, r.operation, r.argument, r.address, r.length
            );
        }
    }
    pub(super) fn emit_task_output(&self, group: u64, turn: u64) {
        if self.length == 0 {
            return;
        }
        let _ = write!(
            Uart,
            "HMTASK OUTPUT group={group} turn={turn} ordinal={} length={} hex=",
            self.ordinal, self.length
        );
        for byte in &self.output[..self.length] {
            let _ = write!(Uart, "{byte:02x}");
        }
        let _ = writeln!(Uart);
    }
    pub(super) fn record_reply(&mut self, reply: native::Response) {
        if self.reply.replace(reply).is_some() {
            fatal();
        }
    }
    pub(super) fn emit(&self, group: u64, turn: u64) {
        let Some(request) = self.request else {
            return;
        };
        let ordinal = self.ordinal;
        let _ = writeln!(
            Uart,
            "HMRT CALL group={group} turn={turn} ordinal={ordinal} pc={} version={} operation={} arg1={} arg2={} arg3={}",
            self.pc,
            request.version,
            request.operation,
            request.argument,
            request.address,
            request.length
        );
        if self.length != 0 {
            let _ = write!(
                Uart,
                "HMRT OUTPUT group={group} turn={turn} ordinal={ordinal} length={} hex=",
                self.length
            );
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for byte in &self.output[..self.length] {
                uart(HEX[(byte >> 4) as usize]);
                uart(HEX[(byte & 15) as usize]);
            }
            uart(b'\n');
        }
        if let Some(reply) = self.reply {
            let _ = writeln!(
                Uart,
                "HMRT RESULT group={group} turn={turn} ordinal={ordinal} status={} bytes={}",
                reply.status as u64, reply.written
            );
        }
    }
}
struct GroupConsole<'space, 'pool, 'capture> {
    output: OutputMode,
    space: &'space Space<'pool>,
    capture: &'capture mut GroupCapture,
}
impl Host for GroupConsole<'_, '_, '_> {
    fn copy_from_user(&mut self, address: u64, output: &mut [u8]) -> Result<(), native::CopyError> {
        self.space
            .read_into(address, output)
            .map_err(|_| native::CopyError)
    }
    fn write_console(&mut self, bytes: &[u8]) {
        if bytes.len() > native::MAX_WRITE || self.capture.output_recorded {
            fatal();
        }
        self.capture.output_recorded = true;
        if self.output == OutputMode::Terminal {
            // Real terminal output remains synchronous and inside the charge.
            for byte in bytes {
                if *byte == b'\n' {
                    uart(b'\r');
                }
                uart(*byte);
            }
        } else {
            // The Trace sink synchronously owns the complete output record.
            // Only its diagnostic serialization is deferred; the retained BUSY
            // lease prevents another user entry before the mandatory flush.
            self.capture.output[..bytes.len()].copy_from_slice(bytes);
            self.capture.length = bytes.len();
        }
    }
}
/// Group-only compiler memory boundary; legacy counter observations are unchanged.
#[inline(always)]
pub(super) fn group_counter(process: &Process<'_>, capture: &GroupCapture) -> u64 {
    let value;
    // SAFETY: Admitted EL1 counter read. Opaque addresses expose both owned
    // objects to the compiler's memory clobber; instructions neither dereference
    // nor mutate them. No nomem/readonly promise permits moving the capture or
    // Process updates past this observation. ISB is not a data-completion fence.
    unsafe {
        asm!("isb", "mrs {value}, cntpct_el0", "/* {owner} {capture} */",
            value = out(reg) value,
            owner = in(reg) core::ptr::from_ref(process),
            capture = in(reg) core::ptr::from_ref(capture),
            options(nostack, preserves_flags));
    }
    value
}
/// Task-only ordering boundary covers detached policy owners and Group commits.
#[inline(always)]
pub(super) fn task_counter<G, W, C>(
    group: &G,
    process: &Process<'_>,
    work: &W,
    capture: &C,
) -> u64 {
    let value;
    // SAFETY: Admitted EL1 counter observation. Opaque addresses are compiler
    // memory inputs only; assembly does not dereference, retain or modify them.
    // Default memory effects order owned policy and Group changes around the read.
    unsafe {
        asm!("isb", "mrs {value}, cntpct_el0", "/* {group} {owner} {work} {capture} */",
            value = out(reg) value, group = in(reg) core::ptr::from_ref(group),
            owner = in(reg) core::ptr::from_ref(process), work = in(reg) core::ptr::from_ref(work),
            capture = in(reg) core::ptr::from_ref(capture), options(nostack, preserves_flags));
    }
    value
}
pub(super) struct Uart;
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
pub(super) fn fatal() -> ! {
    // SAFETY: Nonreturning stack-independent failure never reclaims active state.
    unsafe { arm_process_kernel_fatal() }
}
pub(super) fn counter() -> u64 {
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
pub(super) fn observe() -> [u64; 26] {
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

pub(super) fn admission() -> Result<(), Error> {
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
