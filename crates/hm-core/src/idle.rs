//! Fixed CPU0 masked-WFI transaction; design0031 and its cleanup refinement.
//! All constructors are private to the boot-only profile until production input exists.
use super::process;
use core::arch::{asm, global_asm};
use core::marker::PhantomData;
use hm_kernel::cpu_topology::Topology;

const TIMER: u32 = 1 << 30;
const UART: u32 = 1 << 1;
const POLL_LIMIT: u32 = 1024;
pub(super) const WALL_SECONDS: u64 = 300;

// SAFETY: The private immutable leaf preserves AAPCS64, executes with masked
// DAIF under the boot root, and returns the first ordered post-WFI counter.
global_asm!(include_str!("idle-aarch64.S"));
// SAFETY: Boot owns these permanent aligned tables and fixed assembly symbol.
// Only the admitted CPU0 transaction can invoke the leaf; no EL0 pointer enters.
unsafe extern "C" {
    static arm_root: [u64; 512];
    static arm_device_l2: [u64; 512];
    static arm_gic_l3: [u64; 512];
    static arm_device_l3: [u64; 512];
    fn arm_idle_wfi() -> u64;
    static arm_idle_resume: u8;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Context,
    Busy,
    Clock,
    Configuration,
}

enum TerminalState {
    Monitor,
    Reserved,
    Unavailable,
}
pub(super) struct Terminal {
    state: TerminalState,
}
impl Terminal {
    pub(super) const fn new() -> Self {
        Self {
            state: TerminalState::Monitor,
        }
    }
    pub(super) fn monitor(&self) -> bool {
        matches!(self.state, TerminalState::Monitor)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Case {
    PendingByte,
    ExpiredLimit,
    UartWait,
    TimerWait,
    ProductionWall,
}
const SHORT: [Case; 4] = [
    Case::PendingByte,
    Case::ExpiredLimit,
    Case::UartWait,
    Case::TimerWait,
];
const WALL: [Case; 1] = [Case::ProductionWall];
pub(super) fn profile_valid() -> bool {
    matches!(option_env!("HM_IDLE_PROFILE"), Some("short" | "wall"))
}
impl Case {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::PendingByte => "pending_byte",
            Self::ExpiredLimit => "expired_limit",
            Self::UartWait => "uart_wait",
            Self::TimerWait => "timer_wait",
            Self::ProductionWall => "production_wall",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Pump {
    count: u8,
    dr: u32,
    empty: bool,
}
impl Pump {
    pub(super) fn count(self) -> u8 {
        self.count
    }
    pub(super) fn dr(self) -> u32 {
        self.dr
    }
    pub(super) fn empty(self) -> bool {
        self.empty
    }
}

/// Feature-only proof that this boot has not initialized/entered a Process.
/// It cannot construct the future production inactive-Process witness.
pub(super) struct BootTest<'a> {
    terminal: &'a mut Terminal,
    _execution: process::Lease,
    hardware: Hardware,
    clock: Clock,
    frequency: u64,
    start: u64,
    wall_end: u64,
    wall: bool,
    next: usize,
    pending_pump: bool,
    failed: bool,
    finished: bool,
}
impl<'a> BootTest<'a> {
    /// Actual initialized BootPlatform, immutable tables, no Process initialized
    /// or entered, no peer/DMA starts, and exclusive terminal boundary required.
    // SAFETY: Only the fixed boot wrapper supplies that permanent boot proof.
    pub(super) unsafe fn from_boot(
        terminal: &'a mut Terminal,
        topology: &Topology,
    ) -> Result<Self, Error> {
        if topology.affinities() != [0] || !terminal.monitor() {
            return Err(Error::Busy);
        }
        let wall = match option_env!("HM_IDLE_PROFILE") {
            Some("short") => false,
            Some("wall") => true,
            _ => return Err(Error::Configuration),
        };
        let execution = process::Lease::acquire().map_err(|_| Error::Busy)?;
        // SAFETY: Preserve caller's actual one-CPU boot proof under the lease.
        let hardware = unsafe { Hardware::admit() }?;
        let frequency: u64;
        // SAFETY: Admitted EL1 CPU exposes the physical counter frequency.
        unsafe {
            asm!("mrs {}, cntfrq_el0", out(reg)frequency, options(nostack, preserves_flags));
        }
        if !(1000..=1_000_000_000).contains(&frequency) {
            return Err(Error::Clock);
        }
        let mut clock = Clock {
            last: 0,
            poisoned: false,
            samples: 0,
        };
        let start = clock.sample();
        let wall_end = frequency
            .checked_mul(WALL_SECONDS)
            .and_then(|ticks| start.checked_add(ticks))
            .filter(|end| *end <= process::MAX_COUNT)
            .ok_or(Error::Clock)?;
        if clock.poisoned {
            return Err(Error::Clock);
        }
        terminal.state = TerminalState::Reserved;
        Ok(Self {
            terminal,
            _execution: execution,
            hardware,
            clock,
            frequency,
            start,
            wall_end,
            wall,
            next: 0,
            pending_pump: false,
            failed: false,
            finished: false,
        })
    }
    pub(super) fn profile(&self) -> &'static str {
        if self.wall { "wall" } else { "short" }
    }
    pub(super) fn cases(&self) -> &'static [Case] {
        if self.wall { &WALL } else { &SHORT }
    }
    pub(super) fn frequency(&self) -> u64 {
        self.frequency
    }
    pub(super) fn start(&self) -> u64 {
        self.start
    }
    pub(super) fn wall_end(&self) -> u64 {
        self.wall_end
    }
    fn short_limit(&self, now: u64, seconds: u64) -> Result<u64, Error> {
        self.frequency
            .checked_mul(seconds)
            .and_then(|ticks| now.checked_add(ticks))
            .map(|end| end.min(self.wall_end))
            .ok_or(Error::Clock)
    }
    pub(super) fn wait(&mut self, case: Case) -> Result<Observation, Error> {
        if self.failed || self.pending_pump || self.cases().get(self.next) != Some(&case) {
            return Err(Error::Context);
        }
        // Every early error retires this witness. Success clears this flag only
        // after verified restoration; no failed call may be retried/rebased.
        self.failed = true;
        let samples = self.clock.samples;
        if case == Case::PendingByte {
            let now = self.clock.sample();
            let readiness_end = self.short_limit(now, 5)?;
            let mut ready = false;
            for _ in 0..16_777_216u32 {
                let present = self.hardware.read(Reg::Fr) & 16 == 0;
                let count = self.clock.sample();
                if self.clock.poisoned {
                    return Err(Error::Clock);
                }
                if count >= readiness_end {
                    break;
                }
                if present {
                    ready = true;
                    break;
                }
            }
            if !ready {
                return Err(Error::Configuration);
            }
        } else if self.hardware.read(Reg::Fr) & 16 == 0 {
            return Err(Error::Configuration);
        }
        let now = self.clock.sample();
        if self.clock.poisoned {
            return Err(Error::Clock);
        }
        let limit = match case {
            Case::ExpiredLimit => now,
            Case::ProductionWall => self.wall_end,
            Case::UartWait => self.short_limit(now, 5)?,
            _ => self.short_limit(now, 1)?,
        };
        if limit > self.wall_end {
            return Err(Error::Context);
        }
        let mut observation = wait(&self.hardware, &mut self.clock, limit)?;
        observation.samples = self.clock.samples.saturating_sub(samples);
        self.pending_pump = true;
        self.failed = false;
        Ok(observation)
    }
    pub(super) fn pump_one(&mut self) -> Result<Pump, Error> {
        if self.failed || !self.pending_pump {
            return Err(Error::Context);
        }
        self.failed = true;
        let (count, dr) = if self.hardware.read(Reg::Fr) & 16 == 0 {
            let dr = self.hardware.read(Reg::Dr);
            if dr & !255 != 0 {
                return Err(Error::Configuration);
            }
            (1, dr)
        } else {
            (0, 0)
        };
        let empty = self.hardware.read(Reg::Fr) & 16 != 0;
        if !empty {
            return Err(Error::Configuration);
        }
        self.pending_pump = false;
        self.next += 1;
        self.failed = false;
        Ok(Pump { count, dr, empty })
    }
    pub(super) fn finish(mut self) -> Result<(), Error> {
        if self.failed
            || self.pending_pump
            || self.next != self.cases().len()
            || self.hardware.read(Reg::Fr) & 16 == 0
        {
            return Err(Error::Context);
        }
        process::admission().map_err(|_| Error::Context)?;
        if self.hardware.read(Reg::Imsc) != 0 {
            return Err(Error::Context);
        }
        self.terminal.state = TerminalState::Monitor;
        self.finished = true;
        Ok(())
    }
}
impl Drop for BootTest<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.terminal.state = TerminalState::Unavailable;
        }
    }
}

