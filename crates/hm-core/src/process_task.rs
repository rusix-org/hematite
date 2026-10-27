//! Lease-bound task authority and owned syscall settlement; design 0029.
use super::*;
use hm_kernel::native::{
    self, Admission, Outcome, ParkedSession, Response, Session, Status, TaskAdmission,
    TaskOperation,
};
use hm_kernel::task_wire::{self, Rights, WaitMode};
#[cfg(feature = "task-negative-tests")]
#[path = "process_task_negative.rs"]
mod negative;
#[path = "process_task_turn.rs"]
mod turn;
#[cfg(feature = "task-negative-tests")]
pub(crate) use negative::{HookStatus, SeedKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
// Fixed copied reports remain valid after owner teardown without allocating.
// Their actual sizes and retained caller frames are part of the stack gate.
#[allow(clippy::large_enum_variant)]
pub enum RootEvidence {
    Executed(StepReport),
    Administrative { trigger: Option<TaskId> },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootClosure {
    root: TaskInfo,
    end: End,
    evidence: RootEvidence,
}
impl RootClosure {
    getters!(root:TaskInfo,end:End,evidence:RootEvidence);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskCompletion {
    closure: RootClosure,
    live_frames: usize,
}
impl TaskCompletion {
    getters!(closure:RootClosure,live_frames:usize);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Management {
    root: TaskId,
    woken: Option<TaskId>,
    reaped: u8,
    children_remaining: u8,
    closing: bool,
    live_frames: usize,
    closed_root: Option<RootClosure>,
}
impl Management {
    getters!(root:TaskId,woken:Option<TaskId>,reaped:u8,children_remaining:u8,closing:bool,live_frames:usize,closed_root:Option<RootClosure>);
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskTurn {
    Step {
        report: StepReport,
        terminal: Option<End>,
        management: Management,
    },
    RootCompleted {
        completion: TaskCompletion,
        management: Management,
    },
    Administrative {
        management: Management,
    },
}
#[derive(Clone, Copy)]
struct Cap {
    generation: u32,
    child: TaskId,
    rights: Rights,
}
#[derive(Clone, Copy)]
struct Pending {
    child: TaskId,
    slot: usize,
    generation: u32,
}
struct Waiting {
    session: ParkedSession,
    pending: Pending,
}
pub(super) struct Authority<'p> {
    root: TaskId,
    launch: Option<u32>,
    console_allowed: bool,
    caps: [Option<Cap>; 2],
    catalog: [KernelBox<'p, Image<'static>>; 2],
    closing: Option<RootClosure>,
    published: Option<TaskCompletion>,
    waiting: Option<Waiting>,
    dispatching: Option<TaskId>,
    sequence: u64,
    #[cfg(feature = "task-negative-tests")]
    negative: Option<negative::HookState>,
}
const IMAGE_FRAMES: usize = core::mem::size_of::<Image<'static>>().div_ceil(4096);
const AUTHORITY_FRAMES: usize = core::mem::size_of::<Authority<'static>>().div_ceil(4096);
pub const METADATA_FRAMES: usize = 2 * IMAGE_FRAMES + AUTHORITY_FRAMES;
const _: () = assert!(core::mem::align_of::<Authority<'static>>() <= 4096);
const _: () = assert!(core::mem::align_of::<Image<'static>>() <= 4096);

#[inline(never)]
fn catalog_image<'p>(
    pool: &'p Pool,
    archive: &'static [u8],
    path: &str,
) -> Result<KernelBox<'p, Image<'static>>, Error> {
    let admitted = hm_kernel::boot_image::admit(archive, path).map_err(Error::Catalog)?;
    pool.box_value(admitted).map_err(|_| Error::Resources)
}
#[inline(never)]
fn authority_box<'p>(
    pool: &'p Pool,
    authority: Authority<'p>,
) -> Result<KernelBox<'p, Authority<'p>>, Error> {
    pool.box_value(authority).map_err(|_| Error::Resources)
}
fn budget(executor: &Executor) -> Result<Budget, Error> {
    Budget::new(executor.ticks(), executor.frequency() / 1000).map_err(|_| Error::Unavailable)
}
fn charge(image: &Image<'_>) -> Result<usize, Error> {
    image
        .page_count()
        .checked_add(29)
        .and_then(|n| n.checked_add(BOX_FRAMES))
        .ok_or(Error::Bounds)
}
fn populate<'p>(
    slot: &mut Slot<'p>,
    owner: KernelBox<'p, Process<'p>>,
    id: TaskId,
    budget: Budget,
) {
    let info = TaskInfo {
        id,
        state: TaskState::Ready,
        root: owner.root_address(),
        process_frames: owner.frame_count(),
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
}
impl<'e, 'p> Group<'e, 'p> {
    // Preserve the original single constructor frame in ordinary profiles.
    #[inline(always)]
    pub(crate) fn new_task(
        executor: &'e mut Executor,
        factory: Factory<'p>,
        root_image: &Image<'_>,
        archive: &'static [u8],
        console_allowed: bool,
    ) -> Result<Self, Error> {
        Self::new_task_inner(
            executor,
            factory,
            root_image,
            archive,
            console_allowed,
            #[cfg(feature = "task-negative-tests")]
            None,
        )
    }
    #[inline(never)]
    fn new_task_inner(
        executor: &'e mut Executor,
        factory: Factory<'p>,
        root_image: &Image<'_>,
        archive: &'static [u8],
        console_allowed: bool,
        #[cfg(feature = "task-negative-tests")] negative: Option<process::NegativeCase>,
    ) -> Result<Self, Error> {
        if executor.retired() {
            return Err(Error::Unavailable);
        }
        if executor.cpus() != 1 || factory.cpus() != 1 {
            return Err(Error::Unsupported);
        }
        let lease = match Lease::acquire() {
            Ok(v) => v,
            Err(e) => {
                if e != process::Error::Busy {
                    executor.retire();
                }
                return Err(lease_error(e));
            }
        };
        if process::admission().is_err() {
            executor.retire();
            return Err(Error::Unavailable);
        }
        if !executor.group_namespace_available() {
            return Err(Error::Resources);
        }
        let mut group = Self::empty_under_lease(executor, factory, MAX_FRAMES, &lease)?;
        #[cfg(feature = "task-negative-tests")]
        if negative == Some(process::NegativeCase::SlotExhaustion) {
            for slot in &mut group.slots[1..] {
                slot.next = u32::MAX;
            }
            let _ = writeln!(Uart, "HMTASK SEED kind=child_slots_max value=4294967295");
        }
        let before = group.factory.pool().stats().free;
        let first = catalog_image(group.factory.pool(), archive, "bin/worker")?;
        let second = catalog_image(group.factory.pool(), archive, "demo/loop")?;
        let root_charge = charge(root_image)?;
        let total = METADATA_FRAMES
            .checked_add(root_charge)
            .filter(|n| *n <= MAX_FRAMES)
            .ok_or(Error::Resources)?;
        let generation = group.executor.issue_task_cap().ok_or(Error::Resources)?;
        let token = (u64::from(generation) << 32) | 2;
        let mut bytes = [0; task_wire::GRANT_SIZE];
        task_wire::Grant::new(token, console_allowed)
            .and_then(|g| g.encode_into(&mut bytes))
            .map_err(|_| Error::Bounds)?;
        let input = Input::from_bytes(&bytes).map_err(|_| Error::Bounds)?;
        let root_budget = budget(group.executor)?;
        let owner = load_boxed(
            &group.factory,
            group.executor,
            root_image,
            true,
            Some(&input),
        )?;
        if owner.frame_count() + BOX_FRAMES != root_charge {
            group.executor.retire();
            return Err(Error::Unavailable);
        }
        let root = TaskId {
            group: group.namespace,
            slot: 0,
            generation: group.slots[0].next,
        };
        let authority = authority_box(
            group.factory.pool(),
            Authority {
                root,
                launch: Some(generation),
                console_allowed,
                caps: [None; 2],
                catalog: [first, second],
                closing: None,
                published: None,
                waiting: None,
                dispatching: None,
                sequence: 0,
                #[cfg(feature = "task-negative-tests")]
                negative: negative.map(negative::HookState::new),
            },
        )?;
        if before.checked_sub(group.factory.pool().stats().free) != Some(total) {
            group.executor.retire();
            return Err(Error::Unavailable);
        }
        populate(&mut group.slots[0], owner, root, root_budget);
        group.charged = total;
        group.task = Some(authority);
        Ok(group)
    }
    pub fn task_root(&self) -> Result<TaskId, Error> {
        self.task.as_ref().map(|a| a.root).ok_or(Error::Mode)
    }
    /// Read-only test observation, also available after task metadata is reaped.
    #[cfg(any(feature = "task-tests", feature = "task-negative-tests"))]
    pub(crate) fn task_issuance_snapshot(&self) -> (u32, Option<u32>) {
        self.executor.task_issuance_snapshot()
    }
    /// Report only the currently stored launch grant, never reconstruct a revoked one.
    #[cfg(any(feature = "task-tests", feature = "task-negative-tests"))]
    pub(crate) fn task_launch_generation(&self) -> Result<u32, Error> {
        self.task
            .as_ref()
            .and_then(|authority| authority.launch)
            .ok_or(Error::Mode)
    }
    fn authority(&self) -> &Authority<'p> {
        self.task.as_ref().unwrap_or_else(|| process::fatal())
    }
    fn authority_mut(&mut self) -> &mut Authority<'p> {
        self.task.as_mut().unwrap_or_else(|| process::fatal())
    }
    fn root_gate(&self, id: TaskId) -> Result<(), Error> {
        if self.task_root()? == id {
            Ok(())
        } else {
            Err(Error::Mode)
        }
    }
    pub(super) fn task_info(&self, id: TaskId) -> Result<TaskInfo, Error> {
        self.root_gate(id)?;
        let mut info = self.slots[self.index(id)?].info();
        let a = self.authority();
        info.state = if a.published.is_some() {
            TaskState::Completed
        } else if a.closing.is_some() {
            TaskState::Closing
        } else if self.slots[id.slot()].cancel {
            TaskState::CancelPending
        } else if a.waiting.is_some() {
            TaskState::Waiting
        } else {
            TaskState::Ready
        };
        Ok(info)
    }
    pub(super) fn task_cancel(&mut self, id: TaskId) -> Result<(), Error> {
        self.root_gate(id)?;
        if self.authority().closing.is_none() && self.authority().published.is_none() {
            self.slots[id.slot()].cancel = true;
        }
        Ok(())
    }
    pub fn task_completion(&self, id: TaskId) -> Result<Option<TaskCompletion>, Error> {
        self.root_gate(id)?;
        Ok(self.authority().published)
    }
    pub fn task_reap(&mut self, id: TaskId) -> Result<TaskCompletion, Error> {
        self.root_gate(id)?;
        let done = self.authority().published.ok_or(Error::Pending)?;
        let lease = match Lease::acquire() {
            Ok(l) => l,
            Err(e) => {
                if e != process::Error::Busy {
                    self.executor.retire();
                }
                return Err(lease_error(e));
            }
        };
        let slot = &mut self.slots[id.slot()];
        slot.next = id.generation.checked_add(1).unwrap_or(0);
        slot.info = None;
        slot.budget = None;
        slot.cancel = false;
        drop(self.task.take());
        self.charged = self
            .charged
            .checked_sub(METADATA_FRAMES)
            .unwrap_or_else(|| process::fatal());
        if self.charged != 0 {
            process::fatal();
        }
        self.task_closed = true;
        drop(lease);
        Ok(done)
    }
}

#[derive(Clone, Copy, Default)]
struct Counters {
    calls: usize,
    written: usize,
}
enum Settlement {
    Reply { session: Session, outcome: Outcome },
    Park(Waiting),
}
struct Work {
    counters: Counters,
    settlement: Option<Settlement>,
    failed: bool,
}
impl Work {
    fn new() -> Self {
        Self {
            counters: Counters::default(),
            settlement: None,
            failed: false,
        }
    }
}
// Fixed single-turn bytes. No descriptor, source or output borrow survives copying.
#[derive(Clone, Copy)]
struct Commit {
    child: TaskId,
    token: u64,
    program: u32,
    console: u32,
    entry: u64,
    root: u64,
    frames: usize,
    before: usize,
    after: usize,
}
#[derive(Clone, Copy)]
struct Consume {
    pending: Pending,
    outcome: u64,
}
#[derive(Clone, Copy)]
struct Settle {
    kind: &'static str,
    status: u64,
    result: u64,
    pending: Option<Pending>,
}
struct TaskCapture {
    descriptor: [u8; 64],
    input: [u8; 256],
    output: GroupCapture,
    commit: Option<Commit>,
    consumed: Option<Consume>,
    descriptor_copied: bool,
    input_copied: usize,
    settle: Option<Settle>,
    #[cfg(feature = "task-negative-tests")]
    injection: Option<negative::Injection>,
}
impl TaskCapture {
    fn new() -> Self {
        Self {
            descriptor: [0; 64],
            input: [0; 256],
            output: GroupCapture::new(),
            commit: None,
            consumed: None,
            descriptor_copied: false,
            input_copied: 0,
            settle: None,
            #[cfg(feature = "task-negative-tests")]
            injection: None,
        }
    }
}
const _: () = assert!(
    core::mem::size_of::<TaskCapture>()
        + core::mem::size_of::<Work>()
        + core::mem::size_of::<Management>()
        <= 1536
);
fn response(status: Status, written: u64) -> Response {
    Response { status, written }
}
fn reply(guard: TaskAdmission, status: Status, written: u64) -> Settlement {
    let (session, outcome) = guard.reply(response(status, written));
    Settlement::Reply { session, outcome }
}
fn wire_error(error: task_wire::Error) -> Status {
    match error {
        task_wire::Error::Length => Status::Length,
        task_wire::Error::Address => Status::Address,
        task_wire::Error::Header
        | task_wire::Error::Flags
        | task_wire::Error::Reserved
        | task_wire::Error::Rights
        | task_wire::Error::Frames
        | task_wire::Error::Input
        | task_wire::Error::Arguments => Status::Argument,
        _ => process::fatal(),
    }
}
fn completion_word(end: End) -> u64 {
    match end {
        End::Exited(code) => task_wire::Completion::Exited(code),
        End::Fault => task_wire::Completion::Fault,
        End::Budget => task_wire::Completion::Budget,
        End::CallLimit => task_wire::Completion::CallLimit,
        End::TurnLimit => task_wire::Completion::TurnLimit,
        End::Cancelled => task_wire::Completion::Cancelled,
        End::Unavailable => task_wire::Completion::Unavailable,
    }
    .encode()
}

impl<'e, 'p> Group<'e, 'p> {
    fn selected_owner(&self, index: usize) -> &Process<'p> {
        self.slots[index]
            .owner
            .as_ref()
            .unwrap_or_else(|| process::fatal())
    }
    fn selected_owner_mut(&mut self, index: usize) -> &mut Process<'p> {
        self.slots[index]
            .owner
            .as_mut()
            .unwrap_or_else(|| process::fatal())
    }
    fn lookup_cap(&self, parent: TaskId, token: u64) -> Result<(usize, Cap), Status> {
        if parent != self.authority().root {
            return Err(Status::Handle);
        }
        let low = token as u32;
        if !(3..=4).contains(&low) || token >> 32 == 0 {
            return Err(Status::Handle);
        }
        let index = (low - 3) as usize;
        let cap = self.authority().caps[index].ok_or(Status::Handle)?;
        if cap.generation != (token >> 32) as u32 || self.index(cap.child).is_err() {
            return Err(Status::Handle);
        }
        Ok((index, cap))
    }
    fn consume_child(&mut self, pending: Pending) -> Result<Completion, Error> {
        let cap = self.authority().caps[pending.slot].ok_or(Error::Unavailable)?;
        if cap.generation != pending.generation || cap.child != pending.child {
            return Err(Error::Unavailable);
        }
        let done = self.reap_internal(pending.child)?;
        self.authority_mut().caps[pending.slot] = None;
        Ok(done)
    }
    fn dispatch_owned(
        &mut self,
        index: usize,
        guard: TaskAdmission,
        capture: &mut TaskCapture,
        _lease: &Lease,
    ) -> Result<Settlement, Error> {
        let request = guard.request();
        let parent = self.slots[index].info.ok_or(Error::Unavailable)?.id;
        match guard.operation() {
            TaskOperation::Spawn => {
                let a = self.authority();
                if parent != a.root
                    || a.launch.is_none()
                    || request.argument as u32 != 2
                    || request.argument >> 32 != u64::from(a.launch.unwrap_or(0))
                {
                    return Ok(reply(guard, Status::Handle, 0));
                }
                if request.length != 64 {
                    return Ok(reply(guard, Status::Length, 0));
                }
                if self
                    .selected_owner(index)
                    .copy_task_bytes(request.address, &mut capture.descriptor)
                    .is_err()
                {
                    return Ok(reply(guard, Status::Address, 0));
                }
                capture.descriptor_copied = true;
                let spawn = match task_wire::Spawn::decode(&capture.descriptor) {
                    Ok(s) => s,
                    Err(e) => return Ok(reply(guard, wire_error(e), 0)),
                };
                let program = match spawn.program() {
                    1 => 0,
                    2 => 1,
                    _ => return Ok(reply(guard, Status::Argument, 0)),
                };
                if spawn.console() && !self.authority().console_allowed {
                    return Ok(reply(guard, Status::Denied, 0));
                }
                let input = match spawn.input() {
                    None => Input::absent(),
                    Some((address, length)) => {
                        if length != 0
                            && self
                                .selected_owner(index)
                                .copy_task_bytes(address, &mut capture.input[..length as usize])
                                .is_err()
                        {
                            return Ok(reply(guard, Status::Address, 0));
                        }
                        capture.input_copied = length as usize;
                        Input::from_bytes(&capture.input[..length as usize])
                            .map_err(|_| Error::Unavailable)?
                    }
                };
                let Some(cap_slot) = self.authority().caps.iter().position(Option::is_none) else {
                    return Ok(reply(guard, Status::Resources, 0));
                };
                let Some(child_slot) = self
                    .slots
                    .iter()
                    .position(|s| s.info.is_none() && s.next != 0)
                else {
                    return Ok(reply(guard, Status::Resources, 0));
                };
                let required = charge(&self.authority().catalog[program])?;
                if required > spawn.max_frames() as usize {
                    return Ok(reply(guard, Status::Resources, 0));
                }
                let Some(total) = self
                    .charged
                    .checked_add(required)
                    .filter(|n| *n <= self.limit)
                else {
                    return Ok(reply(guard, Status::Resources, 0));
                };
                let child_budget = budget(self.executor)?;
                let Some(generation) = self.executor.issue_task_cap() else {
                    return Ok(reply(guard, Status::Resources, 0));
                };
                let child = TaskId {
                    group: self.namespace,
                    slot: child_slot as u8,
                    generation: self.slots[child_slot].next,
                };
                let image = &self.task.as_ref().ok_or(Error::Unavailable)?.catalog[program];
                let before = self.factory.pool().stats().free;
                let owner = match load_boxed(
                    &self.factory,
                    self.executor,
                    image,
                    spawn.console(),
                    Some(&input),
                ) {
                    Ok(owner) => owner,
                    Err(Error::Resources) => return Ok(reply(guard, Status::Resources, 0)),
                    Err(e) => return Err(e),
                };
                if owner.frame_count() + BOX_FRAMES != required
                    || self.authority().dispatching != Some(parent)
                    || self.slots[child_slot].info.is_some()
                    || self.authority().caps[cap_slot].is_some()
                {
                    return Err(Error::Unavailable);
                }
                let token = (u64::from(generation) << 32) | (cap_slot as u64 + 3);
                let committed = Commit {
                    child,
                    token,
                    program: spawn.program(),
                    console: (owner.handle() >> 32) as u32,
                    entry: owner.entry(),
                    root: owner.root_address(),
                    frames: owner.frame_count(),
                    before,
                    after: self.factory.pool().stats().free,
                };
                populate(&mut self.slots[child_slot], owner, child, child_budget);
                self.authority_mut().caps[cap_slot] = Some(Cap {
                    generation,
                    child,
                    rights: spawn.rights(),
                });
                self.charged = total;
                if capture.commit.replace(committed).is_some() {
                    process::fatal();
                }
                #[cfg(feature = "task-negative-tests")]
                self.negative_commit(child, spawn.program());
                Ok(reply(guard, Status::Ok, token))
            }
            TaskOperation::Wait | TaskOperation::Cancel => {
                let (slot, cap) = match self.lookup_cap(parent, request.argument) {
                    Ok(v) => v,
                    Err(status) => return Ok(reply(guard, status, 0)),
                };
                let pending = Pending {
                    child: cap.child,
                    slot,
                    generation: cap.generation,
                };
                if guard.operation() == TaskOperation::Cancel {
                    if cap.rights != Rights::WaitCancel {
                        return Ok(reply(guard, Status::Denied, 0));
                    }
                    if let Err(e) = task_wire::cancel_arguments(request.address, request.length) {
                        return Ok(reply(guard, wire_error(e), 0));
                    }
                    let child = &mut self.slots[cap.child.slot()];
                    if child.completion.is_some() {
                        return Ok(reply(guard, Status::Ok, 1));
                    }
                    child.cancel = true;
                    Ok(reply(guard, Status::Ok, 0))
                } else {
                    let mode = match WaitMode::decode(request.address, request.length) {
                        Ok(m) => m,
                        Err(e) => return Ok(reply(guard, wire_error(e), 0)),
                    };
                    if self.slots[cap.child.slot()].completion.is_some() {
                        let done = self.consume_child(pending)?;
                        if capture
                            .consumed
                            .replace(Consume {
                                pending,
                                outcome: completion_word(done.end),
                            })
                            .is_some()
                        {
                            process::fatal();
                        }
                        Ok(reply(guard, Status::Ok, completion_word(done.end)))
                    } else if mode == WaitMode::Poll {
                        Ok(reply(guard, Status::Pending, 0))
                    } else {
                        let session = guard.park_wait().map_err(|_| Error::Unavailable)?;
                        Ok(Settlement::Park(Waiting { session, pending }))
                    }
                }
            }
        }
    }
    fn dispatch_task(
        &mut self,
        index: usize,
        work: &mut Work,
        capture: &mut TaskCapture,
        lease: &Lease,
    ) {
        let id = self.slots[index]
            .info
            .unwrap_or_else(|| process::fatal())
            .id;
        if self.authority().dispatching.is_some() {
            process::fatal();
        }
        self.authority_mut().dispatching = Some(id);
        let owner = self.slots[index]
            .owner
            .as_mut()
            .unwrap_or_else(|| process::fatal());
        let admission = self.executor.admit_task_call(owner, &mut capture.output);
        let settlement = match admission {
            Admission::Complete { session, outcome } => {
                work.counters = Counters {
                    calls: session.calls(),
                    written: session.written(),
                };
                Ok(Settlement::Reply { session, outcome })
            }
            Admission::Task(guard) => {
                work.counters = Counters {
                    calls: guard.calls(),
                    written: guard.written(),
                };
                #[cfg(feature = "task-negative-tests")]
                let abandon = self.negative_abandon(index, &guard, capture);
                #[cfg(not(feature = "task-negative-tests"))]
                let abandon = false;
                if abandon {
                    // Actual owned admission is consumed, preserving its copied counters.
                    let _abandoned = guard;
                    Err(Error::Unavailable)
                } else {
                    self.dispatch_owned(index, guard, capture, lease)
                }
            }
        };
        match settlement {
            Ok(s) => {
                if let Settlement::Reply {
                    outcome: Outcome::Reply(reply),
                    ..
                } = &s
                {
                    capture.output.record_reply(*reply);
                }
                capture.settle = Some(match &s {
                    Settlement::Reply {
                        outcome: Outcome::Reply(r),
                        ..
                    } => Settle {
                        kind: "reply",
                        status: r.status as u64,
                        result: r.written,
                        pending: None,
                    },
                    Settlement::Reply {
                        outcome: Outcome::Stop(_),
                        ..
                    } => Settle {
                        kind: "terminate",
                        status: 0,
                        result: 0,
                        pending: None,
                    },
                    Settlement::Park(waiting) => Settle {
                        kind: "park",
                        status: 0,
                        result: 0,
                        pending: Some(waiting.pending),
                    },
                });
                work.settlement = Some(s);
            }
            Err(_) => {
                capture.settle = Some(Settle {
                    kind: "failure",
                    status: 0,
                    result: 0,
                    pending: None,
                });
                work.failed = true;
                self.executor.retire();
            }
        }
    }
}
