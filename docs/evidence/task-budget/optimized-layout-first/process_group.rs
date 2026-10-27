//! Inactive owned process groups and bounded CPU0 turns; design 0028.
use super::frames::{KernelBox, Pool};
use super::process::{self, Executor, GroupCapture, Lease, Process, Reason, Uart};
use super::user_space::{self, Factory, Space};
use core::fmt::Write;
use hm_kernel::elf::Image;
use hm_kernel::process_budget::{Budget, Finish, Snapshot};
use hm_kernel::startup_input::Input;

pub const CAPACITY: usize = 4;
pub const MAX_FRAMES: usize = 512;
pub const PROCESS_BYTES: usize = core::mem::size_of::<Process<'static>>();
pub const BOX_FRAMES: usize = PROCESS_BYTES.div_ceil(4096);
const _: () = assert!(PROCESS_BYTES <= 4 * 4096);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskId {
    group: u64,
    slot: u8,
    generation: u32,
}
impl TaskId {
    pub fn group(self) -> u64 {
        self.group
    }
    pub fn slot(self) -> usize {
        self.slot as usize
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    None,
    Svc,
    Timer,
    Spurious,
    Fault,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum End {
    Exited(u8),
    Fault,
    Budget,
    CallLimit,
    TurnLimit,
    Cancelled,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Ready,
    CancelPending,
    Waiting,
    Closing,
    Completed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Mode,
    Catalog(hm_kernel::boot_image::AdmissionError),
    Unsupported,
    Bounds,
    Full,
    Resources,
    Stale,
    Pending,
    Busy,
    Unavailable,
    Load(user_space::Error),
    Prepare(process::Error),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskInfo {
    id: TaskId,
    state: TaskState,
    root: u64,
    process_frames: usize,
    box_frames: usize,
    console_generation: Option<u32>,
    budget: Snapshot,
}
macro_rules! getters { ($($name:ident:$ty:ty),* $(,)?) => {$ (pub fn $name(&self)->$ty {self.$name})*}; }
#[path = "/workspace/crates/hm-core/src/process_task.rs"]
mod task;
// Named copied report types are the public API even when this boot profile
// consumes their getters through inferred return types rather than imports.
#[cfg(feature = "task-negative-tests")]
pub(crate) use task::{HookStatus, SeedKind};
#[allow(unused_imports)]
pub use task::{Management, RootClosure, RootEvidence, TaskCompletion, TaskTurn};

impl TaskInfo {
    getters!(id:TaskId,state:TaskState,root:u64,process_frames:usize,box_frames:usize,console_generation:Option<u32>,budget:Snapshot);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    start: u64,
    end: u64,
    comparator: u64,
    hard_end: u64,
    accounted: bool,
}
impl Timing {
    getters!(start:u64,end:u64,comparator:u64,hard_end:u64,accounted:bool);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepReport {
    task: TaskInfo,
    event: Event,
    ordinal: u64,
    entered: bool,
    entry_pc: u64,
    elr: u64,
    esr: u64,
    far: u64,
    spsr: u64,
    calls: usize,
    written: usize,
    irqs: u64,
    spurious: u64,
    boot_root: u64,
    vbar: u64,
    controls: bool,
    charged_delta: u64,
    timing: Option<Timing>,
    live_frames: usize,
}
impl StepReport {
    getters!(task:TaskInfo,event:Event,ordinal:u64,entered:bool,entry_pc:u64,elr:u64,esr:u64,far:u64,spsr:u64,calls:usize,written:usize,irqs:u64,spurious:u64,boot_root:u64,vbar:u64,controls:bool,charged_delta:u64,timing:Option<Timing>,live_frames:usize);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Completion {
    end: End,
    report: StepReport,
}
impl Completion {
    getters!(end:End,report:StepReport);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// Fixed copied reports remain valid after owner teardown without allocating.
// Their actual sizes and retained caller frames are part of the stack gate.
#[allow(clippy::large_enum_variant)]
pub enum Turn {
    Task(TaskTurn),
    Idle,
    Progress(StepReport),
    Completed(Completion),
}
#[cfg(feature = "runtime-tests")]
#[derive(Clone, Copy)]
pub(super) enum ContextProbe {
    A,
    B,
}
struct Slot<'pool> {
    owner: Option<KernelBox<'pool, Process<'pool>>>,
    budget: Option<Budget>,
    info: Option<TaskInfo>,
    completion: Option<Completion>,
    next: u32,
    ordinal: u64,
    irqs: u64,
    spurious: u64,
    cancel: bool,
    cached: bool,
    last_event: Event,
    #[cfg(feature = "runtime-tests")]
    probe: Option<ContextProbe>,
}
impl Slot<'_> {
    const fn new() -> Self {
        Self {
            owner: None,
            budget: None,
            info: None,
            completion: None,
            next: 1,
            ordinal: 0,
            irqs: 0,
            spurious: 0,
            cancel: false,
            cached: false,
            last_event: Event::None,
            #[cfg(feature = "runtime-tests")]
            probe: None,
        }
    }
    fn info(&self) -> TaskInfo {
        let mut info = self.info.unwrap_or_else(|| process::fatal());
        info.state = if self.completion.is_some() {
            TaskState::Completed
        } else if self.cancel {
            TaskState::CancelPending
        } else {
            TaskState::Ready
        };
        info.budget = self
            .budget
            .as_ref()
            .unwrap_or_else(|| process::fatal())
            .snapshot();
        info
    }
}
pub struct Group<'executor, 'pool> {
    executor: &'executor mut Executor,
    factory: Factory<'pool>,
    slots: [Slot<'pool>; CAPACITY],
    namespace: u64,
    limit: usize,
    charged: usize,
    cursor: usize,
    task: Option<KernelBox<'pool, task::Authority<'pool>>>,
    task_closed: bool,
}
impl<'executor, 'pool> Group<'executor, 'pool> {
    pub fn new(
        executor: &'executor mut Executor,
        factory: Factory<'pool>,
        frame_limit: usize,
    ) -> Result<Self, Error> {
        if !(1..=MAX_FRAMES).contains(&frame_limit) {
            return Err(Error::Bounds);
        }
        if executor.cpus() != 1 || factory.cpus() != 1 {
            return Err(Error::Unsupported);
        }
        if executor.retired() {
            return Err(Error::Unavailable);
        }
        let _lease = match Lease::acquire() {
            Ok(lease) => lease,
            Err(error) => {
                if error != process::Error::Busy {
                    executor.retire();
                }
                return Err(lease_error(error));
            }
        };
        if process::admission().is_err() {
            executor.retire();
            return Err(Error::Unavailable);
        }
        Self::empty_under_lease(executor, factory, frame_limit, &_lease).map_err(|e| {
            if e == Error::Resources {
                Error::Unavailable
            } else {
                e
            }
        })
    }
    fn empty_under_lease(
        executor: &'executor mut Executor,
        factory: Factory<'pool>,
        frame_limit: usize,
        _lease: &Lease,
    ) -> Result<Self, Error> {
        let namespace = executor.issue_group().map_err(|_| Error::Resources)?;
        Ok(Self {
            executor,
            factory,
            slots: [const { Slot::new() }; CAPACITY],
            namespace,
            limit: frame_limit,
            charged: 0,
            cursor: 0,
            task: None,
            task_closed: false,
        })
    }
    pub fn charged_frames(&self) -> usize {
        self.charged
    }
    pub fn last_prepared_generation(&self) -> Option<u32> {
        self.executor.last_prepared_generation()
    }
    pub fn namespace(&self) -> u64 {
        self.namespace
    }
    #[inline(never)]
    pub fn admit(
        &mut self,
        image: &Image<'_>,
        console: bool,
        input: Option<&Input>,
    ) -> Result<TaskId, Error> {
        if self.task.is_some() || self.task_closed {
            return Err(Error::Mode);
        }
        if self.executor.retired() {
            return Err(Error::Unavailable);
        }
        let index = self
            .slots
            .iter()
            .position(|s| s.info.is_none() && s.next != 0)
            .ok_or(Error::Full)?;
        let _lease = match Lease::acquire() {
            Ok(lease) => lease,
            Err(error) => {
                if error != process::Error::Busy {
                    self.executor.retire();
                }
                return Err(lease_error(error));
            }
        };
        if process::admission().is_err() {
            self.executor.retire();
            return Err(Error::Unavailable);
        }
        let frames = image.page_count().checked_add(29).ok_or(Error::Bounds)?;
        let charge = frames.checked_add(BOX_FRAMES).ok_or(Error::Bounds)?;
        let total = self
            .charged
            .checked_add(charge)
            .filter(|n| *n <= self.limit)
            .ok_or(Error::Resources)?;
        let budget = Budget::new(self.executor.ticks(), self.executor.frequency() / 1000)
            .map_err(|_| Error::Unavailable)?;
        let owner = load_boxed(&self.factory, self.executor, image, console, input)?;
        if owner.frame_count() != frames {
            self.executor.retire();
            drop(owner);
            return Err(Error::Unavailable);
        }
        let slot = &mut self.slots[index];
        let id = TaskId {
            group: self.namespace,
            slot: index as u8,
            generation: slot.next,
        };
        let info = TaskInfo {
            id,
            state: TaskState::Ready,
            root: owner.root_address(),
            process_frames: frames,
            box_frames: BOX_FRAMES,
            console_generation: (owner.handle() != 0).then_some((owner.handle() >> 32) as u32),
            budget: budget.snapshot(),
        };
        slot.owner = Some(owner);
        slot.budget = Some(budget);
        slot.info = Some(info);
        slot.completion = None;
        slot.ordinal = 0;
        slot.irqs = 0;
        slot.spurious = 0;
        slot.cancel = false;
        slot.cached = false;
        slot.last_event = Event::None;
        #[cfg(feature = "runtime-tests")]
        {
            slot.probe = None;
        }
        self.charged = total;
        Ok(id)
    }
    fn index(&self, id: TaskId) -> Result<usize, Error> {
        let index = id.slot();
        if id.group != self.namespace
            || index >= CAPACITY
            || self.slots[index].info.is_none_or(|info| info.id != id)
        {
            return Err(Error::Stale);
        }
        Ok(index)
    }
    pub fn info(&self, id: TaskId) -> Result<TaskInfo, Error> {
        if self.task.is_some() || self.task_closed {
            return self.task_info(id);
        }
        Ok(self.slots[self.index(id)?].info())
    }
    pub fn request_cancel(&mut self, id: TaskId) -> Result<(), Error> {
        if self.task.is_some() || self.task_closed {
            return self.task_cancel(id);
        }
        let index = self.index(id)?;
        if self.slots[index].completion.is_none() {
            self.slots[index].cancel = true;
        }
        Ok(())
    }
    pub fn completion(&self, id: TaskId) -> Result<Option<Completion>, Error> {
        if self.task.is_some() || self.task_closed {
            return Err(Error::Mode);
        }
        Ok(self.slots[self.index(id)?].completion)
    }
    pub fn reap(&mut self, id: TaskId) -> Result<Completion, Error> {
        if self.task.is_some() || self.task_closed {
            return Err(Error::Mode);
        }
        self.reap_internal(id)
    }
    fn reap_internal(&mut self, id: TaskId) -> Result<Completion, Error> {
        let index = self.index(id)?;
        let slot = &mut self.slots[index];
        let completion = slot.completion.ok_or(Error::Pending)?;
        slot.next = id.generation.checked_add(1).unwrap_or(0);
        slot.info = None;
        slot.completion = None;
        slot.budget = None;
        slot.cancel = false;
        Ok(completion)
    }
    #[inline(never)]
    pub fn turn(&mut self) -> Result<Turn, Error> {
        if self.task.is_some() {
            return self.task_turn();
        }
        if self.task_closed {
            return Ok(Turn::Idle);
        }
        let lease = match Lease::acquire() {
            Ok(lease) => lease,
            Err(error) => {
                if error != process::Error::Busy {
                    self.executor.retire();
                }
                return Err(lease_error(error));
            }
        };
        let retired = self.executor.retired();
        let selected = (0..CAPACITY)
            .map(|n| (self.cursor + n) % CAPACITY)
            .find(|index| {
                let slot = &self.slots[*index];
                slot.owner.is_some() && (slot.cancel || !retired)
            });
        let Some(index) = selected else {
            return if retired && self.slots.iter().any(|s| s.owner.is_some()) {
                Err(Error::Unavailable)
            } else {
                Ok(Turn::Idle)
            };
        };
        if !self.slots[index].cancel && process::admission().is_err() {
            self.executor.retire();
            return Err(Error::Unavailable);
        }
        self.cursor = (index + 1) % CAPACITY;
        let before = self.factory.pool().stats().free;
        let tracing = self.executor.tracing();
        let slot = &mut self.slots[index];
        slot.ordinal = slot
            .ordinal
            .checked_add(1)
            .unwrap_or_else(|| process::fatal());
        let info = slot.info();
        let prior = info.budget;
        let owner = slot.owner.as_ref().unwrap_or_else(|| process::fatal());
        if tracing {
            let _ = writeln!(
                Uart,
                "HMRT STEP group={} turn={} slot={} generation={} state={} calls={} written={} irqs={} remaining={} charged={} turns={} before={}",
                self.namespace,
                slot.ordinal,
                index,
                info.id.generation,
                if slot.cancel {
                    "cancel_pending"
                } else {
                    "live"
                },
                owner.calls(),
                owner.written(),
                slot.irqs,
                prior.remaining,
                prior.charged,
                prior.turns,
                before
            );
        }
        let saved = process::observe();
        let mut report = StepReport {
            task: info,
            event: Event::None,
            ordinal: slot.ordinal,
            entered: false,
            entry_pc: owner.pc(),
            elr: owner.pc(),
            esr: 0,
            far: 0,
            spsr: 0,
            calls: owner.calls(),
            written: owner.written(),
            irqs: slot.irqs,
            spurious: slot.spurious,
            boot_root: saved[0],
            vbar: saved[1],
            controls: true,
            charged_delta: 0,
            timing: None,
            live_frames: self.charged,
        };
        if slot.ordinal > 1 {
            let (esr, far, elr, spsr) = owner.trap();
            report.esr = esr;
            report.far = far;
            report.elr = elr;
            report.spsr = spsr;
        }
        let mut end = if slot.cancel {
            Some(End::Cancelled)
        } else if prior.turns >= 256 {
            Some(End::TurnLimit)
        } else {
            None
        };
        if end.is_none() {
            end = run_turn(self.executor, slot, &lease, &mut report, tracing);
        }
        report.task = slot.info();
        report.calls = slot
            .owner
            .as_ref()
            .unwrap_or_else(|| process::fatal())
            .calls();
        report.written = slot
            .owner
            .as_ref()
            .unwrap_or_else(|| process::fatal())
            .written();
        report.irqs = slot.irqs;
        report.spurious = slot.spurious;
        report.charged_delta = report
            .task
            .budget
            .charged
            .checked_sub(prior.charged)
            .unwrap_or_else(|| process::fatal());
        slot.last_event = report.event;
        let outcome = if let Some(end) = end {
            let _ = slot
                .budget
                .as_mut()
                .unwrap_or_else(|| process::fatal())
                .stop();
            drop(slot.owner.take());
            self.charged = self
                .charged
                .checked_sub(info.process_frames + info.box_frames)
                .unwrap_or_else(|| process::fatal());
            report.task.state = TaskState::Completed;
            report.live_frames = self.charged;
            let completion = Completion { end, report };
            slot.completion = Some(completion);
            Turn::Completed(completion)
        } else {
            Turn::Progress(report)
        };
        if tracing {
            trace_return(
                self.namespace,
                &outcome,
                before,
                self.factory.pool().stats().free,
            );
        }
        drop(lease);
        Ok(outcome)
    }
    #[cfg(feature = "runtime-tests")]
    #[inline(never)]
    pub(super) fn admit_context_probe(&mut self, probe: ContextProbe) -> Result<TaskId, Error> {
        let path = match probe {
            ContextProbe::A => "bin/context_irq_a",
            ContextProbe::B => "bin/context_irq_b",
        };
        let image = hm_kernel::boot_image::admit(super::runtime_inputs::ARCHIVE, path)
            .map_err(|_| Error::Bounds)?;
        let id = self.admit(&image, true, None)?;
        self.slots[id.slot()].probe = Some(probe);
        Ok(id)
    }
    #[cfg(feature = "runtime-tests")]
    pub(super) fn test_release_word(&self, id: TaskId) -> Result<u64, Error> {
        let index = self.index(id)?;
        let _lease = Lease::acquire().map_err(lease_error)?;
        self.slots[index]
            .owner
            .as_ref()
            .ok_or(Error::Bounds)?
            .runtime_release_word()
            .map_err(|_| Error::Bounds)
    }
    #[cfg(feature = "runtime-tests")]
    pub(super) fn release_context_probe(&mut self, id: TaskId) -> Result<(), Error> {
        let index = self.index(id)?;
        let _lease = Lease::acquire().map_err(lease_error)?;
        let slot = &mut self.slots[index];
        if slot.probe.is_none() || slot.cancel || slot.irqs < 2 || slot.last_event != Event::Timer {
            return Err(Error::Bounds);
        }
        let owner = slot.owner.as_mut().ok_or(Error::Bounds)?;
        if !(0x1_0000_0100..0x1_0000_0108).contains(&owner.pc()) {
            return Err(Error::Bounds);
        }
        owner.release_runtime_probe().map_err(|_| Error::Bounds)
    }
}
fn lease_error(error: process::Error) -> Error {
    if error == process::Error::Busy {
        Error::Busy
    } else {
        Error::Unavailable
    }
}
#[inline(never)]
fn load_boxed<'pool>(
    factory: &Factory<'pool>,
    executor: &mut Executor,
    image: &Image<'_>,
    console: bool,
    input: Option<&Input>,
) -> Result<KernelBox<'pool, Process<'pool>>, Error> {
    let loaded = factory.load(image, image.page_count() + 28);
    let space = match loaded {
        Ok(space) => space,
        Err(error) => {
            if !matches!(
                error,
                user_space::Error::Budget | user_space::Error::Exhausted
            ) {
                executor.retire();
            }
            return Err(
                if matches!(
                    error,
                    user_space::Error::Budget | user_space::Error::Exhausted
                ) {
                    Error::Resources
                } else {
                    Error::Load(error)
                },
            );
        }
    };
    box_loaded(factory.pool(), executor, space, console, input)
}
#[inline(never)]
fn box_loaded<'pool>(
    pool: &'pool Pool,
    executor: &mut Executor,
    space: Space<'pool>,
    console: bool,
    input: Option<&Input>,
) -> Result<KernelBox<'pool, Process<'pool>>, Error> {
    let limit = space.frame_count() + 1;
    let prepared = match input {
        Some(input) => executor.prepare_with_input(space, console, limit, input),
        None => executor.prepare(space, console, limit),
    };
    let process = match prepared {
        Ok(process) => process,
        Err(ref failure) => {
            let error = failure.0;
            if !matches!(error, process::Error::Budget | process::Error::Exhausted) {
                executor.retire();
            }
            return Err(
                if matches!(error, process::Error::Budget | process::Error::Exhausted) {
                    Error::Resources
                } else {
                    Error::Prepare(error)
                },
            );
        }
    };
    box_process(pool, process)
}
#[inline(never)]
fn box_process<'pool>(
    pool: &'pool Pool,
    process: Process<'pool>,
) -> Result<KernelBox<'pool, Process<'pool>>, Error> {
    let boxed = pool.box_value(process);
    match boxed {
        Ok(owner) => Ok(owner),
        Err(ref _failure) => Err(Error::Resources),
    }
}
#[inline(never)]
fn run_turn(
    executor: &mut Executor,
    slot: &mut Slot<'_>,
    lease: &Lease,
    report: &mut StepReport,
    tracing: bool,
) -> Option<End> {
    let budget = slot.budget.as_mut().unwrap_or_else(|| process::fatal());
    let remaining = budget.snapshot().remaining;
    let owner = slot.owner.as_mut().unwrap_or_else(|| process::fatal());
    let mut capture = GroupCapture::new();
    let start = process::group_counter(owner, &capture);
    if start > process::MAX_COUNT || remaining > process::MAX_COUNT - start {
        executor.retire();
        return Some(End::Unavailable);
    }
    let comparator = match budget.begin(start) {
        Ok(value) => value,
        Err(_) => {
            executor.retire();
            return Some(End::Unavailable);
        }
    };
    let hard_end = start + remaining;
    let group = report.task.id.group;
    let turn = report.ordinal;
    let mut end = None;
    let mut last = start;
    let mut clock_failed = false;
    if process::admission().is_err() {
        end = Some(End::Unavailable);
    }
    if end.is_none() && !slot.cached {
        if owner.prepare_instructions().is_err() {
            end = Some(End::Unavailable);
        } else {
            slot.cached = true;
        }
    }
    if end.is_none() && !owner.executable_pc() {
        end = Some(End::Fault);
    }
    if end.is_none() {
        let saved = process::observe();
        report.boot_root = saved[0];
        report.vbar = saved[1];
        let now = process::group_counter(owner, &capture);
        if now < last || now > process::MAX_COUNT {
            clock_failed = true;
            end = Some(End::Unavailable);
        } else if now >= hard_end {
            end = Some(End::Budget);
        } else {
            last = now;
            let kind = executor.group_slice(owner, comparator, &saved, lease);
            report.entered = true;
            let (esr, far, elr, spsr) = owner.trap();
            report.esr = esr;
            report.far = far;
            report.elr = elr;
            report.spsr = spsr;
            report.event = match kind {
                1 => Event::Svc,
                2 => Event::Fault,
                3 => Event::Timer,
                4 => Event::Spurious,
                _ => process::fatal(),
            };
            match kind {
                2 => end = Some(End::Fault),
                3 => slot.irqs = slot.irqs.checked_add(1).unwrap_or_else(|| process::fatal()),
                4 => {
                    slot.spurious += 1;
                    if slot.spurious > 8 {
                        end = Some(End::Unavailable);
                    }
                }
                1 => {
                    let now = process::group_counter(owner, &capture);
                    if now < last || now > process::MAX_COUNT {
                        clock_failed = true;
                        end = Some(End::Unavailable);
                    } else if now >= hard_end {
                        end = Some(End::Budget);
                    } else {
                        last = now;
                        if let Some((reason, code)) = executor.dispatch_group(owner, &mut capture) {
                            end = Some(match reason {
                                Reason::Exit => End::Exited(code),
                                Reason::CallLimit => End::CallLimit,
                                _ => process::fatal(),
                            });
                        }
                    }
                }
                _ => process::fatal(),
            }
        }
    }
    let finish = process::group_counter(owner, &capture);
    let result = if clock_failed || finish < last || finish > process::MAX_COUNT {
        // An invalid hardware observation cannot commit a trusted charge.
        // Running rejects stop and poisons transactionally; retain the actual
        // observed end in Timing without inventing a replacement timestamp.
        let _ = budget.stop();
        Err(hm_kernel::process_budget::Error::Clock)
    } else {
        budget.finish(finish)
    };
    let accounted = result.is_ok();
    report.timing = Some(Timing {
        start,
        end: finish,
        comparator,
        hard_end,
        accounted,
    });
    if result.is_err() {
        end = Some(End::Unavailable);
    } else if result == Ok(Finish::Exhausted) && end.is_none() {
        end = Some(End::Budget);
    }
    if end == Some(End::Unavailable) {
        executor.retire();
    }
    if tracing {
        // Serialization is retrospective and quiescent. All captured bytes,
        // policy updates and actual context transitions precede the final read.
        // The caller retains BUSY until this flush and terminal reclamation end.
        let _ = writeln!(
            Uart,
            "HMRT CLOCK group={group} turn={turn} begin={start} remaining={remaining} comparator={comparator} whole_end={hard_end}"
        );
        if report.entered {
            let _ = writeln!(
                Uart,
                "HMRT ENTER group={group} turn={turn} pc={} root={}",
                report.entry_pc, report.task.root
            );
            let _ = writeln!(
                Uart,
                "HMRT TRAP group={group} turn={turn} kind={} esr={} far={} elr={} spsr={}",
                match report.event {
                    Event::Svc => "svc",
                    Event::Timer => "irq",
                    Event::Fault => "fault",
                    Event::Spurious => "spurious",
                    Event::None => process::fatal(),
                },
                report.esr,
                report.far,
                report.elr,
                report.spsr
            );
        }
        capture.emit(group, turn);
        let state = budget.snapshot();
        let _ = writeln!(
            Uart,
            "HMRT CHARGE group={group} turn={turn} end={finish} accounted={} remaining={} charged={} turns={}",
            u8::from(accounted),
            state.remaining,
            state.charged,
            state.turns
        );
    }
    end
}

fn trace_return(group: u64, turn: &Turn, before: usize, after: usize) {
    let (report, end) = match turn {
        Turn::Progress(report) => (report, None),
        Turn::Completed(done) => (&done.report, Some(done.end)),
        Turn::Idle => return,
        Turn::Task(_) => process::fatal(),
    };
    let (terminal, code) = match end {
        Some(End::Exited(code)) => ("exit", code),
        Some(End::Fault) => ("fault", 0),
        Some(End::Budget) => ("budget", 0),
        Some(End::CallLimit) => ("call_limit", 0),
        Some(End::TurnLimit) => ("turn_limit", 0),
        Some(End::Cancelled) => ("cancelled", 0),
        Some(End::Unavailable) => ("unavailable", 0),
        None => ("none", 0),
    };
    let event = match report.event {
        Event::None => "none",
        Event::Svc => "svc",
        Event::Timer => "timer",
        Event::Spurious => "spurious",
        Event::Fault => "fault",
    };
    let state = report.task.budget;
    let _ = writeln!(
        Uart,
        "HMRT RETURN group={group} turn={} event={event} terminal={terminal} entered={} code={code} calls={} written={} irqs={} elr={} remaining={} charged={} turns={} root={} vbar={} controls=1 before={before} after={after}",
        report.ordinal,
        u8::from(report.entered),
        report.calls,
        report.written,
        report.irqs,
        report.elr,
        state.remaining,
        state.charged,
        state.turns,
        report.boot_root,
        report.vbar
    );
}

pub const TASK_LAYOUT: [usize; 6] = [
    core::mem::size_of::<Image<'static>>(),
    core::mem::size_of::<Image<'static>>().div_ceil(4096),
    core::mem::size_of::<task::Authority<'static>>(),
    core::mem::size_of::<task::Authority<'static>>().div_ceil(4096),
    core::mem::size_of::<Group<'static, 'static>>(),
    task::METADATA_FRAMES,
];