#[derive(Clone, Copy)]
enum Reg {
    Dctl,
    Dtype,
    Group0,
    Group1,
    Config1,
    Config2,
    Priority7,
    Priority8,
    Target8,
    Enable0,
    Enable1,
    Disable0,
    Disable1,
    Pending0,
    Pending1,
    Clear0,
    Clear1,
    Active0,
    Active1,
    Cctl,
    Pmr,
    Bpr,
    Iar,
    Eoir,
    Iidr,
    Fr,
    Rsr,
    Ris,
    Mis,
    Imsc,
    Dr,
}
impl Reg {
    fn address(self) -> usize {
        match self {
            Self::Dctl => 0x0800_0000,
            Self::Dtype => 0x0800_0004,
            Self::Group0 => 0x0800_0080,
            Self::Group1 => 0x0800_0084,
            Self::Config1 => 0x0800_0c04,
            Self::Config2 => 0x0800_0c08,
            Self::Priority7 => 0x0800_041c,
            Self::Priority8 => 0x0800_0420,
            Self::Target8 => 0x0800_0820,
            Self::Enable0 => 0x0800_0100,
            Self::Enable1 => 0x0800_0104,
            Self::Disable0 => 0x0800_0180,
            Self::Disable1 => 0x0800_0184,
            Self::Pending0 => 0x0800_0200,
            Self::Pending1 => 0x0800_0204,
            Self::Clear0 => 0x0800_0280,
            Self::Clear1 => 0x0800_0284,
            Self::Active0 => 0x0800_0300,
            Self::Active1 => 0x0800_0304,
            Self::Cctl => 0x0801_0000,
            Self::Pmr => 0x0801_0004,
            Self::Bpr => 0x0801_0008,
            Self::Iar => 0x0801_000c,
            Self::Eoir => 0x0801_0010,
            Self::Iidr => 0x0801_00fc,
            Self::Fr => 0x0900_0018,
            Self::Rsr => 0x0900_0004,
            Self::Ris => 0x0900_003c,
            Self::Mis => 0x0900_0040,
            Self::Imsc => 0x0900_0038,
            Self::Dr => 0x0900_0000,
        }
    }
}

const FIELDS: [(Reg, u32, u32); 9] = [
    (Reg::Group0, TIMER, 0),
    (Reg::Group1, UART, 0),
    (Reg::Config1, 1 << 29, 0),
    (Reg::Config2, 1 << 3, 0),
    (Reg::Priority7, 0x00ff_0000, 0x0080_0000),
    (Reg::Priority8, 0x0000_ff00, 0x0000_8000),
    // The admitted one-interface GIC has RAZ/WI SPI targets and implicit CPU0
    // routing. Retain its exact zero observation; never program this field.
    (Reg::Target8, 0, 0),
    (Reg::Pmr, 0xff, 0xff),
    (Reg::Bpr, 7, 0),
];

/// An unforgeable local proof of the fixed permanent device mappings and CPU.
/// No callback or context-switching operation exists on this private owner.
struct Hardware {
    _local: PhantomData<*mut ()>,
}
impl Hardware {
    /// Caller supplies actual immutable BootPlatform tables, single CPU and no
    /// DMA or external CPU starts; the retained execution lease excludes entry.
    // SAFETY: The caller retains that permanent boot proof for this whole owner.
    unsafe fn admit() -> Result<Self, Error> {
        super::user_space::context().map_err(|_| Error::Context)?;
        let root = core::ptr::addr_of!(arm_root);
        let device = core::ptr::addr_of!(arm_device_l2);
        let gic = core::ptr::addr_of!(arm_gic_l3);
        let uart = core::ptr::addr_of!(arm_device_l3);
        // SAFETY: Fixed linker tables are permanent kernel data. Each parent is
        // checked before the next retained table's contents are consulted.
        let mapped = unsafe {
            (root.cast::<u64>()).read_volatile() == device as u64 | 3
                && device.cast::<u64>().add(64).read_volatile() == gic as u64 | 3
                && device.cast::<u64>().add(72).read_volatile() == uart as u64 | 3
                && gic.cast::<u64>().read_volatile() == 0x0060_0000_0800_0407
                && gic.cast::<u64>().add(16).read_volatile() == 0x0060_0000_0801_0407
                && uart.cast::<u64>().read_volatile() == 0x0060_0000_0900_0407
        };
        if !mapped {
            return Err(Error::Context);
        }
        let hardware = Self {
            _local: PhantomData,
        };
        let typer = hardware.read(Reg::Dtype);
        // Establish SPI33 existence before any bank1/config/target access.
        if typer & 31 < 1
            || (typer >> 5) & 7 != 0
            || typer & (1 << 10) != 0
            || hardware.read(Reg::Iidr) >> 16 & 15 != 2
        {
            return Err(Error::Context);
        }
        process::admission().map_err(|_| Error::Context)?;
        if hardware.read(Reg::Imsc) != 0 {
            return Err(Error::Context);
        }
        Ok(hardware)
    }
    fn read(&self, reg: Reg) -> u32 {
        // SAFETY: The retained hardware proof admits exactly these aligned
        // Device-nGnRnE registers, boot root, CPU0 and masked execution context.
        unsafe { (reg.address() as *const u32).read_volatile() }
    }
    fn write(&self, reg: Reg, value: u32) {
        // SAFETY: Only the private fixed protocol calls this operation under
        // retained CPU/RX ownership; none of these registers initiate DMA.
        unsafe {
            (reg.address() as *mut u32).write_volatile(value);
        }
        dsb();
    }
    fn checked(&self, reg: Reg, value: u32) -> bool {
        self.write(reg, value);
        self.read(reg) == value
    }
    fn ctl(&self) -> u64 {
        let value;
        // SAFETY: Retained admitted EL1 CPU context permits physical timer access.
        unsafe {
            asm!("mrs {}, cntp_ctl_el0", out(reg)value, options(nostack, preserves_flags));
        }
        value
    }
    fn cval(&self) -> u64 {
        let value;
        // SAFETY: The private hardware owner retains admitted EL1 timer access.
        unsafe {
            asm!("mrs {}, cntp_cval_el0", out(reg)value, options(nostack, preserves_flags));
        }
        value
    }
    fn set_ctl(&self, value: u64) {
        // SAFETY: Only fixed values1/2 reach this private protocol operation;
        // it cannot change roots, IRQ masks or user-memory ownership.
        unsafe {
            asm!("msr cntp_ctl_el0, {}", "isb", in(reg)value, options(nostack, preserves_flags));
        }
    }
    fn set_cval(&self, value: u64) {
        // SAFETY: A private authenticated limit or saved admitted comparator is
        // supplied under the retained lease; ISB orders subsequent observations.
        unsafe {
            asm!("msr cntp_cval_el0, {}", "isb", in(reg)value, options(nostack, preserves_flags));
        }
    }
    fn causes(&self) -> Causes {
        Causes {
            ctl: self.ctl(),
            fr: self.read(Reg::Fr),
            ris: self.read(Reg::Ris),
            mis: self.read(Reg::Mis),
            rsr: self.read(Reg::Rsr),
        }
    }
    fn fatal(&self) -> ! {
        self.write(Reg::Disable0, TIMER);
        self.write(Reg::Disable1, UART);
        self.set_ctl(2);
        self.write(Reg::Imsc, 0);
        self.write(Reg::Cctl, 0);
        self.write(Reg::Dctl, 0);
        process::fatal()
    }
}
fn dsb() {
    // SAFETY: Fixed barrier has compiler memory effects and changes no ownership.
    unsafe {
        asm!("dsb sy", options(nostack, preserves_flags));
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Causes {
    pub ctl: u64,
    pub fr: u32,
    pub ris: u32,
    pub mis: u32,
    pub rsr: u32,
}
impl Causes {
    fn ready(self) -> bool {
        self.ctl & 4 != 0
            || self.fr & 16 == 0
            || self.ris & 0x7d0 != 0
            || self.mis & 0x7d0 != 0
            || self.rsr & 15 != 0
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct Poll {
    pub enable: u32,
    pub pending: u32,
    pub active: u32,
    pub counter: u64,
    pub iterations: u32,
}
#[derive(Clone, Copy, Default)]
pub(super) struct Ack {
    pub iar: u32,
    pub source: u64,
    pub before: Poll,
    pub after: Poll,
}

struct Clock {
    last: u64,
    poisoned: bool,
    samples: u64,
}
impl Clock {
    fn accept(&mut self, value: u64) -> u64 {
        self.samples = self.samples.saturating_add(1);
        if value > process::MAX_COUNT || value < self.last {
            self.poisoned = true;
        } else {
            self.last = value;
        }
        value
    }
    fn sample(&mut self) -> u64 {
        let value;
        // SAFETY: EL1 counter access with compiler memory effects orders prior
        // observation stores. Poison is sticky and never reset by a later read.
        unsafe {
            asm!("isb", "mrs {}, cntpct_el0", out(reg)value, options(nostack, preserves_flags));
        }
        self.accept(value)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Snapshot {
    pub fields: [u32; 9],
    pub enable: [u32; 2],
    pub imsc: u32,
    pub dctl: u32,
    pub cctl: u32,
    pub controls: [u64; 26],
}
impl Snapshot {
    fn read(hardware: &Hardware) -> Self {
        Self {
            fields: FIELDS.map(|(reg, _, _)| hardware.read(reg)),
            enable: [hardware.read(Reg::Enable0), hardware.read(Reg::Enable1)],
            imsc: hardware.read(Reg::Imsc),
            dctl: hardware.read(Reg::Dctl),
            cctl: hardware.read(Reg::Cctl),
            controls: process::observe(),
        }
    }
    fn admissible(&self) -> bool {
        self.controls[21] == 2
            && self.imsc == 0
            && self.dctl == 0
            && self.cctl == 0
            && self.enable[0] & 0xffff_0000 == 0
            && self.enable[1] == 0
            && self.fields[7] & !255 == 0
            && self.fields[8] & !7 == 0
            && self.fields[6] == 0
    }
    fn matches(&self, after: &Self) -> bool {
        after.fields == self.fields
            && after.enable == self.enable
            && after.imsc == self.imsc
            && after.dctl == self.dctl
            && after.cctl == self.cctl
            && after.controls == self.controls
    }
}

#[derive(Clone, Copy)]
pub(super) struct Observation {
    pub initial: u64,
    pub precheck: u64,
    pub post: u64,
    pub final_count: u64,
    pub limit: u64,
    pub entered: bool,
    pub pre: Causes,
    pub after: Causes,
    pub acks: [Ack; 2],
    pub ack_count: usize,
    pub spurious: bool,
    pub quiet: [Poll; 2],
    pub saved: Snapshot,
    pub restored: Snapshot,
    pub setup: Setup,
    pub samples: u64,
}
const _: () = assert!(core::mem::size_of::<Observation>() <= 1536);
pub(super) fn leaf_addresses() -> (u64, u64) {
    (
        arm_idle_wfi as *const () as u64,
        core::ptr::addr_of!(arm_idle_resume) as u64,
    )
}

#[derive(Clone, Copy)]
pub(super) struct Setup {
    pub changed: u64,
    pub cval: u64,
    pub ctl: u64,
    pub fields: [u32; 9],
    pub enable: [u32; 2],
    pub imsc: u32,
    pub dctl: u32,
    pub cctl: u32,
}
impl Setup {
    fn entry(saved: Snapshot) -> Self {
        Self {
            changed: 0,
            cval: saved.controls[20],
            ctl: saved.controls[21],
            fields: saved.fields,
            enable: saved.enable,
            imsc: saved.imsc,
            dctl: saved.dctl,
            cctl: saved.cctl,
        }
    }
}

struct Transaction<'a> {
    hardware: &'a Hardware,
    saved: Snapshot,
    dirty: u64,
    clock: Clock,
    setup: Setup,
}
impl Transaction<'_> {
    fn configure(&mut self, limit: u64) -> bool {
        for (index, (reg, mask, requested)) in FIELDS.into_iter().enumerate() {
            if mask == 0 {
                continue;
            }
            self.dirty |= 1 << (index + 26);
            if !self
                .hardware
                .checked(reg, (self.saved.fields[index] & !mask) | requested)
            {
                return false;
            }
            self.setup.fields[index] = self.hardware.read(reg);
        }
        self.dirty |= 1 << 21;
        self.hardware.set_ctl(2);
        if self.hardware.ctl() & 3 != 2 {
            return false;
        }
        self.dirty |= 1 << 20;
        self.hardware.set_cval(limit);
        if self.hardware.cval() != limit {
            return false;
        }
        self.setup.cval = self.hardware.cval();
        self.dirty |= 1 << 35;
        self.hardware.write(Reg::Enable0, TIMER);
        if self.hardware.read(Reg::Enable0) != self.saved.enable[0] | TIMER {
            return false;
        }
        self.setup.enable[0] = self.hardware.read(Reg::Enable0);
        self.dirty |= 1 << 36;
        self.hardware.write(Reg::Enable1, UART);
        if self.hardware.read(Reg::Enable1) != self.saved.enable[1] | UART {
            return false;
        }
        self.setup.enable[1] = self.hardware.read(Reg::Enable1);
        self.dirty |= 1 << 38;
        if !self.hardware.checked(Reg::Dctl, 1) {
            return false;
        }
        self.setup.dctl = self.hardware.read(Reg::Dctl);
        self.dirty |= 1 << 39;
        if !self.hardware.checked(Reg::Cctl, 1) {
            return false;
        }
        self.setup.cctl = self.hardware.read(Reg::Cctl);
        self.dirty |= 1 << 37;
        if !self.hardware.checked(Reg::Imsc, 0x7d0) {
            return false;
        }
        self.setup.imsc = self.hardware.read(Reg::Imsc);
        self.hardware.set_ctl(1);
        self.setup.ctl = self.hardware.ctl() & 3;
        self.setup.changed = self.dirty;
        self.setup.ctl == 1
    }
    fn poll(&mut self, id: u32, active: bool) -> Poll {
        let (enable, pending, state, mask) = if id == 30 {
            (Reg::Enable0, Reg::Pending0, Reg::Active0, TIMER)
        } else {
            (Reg::Enable1, Reg::Pending1, Reg::Active1, UART)
        };
        for iterations in 1..=POLL_LIMIT {
            let sample = Poll {
                enable: self.hardware.read(enable) & mask,
                pending: self.hardware.read(pending) & mask,
                active: self.hardware.read(state) & mask,
                counter: self.clock.sample(),
                iterations,
            };
            // Hardware completion never requires a valid/recovered clock.
            if sample.enable == 0
                && sample.pending == 0
                && sample.active == if active { mask } else { 0 }
            {
                return sample;
            }
        }
        self.hardware.fatal()
    }
    fn service(&mut self, iar: u32) -> Ack {
        let id = iar & 1023;
        let source;
        if id == 30 {
            self.hardware.write(Reg::Disable0, TIMER);
            if self.hardware.read(Reg::Enable0) & TIMER != 0 {
                self.hardware.fatal();
            }
            self.hardware.set_ctl(2);
            source = self.hardware.ctl() & 3;
            if source != 2 {
                self.hardware.fatal();
            }
        } else {
            self.hardware.write(Reg::Disable1, UART);
            if self.hardware.read(Reg::Enable1) & UART != 0 {
                self.hardware.fatal();
            }
            if !self.hardware.checked(Reg::Imsc, 0) {
                self.hardware.fatal();
            }
            source = 0;
        }
        dsb();
        let before = self.poll(id, true);
        dsb();
        self.hardware.write(Reg::Eoir, iar);
        let after = self.poll(id, false);
        Ack {
            iar,
            source,
            before,
            after,
        }
    }
    fn restore(&mut self, seen: u8) -> [Poll; 2] {
        // Every suppression write has a complete initialized snapshot, even
        // when configuration failed before reaching that field's setup stage.
        self.dirty |= (1 << 21) | (0x1f << 35);
        self.hardware.write(Reg::Disable0, TIMER);
        self.hardware.write(Reg::Disable1, UART);
        self.hardware.set_ctl(2);
        if self.hardware.ctl() & 3 != 2
            || !self.hardware.checked(Reg::Imsc, 0)
            || !self.hardware.checked(Reg::Cctl, 0)
            || !self.hardware.checked(Reg::Dctl, 0)
        {
            self.hardware.fatal();
        }
        if self.hardware.read(Reg::Active0) & TIMER != 0
            || self.hardware.read(Reg::Active1) & UART != 0
        {
            self.hardware.fatal();
        }
        // Only unacknowledged stale software latches can remain here. GICv2
        // pending reads still include an asserted level after this owned clear.
        if seen & 1 == 0 {
            self.hardware.write(Reg::Clear0, TIMER);
        }
        if seen & 2 == 0 {
            self.hardware.write(Reg::Clear1, UART);
        }
        let quiet = [self.poll(30, false), self.poll(33, false)];
        for (index, (reg, _, _)) in FIELDS.into_iter().enumerate() {
            if self.dirty & (1 << (index + 26)) != 0
                && !self.hardware.checked(reg, self.saved.fields[index])
            {
                self.hardware.fatal();
            }
        }
        if self.dirty & (1 << 20) != 0 {
            self.hardware.set_cval(self.saved.controls[20]);
            if self.hardware.cval() != self.saved.controls[20] {
                self.hardware.fatal();
            }
        }
        self.hardware.set_ctl(self.saved.controls[21]);
        dsb();
        quiet
    }
}

/// Private fixed transaction. The owner supplies an already authenticated limit
/// bound to its immutable wall_end; no callback or mutable MMIO escapes.
fn wait(hardware: &Hardware, clock: &mut Clock, limit: u64) -> Result<Observation, Error> {
    process::admission().map_err(|_| Error::Context)?;
    let saved = Snapshot::read(hardware);
    if !saved.admissible() || limit > process::MAX_COUNT {
        return Err(Error::Context);
    }
    let initial = clock.sample();
    if clock.poisoned {
        return Err(Error::Clock);
    }
    let mut tx = Transaction {
        hardware,
        saved,
        dirty: 0,
        clock: Clock {
            last: clock.last,
            poisoned: false,
            samples: 0,
        },
        setup: Setup::entry(saved),
    };
    let mut result = Observation {
        initial,
        precheck: initial,
        post: 0,
        final_count: initial,
        limit,
        entered: false,
        pre: Causes::default(),
        after: Causes::default(),
        acks: [Ack::default(); 2],
        ack_count: 0,
        spurious: false,
        quiet: [Poll::default(); 2],
        saved,
        restored: saved,
        setup: Setup::entry(saved),
        samples: 1,
    };
    if initial >= limit {
        return Ok(result);
    }
    if !tx.configure(limit) {
        tx.restore(0);
        if !saved.matches(&Snapshot::read(hardware)) {
            hardware.fatal();
        }
        tx.clock.sample();
        clock.last = tx.clock.last;
        clock.poisoned |= tx.clock.poisoned;
        clock.samples = clock.samples.saturating_add(tx.clock.samples);
        return Err(if clock.poisoned {
            Error::Clock
        } else {
            Error::Configuration
        });
    }
    result.precheck = tx.clock.sample();
    result.pre = hardware.causes();
    if !tx.clock.poisoned && result.precheck < limit && !result.pre.ready() {
        result.entered = true;
        dsb();
        // SAFETY: Retained CPU0 execution/RX owners, boot root and masked DAIF
        // cover this fixed no-callback leaf. Default extern memory effects bind
        // observation storage; the next instruction is the unique resume path.
        result.post = tx.clock.accept(unsafe { arm_idle_wfi() });
    } else {
        result.post = 0;
    }
    if result.entered {
        result.after = hardware.causes();
    }
    let mut seen = 0u8;
    for index in 0..2 {
        let iar = hardware.read(Reg::Iar);
        let id = iar & 1023;
        if id == 1023 {
            result.spurious = true;
            break;
        }
        let bit = match id {
            30 => 1,
            33 => 2,
            _ => hardware.fatal(),
        };
        if seen & bit != 0 {
            hardware.fatal();
        }
        seen |= bit;
        result.acks[index] = tx.service(iar);
        result.ack_count += 1;
    }
    result.quiet = tx.restore(seen);
    result.restored = Snapshot::read(hardware);
    if !saved.matches(&result.restored) {
        hardware.fatal();
    }
    result.setup = tx.setup;
    result.final_count = tx.clock.sample();
    clock.last = tx.clock.last;
    clock.poisoned |= tx.clock.poisoned;
    clock.samples = clock.samples.saturating_add(tx.clock.samples);
    result.samples = tx.clock.samples.saturating_add(1);
    if clock.poisoned {
        Err(Error::Clock)
    } else {
        Ok(result)
    }
}
